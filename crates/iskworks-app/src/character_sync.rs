use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use iskworks_core::{
    training::{derive_training_state, SkillQueueEntry, TrainingState},
    CharacterSourceKind, CharacterSourceSyncState, ConnectedCharacter, ConnectedCharacterId,
    InventoryError,
};
use iskworks_esi::{
    EsiError, EsiTransport, EveEntityName, ASSET_SCOPE, INDUSTRY_JOBS_SCOPE, LOCATION_SCOPE,
    PLANETS_SCOPE, SKILLS_SCOPE, SKILL_QUEUE_SCOPE, WALLET_SCOPE,
};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::esi_service::{reconnect_status, EsiApplicationError, EsiSyncDispatcher};

const LEASE_SECONDS: i64 = 120;
/// How long a connection backs off after a transient token-refresh failure.
const TOKEN_FAILURE_BACKOFF: Duration = Duration::minutes(15);

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CharacterSyncOutcome {
    Succeeded,
    Failed,
    Skipped,
}

/// Persistence for `character_source_sync_state`, kept narrow (register,
/// claim, complete, fail) and separate from `EsiTransport`/token concerns so
/// the sync orchestration below can be driven by fakes in tests, matching
/// `market_esi.rs`'s `MarketRefreshRepository` shape.
#[async_trait]
pub trait CharacterSyncRepository: Send + Sync {
    async fn register_character_sources(
        &self,
        connection_id: ConnectedCharacterId,
    ) -> Result<(), InventoryError>;
    async fn begin_character_source_refresh(
        &self,
        connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, InventoryError>;
    async fn complete_character_source_refresh(
        &self,
        connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        summary: Value,
        observed_at: DateTime<Utc>,
    ) -> Result<bool, InventoryError>;
    async fn fail_character_source_refresh(
        &self,
        connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        claim: DateTime<Utc>,
        attempted_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        error_message: String,
    ) -> Result<bool, InventoryError>;
    /// Pushes every due source of this connection out to `until` -- for a
    /// failure that blocks all of them at once (the token refresh).
    async fn defer_character_sources(
        &self,
        connection_id: ConnectedCharacterId,
        until: DateTime<Utc>,
    ) -> Result<(), InventoryError>;
    /// Whatever names are already cached in `eve_entity_names` for these
    /// IDs -- unresolved IDs are simply absent, never fetched here.
    async fn entity_names(&self, ids: &[i64]) -> Result<HashMap<i64, String>, InventoryError>;
    async fn cache_entity_names(&self, names: &[EveEntityName]) -> Result<(), InventoryError>;
    /// Every source's current persisted state for this connection --
    /// needed so name resolution can also pick up a corporation/solar-system
    /// id that was fetched on some earlier pass (before this ID's name was
    /// ever cached), not just one freshly fetched in *this* pass. Without
    /// this, a connection whose CharacterInfo/Location is still fresh but
    /// whose names were never cached would wait for its next natural
    /// refresh (up to 24h for CharacterInfo) before ever resolving.
    async fn character_source_state(
        &self,
        connection_id: ConnectedCharacterId,
    ) -> Result<Vec<CharacterSourceSyncState>, InventoryError>;
}

#[async_trait]
impl CharacterSyncRepository for iskworks_storage::PgEsiRepository {
    async fn register_character_sources(
        &self,
        connection_id: ConnectedCharacterId,
    ) -> Result<(), InventoryError> {
        self.register_character_sources(connection_id).await
    }

    async fn begin_character_source_refresh(
        &self,
        connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, InventoryError> {
        self.begin_character_source_refresh(
            connection_id,
            source_kind,
            attempted_at,
            lease_expires_at,
        )
        .await
    }

    async fn complete_character_source_refresh(
        &self,
        connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        summary: Value,
        observed_at: DateTime<Utc>,
    ) -> Result<bool, InventoryError> {
        self.complete_character_source_refresh(
            connection_id,
            source_kind,
            claim,
            next_refresh_at,
            summary,
            observed_at,
        )
        .await
    }

    async fn fail_character_source_refresh(
        &self,
        connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        claim: DateTime<Utc>,
        attempted_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        error_message: String,
    ) -> Result<bool, InventoryError> {
        self.fail_character_source_refresh(
            connection_id,
            source_kind,
            claim,
            attempted_at,
            next_refresh_at,
            error_message,
        )
        .await
    }

    async fn defer_character_sources(
        &self,
        connection_id: ConnectedCharacterId,
        until: DateTime<Utc>,
    ) -> Result<(), InventoryError> {
        self.defer_character_sources(connection_id, until).await
    }

    async fn entity_names(&self, ids: &[i64]) -> Result<HashMap<i64, String>, InventoryError> {
        self.entity_names(ids).await
    }

    async fn cache_entity_names(&self, names: &[EveEntityName]) -> Result<(), InventoryError> {
        self.cache_entity_names(names).await
    }

    async fn character_source_state(
        &self,
        connection_id: ConnectedCharacterId,
    ) -> Result<Vec<CharacterSourceSyncState>, InventoryError> {
        self.character_source_state(connection_id).await
    }
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

fn refresh_interval(kind: CharacterSourceKind) -> Duration {
    match kind {
        CharacterSourceKind::CharacterInfo => Duration::hours(24),
        CharacterSourceKind::Location => Duration::minutes(10),
        CharacterSourceKind::Skills => Duration::minutes(60),
        CharacterSourceKind::Wallet => Duration::minutes(15),
        CharacterSourceKind::IndustryJobs => Duration::minutes(30),
        CharacterSourceKind::Assets => Duration::minutes(60),
        CharacterSourceKind::WalletTransactions => Duration::minutes(30),
        CharacterSourceKind::Planets => Duration::minutes(30),
    }
}

/// The first retry after a failed source refresh. The storage layer doubles
/// it for each consecutive failure (capped at 6 hours), so a source that
/// keeps failing -- a 403 the character can't fix, an endpoint that's down
/// -- stops spending ESI's error budget every five minutes.
fn retry_interval() -> Duration {
    Duration::minutes(5)
}

/// Why a source refresh failed, and how long ESI asked us to wait before
/// trying again (`Retry-After` on a 429, or the error-limit reset).
#[derive(Debug)]
pub(crate) struct SourceFailure {
    message: String,
    retry_after: Option<Duration>,
}

impl SourceFailure {
    /// The earliest retry: the first backoff step, or ESI's own wait when
    /// that is longer.
    fn retry_floor(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        now + self.retry_after.unwrap_or_default().max(retry_interval())
    }
}

impl From<EsiError> for SourceFailure {
    fn from(error: EsiError) -> Self {
        let retry_after = match &error {
            EsiError::RateLimited {
                retry_after_seconds: Some(seconds),
            }
            | EsiError::EsiErrorLimit {
                reset_seconds: Some(seconds),
            }
            | EsiError::ServerDowntime {
                retry_after_seconds: Some(seconds),
            } => i64::try_from(*seconds).ok().map(Duration::seconds),
            _ => None,
        };
        Self {
            message: error.to_string(),
            retry_after,
        }
    }
}

impl From<EsiApplicationError> for SourceFailure {
    fn from(error: EsiApplicationError) -> Self {
        match error {
            EsiApplicationError::Protocol(error) => error.into(),
            other => other.to_string().into(),
        }
    }
}

impl From<serde_json::Error> for SourceFailure {
    fn from(error: serde_json::Error) -> Self {
        error.to_string().into()
    }
}

impl From<String> for SourceFailure {
    fn from(message: String) -> Self {
        Self {
            message,
            retry_after: None,
        }
    }
}

/// Schedule the next `Skills` source refresh to land near the currently
/// training entry's known `finish_date` (capped by the flat interval as a
/// ceiling) instead of always waiting out the full flat interval -- reuses
/// the existing 30-second `due_character_sources` poll, no new scheduler.
///
/// A `CachedQueueExpired` result means we already know this very summary
/// is stale (every cached entry's `finish_date` is `<= now`) -- that must
/// not fall back to the flat ceiling, or reconciliation for data we
/// already know is stale would wait out a full cadence unnecessarily.
fn skills_next_refresh_at(summary: &Value, now: DateTime<Utc>) -> DateTime<Utc> {
    let ceiling = now + refresh_interval(CharacterSourceKind::Skills);
    let entries = parse_skill_queue_entries(summary);
    match derive_training_state(&entries, now) {
        TrainingState::Active { entry } => {
            let finish = entry
                .finish_date
                .expect("derive_training_state only returns Active for fully-dated entries");
            (finish + Duration::seconds(60)).min(ceiling)
        }
        TrainingState::CachedQueueExpired { .. } => now,
        TrainingState::Paused { .. } | TrainingState::Empty => ceiling,
    }
}

fn parse_skill_queue_entries(summary: &Value) -> Vec<SkillQueueEntry> {
    let Some(queue) = summary.get("skillQueue").and_then(Value::as_array) else {
        return Vec::new();
    };
    queue
        .iter()
        .filter_map(|entry| {
            Some(SkillQueueEntry {
                skill_id: entry.get("skill_id").and_then(Value::as_i64)?,
                finished_level: entry.get("finished_level").and_then(Value::as_i64)?,
                queue_position: entry.get("queue_position").and_then(Value::as_i64)?,
                start_date: entry
                    .get("start_date")
                    .and_then(Value::as_str)
                    .and_then(|text| text.parse::<DateTime<Utc>>().ok()),
                finish_date: entry
                    .get("finish_date")
                    .and_then(Value::as_str)
                    .and_then(|text| text.parse::<DateTime<Utc>>().ok()),
                training_start_sp: entry.get("training_start_sp").and_then(Value::as_i64),
                level_end_sp: entry.get("level_end_sp").and_then(Value::as_i64),
            })
        })
        .collect()
}

#[derive(Clone)]
pub struct CharacterSyncService {
    repository: Arc<dyn CharacterSyncRepository>,
    transport: Arc<dyn EsiTransport>,
    token_provider: Arc<dyn AccessTokenProvider>,
    esi_sync: Arc<dyn EsiSyncDispatcher>,
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
        }
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

        let mut outcomes = Vec::with_capacity(8);
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
                    self.fetch_industry_jobs_summary(&access_token, &connection)
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
    ) -> Option<DateTime<Utc>> {
        let lease_expires_at = now + Duration::seconds(LEASE_SECONDS);
        self.repository
            .begin_character_source_refresh(connection_id, kind, now, lease_expires_at)
            .await
            .unwrap_or(None)
    }

    async fn finish_source(
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

    async fn fetch_industry_jobs_summary(
        &self,
        access_token: &str,
        connection: &ConnectedCharacter,
    ) -> Result<Value, SourceFailure> {
        let response = self
            .transport
            .character_industry_jobs(access_token, connection.eve_character_id)
            .await
            .map_err(SourceFailure::from)?;
        let active_count = response
            .records
            .iter()
            .filter(|job| job.status == "active")
            .count();
        let jobs = serde_json::to_value(&response.records).map_err(SourceFailure::from)?;
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
