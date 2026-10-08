use std::env;
use std::sync::Arc;

use iskworks_core::{
    hash_invite_code_from_raw, AuthError, AuthenticatedUser, EveIdentity, InviteGrant,
    SessionService, SessionToken, UserRepository,
};
use iskworks_esi::{
    authorization_url, hash_state, new_pkce, EsiError, EsiTransport, HttpEsiTransport, OAuthState,
    PkceVerifier, SecretCipher,
};
use iskworks_storage::{OwnerHashCheck, PgInviteRepository, PgSessionRepository, PgUserRepository};
use thiserror::Error;

const DEFAULT_LOGIN_AUTH_URL: &str = "https://login.eveonline.com/v2/oauth/authorize";
const DEFAULT_LOGIN_TOKEN_URL: &str = "https://login.eveonline.com/v2/oauth/token";
const DEFAULT_LOGIN_JWKS_URL: &str = "https://login.eveonline.com/oauth/jwks";
const DEFAULT_LOGIN_ISSUER: &str = "https://login.eveonline.com";

/// The name of the httpOnly session cookie set on successful login.
pub const SESSION_COOKIE_NAME: &str = "iskworks_session";

#[derive(Debug, Error)]
pub enum AuthApplicationError {
    #[error("{0}")]
    Protocol(#[from] EsiError),
    #[error("{0}")]
    Persistence(#[from] AuthError),
    #[error("{0}")]
    Configuration(String),
    /// Invite-only mode is on and a genuinely new identity tried to
    /// provision without a valid invite attached to its pending-auth row.
    #[error("an invite code is required to create a new ISK Works account")]
    InviteRequired,
    /// An invite was supplied (at login start, or carried into the callback)
    /// but is not usable — unknown, expired, disabled, or already fully
    /// used. Collapsed to one public state on purpose; see `error.rs`.
    #[error("this invite code is not valid")]
    InviteInvalid,
    /// An admin has blocked this account from signing in.
    #[error("this account has been disabled")]
    AccountDisabled,
    /// The character now belongs to a different EVE account than the one
    /// that created this ISK Works account (e.g. sold on the Character
    /// Bazaar). Signing in would hand the new owner the previous owner's
    /// workspace, so it's refused until an admin sorts it out.
    #[error("this character has been transferred to another EVE account")]
    CharacterTransferred,
}

/// What `AuthService::begin_login` hands the route: the URL to send the
/// browser to, and the flow's `state` to bind to that browser (see
/// `cookies::OAUTH_STATE_COOKIE_NAME`).
#[derive(Debug)]
pub struct LoginStart {
    pub authorization_url: String,
    pub state: OAuthState,
}

/// EVE SSO login — deliberately separate from `EsiApplicationService`'s
/// character-link OAuth flow at the domain level: identity only, no ESI scopes requested, its own pending-authorization
/// table, since login is what creates a workspace rather than attaching to
/// one that already exists. Shares the EVE app registration, client ID, and
/// callback URL with the character-link flow, though — EVE's developer
/// portal only allows one callback URL per app, so both flows land on
/// `/api/eve/oauth/callback` and the route handler (`routes/esi.rs`) tells
/// them apart by which pending-authorization table `state` belongs to.
#[derive(Clone)]
pub struct AuthService {
    users: Arc<PgUserRepository>,
    sessions: Arc<PgSessionRepository>,
    invites: Arc<PgInviteRepository>,
    transport: Arc<dyn EsiTransport>,
    cipher: SecretCipher,
    client_id: String,
    authorization_url: String,
    redirect_uri: String,
    web_app_url: String,
    cookie_secure: bool,
    /// `ISKWORKS_INVITE_REQUIRED`. When true, a genuinely new EVE identity
    /// must present a valid invite before a user/workspace is provisioned.
    /// Existing users are unaffected. When false, current open-registration
    /// behavior. Never inferred from whether invite rows exist.
    invite_required: bool,
}

impl AuthService {
    pub fn from_env(
        users: Arc<PgUserRepository>,
        sessions: Arc<PgSessionRepository>,
        invites: Arc<PgInviteRepository>,
    ) -> Result<Option<Self>, AuthApplicationError> {
        let Some(client_id) = env::var("EVE_SSO_CLIENT_ID")
            .ok()
            .filter(|value| !value.trim().is_empty())
        else {
            return Ok(None);
        };
        // Same env var (and same default) EsiApplicationService reads for the
        // character-link flow — one EVE app, one callback URL, shared by both.
        let redirect_uri = env_or_default(
            "EVE_SSO_REDIRECT_URI",
            "http://127.0.0.1:8080/api/eve/oauth/callback",
        );
        let web_app_url = env_or_default("WEB_APP_URL", "http://127.0.0.1:5173");
        let key = env::var("TOKEN_ENCRYPTION_KEY").map_err(|_| {
            AuthApplicationError::Configuration(
                "TOKEN_ENCRYPTION_KEY is required when EVE SSO login is enabled.".to_string(),
            )
        })?;
        let cipher = SecretCipher::from_base64(&key)?;
        let transport: Arc<dyn EsiTransport> = Arc::new(HttpEsiTransport::new(
            client_id.clone(),
            redirect_uri.clone(),
            env_or_default("EVE_SSO_TOKEN_URL", DEFAULT_LOGIN_TOKEN_URL),
            env_or_default("EVE_SSO_JWKS_URL", DEFAULT_LOGIN_JWKS_URL),
            String::new(),
            env_or_default("EVE_SSO_ISSUER", DEFAULT_LOGIN_ISSUER),
        ));
        let cookie_secure = web_app_url.starts_with("https://");
        Ok(Some(Self {
            users,
            sessions,
            invites,
            transport,
            cipher,
            client_id,
            authorization_url: env_or_default("EVE_SSO_AUTHORIZATION_URL", DEFAULT_LOGIN_AUTH_URL),
            redirect_uri,
            web_app_url,
            cookie_secure,
            invite_required: crate::invite_required_from_env(),
        }))
    }

    /// Whether invite-only new-user provisioning is enforced
    /// (`ISKWORKS_INVITE_REQUIRED`). Surfaced to the frontend via
    /// `GET /api/auth/session` so the sign-in screen can show the invite
    /// affordance.
    #[must_use]
    pub fn invite_required(&self) -> bool {
        self.invite_required
    }

    #[must_use]
    pub fn web_app_url(&self) -> &str {
        &self.web_app_url
    }

    /// Whether the session cookie should carry the `Secure` flag — tied to
    /// `WEB_APP_URL`'s scheme so local HTTP dev (`http://127.0.0.1:...`)
    /// still works, while the `https://` deployed origin gets it enforced.
    #[must_use]
    pub fn cookie_secure(&self) -> bool {
        self.cookie_secure
    }

    /// Start an EVE SSO login. `invite_code` is the raw string the user
    /// typed into the sign-in form, if any.
    ///
    /// If a non-blank code is supplied it is validated now (normalize →
    /// hash → redeemable lookup) so an obviously bad code is rejected
    /// *before* the EVE round-trip, and the resolved `invite_id` — never the
    /// raw code — is stored on the pending-auth row. A blank/absent code is
    /// fine here even in invite-required mode: returning users don't have
    /// one, and the authoritative gate is `finish_login`, which only trips
    /// for a genuinely new identity.
    ///
    /// The raw code never enters the OAuth `state`, the authorization URL,
    /// or any log line.
    pub async fn begin_login(
        &self,
        invite_code: Option<&str>,
    ) -> Result<LoginStart, AuthApplicationError> {
        let invite_id = match invite_code.map(str::trim).filter(|code| !code.is_empty()) {
            Some(raw) => {
                let hash = hash_invite_code_from_raw(raw);
                match self.invites.find_redeemable_invite_id(&hash).await? {
                    Some(id) => Some(id),
                    None => return Err(AuthApplicationError::InviteInvalid),
                }
            }
            None => None,
        };

        let state = OAuthState::generate();
        let (verifier, challenge) = new_pkce();
        self.users
            .begin_login_authorization(
                hash_state(state.expose()),
                self.cipher.encrypt(verifier.expose())?,
                invite_id.map(|id| id.0),
            )
            .await?;
        let authorization_url = authorization_url(
            &self.authorization_url,
            &self.client_id,
            &self.redirect_uri,
            &[],
            &state,
            &challenge,
        )?;
        Ok(LoginStart {
            authorization_url,
            state,
        })
    }

    /// Returns `None`, not an error, when `state` doesn't belong to a login
    /// attempt — the route handler tries `EsiApplicationService`'s
    /// character-link equivalent next in that case, since both flows land on
    /// the same callback URL. See the type's doc comment.
    pub async fn try_consume_login(
        &self,
        state: &str,
    ) -> Result<Option<iskworks_storage::PendingLoginAuthorization>, AuthApplicationError> {
        match self
            .users
            .consume_login_authorization(&hash_state(state))
            .await
        {
            Ok(pending) => Ok(Some(pending)),
            Err(AuthError::InvalidLoginState) => Ok(None),
            Err(other) => Err(other.into()),
        }
    }

    pub async fn finish_login(
        &self,
        pending: iskworks_storage::PendingLoginAuthorization,
        code: &str,
    ) -> Result<SessionToken, AuthApplicationError> {
        let verifier = PkceVerifier::from_secret(self.cipher.decrypt(&pending.verifier)?)?;
        let token = self.transport.exchange_code(code, &verifier).await?;
        let identity = EveIdentity {
            character_id: token.identity.character_id,
            character_name: token.identity.character_name,
        };
        let owner_hash = token.owner_hash;
        if owner_hash.is_none() {
            tracing::warn!(
                character_id = identity.character_id,
                "EVE SSO token carried no owner claim; skipping ownership check"
            );
        }
        if let Some(owner_hash) = &owner_hash {
            if self
                .users
                .check_owner_hash(identity.character_id, owner_hash)
                .await?
                == OwnerHashCheck::Changed
            {
                tracing::warn!(
                    character_id = identity.character_id,
                    "sign-in refused: character changed EVE account owner"
                );
                return Err(AuthApplicationError::CharacterTransferred);
            }
        }

        // Distinguish returning vs. new here so the invite gate only ever
        // applies to a genuinely new identity. The provisioning call
        // re-checks `FOR UPDATE` inside its transaction, so a returning user
        // that races through is still served without consuming an invite.
        let existing = self
            .users
            .find_by_character_id(identity.character_id)
            .await?;
        if existing
            .as_ref()
            .is_some_and(|user| user.disabled_at.is_some())
        {
            return Err(AuthApplicationError::AccountDisabled);
        }
        let returning = existing.is_some();
        let grant = match (returning, self.invite_required, pending.invite_id) {
            // Returning user, or open-registration mode: no invite consumed.
            (true, _, _) | (false, false, _) => InviteGrant::NotRequired,
            // New identity in invite mode with a pre-validated invite.
            (false, true, Some(invite_id)) => InviteGrant::Required(invite_id),
            // New identity in invite mode with nothing attached.
            (false, true, None) => return Err(AuthApplicationError::InviteRequired),
        };

        let user = match self
            .users
            .claim_unclaimed_workspace_or_provision(identity, grant)
            .await
        {
            Ok(user) => user,
            // The invite expired / was disabled / was exhausted between the
            // login-start check and now — provisioning rolled back, nothing
            // created.
            Err(AuthError::InviteRejected) => return Err(AuthApplicationError::InviteInvalid),
            Err(other) => return Err(other.into()),
        };
        if let Some(owner_hash) = &owner_hash {
            // A brand-new user: record the owner it was provisioned under.
            self.users
                .check_owner_hash(user.eve_character_id, owner_hash)
                .await?;
        }
        self.users.touch_last_login(user.id).await?;

        let session_token = SessionService::new(self.sessions.clone())
            .issue_session(user.id)
            .await?;
        Ok(session_token)
    }

    pub async fn logout(&self, token: &SessionToken) -> Result<(), AuthApplicationError> {
        SessionService::new(self.sessions.clone())
            .revoke(token)
            .await?;
        Ok(())
    }

    pub async fn resolve_session(
        &self,
        token: &SessionToken,
    ) -> Result<Option<AuthenticatedUser>, AuthApplicationError> {
        Ok(SessionService::new(self.sessions.clone())
            .resolve(token)
            .await?)
    }
}

#[cfg(all(feature = "dev-auth", debug_assertions))]
impl AuthService {
    /// Bypasses the OAuth round-trip entirely — no network call, no real
    /// EVE character required. Only reachable at all when `routes::dev_auth`
    /// mounts its route, which itself requires `ISKWORKS_DEV_AUTH=1`; see
    /// that module's doc comment for the full three-layer safety design.
    pub async fn dev_login(
        &self,
        character_id: i64,
        character_name: String,
    ) -> Result<SessionToken, AuthApplicationError> {
        let user = self
            .users
            .claim_unclaimed_workspace_or_provision(
                EveIdentity {
                    character_id,
                    character_name,
                },
                // The dev backdoor never gates on invites.
                InviteGrant::NotRequired,
            )
            .await?;
        self.users.touch_last_login(user.id).await?;

        let session_token = SessionService::new(self.sessions.clone())
            .issue_session(user.id)
            .await?;
        Ok(session_token)
    }
}

#[cfg(all(test, feature = "dev-auth"))]
impl AuthService {
    pub(crate) fn new_for_dev_auth_tests(
        users: Arc<PgUserRepository>,
        sessions: Arc<PgSessionRepository>,
        invites: Arc<PgInviteRepository>,
    ) -> Self {
        Self::new_for_tests(
            users,
            sessions,
            invites,
            Arc::new(iskworks_esi::UnusedEsiTransport),
        )
    }
}

#[cfg(test)]
impl AuthService {
    /// An `AuthService` whose pool never connects, for router tests that
    /// only need auth to be *configured*: a request without a session
    /// cookie never reaches the database. Anything that does query fails.
    pub(crate) fn new_unconnected_for_tests() -> Self {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused@127.0.0.1:1/unused")
            .expect("a well-formed URL");
        Self::new_for_tests(
            Arc::new(PgUserRepository::new(pool.clone())),
            Arc::new(PgSessionRepository::new(pool.clone())),
            Arc::new(PgInviteRepository::new(pool)),
            Arc::new(iskworks_esi::UnusedEsiTransport),
        )
    }
}

fn env_or_default(name: &str, default: &str) -> String {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

#[cfg(test)]
impl AuthService {
    /// `pub(crate)` (not just private) because other route modules' own
    /// integration tests (e.g. `routes::esi::tests`) need a real `AuthService`
    /// too, and privacy is per-module in Rust — a plain `fn` here would only
    /// be visible within this module.
    pub(crate) fn new_for_tests(
        users: Arc<PgUserRepository>,
        sessions: Arc<PgSessionRepository>,
        invites: Arc<PgInviteRepository>,
        transport: Arc<dyn EsiTransport>,
    ) -> Self {
        Self {
            users,
            sessions,
            invites,
            transport,
            cipher: SecretCipher::for_tests(),
            client_id: "test-client".to_string(),
            authorization_url: DEFAULT_LOGIN_AUTH_URL.to_string(),
            redirect_uri: "http://127.0.0.1:8080/api/eve/oauth/callback".to_string(),
            web_app_url: "http://127.0.0.1:5173".to_string(),
            cookie_secure: false,
            invite_required: false,
        }
    }

    /// Flip invite-only mode on for a test without going through env.
    pub(crate) fn with_invite_required(mut self, value: bool) -> Self {
        self.invite_required = value;
        self
    }
}

/// Resolves a different identity per `exchange_code` call — the
/// "authorization code" is `"<character_id>:<character_name>[:<owner hash>]"`
/// (owner defaults to one fixed per character). Shared by
/// any test (in this module or elsewhere in the crate) that needs two real,
/// distinct characters to log in through the same running router.
#[cfg(test)]
pub(crate) struct MultiIdentityFakeTransport;

#[cfg(test)]
#[async_trait::async_trait]
impl EsiTransport for MultiIdentityFakeTransport {
    async fn exchange_code(
        &self,
        code: &str,
        _verifier: &PkceVerifier,
    ) -> Result<iskworks_esi::AuthenticatedToken, EsiError> {
        let mut parts = code.splitn(3, ':');
        let character_id = parts
            .next()
            .expect("test code is 'character_id:character_name[:owner]'");
        let character_name = parts
            .next()
            .expect("test code is 'character_id:character_name[:owner]'");
        let owner_hash = parts
            .next()
            .map_or_else(|| format!("owner-of-{character_id}"), str::to_string);
        Ok(iskworks_esi::AuthenticatedToken {
            access_token: "fake-access".to_string(),
            refresh_token: "fake-refresh".to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            owner_hash: Some(owner_hash),
            identity: iskworks_esi::Identity {
                character_id: character_id.parse().expect("test character id is numeric"),
                character_name: character_name.to_string(),
                scopes: std::collections::BTreeSet::new(),
            },
        })
    }

    async fn refresh(
        &self,
        _refresh_token: &str,
    ) -> Result<iskworks_esi::RefreshedToken, EsiError> {
        unimplemented!("login never refreshes a token")
    }

    async fn assets(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<iskworks_esi::EsiResponse<iskworks_esi::AssetObservation>, EsiError> {
        unimplemented!("login never reads ESI resources")
    }

    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        _from_id: Option<i64>,
        _etag: Option<&str>,
    ) -> Result<iskworks_esi::EsiResponse<iskworks_esi::WalletTransactionObservation>, EsiError>
    {
        unimplemented!("login never reads ESI resources")
    }

    async fn structure(
        &self,
        _access_token: &str,
        _structure_id: i64,
    ) -> Result<iskworks_esi::StructureInformation, EsiError> {
        unimplemented!("login never reads ESI resources")
    }

    async fn industry_systems(
        &self,
    ) -> Result<iskworks_esi::EsiResponse<iskworks_esi::IndustrySystemCostIndex>, EsiError> {
        unimplemented!("login never reads ESI resources")
    }
}

#[cfg(test)]
mod tests;
