use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use iskworks_core::{
    calendar_event::{CalendarEventResponse, CharacterCalendarEvent},
    is_running_job_status,
    market_orders::{CharacterMarketOrder, OrderRange},
    CharacterSourceKind, CharacterSyncRepository, ConnectedCharacter, ConnectedCharacterId,
    MarketOrderSide,
};
use iskworks_esi::{
    CharacterIndustryJobObservation, CharacterOrderObservation, EsiTransport, ASSET_SCOPE,
    CALENDAR_SCOPE, INDUSTRY_JOBS_SCOPE, LOCATION_SCOPE, MARKET_ORDERS_SCOPE, PLANETS_SCOPE,
    SKILLS_SCOPE, SKILL_QUEUE_SCOPE, WALLET_SCOPE,
};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::esi_service::{reconnect_status, EsiApplicationError, EsiSyncDispatcher};
use crate::sync_metrics::{record_sync_run, SyncResult};

mod policy;

pub(crate) use policy::SourceFailure;
use policy::{refresh_interval, skills_next_refresh_at};

const LEASE_SECONDS: i64 = 120;
/// ESI returns at most this many calendar events per page.
const CALENDAR_PAGE_LIMIT: usize = 50;
/// Upper bound on calendar pages per sync (500 upcoming events).
const MAX_CALENDAR_PAGES: usize = 10;

/// A source this pass claimed: the repository's lease, and when work on
/// it began (for the sync duration metric).
struct SourceClaim {
    lease: DateTime<Utc>,
    started: Instant,
}
/// How long a connection backs off after a transient token-refresh failure.
const TOKEN_FAILURE_BACKOFF: Duration = Duration::minutes(15);

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CharacterSyncOutcome {
    Succeeded,
    Failed,
    Skipped,
}

/// Gets a valid (refreshed-if-needed) ESI access token for a connection.
/// Separated from `CharacterSyncRepository` because obtaining one requires
/// the OAuth refresh flow (transport call + encrypted-token read/write),
/// which `EsiApplicationService::refresh` already implements correctly --
/// this trait lets the sync service reuse that logic without duplicating
/// it, and lets tests swap in a canned token instead.
#[async_trait]
pub trait AccessTokenProvider: Send + Sync {
    async fn valid_access_token(
        &self,
        connection_id: ConnectedCharacterId,
    ) -> Result<(ConnectedCharacter, String), EsiApplicationError>;
}

#[async_trait]
impl AccessTokenProvider for crate::esi_service::EsiApplicationService {
    async fn valid_access_token(
        &self,
        connection_id: ConnectedCharacterId,
    ) -> Result<(ConnectedCharacter, String), EsiApplicationError> {
        self.refresh(connection_id).await
    }
}

fn granted(connection: &ConnectedCharacter, scope: &str) -> bool {
    connection.granted_scopes.iter().any(|value| value == scope)
}

/// Shutdown mid-sync: mark every source not yet attempted this pass as
/// `Skipped` (it stays due and is retried on the next pass) without
/// persisting a failure for any of them. Cancellation is control flow,
/// not a sync failure.
fn pad_skipped(
    mut outcomes: Vec<(CharacterSourceKind, CharacterSyncOutcome)>,
) -> Vec<(CharacterSourceKind, CharacterSyncOutcome)> {
    for kind in CharacterSourceKind::all() {
        if !outcomes.iter().any(|(done, _)| *done == kind) {
            outcomes.push((kind, CharacterSyncOutcome::Skipped));
        }
    }
    outcomes
}

fn calendar_response(response: iskworks_esi::CalendarEventResponse) -> CalendarEventResponse {
    match response {
        iskworks_esi::CalendarEventResponse::Accepted => CalendarEventResponse::Accepted,
        iskworks_esi::CalendarEventResponse::Declined => CalendarEventResponse::Declined,
        iskworks_esi::CalendarEventResponse::Tentative => CalendarEventResponse::Tentative,
        iskworks_esi::CalendarEventResponse::NotResponded => CalendarEventResponse::NotResponded,
    }
}

fn market_order(order: CharacterOrderObservation) -> Result<CharacterMarketOrder, SourceFailure> {
    let range = OrderRange::from_esi(&order.order_range).ok_or_else(|| {
        SourceFailure::from(format!(
            "order {} has an unknown range {:?}",
            order.order_id, order.order_range
        ))
    })?;
    let count = |value: u64| {
        i64::try_from(value).map_err(|_| {
            SourceFailure::from(format!("order {} volume is out of range", order.order_id))
        })
    };
    Ok(CharacterMarketOrder {
        order_id: order.order_id,
        type_id: order.type_id,
        region_id: order.region_id,
        location_id: order.location_id,
        side: if order.is_buy_order {
            MarketOrderSide::Buy
        } else {
            MarketOrderSide::Sell
        },
        is_corporation: order.is_corporation,
        price: order.price,
        volume_total: count(order.volume_total)?,
        volume_remain: count(order.volume_remain)?,
        min_volume: count(order.min_volume)?,
        range,
        issued_at: order.issued_at,
        duration_days: i32::try_from(order.duration_days).unwrap_or(i32::MAX),
        escrow: order.escrow,
    })
}

#[derive(Clone)]
pub struct CharacterSyncService {
    repository: Arc<dyn CharacterSyncRepository>,
    transport: Arc<dyn EsiTransport>,
    token_provider: Arc<dyn AccessTokenProvider>,
    esi_sync: Arc<dyn EsiSyncDispatcher>,
    jobs_synced: Option<Arc<dyn crate::IndustryJobsSynced>>,
}

impl CharacterSyncService {
    #[must_use]
    pub fn new<R, P, D>(
        repository: Arc<R>,
        transport: Arc<dyn EsiTransport>,
        token_provider: Arc<P>,
        esi_sync: Arc<D>,
    ) -> Self
    where
        R: CharacterSyncRepository + 'static,
        P: AccessTokenProvider + 'static,
        D: EsiSyncDispatcher + 'static,
    {
        Self {
            repository,
            transport,
            token_provider,
            esi_sync,
            jobs_synced: None,
        }
    }

    /// Told after each successful `industry_jobs` refresh, e.g. to link
    /// Board tickets to new jobs.
    #[must_use]
    pub fn with_industry_jobs_synced(mut self, hook: Arc<dyn crate::IndustryJobsSynced>) -> Self {
        self.jobs_synced = Some(hook);
        self
    }

    /// Attempts every due `CharacterSourceKind` for one connection. Never
    /// hard-fails at the top level -- a broken token, a missing scope, or an
    /// ESI outage for one source must not prevent the others from syncing.
    /// Callable both as
    /// a manual recovery action and from the worker's automatic loop.
    pub async fn sync_connection(
        &self,
        connection_id: ConnectedCharacterId,
        cancel: &CancellationToken,
    ) -> Vec<(CharacterSourceKind, CharacterSyncOutcome)> {
        let now = Utc::now();
        let (connection, access_token) =
            match self.token_provider.valid_access_token(connection_id).await {
                Ok(value) => value,
                Err(error) => {
                    // A refresh EVE rejected for good has already flagged the
                    // connection, which takes it out of the due queue. Any
                    // other failure backs the whole connection off -- without
                    // that its sources stay due, at the head of the queue,
                    // and are retried (one SSO call each) every pass.
                    if reconnect_status(&error).is_none() {
                        tracing::warn!(
                            %error,
                            connection_id = %connection_id.0,
                            "character sync token refresh failed; backing off"
                        );
                        if let Err(defer_error) = self
                            .repository
                            .defer_character_sources(connection_id, now + TOKEN_FAILURE_BACKOFF)
                            .await
                        {
                            tracing::warn!(
                                error = %defer_error,
                                connection_id = %connection_id.0,
                                "could not back off character sources"
                            );
                        }
                    }
                    return CharacterSourceKind::all()
                        .into_iter()
                        .map(|kind| (kind, CharacterSyncOutcome::Skipped))
                        .collect();
                }
            };

        if let Err(error) = self
            .repository
            .register_character_sources(connection_id)
            .await
        {
            tracing::warn!(
                %error,
                connection_id = %connection_id.0,
                "character source registration failed"
            );
        }

        let mut outcomes = Vec::with_capacity(CharacterSourceKind::all().len());
        let mut resolvable_ids = Vec::new();

        if cancel.is_cancelled() {
            return pad_skipped(outcomes);
        }
        let outcome = match self
            .claim_source(connection_id, CharacterSourceKind::CharacterInfo, now)
            .await
        {
            Some(claim) => {
                let result = self.fetch_character_info_summary(&connection).await;
                if let Ok(summary) = &result {
                    if let Some(id) = summary.get("corporation_id").and_then(Value::as_i64) {
                        resolvable_ids.push(id);
                    }
                }
                self.finish_source(
                    connection_id,
                    CharacterSourceKind::CharacterInfo,
                    claim,
                    now,
                    result,
                )
                .await
            }
            None => CharacterSyncOutcome::Skipped,
        };
        outcomes.push((CharacterSourceKind::CharacterInfo, outcome));

        if cancel.is_cancelled() {
            return pad_skipped(outcomes);
        }
        let outcome = match self
            .claim_source(connection_id, CharacterSourceKind::Location, now)
            .await
        {
            Some(claim) => {
                let result = if granted(&connection, LOCATION_SCOPE) {
                    self.fetch_location_summary(&access_token, &connection)
                        .await
                } else {
                    Err(SourceFailure::from(format!(
                        "missing scope: {LOCATION_SCOPE}"
                    )))
                };
                if let Ok(summary) = &result {
                    if let Some(id) = summary.get("solar_system_id").and_then(Value::as_i64) {
                        resolvable_ids.push(id);
                    }
                }
                self.finish_source(
                    connection_id,
                    CharacterSourceKind::Location,
                    claim,
                    now,
                    result,
                )
                .await
            }
            None => CharacterSyncOutcome::Skipped,
        };
        outcomes.push((CharacterSourceKind::Location, outcome));

        if cancel.is_cancelled() {
            return pad_skipped(outcomes);
        }
        let outcome = match self
            .claim_source(connection_id, CharacterSourceKind::Skills, now)
            .await
        {
            Some(claim) => {
                let result = if granted(&connection, SKILLS_SCOPE) {
                    self.fetch_skills_summary(&access_token, &connection).await
                } else {
                    Err(SourceFailure::from(format!(
                        "missing scope: {SKILLS_SCOPE}"
                    )))
                };
                self.finish_source(
                    connection_id,
                    CharacterSourceKind::Skills,
                    claim,
                    now,
                    result,
                )
                .await
            }
            None => CharacterSyncOutcome::Skipped,
        };
        outcomes.push((CharacterSourceKind::Skills, outcome));

        if cancel.is_cancelled() {
            return pad_skipped(outcomes);
        }
        let outcome = match self
            .claim_source(connection_id, CharacterSourceKind::Wallet, now)
            .await
        {
            Some(claim) => {
                let result = if granted(&connection, WALLET_SCOPE) {
                    self.fetch_wallet_summary(&access_token, &connection).await
                } else {
                    Err(SourceFailure::from(format!(
                        "missing scope: {WALLET_SCOPE}"
                    )))
                };
                self.finish_source(
                    connection_id,
                    CharacterSourceKind::Wallet,
                    claim,
                    now,
                    result,
                )
                .await
            }
            None => CharacterSyncOutcome::Skipped,
        };
        outcomes.push((CharacterSourceKind::Wallet, outcome));

        if cancel.is_cancelled() {
            return pad_skipped(outcomes);
        }
        let outcome = match self
            .claim_source(connection_id, CharacterSourceKind::IndustryJobs, now)
            .await
        {
            Some(claim) => {
                let result = if granted(&connection, INDUSTRY_JOBS_SCOPE) {
                    self.fetch_industry_jobs_summary(&access_token, connection_id, &connection, now)
                        .await
                } else {
                    Err(SourceFailure::from(format!(
                        "missing scope: {INDUSTRY_JOBS_SCOPE}"
                    )))
                };
                self.finish_source(
                    connection_id,
                    CharacterSourceKind::IndustryJobs,
                    claim,
                    now,
                    result,
                )
                .await
            }
            None => CharacterSyncOutcome::Skipped,
        };
        if outcome == CharacterSyncOutcome::Succeeded {
            if let Some(hook) = &self.jobs_synced {
                hook.industry_jobs_synced(connection.workspace_id).await;
            }
        }
        outcomes.push((CharacterSourceKind::IndustryJobs, outcome));

        if cancel.is_cancelled() {
            return pad_skipped(outcomes);
        }
        let outcome = match self
            .claim_source(connection_id, CharacterSourceKind::Assets, now)
            .await
        {
            Some(claim) => {
                let result = if granted(&connection, ASSET_SCOPE) {
                    self.fetch_assets_summary(&connection, &access_token).await
                } else {
                    Err(SourceFailure::from(format!("missing scope: {ASSET_SCOPE}")))
                };
                self.finish_source(
                    connection_id,
                    CharacterSourceKind::Assets,
                    claim,
                    now,
                    result,
                )
                .await
            }
            None => CharacterSyncOutcome::Skipped,
        };
        outcomes.push((CharacterSourceKind::Assets, outcome));

        if cancel.is_cancelled() {
            return pad_skipped(outcomes);
        }
        let outcome = match self
            .claim_source(connection_id, CharacterSourceKind::WalletTransactions, now)
            .await
        {
            Some(claim) => {
                let result = if granted(&connection, WALLET_SCOPE) {
                    self.fetch_wallet_transactions_summary(&connection, &access_token)
                        .await
                } else {
                    Err(SourceFailure::from(format!(
                        "missing scope: {WALLET_SCOPE}"
                    )))
                };
                self.finish_source(
                    connection_id,
                    CharacterSourceKind::WalletTransactions,
                    claim,
                    now,
                    result,
                )
                .await
            }
            None => CharacterSyncOutcome::Skipped,
        };
        outcomes.push((CharacterSourceKind::WalletTransactions, outcome));

        if cancel.is_cancelled() {
            return pad_skipped(outcomes);
        }
        let outcome = match self
            .claim_source(connection_id, CharacterSourceKind::Planets, now)
            .await
        {
            Some(claim) => {
                let result = if granted(&connection, PLANETS_SCOPE) {
                    self.fetch_planets_summary(&access_token, &connection).await
                } else {
                    Err(SourceFailure::from(format!(
                        "missing scope: {PLANETS_SCOPE}"
                    )))
                };
                self.finish_source(
                    connection_id,
                    CharacterSourceKind::Planets,
                    claim,
                    now,
                    result,
                )
                .await
            }
            None => CharacterSyncOutcome::Skipped,
        };
        outcomes.push((CharacterSourceKind::Planets, outcome));

        if cancel.is_cancelled() {
            return pad_skipped(outcomes);
        }
        let outcome = match self
            .claim_source(connection_id, CharacterSourceKind::Calendar, now)
            .await
        {
            Some(claim) => {
                let result = if granted(&connection, CALENDAR_SCOPE) {
                    self.sync_calendar_events(&access_token, &connection, now)
                        .await
                } else {
                    Err(SourceFailure::from(format!(
                        "missing scope: {CALENDAR_SCOPE}"
                    )))
                };
                self.finish_source(
                    connection_id,
                    CharacterSourceKind::Calendar,
                    claim,
                    now,
                    result,
                )
                .await
            }
            None => CharacterSyncOutcome::Skipped,
        };
        outcomes.push((CharacterSourceKind::Calendar, outcome));

        if cancel.is_cancelled() {
            return pad_skipped(outcomes);
        }
        let outcome = match self
            .claim_source(connection_id, CharacterSourceKind::MarketOrders, now)
            .await
        {
            Some(claim) => {
                let result = if granted(&connection, MARKET_ORDERS_SCOPE) {
                    self.sync_market_orders(&access_token, &connection, now)
                        .await
                } else {
                    Err(SourceFailure::from(format!(
                        "missing scope: {MARKET_ORDERS_SCOPE}"
                    )))
                };
                self.finish_source(
                    connection_id,
                    CharacterSourceKind::MarketOrders,
                    claim,
                    now,
                    result,
                )
                .await
            }
            None => CharacterSyncOutcome::Skipped,
        };
        outcomes.push((CharacterSourceKind::MarketOrders, outcome));

        if !cancel.is_cancelled() {
            resolvable_ids.extend(self.previously_synced_resolvable_ids(connection_id).await);
            if !resolvable_ids.is_empty() {
                self.resolve_and_cache_names(&resolvable_ids).await;
            }
        }

        outcomes
    }

    /// Corp/solar-system IDs from whatever CharacterInfo/Location already
    /// persisted on an earlier pass -- catches a connection whose id was
    /// fetched before this ID's name was cached and isn't due to refresh again for a while. `resolve_and_cache_names`
    /// already skips anything already cached, so re-collecting these every
    /// pass is cheap and self-correcting rather than a one-time backfill.
    async fn previously_synced_resolvable_ids(
        &self,
        connection_id: ConnectedCharacterId,
    ) -> Vec<i64> {
        let sources = match self.repository.character_source_state(connection_id).await {
            Ok(sources) => sources,
            Err(error) => {
                tracing::warn!(%error, connection_id = %connection_id.0, "failed to read persisted character sources for name resolution");
                return Vec::new();
            }
        };
        sources
            .iter()
            .filter_map(|source| {
                let summary = source.summary.as_ref()?;
                let key = match source.source_kind {
                    CharacterSourceKind::CharacterInfo => "corporation_id",
                    CharacterSourceKind::Location => "solar_system_id",
                    _ => return None,
                };
                summary.get(key).and_then(Value::as_i64)
            })
            .collect()
    }

    /// Best-effort: resolves whichever of `ids` aren't already cached in
    /// `eve_entity_names` via one bulk `universe_names` call, then caches
    /// the result. Never affects the sync outcome above -- a corp/system
    /// name that fails to resolve just means the roster shows the ID
    /// instead of a name a little longer, not a failed sync.
    async fn resolve_and_cache_names(&self, ids: &[i64]) {
        let mut seen = std::collections::HashSet::new();
        let unique_ids: Vec<i64> = ids.iter().copied().filter(|id| seen.insert(*id)).collect();
        let cached = match self.repository.entity_names(&unique_ids).await {
            Ok(cached) => cached,
            Err(error) => {
                tracing::warn!(%error, "entity name cache lookup failed");
                return;
            }
        };
        let missing: Vec<i64> = unique_ids
            .into_iter()
            .filter(|id| !cached.contains_key(id))
            .collect();
        if missing.is_empty() {
            return;
        }
        let names = match self.transport.universe_names(&missing).await {
            Ok(names) => names,
            Err(error) => {
                tracing::warn!(%error, "universe name resolution failed");
                return;
            }
        };
        if let Err(error) = self.repository.cache_entity_names(&names).await {
            tracing::warn!(%error, "entity name cache write failed");
        }
    }

    async fn claim_source(
        &self,
        connection_id: ConnectedCharacterId,
        kind: CharacterSourceKind,
        now: DateTime<Utc>,
    ) -> Option<SourceClaim> {
        let lease_expires_at = now + Duration::seconds(LEASE_SECONDS);
        self.repository
            .begin_character_source_refresh(connection_id, kind, now, lease_expires_at)
            .await
            .unwrap_or(None)
            .map(|lease| SourceClaim {
                lease,
                started: Instant::now(),
            })
    }

    async fn finish_source(
        &self,
        connection_id: ConnectedCharacterId,
        kind: CharacterSourceKind,
        claim: SourceClaim,
        now: DateTime<Utc>,
        result: Result<Value, SourceFailure>,
    ) -> CharacterSyncOutcome {
        let outcome = self
            .persist_source_result(connection_id, kind, claim.lease, now, result)
            .await;
        let result = if outcome == CharacterSyncOutcome::Succeeded {
            SyncResult::Success
        } else {
            SyncResult::Failed
        };
        record_sync_run(kind.as_db_str(), result, claim.started);
        outcome
    }

    async fn persist_source_result(
        &self,
        connection_id: ConnectedCharacterId,
        kind: CharacterSourceKind,
        claim: DateTime<Utc>,
        now: DateTime<Utc>,
        result: Result<Value, SourceFailure>,
    ) -> CharacterSyncOutcome {
        match result {
            Ok(summary) => {
                let next_refresh_at = if kind == CharacterSourceKind::Skills {
                    skills_next_refresh_at(&summary, now)
                } else {
                    now + refresh_interval(kind)
                };
                match self
                    .repository
                    .complete_character_source_refresh(
                        connection_id,
                        kind,
                        claim,
                        next_refresh_at,
                        summary,
                        now,
                    )
                    .await
                {
                    Ok(true) => CharacterSyncOutcome::Succeeded,
                    _ => CharacterSyncOutcome::Failed,
                }
            }
            Err(failure) => {
                let next_refresh_at = failure.retry_floor(now);
                if let Err(persistence_error) = self
                    .repository
                    .fail_character_source_refresh(
                        connection_id,
                        kind,
                        claim,
                        now,
                        next_refresh_at,
                        failure.message,
                    )
                    .await
                {
                    tracing::warn!(
                        %persistence_error,
                        connection_id = %connection_id.0,
                        source_kind = kind.as_db_str(),
                        "character source failure recording failed"
                    );
                }
                CharacterSyncOutcome::Failed
            }
        }
    }

    async fn fetch_character_info_summary(
        &self,
        connection: &ConnectedCharacter,
    ) -> Result<Value, SourceFailure> {
        let response = self
            .transport
            .character_public_info(connection.eve_character_id)
            .await
            .map_err(SourceFailure::from)?;
        let info = response
            .records
            .into_iter()
            .next()
            .ok_or_else(|| "ESI returned no character info record".to_string())?;
        serde_json::to_value(&info).map_err(SourceFailure::from)
    }

    async fn fetch_location_summary(
        &self,
        access_token: &str,
        connection: &ConnectedCharacter,
    ) -> Result<Value, SourceFailure> {
        let response = self
            .transport
            .character_location(access_token, connection.eve_character_id)
            .await
            .map_err(SourceFailure::from)?;
        let location = response
            .records
            .into_iter()
            .next()
            .ok_or_else(|| "ESI returned no location record".to_string())?;
        serde_json::to_value(&location).map_err(SourceFailure::from)
    }

    async fn fetch_skills_summary(
        &self,
        access_token: &str,
        connection: &ConnectedCharacter,
    ) -> Result<Value, SourceFailure> {
        let response = self
            .transport
            .character_skills(access_token, connection.eve_character_id)
            .await
            .map_err(SourceFailure::from)?;
        let skills = response
            .records
            .into_iter()
            .next()
            .ok_or_else(|| "ESI returned no skills record".to_string())?;
        let mut summary = serde_json::to_value(&skills).map_err(SourceFailure::from)?;
        // Same scope as the skills read. Training times degrade to estimates
        // without it, so a failure here leaves the key out rather than
        // failing the whole source.
        if let Ok(attributes) = self
            .transport
            .character_attributes(access_token, connection.eve_character_id)
            .await
        {
            if let (Value::Object(map), Some(Ok(attributes_value))) = (
                &mut summary,
                attributes.records.first().map(serde_json::to_value),
            ) {
                map.insert("attributes".to_string(), attributes_value);
            }
        }
        if granted(connection, SKILL_QUEUE_SCOPE) {
            if let Ok(queue) = self
                .transport
                .character_skill_queue(access_token, connection.eve_character_id)
                .await
            {
                if let (Value::Object(map), Ok(queue_value)) =
                    (&mut summary, serde_json::to_value(&queue.records))
                {
                    map.insert("skillQueue".to_string(), queue_value);
                }
            }
        }
        Ok(summary)
    }

    async fn fetch_wallet_summary(
        &self,
        access_token: &str,
        connection: &ConnectedCharacter,
    ) -> Result<Value, SourceFailure> {
        let response = self
            .transport
            .wallet_balance(access_token, connection.eve_character_id)
            .await
            .map_err(SourceFailure::from)?;
        let balance = response
            .records
            .into_iter()
            .next()
            .ok_or_else(|| "ESI returned no wallet balance record".to_string())?;
        serde_json::to_value(&balance).map_err(SourceFailure::from)
    }

    /// One list call plus one layout call per colony. Each planet's header
    /// fields are flattened next to its `pins`, so the stored summary is
    /// `{"planets": [{planet_id, planet_type, ..., last_update, pins: [...]}]}`
    /// -- the shape `iskworks_core::planetary::PlanetLayout` deserializes.
    /// Any single planet failing fails the whole source: a partial colony
    /// list would silently drop alerts.
    async fn fetch_planets_summary(
        &self,
        access_token: &str,
        connection: &ConnectedCharacter,
    ) -> Result<Value, SourceFailure> {
        let planets = self
            .transport
            .character_planets(access_token, connection.eve_character_id)
            .await
            .map_err(SourceFailure::from)?;
        let mut layouts = Vec::with_capacity(planets.records.len());
        for planet in planets.records {
            let detail = self
                .transport
                .character_planet_detail(
                    access_token,
                    connection.eve_character_id,
                    planet.planet_id,
                )
                .await
                .map_err(SourceFailure::from)?
                .records
                .into_iter()
                .next()
                .ok_or_else(|| format!("ESI returned no layout for planet {}", planet.planet_id))?;
            let mut layout = serde_json::to_value(&planet).map_err(SourceFailure::from)?;
            layout["pins"] = serde_json::to_value(&detail.pins).map_err(SourceFailure::from)?;
            layouts.push(layout);
        }
        Ok(serde_json::json!({ "planets": layouts }))
    }

    /// Replaces the connection's open orders with ESI's active list; orders
    /// it no longer lists are closed. A failed fetch or an unreadable order
    /// fails the source before anything is stored, so open orders are never
    /// closed by mistake. Corporation orders are stored but counted apart.
    async fn sync_market_orders(
        &self,
        access_token: &str,
        connection: &ConnectedCharacter,
        now: DateTime<Utc>,
    ) -> Result<Value, SourceFailure> {
        let orders = self
            .transport
            .character_orders(access_token, connection.eve_character_id)
            .await
            .map_err(SourceFailure::from)?
            .records
            .into_iter()
            .map(market_order)
            .collect::<Result<Vec<_>, _>>()?;
        self.repository
            .store_market_orders(connection.id, &orders, now)
            .await
            .map_err(|error| SourceFailure::from(error.to_string()))?;
        let personal = |side| {
            orders
                .iter()
                .filter(|order| !order.is_corporation && order.side == side)
                .count()
        };
        Ok(serde_json::json!({
            "openSellOrders": personal(MarketOrderSide::Sell),
            "openBuyOrders": personal(MarketOrderSide::Buy),
            "corporationOrders": orders.iter().filter(|order| order.is_corporation).count(),
        }))
    }

    /// Pages through the upcoming events (`from_event` = the last event
    /// seen), fetches details only for events with no current stored copy in
    /// the workspace, and replaces the connection's stored set -- events ESI
    /// no longer lists are dropped. Any failed call fails the source before
    /// anything is stored. The summary only records counts.
    async fn sync_calendar_events(
        &self,
        access_token: &str,
        connection: &ConnectedCharacter,
        now: DateTime<Utc>,
    ) -> Result<Value, SourceFailure> {
        let character_id = connection.eve_character_id;
        let mut summaries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut from_event = None;
        let mut pages = 0;
        while pages < MAX_CALENDAR_PAGES {
            let page = self
                .transport
                .character_calendar(access_token, character_id, from_event)
                .await
                .map_err(SourceFailure::from)?
                .records;
            pages += 1;
            let full_page = page.len() >= CALENDAR_PAGE_LIMIT;
            let mut advanced = false;
            for event in page {
                from_event = Some(event.event_id);
                if seen.insert(event.event_id) {
                    advanced = true;
                    summaries.push(event);
                }
            }
            if !full_page || !advanced {
                break;
            }
        }

        let event_ids: Vec<i64> = summaries.iter().map(|event| event.event_id).collect();
        let copies = self
            .repository
            .calendar_event_copies(connection.workspace_id, &event_ids)
            .await
            .map_err(|error| SourceFailure::from(error.to_string()))?;
        let mut events = Vec::with_capacity(summaries.len());
        let mut details_fetched = 0;
        for summary in summaries {
            let reusable = copies
                .iter()
                .filter(|copy| {
                    copy.event_id == summary.event_id
                        && copy.details_current(
                            summary.event_date,
                            &summary.title,
                            summary.importance,
                        )
                })
                // This connection's own copy first, then the newest.
                .max_by_key(|copy| (copy.connection_id == connection.id, copy.observed_at));
            let (duration_minutes, owner_id, owner_name, owner_type, text) = match reusable {
                Some(copy) => (
                    copy.duration_minutes,
                    copy.owner_id,
                    copy.owner_name.clone(),
                    copy.owner_type.clone(),
                    copy.text.clone(),
                ),
                None => {
                    let detail = self
                        .transport
                        .character_calendar_event(access_token, character_id, summary.event_id)
                        .await
                        .map_err(SourceFailure::from)?
                        .records
                        .into_iter()
                        .next()
                        .ok_or_else(|| {
                            format!("ESI returned no details for event {}", summary.event_id)
                        })?;
                    details_fetched += 1;
                    (
                        detail.duration_minutes.max(0),
                        detail.owner_id,
                        detail.owner_name,
                        detail.owner_type,
                        detail.text,
                    )
                }
            };
            events.push(CharacterCalendarEvent {
                connection_id: connection.id,
                event_id: summary.event_id,
                event_date: summary.event_date,
                title: summary.title,
                importance: summary.importance,
                response: calendar_response(summary.response),
                duration_minutes,
                owner_id,
                owner_name,
                owner_type,
                text,
                observed_at: now,
            });
        }
        self.repository
            .store_calendar_events(connection.id, &events)
            .await
            .map_err(|error| SourceFailure::from(error.to_string()))?;
        Ok(serde_json::json!({
            "eventCount": events.len(),
            "detailsFetched": details_fetched,
            "pages": pages,
        }))
    }

    /// Records every returned job (running and the last 90 days of finished
    /// ones) in the Jobs history, and summarises only the running ones:
    /// roster slots, calendar and ticket linking read the summary and must
    /// see what they did before completed jobs were requested.
    async fn fetch_industry_jobs_summary(
        &self,
        access_token: &str,
        connection_id: ConnectedCharacterId,
        connection: &ConnectedCharacter,
        observed_at: DateTime<Utc>,
    ) -> Result<Value, SourceFailure> {
        let response = self
            .transport
            .character_industry_jobs(access_token, connection.eve_character_id)
            .await
            .map_err(SourceFailure::from)?;
        self.repository
            .record_industry_jobs(connection_id, &response.records, observed_at)
            .await
            .map_err(|error| SourceFailure::from(error.to_string()))?;
        let running: Vec<&CharacterIndustryJobObservation> = response
            .records
            .iter()
            .filter(|job| is_running_job_status(&job.status))
            .collect();
        let active_count = running.iter().filter(|job| job.status == "active").count();
        let jobs = serde_json::to_value(&running).map_err(SourceFailure::from)?;
        Ok(serde_json::json!({ "jobs": jobs, "activeCount": active_count }))
    }

    /// Delegates to `esi_sync` (backed by `EsiApplicationService::sync_assets`
    /// in production) rather than calling `self.transport` directly like the
    /// sources above -- assets need the full paginate/ETag-checkpoint/
    /// persist-to-`esi_asset_observations` pipeline that method already
    /// implements and tests, not a single-response fetch. The
    /// `character_source_sync_state` row this populates is a health/staleness
    /// signal for the roster only; the actual holdings data lives in
    /// `esi_asset_snapshots`/`esi_asset_observations`, written by that
    /// pipeline directly.
    async fn fetch_assets_summary(
        &self,
        connection: &ConnectedCharacter,
        token: &str,
    ) -> Result<Value, SourceFailure> {
        let run = self
            .esi_sync
            .sync_assets(connection, token)
            .await
            .map_err(SourceFailure::from)?;
        serde_json::to_value(&run).map_err(SourceFailure::from)
    }

    /// See `fetch_assets_summary` -- same reasoning, delegates to
    /// `EsiApplicationService::sync_wallet`'s existing paginated,
    /// ETag-checkpointed wallet-transactions fetch instead of duplicating it.
    async fn fetch_wallet_transactions_summary(
        &self,
        connection: &ConnectedCharacter,
        token: &str,
    ) -> Result<Value, SourceFailure> {
        let run = self
            .esi_sync
            .sync_wallet_transactions(connection, token)
            .await
            .map_err(SourceFailure::from)?;
        serde_json::to_value(&run).map_err(SourceFailure::from)
    }
}

#[cfg(test)]
mod tests;
