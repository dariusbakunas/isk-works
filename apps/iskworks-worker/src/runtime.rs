use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use iskworks_app::{CharacterSyncService, MarketAccessResolver, PublicMarketService};
use iskworks_esi::EsiTransport;
use iskworks_storage::{
    AuthPurgeOutcome, EsiObservationPruneOutcome, PgAuthMaintenance, PgEsiRepository,
    PgMarketRepository,
};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::WorkerConfig;

#[derive(Clone)]
pub struct EvidenceWorker<T: EsiTransport + 'static> {
    config: WorkerConfig,
    esi_repository: Arc<PgEsiRepository>,
    market_repository: Arc<PgMarketRepository>,
    transport: Arc<T>,
    market: PublicMarketService,
    character_sync: Option<CharacterSyncService>,
    auth_maintenance: Option<PgAuthMaintenance>,
}

impl<T: EsiTransport + 'static> EvidenceWorker<T> {
    pub fn new(
        config: WorkerConfig,
        esi_repository: Arc<PgEsiRepository>,
        market_repository: Arc<PgMarketRepository>,
        transport: Arc<T>,
    ) -> Self {
        let market = PublicMarketService::new(market_repository.clone(), Arc::clone(&transport))
            .with_scheduler(
                config.market_item_batch_size,
                config.lease_duration,
                config.market_freshness,
            );
        Self {
            config,
            esi_repository,
            market_repository: Arc::clone(&market_repository),
            market,
            transport,
            character_sync: None,
            auth_maintenance: None,
        }
    }

    /// Character sync needs an authenticated, OAuth-token-refreshing
    /// transport (`CharacterSyncService` already carries its own) and is
    /// only meaningful once EVE SSO is configured -- `None` here (the
    /// default) means the worker's character-sync pass is a no-op, matching
    /// how `EsiApplicationService::from_env` returns `None` rather than an
    /// error when ESI isn't configured.
    #[must_use]
    pub fn with_character_sync(mut self, character_sync: Option<CharacterSyncService>) -> Self {
        self.character_sync = character_sync;
        self
    }

    #[must_use]
    pub fn with_auth_maintenance(mut self, auth_maintenance: Option<PgAuthMaintenance>) -> Self {
        self.auth_maintenance = auth_maintenance;
        self
    }

    /// One periodic purge of expired sessions and stale pending OAuth
    /// authorizations. A no-op when no auth maintenance is wired in.
    pub async fn purge_expired_auth(&self, now: DateTime<Utc>) -> Result<AuthPurgeOutcome, String> {
        match &self.auth_maintenance {
            Some(maintenance) => maintenance
                .purge_expired(now)
                .await
                .map_err(|error| error.to_string()),
            None => Ok(AuthPurgeOutcome::default()),
        }
    }

    /// Mirrors `with_character_sync`'s optionality: `None` when ESI isn't
    /// configured, in which case structure-scoped market candidates simply
    /// fail with a clear message instead of falling back to any other path.
    #[must_use]
    pub fn with_market_access(
        mut self,
        market_access: Option<Arc<dyn MarketAccessResolver>>,
    ) -> Self {
        self.market = self.market.with_market_access(market_access);
        self
    }

    /// One periodic GC pass over orphaned market observation batches. See
    /// `PgMarketRepository::prune_orphaned_market_observations`. Chunked so a
    /// large pre-existing backlog drains over several passes without a long
    /// lock or a WAL spike.
    pub async fn sweep_orphaned_market_observations(
        &self,
        now: DateTime<Utc>,
    ) -> Result<iskworks_storage::MarketObservationPruneOutcome, String> {
        // Orphan batches per chunk -- an ESI batch holds ~1-150 orders, so
        // this deletes low tens of thousands of observation rows per
        // transaction. 2000 chunks (~1M batches) caps one pass.
        const BATCHES_PER_CHUNK: i64 = 500;
        const MAX_CHUNKS_PER_PASS: u32 = 2_000;
        let older_than = now
            - chrono::Duration::from_std(self.config.market_gc_grace)
                .map_err(|error| error.to_string())?;
        self.market_repository
            .prune_orphaned_market_observations(older_than, BATCHES_PER_CHUNK, MAX_CHUNKS_PER_PASS)
            .await
            .map_err(|error| error.to_string())
    }

    /// One retention pass over ESI observations only read at their latest:
    /// superseded asset snapshots, then superseded adjusted prices. Chunked
    /// like the market GC so the backlog left by earlier releases drains
    /// over several passes.
    pub async fn sweep_superseded_esi_observations(
        &self,
    ) -> Result<(EsiObservationPruneOutcome, EsiObservationPruneOutcome), String> {
        // A snapshot is one character's whole asset list (up to tens of
        // thousands of rows with their hierarchy), so keep chunks small.
        const SNAPSHOTS_PER_CHUNK: i64 = 5;
        const MAX_SNAPSHOT_CHUNKS_PER_PASS: u32 = 400;
        // One adjusted-price refresh is ~16k rows.
        const PRICES_PER_CHUNK: i64 = 20_000;
        const MAX_PRICE_CHUNKS_PER_PASS: u32 = 500;
        let assets = self
            .esi_repository
            .prune_superseded_asset_snapshots(SNAPSHOTS_PER_CHUNK, MAX_SNAPSHOT_CHUNKS_PER_PASS)
            .await
            .map_err(|error| error.to_string())?;
        let prices = self
            .esi_repository
            .prune_superseded_adjusted_prices(PRICES_PER_CHUNK, MAX_PRICE_CHUNKS_PER_PASS)
            .await
            .map_err(|error| error.to_string())?;
        Ok((assets, prices))
    }

    /// One market pass: due app-wide public coverage first (one regional
    /// fetch per `(region, type)`, shared by every workspace), then due
    /// per-workspace sources (structure markets, and public scopes not yet
    /// moved to app-wide coverage). Returns how many public rows plus
    /// sources were due.
    pub async fn refresh_due_market(
        &self,
        now: DateTime<Utc>,
        cancel: &CancellationToken,
    ) -> Result<usize, String> {
        let public = self
            .market_repository
            .due_public_market_work(now, self.config.market_item_batch_size)
            .await
            .map_err(|error| error.to_string())?;
        let public_count = public.len();
        self.market.refresh_public(public, cancel).await;
        if cancel.is_cancelled() {
            return Ok(public_count);
        }
        let sources = self
            .market_repository
            .due_market_sources(now, self.config.source_batch_size)
            .await
            .map_err(|error| error.to_string())?;
        let count = sources.len();
        let mut failures = Vec::new();
        for (workspace_id, source_id) in sources {
            // Shutdown: stop advancing to the next source. `refresh_source`
            // itself also stops dispatching mid-source and drains its
            // in-flight per-item tasks promptly.
            if cancel.is_cancelled() {
                break;
            }
            if let Err(error) = self
                .market
                .refresh_source(workspace_id, source_id, cancel)
                .await
            {
                failures.push(format!("{source_id:?}: {error}"));
                continue;
            }
        }
        if !failures.is_empty() {
            return Err(failures.join("; "));
        }
        Ok(public_count + count)
    }

    pub async fn refresh_due_adjusted_prices(&self, now: DateTime<Utc>) -> Result<bool, String> {
        self.esi_repository
            .register_adjusted_price_refresh(now)
            .await
            .map_err(|error| error.to_string())?;
        if !self
            .esi_repository
            .adjusted_price_refresh_due(now)
            .await
            .map_err(|error| error.to_string())?
        {
            return Ok(false);
        }
        let lease_expires_at = now
            + chrono::Duration::from_std(self.config.lease_duration)
                .map_err(|error| error.to_string())?;
        let Some(claim) = self
            .esi_repository
            .begin_adjusted_price_refresh(now, lease_expires_at)
            .await
            .map_err(|error| error.to_string())?
        else {
            return Ok(false);
        };
        match self.transport.market_prices().await {
            Ok(response) => {
                let next_refresh_at = now
                    + chrono::Duration::from_std(self.config.adjusted_price_freshness)
                        .map_err(|error| error.to_string())?;
                self.esi_repository
                    .complete_adjusted_price_refresh(
                        claim,
                        &response.records,
                        now,
                        next_refresh_at,
                        response.metadata.expires_at(),
                        response.metadata.etag.as_deref(),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(true)
            }
            Err(error) => {
                let next_retry = now + retry_floor(self.config.retry_delay, &error);
                self.esi_repository
                    .fail_adjusted_price_refresh(claim, now, next_retry, &error.to_string())
                    .await
                    .map_err(|storage_error| storage_error.to_string())?;
                Err(error.to_string())
            }
        }
    }

    pub async fn refresh_due_system_indices(&self, now: DateTime<Utc>) -> Result<usize, String> {
        let systems = self
            .esi_repository
            .system_cost_index_refresh_candidates(now, self.config.system_batch_size)
            .await
            .map_err(|error| error.to_string())?;
        let lease_expires_at = now
            + chrono::Duration::from_std(self.config.lease_duration)
                .map_err(|error| error.to_string())?;
        let mut claimed = Vec::new();
        let mut persistence_failures = Vec::new();
        for system_id in systems {
            match self
                .esi_repository
                .begin_system_cost_index_refresh(system_id, now, lease_expires_at)
                .await
            {
                Ok(Some(claim)) => claimed.push((system_id, claim)),
                Ok(None) => {}
                Err(error) => persistence_failures.push(format!("{system_id}: {error}")),
            }
        }
        if claimed.is_empty() {
            return if persistence_failures.is_empty() {
                Ok(0)
            } else {
                Err(persistence_failures.join("; "))
            };
        }
        match self.transport.industry_systems().await {
            Ok(response) => {
                let values = response
                    .records
                    .into_iter()
                    .map(|value| (value.solar_system_id, value.manufacturing))
                    .collect::<BTreeMap<_, _>>();
                let next_refresh_at = now
                    + chrono::Duration::from_std(self.config.system_index_freshness)
                        .map_err(|error| error.to_string())?;
                for (system_id, claim) in &claimed {
                    if let Some(value) = values.get(system_id) {
                        if let Err(error) = self
                            .esi_repository
                            .complete_system_cost_index_refresh(
                                *system_id,
                                *claim,
                                *value,
                                now,
                                next_refresh_at,
                                response.metadata.expires_at(),
                                response.metadata.etag.as_deref(),
                            )
                            .await
                        {
                            persistence_failures.push(format!("{system_id}: {error}"));
                        }
                    } else {
                        if let Err(error) = self
                            .esi_repository
                            .fail_system_cost_index_refresh(
                                *system_id,
                                *claim,
                                now,
                                next_refresh_at,
                                "ESI response omitted the registered solar system",
                            )
                            .await
                        {
                            persistence_failures.push(format!("{system_id}: {error}"));
                        }
                    }
                }
                if persistence_failures.is_empty() {
                    Ok(claimed.len())
                } else {
                    Err(persistence_failures.join("; "))
                }
            }
            Err(error) => {
                let next_retry = now + retry_floor(self.config.retry_delay, &error);
                for (system_id, claim) in &claimed {
                    if let Err(storage_error) = self
                        .esi_repository
                        .fail_system_cost_index_refresh(
                            *system_id,
                            *claim,
                            now,
                            next_retry,
                            &error.to_string(),
                        )
                        .await
                    {
                        persistence_failures.push(format!("{system_id}: {storage_error}"));
                    }
                }
                persistence_failures.push(error.to_string());
                Err(persistence_failures.join("; "))
            }
        }
    }

    /// Pulls due `(connection, source_kind)` rows, dedupes to distinct
    /// connections (`CharacterSyncService::sync_connection` already
    /// re-evaluates due-ness per source, so one call per connection covers
    /// every one of its due sources without re-querying), and syncs them
    /// concurrently up to `character_sync_concurrency` -- its own semaphore,
    /// separate from market's, so a burst of character work can't starve
    /// market refresh or vice versa.
    pub async fn refresh_due_character_sources(
        &self,
        now: DateTime<Utc>,
        cancel: &CancellationToken,
    ) -> Result<usize, String> {
        let Some(character_sync) = self.character_sync.clone() else {
            return Ok(0);
        };
        let due = self
            .esi_repository
            .due_character_sources(now, self.config.character_sync_batch_size)
            .await
            .map_err(|error| error.to_string())?;
        let connection_ids: HashSet<_> = due
            .into_iter()
            .map(|(connection_id, _)| connection_id)
            .collect();
        let semaphore = Arc::new(Semaphore::new(self.config.character_sync_concurrency));
        let mut tasks = Vec::with_capacity(connection_ids.len());
        for connection_id in connection_ids {
            // Shutdown: stop dispatching new connections. Connections not
            // spawned here stay due for the next (post-restart) pass.
            if cancel.is_cancelled() {
                break;
            }
            let character_sync = character_sync.clone();
            let semaphore = Arc::clone(&semaphore);
            let cancel = cancel.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = tokio::select! {
                    biased;
                    () = cancel.cancelled() => return,
                    permit = semaphore.acquire_owned() => permit,
                };
                character_sync.sync_connection(connection_id, &cancel).await;
            }));
        }
        let dispatched = tasks.len();
        for task in tasks {
            if let Err(error) = task.await {
                tracing::warn!(%error, "character sync task panicked");
            }
        }
        Ok(dispatched)
    }
}

/// The earliest retry after a failed ESI refresh: the configured delay, or
/// longer when ESI asked us to wait (`EsiError::retry_after`). Storage adds
/// exponential backoff on top for repeated failures.
fn retry_floor(
    retry_delay: std::time::Duration,
    error: &iskworks_esi::EsiError,
) -> chrono::Duration {
    let wait = error.retry_after().unwrap_or_default().max(retry_delay);
    chrono::Duration::from_std(wait).unwrap_or_else(|_| chrono::Duration::hours(6))
}
