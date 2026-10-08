use super::*;

impl EsiApplicationService {
    /// Test constructor -- wires an `EsiApplicationService` from explicit
    /// collaborators with canned config, bypassing `from_env`. Behind the
    /// `test-support` feature because `iskworks-api`'s route tests use it too.
    #[doc(hidden)]
    pub fn new_for_tests(
        repository: Arc<PgEsiRepository>,
        transport: Arc<dyn EsiTransport>,
    ) -> Self {
        Self {
            repository,
            transport,
            cipher: SecretCipher::for_tests(),
            client_id: "test-client".to_string(),
            redirect_uri: "http://127.0.0.1:8080/api/eve/oauth/callback".to_string(),
            authorization_url: DEFAULT_AUTH_URL.to_string(),
            web_app_url: "http://127.0.0.1:5173".to_string(),
            fixture_mode: false,
            industry_index_cache: Arc::new(RwLock::new(None)),
            adjusted_price_cache: Arc::new(RwLock::new(None)),
            manual_sync_gate: Arc::new(ManualSyncGate::default()),
        }
    }
}

/// A working `exchange_code` granting every scope the character-link flow
/// requires, so `finish_authorization`'s `require_requested_scopes` check
/// passes. Shared with `routes::esi::tests`, which needs a real
/// character-link completion to prove the unified callback correctly falls
/// through past the login flow for a state that isn't one. Behind the
/// `test-support` feature because that consumer is a separate crate.
#[doc(hidden)]
pub struct FakeLinkTransport;

#[async_trait::async_trait]
impl EsiTransport for FakeLinkTransport {
    async fn exchange_code(
        &self,
        _code: &str,
        _verifier: &PkceVerifier,
    ) -> Result<AuthenticatedToken, EsiError> {
        Ok(AuthenticatedToken {
            access_token: "fake-access".to_string(),
            refresh_token: "fake-refresh".to_string(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            owner_hash: None,
            identity: Identity {
                character_id: 99,
                character_name: "Linked Character".to_string(),
                scopes: REQUESTED_SCOPES.iter().map(ToString::to_string).collect(),
            },
        })
    }

    async fn refresh(&self, _refresh_token: &str) -> Result<RefreshedToken, EsiError> {
        unimplemented!("test never refreshes a token")
    }

    async fn assets(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<AssetObservation>, EsiError> {
        unimplemented!("test never reads ESI resources")
    }

    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        _from_id: Option<i64>,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<WalletTransactionObservation>, EsiError> {
        unimplemented!("test never reads ESI resources")
    }

    async fn structure(
        &self,
        _access_token: &str,
        _structure_id: i64,
    ) -> Result<StructureInformation, EsiError> {
        unimplemented!("test never reads ESI resources")
    }

    async fn industry_systems(
        &self,
    ) -> Result<EsiResponse<iskworks_esi::IndustrySystemCostIndex>, EsiError> {
        unimplemented!("test never reads ESI resources")
    }
}
