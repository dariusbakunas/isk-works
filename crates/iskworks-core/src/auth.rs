use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

use crate::invite::InviteGrant;
use crate::workspace::WorkspaceId;

pub const SESSION_TTL_DAYS: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub Uuid);

impl UserId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for UserId {
    fn default() -> Self {
        Self::new()
    }
}

/// An opaque, unguessable session credential. Only ever exists in full as the
/// cookie value handed to the browser and transiently at issuance time — what
/// gets persisted (via `hash()`) is a SHA-256 digest, never the raw token, so
/// a database read alone can't be replayed as a session.
#[derive(Clone, Eq, PartialEq)]
pub struct SessionToken(String);

impl SessionToken {
    pub fn generate() -> Self {
        let mut bytes = [0_u8; 32];
        OsRng.fill_bytes(&mut bytes);
        Self(URL_SAFE_NO_PAD.encode(bytes))
    }

    /// Wraps a raw token value read back from a cookie — the caller already
    /// has the token, this doesn't generate or derive anything new.
    pub fn from_raw(raw: String) -> Self {
        Self(raw)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn hash(&self) -> String {
        format!("{:x}", Sha256::digest(self.0.as_bytes()))
    }
}

impl std::fmt::Debug for SessionToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SessionToken([redacted])")
    }
}

/// The identity claim from an EVE SSO login exchange — nothing more than
/// what's needed to resolve or provision a `User`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EveIdentity {
    pub character_id: i64,
    pub character_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub eve_character_id: i64,
    pub eve_character_name: String,
    pub workspace_id: WorkspaceId,
    pub created_at: DateTime<Utc>,
    pub last_login_at: DateTime<Utc>,
    /// Set by an admin to block sign-in; `None` for a normal user.
    pub disabled_at: Option<DateTime<Utc>>,
}

/// What a validated session resolves to — the shape every `workspace_context()`
/// call site needs, without exposing the full `User` record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthenticatedUser {
    pub user_id: UserId,
    pub workspace_id: WorkspaceId,
    pub eve_character_id: i64,
    pub eve_character_name: String,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("persistence failed: {0}")]
    Persistence(String),
    #[error("login authorization state is invalid, expired, or already used")]
    InvalidLoginState,
    /// The invite attached to a new-identity provisioning attempt could not
    /// be consumed — it expired, was disabled, or was exhausted between the
    /// login-start UX check and this authoritative check. Provisioning is
    /// rolled back; no workspace or user is created.
    #[error("invite is no longer valid")]
    InviteRejected,
}

/// Whether a login should claim the one legacy workspace that predates
/// per-user workspaces (existing data is claimed, not guessed) or provision
/// a fresh one. A pure decision over already-fetched state — the fetch (does an
/// unclaimed workspace exist right now) is the caller's I/O.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceProvisioning {
    Claim(WorkspaceId),
    ProvisionNew,
}

pub fn decide_workspace_provisioning(
    unclaimed_workspace_id: Option<WorkspaceId>,
) -> WorkspaceProvisioning {
    match unclaimed_workspace_id {
        Some(id) => WorkspaceProvisioning::Claim(id),
        None => WorkspaceProvisioning::ProvisionNew,
    }
}

#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn create_session(
        &self,
        user_id: UserId,
        token_hash: String,
        expires_at: DateTime<Utc>,
    ) -> Result<(), AuthError>;

    async fn resolve_session(
        &self,
        token_hash: &str,
    ) -> Result<Option<AuthenticatedUser>, AuthError>;

    async fn delete_session(&self, token_hash: &str) -> Result<(), AuthError>;
}

#[async_trait]
impl<T> SessionRepository for Arc<T>
where
    T: SessionRepository + ?Sized,
{
    async fn create_session(
        &self,
        user_id: UserId,
        token_hash: String,
        expires_at: DateTime<Utc>,
    ) -> Result<(), AuthError> {
        (**self)
            .create_session(user_id, token_hash, expires_at)
            .await
    }

    async fn resolve_session(
        &self,
        token_hash: &str,
    ) -> Result<Option<AuthenticatedUser>, AuthError> {
        (**self).resolve_session(token_hash).await
    }

    async fn delete_session(&self, token_hash: &str) -> Result<(), AuthError> {
        (**self).delete_session(token_hash).await
    }
}

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_character_id(&self, eve_character_id: i64) -> Result<Option<User>, AuthError>;

    /// Idempotent per character: a character that already has a `users` row
    /// returns that row untouched (aside from `last_login_at`), and `grant`
    /// is ignored entirely — a returning user never consumes an invite.
    /// Otherwise claims the single pre-existing unclaimed workspace if one
    /// exists, else provisions a fresh workspace + hidden owner for this
    /// character. When `grant` is `InviteGrant::Required`, the invite is
    /// atomically consumed in the same transaction and the whole
    /// provisioning is rolled back (`AuthError::InviteRejected`) if it can
    /// no longer be consumed.
    async fn claim_unclaimed_workspace_or_provision(
        &self,
        identity: EveIdentity,
        grant: InviteGrant,
    ) -> Result<User, AuthError>;

    async fn touch_last_login(&self, user_id: UserId) -> Result<(), AuthError>;
}

#[async_trait]
impl<T> UserRepository for Arc<T>
where
    T: UserRepository + ?Sized,
{
    async fn find_by_character_id(&self, eve_character_id: i64) -> Result<Option<User>, AuthError> {
        (**self).find_by_character_id(eve_character_id).await
    }

    async fn claim_unclaimed_workspace_or_provision(
        &self,
        identity: EveIdentity,
        grant: InviteGrant,
    ) -> Result<User, AuthError> {
        (**self)
            .claim_unclaimed_workspace_or_provision(identity, grant)
            .await
    }

    async fn touch_last_login(&self, user_id: UserId) -> Result<(), AuthError> {
        (**self).touch_last_login(user_id).await
    }
}

/// Session issuance/validation policy (TTL, hashing) — a thin layer over
/// `SessionRepository` so no call site has to remember to hash a token before
/// storing it or compute the expiry itself.
#[derive(Debug)]
pub struct SessionService<S> {
    sessions: S,
}

impl<S> SessionService<S>
where
    S: SessionRepository,
{
    pub fn new(sessions: S) -> Self {
        Self { sessions }
    }

    pub async fn issue_session(&self, user_id: UserId) -> Result<SessionToken, AuthError> {
        let token = SessionToken::generate();
        let expires_at = Utc::now() + Duration::days(SESSION_TTL_DAYS);
        self.sessions
            .create_session(user_id, token.hash(), expires_at)
            .await?;
        Ok(token)
    }

    pub async fn resolve(
        &self,
        token: &SessionToken,
    ) -> Result<Option<AuthenticatedUser>, AuthError> {
        self.sessions.resolve_session(&token.hash()).await
    }

    pub async fn revoke(&self, token: &SessionToken) -> Result<(), AuthError> {
        self.sessions.delete_session(&token.hash()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[test]
    fn session_token_hash_is_deterministic_and_does_not_leak_the_raw_token() {
        let token = SessionToken::generate();
        assert_eq!(token.hash(), token.hash());
        assert!(!token.hash().contains(token.expose()));
        assert_eq!(format!("{token:?}"), "SessionToken([redacted])");
    }

    #[test]
    fn workspace_provisioning_claims_when_an_unclaimed_workspace_exists() {
        let existing = WorkspaceId::new();
        assert_eq!(
            decide_workspace_provisioning(Some(existing)),
            WorkspaceProvisioning::Claim(existing)
        );
        assert_eq!(
            decide_workspace_provisioning(None),
            WorkspaceProvisioning::ProvisionNew
        );
    }

    #[derive(Default)]
    struct MemorySessions {
        by_hash: Mutex<HashMap<String, (AuthenticatedUser, DateTime<Utc>)>>,
    }

    #[async_trait]
    impl SessionRepository for MemorySessions {
        async fn create_session(
            &self,
            user_id: UserId,
            token_hash: String,
            expires_at: DateTime<Utc>,
        ) -> Result<(), AuthError> {
            self.by_hash.lock().unwrap().insert(
                token_hash,
                (
                    AuthenticatedUser {
                        user_id,
                        workspace_id: WorkspaceId::new(),
                        eve_character_id: 90_000_001,
                        eve_character_name: "Test Character".to_string(),
                    },
                    expires_at,
                ),
            );
            Ok(())
        }

        async fn resolve_session(
            &self,
            token_hash: &str,
        ) -> Result<Option<AuthenticatedUser>, AuthError> {
            let guard = self.by_hash.lock().unwrap();
            Ok(guard
                .get(token_hash)
                .and_then(|(user, expires_at)| (*expires_at > Utc::now()).then(|| user.clone())))
        }

        async fn delete_session(&self, token_hash: &str) -> Result<(), AuthError> {
            self.by_hash.lock().unwrap().remove(token_hash);
            Ok(())
        }
    }

    #[tokio::test]
    async fn session_round_trips_through_the_service() {
        let service = SessionService::new(MemorySessions::default());
        let user_id = UserId::new();

        let token = service.issue_session(user_id).await.unwrap();
        let resolved = service.resolve(&token).await.unwrap().unwrap();
        assert_eq!(resolved.user_id, user_id);
    }

    #[tokio::test]
    async fn expired_session_resolves_to_none() {
        let repository = MemorySessions::default();
        let user_id = UserId::new();
        let token = SessionToken::generate();
        repository
            .create_session(user_id, token.hash(), Utc::now() - Duration::seconds(1))
            .await
            .unwrap();

        let service = SessionService::new(repository);
        assert!(service.resolve(&token).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn revoked_session_resolves_to_none() {
        let service = SessionService::new(MemorySessions::default());
        let token = service.issue_session(UserId::new()).await.unwrap();
        assert!(service.resolve(&token).await.unwrap().is_some());

        service.revoke(&token).await.unwrap();
        assert!(service.resolve(&token).await.unwrap().is_none());
    }

    #[derive(Default)]
    struct MemoryUsers {
        by_character_id: Mutex<HashMap<i64, User>>,
        unclaimed_workspace: Mutex<Option<WorkspaceId>>,
    }

    impl MemoryUsers {
        fn with_unclaimed_workspace(workspace_id: WorkspaceId) -> Self {
            Self {
                by_character_id: Mutex::new(HashMap::new()),
                unclaimed_workspace: Mutex::new(Some(workspace_id)),
            }
        }
    }

    #[async_trait]
    impl UserRepository for MemoryUsers {
        async fn find_by_character_id(
            &self,
            eve_character_id: i64,
        ) -> Result<Option<User>, AuthError> {
            Ok(self
                .by_character_id
                .lock()
                .unwrap()
                .get(&eve_character_id)
                .cloned())
        }

        async fn claim_unclaimed_workspace_or_provision(
            &self,
            identity: EveIdentity,
            _grant: InviteGrant,
        ) -> Result<User, AuthError> {
            if let Some(existing) = self.find_by_character_id(identity.character_id).await? {
                return Ok(existing);
            }

            let workspace_id = match decide_workspace_provisioning(
                self.unclaimed_workspace.lock().unwrap().take(),
            ) {
                WorkspaceProvisioning::Claim(id) => id,
                WorkspaceProvisioning::ProvisionNew => WorkspaceId::new(),
            };

            let now = Utc::now();
            let user = User {
                id: UserId::new(),
                eve_character_id: identity.character_id,
                eve_character_name: identity.character_name,
                workspace_id,
                created_at: now,
                last_login_at: now,
                disabled_at: None,
            };
            self.by_character_id
                .lock()
                .unwrap()
                .insert(user.eve_character_id, user.clone());
            Ok(user)
        }

        async fn touch_last_login(&self, user_id: UserId) -> Result<(), AuthError> {
            let mut guard = self.by_character_id.lock().unwrap();
            if let Some(user) = guard.values_mut().find(|user| user.id == user_id) {
                user.last_login_at = Utc::now();
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn first_login_claims_the_pre_existing_unclaimed_workspace() {
        let unclaimed = WorkspaceId::new();
        let repository = MemoryUsers::with_unclaimed_workspace(unclaimed);

        let user = repository
            .claim_unclaimed_workspace_or_provision(
                EveIdentity {
                    character_id: 1,
                    character_name: "First Character".to_string(),
                },
                InviteGrant::NotRequired,
            )
            .await
            .unwrap();

        assert_eq!(user.workspace_id, unclaimed);
    }

    #[tokio::test]
    async fn second_distinct_character_gets_a_freshly_provisioned_workspace() {
        let unclaimed = WorkspaceId::new();
        let repository = MemoryUsers::with_unclaimed_workspace(unclaimed);

        let first = repository
            .claim_unclaimed_workspace_or_provision(
                EveIdentity {
                    character_id: 1,
                    character_name: "First Character".to_string(),
                },
                InviteGrant::NotRequired,
            )
            .await
            .unwrap();
        let second = repository
            .claim_unclaimed_workspace_or_provision(
                EveIdentity {
                    character_id: 2,
                    character_name: "Second Character".to_string(),
                },
                InviteGrant::NotRequired,
            )
            .await
            .unwrap();

        assert_eq!(first.workspace_id, unclaimed);
        assert_ne!(second.workspace_id, unclaimed);
        assert_ne!(second.workspace_id, first.workspace_id);
    }

    #[tokio::test]
    async fn repeat_login_resolves_to_the_same_workspace_not_a_new_one() {
        let repository = MemoryUsers::with_unclaimed_workspace(WorkspaceId::new());
        let identity = EveIdentity {
            character_id: 1,
            character_name: "Repeat Character".to_string(),
        };

        let first = repository
            .claim_unclaimed_workspace_or_provision(identity.clone(), InviteGrant::NotRequired)
            .await
            .unwrap();
        let second = repository
            .claim_unclaimed_workspace_or_provision(identity, InviteGrant::NotRequired)
            .await
            .unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(first.workspace_id, second.workspace_id);
    }
}
