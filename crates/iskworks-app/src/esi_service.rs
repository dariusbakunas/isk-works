//! ESI application-service layer: `EsiApplicationService` (SSO exchange,
//! token refresh, structure/market-access resolution, automatic EIV,
//! system cost indices, asset/wallet sync), the `EsiSyncDispatcher` and
//! `MarketAccessResolver` ports it implements, `EsiApplicationError`, and
//! the response DTOs the API layer serializes.

use std::collections::{BTreeSet, HashMap};
use std::env;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};

use crate::manual_sync_gate::ManualSyncGate;
use iskworks_core::{
    calculate_adjusted_price_eiv, AdjustedPriceEiv, AdjustedPriceRepository, CapturedRecipeLine,
    ConnectedCharacter, ConnectedCharacterId, ConnectionStatus, EsiSyncKind, EsiSyncRun,
    FacilityError, InventoryError, OwnerId, WorkspaceId,
};
use iskworks_esi::{
    authorization_url, hash_state, new_pkce, AssetObservation, AuthenticatedToken,
    BlueprintAssetObservation, CharacterIndustryJobObservation, CharacterLocationObservation,
    CharacterPlanetDetailObservation, CharacterPlanetObservation, CharacterPublicInfo,
    CharacterSkillEntry, CharacterSkillQueueEntry, CharacterSkillsObservation, EncryptedSecret,
    EsiError, EsiResponse, EsiResponseMetadata, EsiTransport, HttpEsiTransport, Identity,
    MarketOrderObservation, OAuthState, PkceVerifier, PlanetExtractorObservation,
    PlanetPinContentObservation, PlanetPinObservation, RefreshedToken, SecretCipher,
    StructureInformation, WalletTransactionObservation, ASSET_SCOPE, BLUEPRINT_SCOPE,
    INDUSTRY_JOBS_SCOPE, LOCATION_SCOPE, MARKET_STRUCTURE_SCOPE, PLANETS_SCOPE, SKILLS_SCOPE,
    SKILL_QUEUE_SCOPE, STRUCTURE_SCOPE, WALLET_SCOPE,
};
use iskworks_storage::{PendingAuthorization, PgEsiRepository, StoredRefreshToken};
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::json;
use thiserror::Error;
use tokio::sync::RwLock;

mod authorization;
mod fixture;
mod pricing;
mod structures;
mod sync;
#[cfg(any(test, feature = "test-support"))]
mod test_support;
use fixture::*;
use structures::*;
#[cfg(any(test, feature = "test-support"))]
pub use test_support::*;

const DEFAULT_AUTH_URL: &str = "https://login.eveonline.com/v2/oauth/authorize";
const DEFAULT_TOKEN_URL: &str = "https://login.eveonline.com/v2/oauth/token";
const DEFAULT_JWKS_URL: &str = "https://login.eveonline.com/oauth/jwks";
const DEFAULT_ESI_URL: &str = "https://esi.evetech.net";
const DEFAULT_ISSUER: &str = "https://login.eveonline.com";
const SYNC_SCOPES: [&str; 3] = [ASSET_SCOPE, BLUEPRINT_SCOPE, WALLET_SCOPE];
const REQUESTED_SCOPES: [&str; 9] = [
    ASSET_SCOPE,
    BLUEPRINT_SCOPE,
    WALLET_SCOPE,
    STRUCTURE_SCOPE,
    LOCATION_SCOPE,
    SKILLS_SCOPE,
    SKILL_QUEUE_SCOPE,
    INDUSTRY_JOBS_SCOPE,
    MARKET_STRUCTURE_SCOPE,
];
/// Requested on every connect but not required by `finish_authorization`:
/// a character that declines one of these still connects, and only the
/// feature that needs it (e.g. Planetary Interaction) reports the missing
/// scope.
const OPTIONAL_SCOPES: [&str; 1] = [PLANETS_SCOPE];
const WALLET_PAGE_LIMIT: usize = 2_500;
const MAX_WALLET_REQUESTS: usize = 20;
/// ESI serves the journal 50 entries to a page over roughly 30 days; this is a
/// runaway guard, not an expected depth.
const MAX_JOURNAL_PAGES: u32 = 100;

#[derive(Clone)]
pub struct EsiApplicationService {
    repository: Arc<PgEsiRepository>,
    transport: Arc<dyn EsiTransport>,
    cipher: SecretCipher,
    client_id: String,
    redirect_uri: String,
    authorization_url: String,
    web_app_url: String,
    fixture_mode: bool,
    industry_index_cache: Arc<RwLock<Option<IndustryIndexCache>>>,
    adjusted_price_cache: Arc<RwLock<Option<AdjustedPriceCache>>>,
    manual_sync_gate: Arc<ManualSyncGate>,
}

#[derive(Debug, Error)]
pub enum EsiApplicationError {
    #[error("{0}")]
    Protocol(#[from] EsiError),
    #[error("{0}")]
    Persistence(#[from] InventoryError),
    #[error("{0}")]
    Facility(#[from] FacilityError),
    #[error("{0}")]
    Configuration(String),
    /// A manual sync of the same data for this character started less than
    /// a minute ago (see `manual_sync_gate`).
    #[error("This character was synced moments ago. Try again in {retry_after_seconds}s.")]
    SyncTooSoon { retry_after_seconds: u64 },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizationStart {
    pub authorization_url: String,
    pub fixture_mode: bool,
    pub connection: Option<ConnectedCharacter>,
    pub requested_scopes: Vec<String>,
    /// The flow's `state`, for the route to bind to the browser via a
    /// cookie. Never serialized; `None` in fixture mode (no redirect).
    #[serde(skip)]
    pub state: Option<OAuthState>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedStructure {
    pub structure: StructureInformation,
    pub connection_id: ConnectedCharacterId,
    pub character_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureResolution {
    pub resolved: Vec<ResolvedStructure>,
    pub unresolved_structure_ids: Vec<i64>,
    pub eligible_character_count: u64,
    pub needs_reconnection: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemCostIndex {
    pub solar_system_id: i64,
    pub manufacturing: String,
    pub reaction: String,
    pub fetched_at: chrono::DateTime<Utc>,
    pub expires_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct IndustryIndexCache {
    values: HashMap<i64, iskworks_esi::IndustrySystemCostIndex>,
    fetched_at: chrono::DateTime<Utc>,
    expires_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct AdjustedPriceCache {
    values: std::collections::BTreeMap<i64, Decimal>,
    fetched_at: chrono::DateTime<Utc>,
    expires_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomaticEiv {
    pub value: Option<iskworks_core::Money>,
    pub missing_type_ids: Vec<i64>,
    pub observed_at: chrono::DateTime<Utc>,
    pub expires_at: chrono::DateTime<Utc>,
}

/// The result of one **bulk** adjusted-price resolution: the raw
/// `type_id -> adjusted_price` map for the requested types that resolved, the
/// ones that did not, and the wall clock the resolution was pinned to. Used by
/// the allocation-aware cost projection (`build_cost`) to compute one EIV per
/// operation without a per-operation repository read.
#[derive(Debug, Clone)]
pub struct AdjustedPriceSet {
    pub values: std::collections::BTreeMap<i64, Decimal>,
    pub missing_type_ids: Vec<i64>,
    pub observed_at: chrono::DateTime<Utc>,
}

/// Lets `CharacterSyncService` dispatch a periodic Assets/WalletTransactions
/// sync through the exact same fetch-with-ETag/paginate/persist logic the
/// manual `POST /sync/assets` and `/sync/wallet` routes use (`sync_assets`/
/// `sync_wallet` below) -- reusing it instead of re-deriving it. Narrower
/// than the public `EsiApplicationService::sync`: it takes an
/// already-obtained access token rather than refreshing one itself, since
/// the caller already holds a fresh token from its own `AccessTokenProvider`
/// call moments earlier -- refreshing twice in one pass would mean two EVE
/// SSO refresh-token round trips per character instead of one.
/// Result of trying to confirm a connected character's docking access and
/// `esi-markets.structure_markets.v1` scope for a structure -- the
/// `Confirmed` variant carries the real, already-fetched first page of
/// market orders so a caller that needs a full refresh never has to re-fetch page 1
/// just to redo what access verification already proved.
#[derive(Debug, Clone)]
pub enum MarketAccessResolution {
    Confirmed {
        connection_id: ConnectedCharacterId,
        character_name: String,
        access_token: String,
        first_page: EsiResponse<MarketOrderObservation>,
    },
    /// No connected character has the `esi-markets.structure_markets.v1`
    /// scope granted at all -- distinct from `AllDenied`, since nobody was
    /// even tried.
    NoEligibleCharacter,
    /// At least one connection (the preferred one, an eligible one, or
    /// both) was tried and every attempt was denied.
    AllDenied,
}

/// Mirrors `resolve_structures`'s "try each eligible character, 403 means
/// try the next one" pattern for structure *market* access -- a genuinely
/// separate ESI scope from `STRUCTURE_SCOPE` (structure names), so a
/// character with one doesn't necessarily have the other. A small trait
/// (same family as `AccessTokenProvider`/`EsiSyncDispatcher`) so callers
/// that only need access resolution (the worker, the manual verify-access
/// route) don't need to depend on the whole `EsiApplicationService`.
#[async_trait]
pub trait MarketAccessResolver: Send + Sync {
    async fn resolve_market_access(
        &self,
        workspace_id: WorkspaceId,
        structure_id: i64,
        solar_system_id: i64,
        preferred_connection_id: Option<ConnectedCharacterId>,
    ) -> Result<MarketAccessResolution, EsiApplicationError>;
}

#[async_trait]
impl MarketAccessResolver for EsiApplicationService {
    async fn resolve_market_access(
        &self,
        workspace_id: WorkspaceId,
        structure_id: i64,
        solar_system_id: i64,
        preferred_connection_id: Option<ConnectedCharacterId>,
    ) -> Result<MarketAccessResolution, EsiApplicationError> {
        let mut attempted_any = false;
        if let Some(preferred_id) = preferred_connection_id {
            if let Ok((connection, access_token)) = self.refresh(preferred_id).await {
                attempted_any = true;
                if let Ok(first_page) = self
                    .transport
                    .structure_market_orders(&access_token, structure_id, solar_system_id, 1, None)
                    .await
                {
                    return Ok(MarketAccessResolution::Confirmed {
                        connection_id: connection.id,
                        character_name: connection.character_name,
                        access_token,
                        first_page,
                    });
                }
                // Denied, or any other failure (rate limit, invalid
                // response, ...) -- either way don't get stuck retrying a
                // connection that just failed; fall through to the
                // eligible-character list below.
            }
        }

        let connections = self.repository.list_connections(workspace_id).await?;
        let eligible = connections
            .iter()
            .filter(|connection| {
                connection.status != ConnectionStatus::Disconnected
                    && Some(connection.id) != preferred_connection_id
                    && connection
                        .granted_scopes
                        .iter()
                        .any(|scope| scope == MARKET_STRUCTURE_SCOPE)
            })
            .cloned()
            .collect::<Vec<_>>();

        for candidate in &eligible {
            let (connection, access_token) = match self.refresh(candidate.id).await {
                Ok(value) => value,
                Err(_) => continue,
            };
            attempted_any = true;
            if let Ok(first_page) = self
                .transport
                .structure_market_orders(&access_token, structure_id, solar_system_id, 1, None)
                .await
            {
                return Ok(MarketAccessResolution::Confirmed {
                    connection_id: connection.id,
                    character_name: connection.character_name,
                    access_token,
                    first_page,
                });
            }
        }

        Ok(if attempted_any {
            MarketAccessResolution::AllDenied
        } else {
            MarketAccessResolution::NoEligibleCharacter
        })
    }
}

#[async_trait]
pub trait EsiSyncDispatcher: Send + Sync {
    async fn sync_assets(
        &self,
        connection: &ConnectedCharacter,
        token: &str,
    ) -> Result<EsiSyncRun, EsiApplicationError>;
    async fn sync_wallet_transactions(
        &self,
        connection: &ConnectedCharacter,
        token: &str,
    ) -> Result<EsiSyncRun, EsiApplicationError>;
}

#[async_trait]
impl EsiSyncDispatcher for EsiApplicationService {
    async fn sync_assets(
        &self,
        connection: &ConnectedCharacter,
        token: &str,
    ) -> Result<EsiSyncRun, EsiApplicationError> {
        EsiApplicationService::sync_assets(self, connection, token).await
    }

    async fn sync_wallet_transactions(
        &self,
        connection: &ConnectedCharacter,
        token: &str,
    ) -> Result<EsiSyncRun, EsiApplicationError> {
        EsiApplicationService::sync_wallet(self, connection, token).await
    }
}

impl EsiApplicationService {
    pub fn from_env(repository: Arc<PgEsiRepository>) -> Result<Option<Self>, EsiApplicationError> {
        let fixture_mode = env::var("ISKWORKS_ESI_MOCK").as_deref() == Ok("1");
        let client_id = env::var("EVE_SSO_CLIENT_ID")
            .ok()
            .filter(|value| !value.trim().is_empty());
        if !fixture_mode && client_id.is_none() {
            return Ok(None);
        }
        let redirect_uri = env_or_default(
            "EVE_SSO_REDIRECT_URI",
            "http://127.0.0.1:8080/api/eve/oauth/callback",
        );
        let web_app_url = env_or_default("WEB_APP_URL", "http://127.0.0.1:5173");
        let key = if fixture_mode {
            env::var("TOKEN_ENCRYPTION_KEY")
                .unwrap_or_else(|_| "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=".to_string())
        } else {
            env::var("TOKEN_ENCRYPTION_KEY").map_err(|_| {
                EsiApplicationError::Configuration(
                    "TOKEN_ENCRYPTION_KEY is required when ESI is enabled.".to_string(),
                )
            })?
        };
        let cipher = SecretCipher::from_base64(&key)?;
        let client_id = client_id.unwrap_or_else(|| "fixture-client".to_string());
        let transport: Arc<dyn EsiTransport> = if fixture_mode {
            Arc::new(FixtureEsiTransport)
        } else {
            Arc::new(HttpEsiTransport::new(
                client_id.clone(),
                redirect_uri.clone(),
                env_or_default("EVE_SSO_TOKEN_URL", DEFAULT_TOKEN_URL),
                env_or_default("EVE_SSO_JWKS_URL", DEFAULT_JWKS_URL),
                env_or_default("EVE_ESI_BASE_URL", DEFAULT_ESI_URL),
                env_or_default("EVE_SSO_ISSUER", DEFAULT_ISSUER),
            ))
        };
        Ok(Some(Self {
            repository,
            transport,
            cipher,
            client_id,
            redirect_uri,
            authorization_url: env_or_default("EVE_SSO_AUTHORIZATION_URL", DEFAULT_AUTH_URL),
            web_app_url,
            fixture_mode,
            industry_index_cache: Arc::new(RwLock::new(None)),
            adjusted_price_cache: Arc::new(RwLock::new(None)),
            manual_sync_gate: Arc::new(ManualSyncGate::default()),
        }))
    }

    #[must_use]
    pub fn fixture_mode(&self) -> bool {
        self.fixture_mode
    }

    #[must_use]
    pub fn web_app_url(&self) -> &str {
        &self.web_app_url
    }

    /// Shares this service's already-configured transport (fixture or real
    /// HTTP) with `CharacterSyncService` -- constructed in `apps/iskworks-api`
    /// (`AppState::with_esi`) and in `apps/iskworks-worker` (`main.rs`) -- so
    /// neither needs to rebuild the fixture-mode/HTTP branching in `from_env`
    /// a second time.
    #[must_use]
    pub fn transport(&self) -> Arc<dyn EsiTransport> {
        Arc::clone(&self.transport)
    }
}

fn env_or_default(name: &str, default: &str) -> String {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

/// The connection status a failed token refresh calls for, when the failure
/// is one only the user can fix by reconnecting: EVE rejected the refresh
/// token (revoked app access, expired, character transferred), or it no
/// longer carries the scopes sync needs. Everything else -- outages, rate
/// limits, our own misconfiguration -- is `None`: the connection is fine and
/// a later retry may succeed.
#[must_use]
pub fn reconnect_status(error: &EsiApplicationError) -> Option<ConnectionStatus> {
    match error {
        EsiApplicationError::Protocol(EsiError::AuthorizationRequired) => {
            Some(ConnectionStatus::NeedsReconnection)
        }
        EsiApplicationError::Protocol(EsiError::MissingScope) => {
            Some(ConnectionStatus::MissingScope)
        }
        _ => None,
    }
}

fn error_code(error: &EsiError) -> &'static str {
    match error {
        EsiError::AuthorizationRequired => "authorization_required",
        EsiError::MissingScope => "missing_scope",
        EsiError::AccessDenied => "access_denied",
        EsiError::RateLimited { .. } => "rate_limited",
        EsiError::EsiErrorLimit { .. } => "esi_error_limit",
        EsiError::ServerDowntime { .. } => "eve_downtime",
        EsiError::TemporaryFailure => "temporary_esi_failure",
        EsiError::InvalidResponse => "invalid_response",
        EsiError::InvalidIdentity => "invalid_identity",
        _ => "esi_failure",
    }
}

#[cfg(test)]
mod tests;
