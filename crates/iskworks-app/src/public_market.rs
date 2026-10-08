use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use iskworks_core::{
    ConnectedCharacterId, EsiMarketObservationBatch, EsiMarketOrder, MarketCoverageItem,
    MarketCoverageRegistration, MarketError, MarketLocationClassification,
    MarketObservationBatchId, MarketOrderSide, MarketRefreshFailure, MarketRepository, MarketScope,
    PriceSourceId, PublicMarketCoverageWork, PublicMarketObservationBatch, WorkspaceId,
};
use iskworks_esi::{EsiError, EsiResponse, EsiTransport, MarketOrderObservation};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::esi_service::{MarketAccessResolution, MarketAccessResolver};

mod fetch;
mod repository;
mod structure;
mod target;
use fetch::*;
pub use repository::*;
use structure::StructureRefreshTarget;
use target::*;

const REFRESH_RETRY_DELAY: Duration = Duration::minutes(1);
const MAX_PAGES_PER_TYPE: u32 = 1_000;
const MAX_FETCH_ATTEMPTS: u32 = 3;
/// How many registered items one API request fetches itself. The rest stay
/// prioritized for the worker. Without a bound, one click on a large market
/// group spawned a fetch per type (up to 20,000), each holding its refresh
/// lease while it waited for one of four request permits; the leases
/// expired in the queue and the worker fetched the same rows again.
const API_INLINE_REFRESH_LIMIT: usize = 25;
const FETCH_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(250);

/// Upper bound on a previous book's order count for a *conditional*
/// (`If-None-Match`) regional-market refresh to be eligible.
///
/// We do not persist the previous `X-Pages` value or per-page ETags. The
/// stored ETag is page 1 only, and ESI paginates regional market orders at
/// 1000 records per page. The safety margin below that boundary avoids
/// conditionally validating books that sit near the pagination boundary
/// (and may already be, or may have just become, multi-page). Multi-page
/// conditional refresh is intentionally deferred.
const CONDITIONAL_REFRESH_MAX_ORDERS: u64 = 900;

#[derive(Clone)]
pub struct PublicMarketService {
    repository: Arc<dyn MarketRefreshRepository>,
    transport: Arc<dyn EsiTransport>,
    /// `None` when ESI isn't configured (matches every other optional
    /// ESI-backed collaborator in the worker, e.g. `character_sync`) --
    /// a structure-scoped candidate fails immediately with a clear message
    /// rather than silently falling back to the public path.
    market_access: Option<Arc<dyn MarketAccessResolver>>,
    request_limit: Arc<Semaphore>,
    /// Shared 429 back-off gate for the public regional market-order
    /// endpoint -- see [`RegionalMarketCooldown`].
    market_cooldown: RegionalMarketCooldown,
    item_batch_size: i64,
    lease_duration: std::time::Duration,
    refresh_interval: std::time::Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MarketRefreshOutcome {
    Succeeded,
    /// ESI answered a conditional request with `304 Not Modified`: the
    /// existing observation batch stays authoritative and only coverage
    /// freshness was advanced. A success, tracked separately so the
    /// optimization is visible in the per-source aggregate log.
    Revalidated,
    Failed,
    Skipped,
}

impl PublicMarketService {
    #[must_use]
    pub fn new<R, T>(repository: Arc<R>, transport: Arc<T>) -> Self
    where
        R: MarketRefreshRepository + 'static,
        T: EsiTransport + 'static,
    {
        Self {
            repository,
            transport,
            market_access: None,
            request_limit: Arc::new(Semaphore::new(4)),
            market_cooldown: RegionalMarketCooldown::default(),
            item_batch_size: 100,
            lease_duration: std::time::Duration::from_secs(120),
            refresh_interval: std::time::Duration::from_secs(300),
        }
    }

    #[must_use]
    pub fn with_scheduler(
        mut self,
        item_batch_size: i64,
        lease_duration: std::time::Duration,
        refresh_interval: std::time::Duration,
    ) -> Self {
        self.item_batch_size = item_batch_size.max(1);
        self.lease_duration = lease_duration;
        self.refresh_interval = refresh_interval;
        self
    }

    /// `None` (the default) leaves structure-scoped candidates failing with
    /// a clear "ESI not configured" message -- mirrors
    /// `EvidenceWorker::with_character_sync`'s optionality for the same
    /// reason (no EVE SSO client configured in this environment).
    #[must_use]
    pub fn with_market_access(mut self, resolver: Option<Arc<dyn MarketAccessResolver>>) -> Self {
        self.market_access = resolver;
        self
    }

    /// Registers `items`, then returns coverage scoped to exactly their own
    /// `type_id`s -- **not** `register`'s raw return value, which (for the
    /// real Postgres-backed repository) is every coverage row for this
    /// price source. Callers count from this result directly -- e.g. the
    /// bulk category-request route's `already_current_count` (see
    /// `routes/market/refresh.rs::request_market_data_for_group`) -- so an
    /// unfiltered list would count "already current" across the whole
    /// source instead of just the requested category's items.
    pub async fn register_and_refresh(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        if let Some(region_id) = self
            .public_region_for_source(workspace_id, source_id)
            .await?
        {
            let coverage = self.register_public(region_id, items).await?;
            let service = self.clone();
            let work = inline_public_work(region_id, &coverage, Utc::now());
            // API-initiated background refresh: its lifecycle is the HTTP
            // request, not worker shutdown, so a fresh never-cancelled token.
            tokio::spawn(async move {
                service
                    .refresh_public(work, &CancellationToken::new())
                    .await;
            });
            return Ok(coverage);
        }
        let type_ids: Vec<i64> = items.iter().map(|item| item.type_id).collect();
        let type_id_set: HashSet<i64> = type_ids.iter().copied().collect();
        let coverage = self
            .repository
            .register(workspace_id, source_id, items)
            .await?
            .into_iter()
            .filter(|item| type_id_set.contains(&item.type_id))
            .collect();
        if !type_ids.is_empty() {
            self.repository
                .prioritize(workspace_id, source_id, &type_ids, Utc::now())
                .await?;
        }
        let candidates = self
            .repository
            .candidates(workspace_id, source_id, Utc::now(), self.item_batch_size)
            .await?;
        if !candidates.is_empty() {
            let scope = self.repository.scope(workspace_id, source_id).await?;
            let service = self.clone();
            // API-initiated background dispatch: its lifecycle is the HTTP
            // request, not worker shutdown, so a fresh never-cancelled token.
            let cancel = CancellationToken::new();
            tokio::spawn(async move {
                service
                    .dispatch_refresh(workspace_id, source_id, scope, candidates, &cancel)
                    .await;
            });
        }
        Ok(coverage)
    }

    /// The synchronous counterpart to `register_and_refresh`: registers and
    /// prioritizes exactly like that method, but awaits the ESI dispatch
    /// instead of spawning it, and dispatches only the caller's own
    /// `items` -- never `item_batch_size` worth of whatever else happens
    /// to be due workspace-wide. Used by the single-item "request market
    /// data" route: the Market Browser only ever shows one item's
    /// prices at a time, so a real, bounded, awaited fetch is cheap enough
    /// to sit in the request/response cycle instead of needing a
    /// spawn-then-poll dance.
    ///
    /// Deliberately does not trust `dispatch_refresh`'s own returned
    /// `Vec<MarketRefreshOutcome>` to report the outcome: that vec is only
    /// reliably ordered against its input for the plain public-path
    /// dispatch (`refresh_via_public_path` spawns and awaits one task per
    /// candidate in order) -- the structure-scoped path
    /// (`dispatch_refresh`'s `Structure` branch) first splits candidates
    /// into "failed to lease" vs. "claimed" via `lease_claimed`, then
    /// appends `refresh_structure_scope`'s own outcomes for the claimed
    /// subset, so the final vec's order does not line up with the
    /// original candidate list. Reading back the persisted coverage row
    /// for exactly `items`' type_ids afterward is correct regardless of
    /// which internal path ran.
    pub async fn register_and_refresh_now(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        if let Some(region_id) = self
            .public_region_for_source(workspace_id, source_id)
            .await?
        {
            let type_ids: Vec<i64> = items.iter().map(|item| item.type_id).collect();
            let coverage = self.register_public(region_id, items).await?;
            self.refresh_public(
                inline_public_work(region_id, &coverage, Utc::now()),
                &CancellationToken::new(),
            )
            .await;
            return self.repository.public_coverage(region_id, &type_ids).await;
        }
        let type_ids: Vec<i64> = items.iter().map(|item| item.type_id).collect();
        self.repository
            .register_coverage_only(workspace_id, source_id, items)
            .await?;
        if type_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.repository
            .prioritize(workspace_id, source_id, &type_ids, Utc::now())
            .await?;
        let candidates = self
            .repository
            .candidates_for_type_ids(workspace_id, source_id, &type_ids, Utc::now())
            .await?;
        if !candidates.is_empty() {
            let scope = self.repository.scope(workspace_id, source_id).await?;
            self.dispatch_refresh(
                workspace_id,
                source_id,
                scope,
                candidates,
                &CancellationToken::new(),
            )
            .await;
        }
        self.repository
            .candidates_for_type_ids(workspace_id, source_id, &type_ids, Utc::now())
            .await
    }

    /// The region whose app-wide book serves this price source's scope, or
    /// `None` for a structure (or unknown) location, which keeps
    /// per-workspace coverage. See `MarketScope::public_region`.
    async fn public_region_for_source(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<Option<i64>, MarketError> {
        let scope = self.repository.scope(workspace_id, source_id).await?;
        let classification = match scope.location_id {
            Some(location_id) => Some(
                self.repository
                    .classify_location(workspace_id, location_id)
                    .await?,
            ),
            None => None,
        };
        Ok(scope.public_region(classification))
    }

    /// Records prioritized app-wide demand for `items` and returns their
    /// app-wide coverage rows.
    async fn register_public(
        &self,
        region_id: i64,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        let type_ids: Vec<i64> = items.iter().map(|item| item.type_id).collect();
        if type_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.repository
            .register_public_demand(region_id, items, true, Utc::now())
            .await?;
        self.repository.public_coverage(region_id, &type_ids).await
    }

    /// `cancel` ties this pass to the worker's shutdown lifecycle: once
    /// fired, no new `refresh_type` task is dispatched and every in-flight
    /// one returns promptly (see `regional_market_request` /
    /// `fetch_type_with_retry`). Non-worker callers
    /// (`register_and_refresh[_now]`) pass a fresh, never-cancelled token.
    pub async fn refresh_source(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        cancel: &CancellationToken,
    ) -> Result<(), MarketError> {
        if cancel.is_cancelled() {
            return Ok(());
        }
        let candidates = self
            .repository
            .candidates(workspace_id, source_id, Utc::now(), self.item_batch_size)
            .await?;
        let requested = candidates.len();
        let started_at = Instant::now();
        let outcomes = if candidates.is_empty() || cancel.is_cancelled() {
            Vec::new()
        } else {
            let scope = self.repository.scope(workspace_id, source_id).await?;
            self.dispatch_refresh(workspace_id, source_id, scope, candidates, cancel)
                .await
        };
        let mut succeeded = 0;
        let mut revalidated = 0;
        let mut failed = 0;
        let mut skipped = 0;
        for outcome in outcomes {
            match outcome {
                MarketRefreshOutcome::Succeeded => succeeded += 1,
                MarketRefreshOutcome::Revalidated => revalidated += 1,
                MarketRefreshOutcome::Failed => failed += 1,
                MarketRefreshOutcome::Skipped => skipped += 1,
            }
        }
        tracing::info!(
            source_id = %source_id.0,
            requested,
            succeeded,
            revalidated,
            failed,
            skipped,
            elapsed_ms = started_at.elapsed().as_millis() as u64,
            "market source refresh completed"
        );
        // No items to (re-)register here, and no coverage list is returned:
        // the only caller (the worker's per-tick poll loop) has no use for
        // it, and reloading it would be the same expensive full-source
        // reload `register_and_refresh_now` avoids above, paid on every
        // due-source tick.
        Ok(())
    }

    /// Refreshes due app-wide `public_market_coverage` rows: one regional
    /// fetch per `(region, type)`, stored whole (station scopes filter it on
    /// read), shared by every workspace. Same lease/fetch/304/backoff path
    /// as per-workspace coverage (`refresh_target`), concurrently under the
    /// shared request limit and rate-limit cooldown.
    pub async fn refresh_public(
        &self,
        work: Vec<PublicMarketCoverageWork>,
        cancel: &CancellationToken,
    ) {
        if work.is_empty() || cancel.is_cancelled() {
            return;
        }
        let requested = work.len();
        let started_at = Instant::now();
        let mut tasks = Vec::with_capacity(work.len());
        for PublicMarketCoverageWork { region_id, item } in work {
            if cancel.is_cancelled() {
                break;
            }
            let service = self.clone();
            let cancel = cancel.clone();
            tasks.push(tokio::spawn(async move {
                service
                    .refresh_target(
                        RefreshTarget::Public { region_id },
                        MarketScope {
                            region_id,
                            location_id: None,
                        },
                        item,
                        &cancel,
                    )
                    .await
            }));
        }
        let (mut succeeded, mut revalidated, mut failed, mut skipped) = (0, 0, 0, 0);
        for task in tasks {
            match task.await {
                Ok(MarketRefreshOutcome::Succeeded) => succeeded += 1,
                Ok(MarketRefreshOutcome::Revalidated) => revalidated += 1,
                Ok(MarketRefreshOutcome::Skipped) => skipped += 1,
                Ok(MarketRefreshOutcome::Failed) => failed += 1,
                Err(error) => {
                    failed += 1;
                    tracing::warn!(%error, "public market refresh task failed");
                }
            }
        }
        tracing::info!(
            requested,
            succeeded,
            revalidated,
            failed,
            skipped,
            elapsed_ms = started_at.elapsed().as_millis() as u64,
            "public market refresh completed"
        );
    }

    /// Routes a batch of due candidates to the fetch mechanism `scope`'s
    /// location actually needs -- the classification-branching dispatcher
    /// both `register_and_refresh` and `refresh_source` funnel through, so
    /// neither call site can silently bypass it. The region-wide sentinel (`location_id: None`) and confirmed NPC
    /// stations keep using the existing per-item `refresh_type` path,
    /// byte-for-byte unchanged; only a resolved `Structure` location and an
    /// `Unknown` one take a different path here.
    async fn dispatch_refresh(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        scope: MarketScope,
        candidates: Vec<MarketCoverageItem>,
        cancel: &CancellationToken,
    ) -> Vec<MarketRefreshOutcome> {
        if candidates.is_empty() || cancel.is_cancelled() {
            return Vec::new();
        }
        let Some(location_id) = scope.location_id else {
            return self
                .refresh_via_public_path(workspace_id, source_id, scope, candidates, cancel)
                .await;
        };

        let classification = match self
            .repository
            .classify_location(workspace_id, location_id)
            .await
        {
            Ok(classification) => classification,
            Err(error) => {
                tracing::warn!(
                    %error,
                    source_id = %source_id.0,
                    location_id,
                    "market location classification failed"
                );
                let attempted_at = Utc::now();
                let (claimed, mut outcomes) = self
                    .lease_claimed(workspace_id, source_id, candidates, attempted_at)
                    .await;
                outcomes.extend(
                    self.fail_claimed(workspace_id, source_id, &claimed, error.to_string())
                        .await,
                );
                return outcomes;
            }
        };

        match classification {
            MarketLocationClassification::NpcStation => {
                self.refresh_via_public_path(workspace_id, source_id, scope, candidates, cancel)
                    .await
            }
            MarketLocationClassification::Structure { solar_system_id } => {
                let attempted_at = Utc::now();
                let (claimed, mut outcomes) = self
                    .lease_claimed(workspace_id, source_id, candidates, attempted_at)
                    .await;
                if !claimed.is_empty() {
                    outcomes.extend(
                        self.refresh_structure_scope(
                            StructureRefreshTarget {
                                workspace_id,
                                source_id,
                                scope,
                                location_id,
                                solar_system_id,
                            },
                            claimed,
                            cancel,
                        )
                        .await,
                    );
                }
                outcomes
            }
            MarketLocationClassification::Unknown => {
                let attempted_at = Utc::now();
                let (claimed, mut outcomes) = self
                    .lease_claimed(workspace_id, source_id, candidates, attempted_at)
                    .await;
                outcomes.extend(
                    self.fail_claimed(
                        workspace_id,
                        source_id,
                        &claimed,
                        format!(
                            "location {location_id} is not a known NPC station or resolved structure -- resolve it first"
                        ),
                    )
                    .await,
                );
                outcomes
            }
        }
    }

    /// The public fetch path: one `refresh_type` task per candidate
    /// against the public regional endpoint. Used for the region-wide
    /// sentinel and for confirmed NPC stations.
    async fn refresh_via_public_path(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        scope: MarketScope,
        candidates: Vec<MarketCoverageItem>,
        cancel: &CancellationToken,
    ) -> Vec<MarketRefreshOutcome> {
        let mut tasks = Vec::with_capacity(candidates.len());
        for item in candidates {
            if cancel.is_cancelled() {
                // Shutdown: stop dispatching. Not-yet-spawned candidates
                // are simply left due for the next (post-restart) pass; no
                // row has been leased for them.
                break;
            }
            let service = self.clone();
            let cancel = cancel.clone();
            tasks.push(tokio::spawn(async move {
                service
                    .refresh_type(workspace_id, source_id, scope, item, &cancel)
                    .await
            }));
        }
        // Every spawned child holds a clone of `cancel`, so on shutdown each
        // returns promptly and this join completes -- no detached work is
        // left running past this function.
        let mut outcomes = Vec::with_capacity(tasks.len());
        for task in tasks {
            match task.await {
                Ok(outcome) => outcomes.push(outcome),
                Err(error) => {
                    outcomes.push(MarketRefreshOutcome::Failed);
                    tracing::warn!(
                        %error,
                        source_id = %source_id.0,
                        "market refresh task failed"
                    );
                }
            }
        }
        outcomes
    }

    /// Leases every candidate up front (mirrors the leasing `refresh_type`
    /// does internally per-item) so a single structure fetch can serve all
    /// of them -- candidates that fail to lease (already leased elsewhere,
    /// or a repository error) are reported immediately and excluded from
    /// the returned claimed set.
    async fn lease_claimed(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        candidates: Vec<MarketCoverageItem>,
        attempted_at: DateTime<Utc>,
    ) -> (
        Vec<(MarketCoverageItem, DateTime<Utc>)>,
        Vec<MarketRefreshOutcome>,
    ) {
        let lease_expires_at = attempted_at
            + Duration::from_std(self.lease_duration).unwrap_or_else(|_| Duration::minutes(2));
        let mut claimed = Vec::with_capacity(candidates.len());
        let mut outcomes = Vec::new();
        for item in candidates {
            match self
                .repository
                .begin(
                    workspace_id,
                    source_id,
                    item.type_id,
                    attempted_at,
                    lease_expires_at,
                )
                .await
            {
                Ok(Some(claim)) => claimed.push((item, claim)),
                Ok(None) => outcomes.push(MarketRefreshOutcome::Skipped),
                Err(error) => {
                    tracing::warn!(
                        %error,
                        source_id = %source_id.0,
                        item_name = %item.type_name,
                        type_id = item.type_id,
                        "market refresh lease failed"
                    );
                    outcomes.push(MarketRefreshOutcome::Failed);
                }
            }
        }
        (claimed, outcomes)
    }

    /// Fails every already-claimed candidate with the same message -- used
    /// when something prevents fetching for the whole batch at once (denied
    /// structure access, an `Unknown` location, a classification error)
    /// rather than per-candidate.
    async fn fail_claimed(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        claimed: &[(MarketCoverageItem, DateTime<Utc>)],
        message: String,
    ) -> Vec<MarketRefreshOutcome> {
        let attempted_at = Utc::now();
        let mut outcomes = Vec::with_capacity(claimed.len());
        for (item, claim) in claimed {
            if let Err(persistence_error) = self
                .repository
                .fail(
                    workspace_id,
                    source_id,
                    MarketRefreshFailure {
                        type_id: item.type_id,
                        claim: *claim,
                        attempted_at,
                        next_refresh_at: attempted_at + REFRESH_RETRY_DELAY,
                        error_message: message.clone(),
                    },
                )
                .await
            {
                tracing::warn!(
                    %persistence_error,
                    type_id = item.type_id,
                    "market refresh failure recording failed"
                );
            }
            outcomes.push(MarketRefreshOutcome::Failed);
        }
        outcomes
    }
}

/// The part of freshly registered `coverage` an API request fetches itself:
/// rows due now (never fetched, or past `next_refresh_at`, which already
/// respects ESI's `Expires`), at most `API_INLINE_REFRESH_LIMIT`. The rest
/// stay prioritized and the worker fetches them.
fn inline_public_work(
    region_id: i64,
    coverage: &[MarketCoverageItem],
    now: DateTime<Utc>,
) -> Vec<PublicMarketCoverageWork> {
    coverage
        .iter()
        .filter(|item| {
            item.refresh_state != iskworks_core::MarketRefreshState::Refreshing
                && item.next_refresh_at.map_or(true, |due| due <= now)
        })
        .take(API_INLINE_REFRESH_LIMIT)
        .cloned()
        .map(|item| PublicMarketCoverageWork { region_id, item })
        .collect()
}

fn to_core_order(
    record: MarketOrderObservation,
    expected_location_id: Option<i64>,
) -> Result<EsiMarketOrder, MarketFetchError> {
    EsiMarketOrder::new(
        record.order_id,
        if record.is_buy_order {
            MarketOrderSide::Buy
        } else {
            MarketOrderSide::Sell
        },
        &record.price.to_string(),
        record.volume_remain,
        record.volume_total,
        record.min_volume,
        record.order_range,
        record.issued_at,
        record.duration_days,
        record.location_id,
        record.system_id,
        expected_location_id,
    )
    .map_err(MarketFetchError::Market)
}

#[cfg(test)]
mod tests;
