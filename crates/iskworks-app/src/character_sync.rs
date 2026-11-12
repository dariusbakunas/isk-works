use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use iskworks_core::{
    CharacterSourceKind, CharacterSyncRepository, ConnectedCharacter, ConnectedCharacterId,
};
use iskworks_esi::{
    EsiTransport, ASSET_SCOPE, CALENDAR_SCOPE, INDUSTRY_JOBS_SCOPE, LOCATION_SCOPE,
    MARKET_ORDERS_SCOPE, PLANETS_SCOPE, SKILLS_SCOPE, WALLET_SCOPE,
};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::esi_service::{reconnect_status, EsiApplicationError, EsiSyncDispatcher};
use crate::sync_metrics::{record_sync_run, SyncResult};

mod policy;
mod sources;

pub(crate) use policy::SourceFailure;
use policy::{refresh_interval, skills_next_refresh_at};

const LEASE_SECONDS: i64 = 120;
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
}

#[cfg(test)]
mod tests;
