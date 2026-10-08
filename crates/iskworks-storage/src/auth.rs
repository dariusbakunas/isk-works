use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_core::{
    decide_workspace_provisioning, AuthError, AuthenticatedUser, EveIdentity, InviteGrant,
    InviteId, NewWorkspace, SessionRepository, User, UserId, UserRepository, WorkspaceId,
    WorkspaceProvisioning,
};

use crate::invite::consume_invite_in_tx;
use iskworks_esi::EncryptedSecret;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::workspace::{insert_owner, insert_workspace};

/// The PKCE verifier for a login (not character-link) OAuth attempt that's in
/// flight. Deliberately has no `workspace_id`/`owner_id` — unlike
/// `eve_oauth_pending_authorizations`, none exists yet at login time.
///
/// `invite_id` is the server-side reference to a pre-validated invite (invite
/// mode only, new-user flow only). The raw invite code is never stored here
/// or anywhere else — only this opaque id.
pub struct PendingLoginAuthorization {
    pub verifier: EncryptedSecret,
    pub invite_id: Option<InviteId>,
}

#[derive(Clone)]
pub struct PgUserRepository {
    pool: PgPool,
}

impl PgUserRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Records a login attempt's PKCE state so the callback can retrieve it.
    /// Not part of the `UserRepository` trait — OAuth pending-state, like
    /// `PgEsiRepository`'s character-link equivalent, is a storage/protocol
    /// concern the API's `AuthService` calls directly.
    pub async fn begin_login_authorization(
        &self,
        state_hash: String,
        verifier: EncryptedSecret,
        invite_id: Option<Uuid>,
    ) -> Result<(), AuthError> {
        sqlx::query(
            r#"
            INSERT INTO login_oauth_pending_authorizations (state_hash, pkce_verifier_encrypted, created_at, invite_id)
            VALUES ($1, $2, now(), $3)
            "#,
        )
        .bind(state_hash)
        .bind(serde_json::to_value(verifier).map_err(map_json_error)?)
        .bind(invite_id)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    pub async fn consume_login_authorization(
        &self,
        state_hash: &str,
    ) -> Result<PendingLoginAuthorization, AuthError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;
        let row = sqlx::query_as::<_, LoginPendingRow>(
            r#"
            SELECT pkce_verifier_encrypted, invite_id
            FROM login_oauth_pending_authorizations
            WHERE state_hash = $1
              AND consumed_at IS NULL
              AND created_at > now() - interval '10 minutes'
            FOR UPDATE
            "#,
        )
        .bind(state_hash)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx_error)?
        .ok_or(AuthError::InvalidLoginState)?;
        sqlx::query(
            "UPDATE login_oauth_pending_authorizations SET consumed_at = now() WHERE state_hash = $1",
        )
        .bind(state_hash)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;
        tx.commit().await.map_err(map_sqlx_error)?;

        Ok(PendingLoginAuthorization {
            verifier: serde_json::from_value(row.pkce_verifier_encrypted)
                .map_err(map_json_error)?,
            invite_id: row.invite_id.map(InviteId),
        })
    }
}

/// Outcome of `PgUserRepository::check_owner_hash`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerHashCheck {
    /// No user signs in with this character yet.
    NoUser,
    /// The user had no owner recorded; this one is now (trust on first use).
    Recorded,
    /// Same EVE account as before.
    Matches,
    /// The character now belongs to a different EVE account.
    Changed,
}

impl PgUserRepository {
    /// Compares a sign-in's JWT owner hash with the one recorded for this
    /// character's user, recording it if none was yet.
    pub async fn check_owner_hash(
        &self,
        eve_character_id: i64,
        owner_hash: &str,
    ) -> Result<OwnerHashCheck, AuthError> {
        let recorded = sqlx::query_scalar::<_, bool>(
            r#"
            UPDATE users SET eve_owner_hash = $2
            WHERE eve_character_id = $1 AND eve_owner_hash IS NULL
            RETURNING true
            "#,
        )
        .bind(eve_character_id)
        .bind(owner_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        if recorded.is_some() {
            return Ok(OwnerHashCheck::Recorded);
        }
        let stored = sqlx::query_scalar::<_, Option<String>>(
            "SELECT eve_owner_hash FROM users WHERE eve_character_id = $1",
        )
        .bind(eve_character_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(match stored {
            None => OwnerHashCheck::NoUser,
            Some(stored) if stored.as_deref() == Some(owner_hash) => OwnerHashCheck::Matches,
            Some(_) => OwnerHashCheck::Changed,
        })
    }
}

#[derive(sqlx::FromRow)]
struct LoginPendingRow {
    pkce_verifier_encrypted: serde_json::Value,
    invite_id: Option<Uuid>,
}

#[async_trait]
impl UserRepository for PgUserRepository {
    async fn find_by_character_id(&self, eve_character_id: i64) -> Result<Option<User>, AuthError> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT id, eve_character_id, eve_character_name, workspace_id, created_at, last_login_at,
                   disabled_at
            FROM users
            WHERE eve_character_id = $1
            "#,
        )
        .bind(eve_character_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.map(UserRow::into_user))
    }

    async fn claim_unclaimed_workspace_or_provision(
        &self,
        identity: EveIdentity,
        grant: InviteGrant,
    ) -> Result<User, AuthError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;

        let existing = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT id, eve_character_id, eve_character_name, workspace_id, created_at, last_login_at,
                   disabled_at
            FROM users
            WHERE eve_character_id = $1
            FOR UPDATE
            "#,
        )
        .bind(identity.character_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        if let Some(existing) = existing {
            // A returning identity never consumes an invite, even if one was
            // attached to the pending-auth row. Grant is ignored here.
            tx.commit().await.map_err(map_sqlx_error)?;
            return Ok(existing.into_user());
        }

        // Genuinely new identity. In invite-required mode, consume the
        // pre-validated invite atomically *before* any workspace/user row is
        // written — a rejected invite (expired/disabled/exhausted since the
        // login-start check) aborts the whole transaction with nothing
        // created.
        if let InviteGrant::Required(invite_id) = grant {
            consume_invite_in_tx(&mut tx, invite_id).await?;
        }

        let unclaimed_workspace_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id FROM workspaces
            WHERE claimed_at IS NULL
            ORDER BY created_at ASC
            LIMIT 1
            FOR UPDATE
            "#,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx_error)?
        .map(WorkspaceId);

        let workspace_id = match decide_workspace_provisioning(unclaimed_workspace_id) {
            WorkspaceProvisioning::Claim(workspace_id) => {
                claim_workspace(&mut tx, workspace_id).await?;
                workspace_id
            }
            WorkspaceProvisioning::ProvisionNew => {
                provision_workspace(&mut tx, &identity.character_name).await?
            }
        };

        let now = crate::db_now();
        let user = User {
            id: UserId::new(),
            eve_character_id: identity.character_id,
            eve_character_name: identity.character_name,
            workspace_id,
            created_at: now,
            last_login_at: now,
            disabled_at: None,
        };
        insert_user(&mut tx, &user).await?;

        tx.commit().await.map_err(map_sqlx_error)?;
        Ok(user)
    }

    async fn touch_last_login(&self, user_id: UserId) -> Result<(), AuthError> {
        sqlx::query("UPDATE users SET last_login_at = now() WHERE id = $1")
            .bind(user_id.0)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }
}

async fn claim_workspace(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: WorkspaceId,
) -> Result<(), AuthError> {
    sqlx::query("UPDATE workspaces SET claimed_at = now() WHERE id = $1")
        .bind(workspace_id.0)
        .execute(&mut **tx)
        .await
        .map_err(map_sqlx_error)?;
    Ok(())
}

async fn provision_workspace(
    tx: &mut Transaction<'_, Postgres>,
    character_name: &str,
) -> Result<WorkspaceId, AuthError> {
    let new_workspace = NewWorkspace::manual(character_name.to_string());
    insert_workspace(tx, &new_workspace.workspace)
        .await
        .map_err(map_sqlx_error)?;
    insert_owner(tx, &new_workspace.owner)
        .await
        .map_err(map_sqlx_error)?;
    sqlx::query("UPDATE workspaces SET claimed_at = now() WHERE id = $1")
        .bind(new_workspace.workspace.id.0)
        .execute(&mut **tx)
        .await
        .map_err(map_sqlx_error)?;
    Ok(new_workspace.workspace.id)
}

async fn insert_user(tx: &mut Transaction<'_, Postgres>, user: &User) -> Result<(), AuthError> {
    sqlx::query(
        r#"
        INSERT INTO users (id, eve_character_id, eve_character_name, workspace_id, created_at, last_login_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(user.id.0)
    .bind(user.eve_character_id)
    .bind(&user.eve_character_name)
    .bind(user.workspace_id.0)
    .bind(user.created_at)
    .bind(user.last_login_at)
    .execute(&mut **tx)
    .await
    .map_err(map_sqlx_error)?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    eve_character_id: i64,
    eve_character_name: String,
    workspace_id: Uuid,
    created_at: DateTime<Utc>,
    last_login_at: DateTime<Utc>,
    disabled_at: Option<DateTime<Utc>>,
}

impl UserRow {
    fn into_user(self) -> User {
        User {
            id: UserId(self.id),
            eve_character_id: self.eve_character_id,
            eve_character_name: self.eve_character_name,
            workspace_id: WorkspaceId(self.workspace_id),
            created_at: self.created_at,
            last_login_at: self.last_login_at,
            disabled_at: self.disabled_at,
        }
    }
}

#[derive(Clone)]
pub struct PgSessionRepository {
    pool: PgPool,
}

impl PgSessionRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SessionRepository for PgSessionRepository {
    async fn create_session(
        &self,
        user_id: UserId,
        token_hash: String,
        expires_at: DateTime<Utc>,
    ) -> Result<(), AuthError> {
        sqlx::query(
            r#"
            INSERT INTO sessions (token_hash, user_id, created_at, expires_at)
            VALUES ($1, $2, now(), $3)
            "#,
        )
        .bind(token_hash)
        .bind(user_id.0)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn resolve_session(
        &self,
        token_hash: &str,
    ) -> Result<Option<AuthenticatedUser>, AuthError> {
        let row = sqlx::query_as::<_, AuthenticatedUserRow>(
            r#"
            SELECT u.id AS user_id, u.workspace_id, u.eve_character_id, u.eve_character_name
            FROM sessions s
            JOIN users u ON u.id = s.user_id
            WHERE s.token_hash = $1 AND s.expires_at > now()
              AND u.disabled_at IS NULL
            "#,
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.map(AuthenticatedUserRow::into_authenticated_user))
    }

    async fn delete_session(&self, token_hash: &str) -> Result<(), AuthError> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
            .bind(token_hash)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct AuthenticatedUserRow {
    user_id: Uuid,
    workspace_id: Uuid,
    eve_character_id: i64,
    eve_character_name: String,
}

impl AuthenticatedUserRow {
    fn into_authenticated_user(self) -> AuthenticatedUser {
        AuthenticatedUser {
            user_id: UserId(self.user_id),
            workspace_id: WorkspaceId(self.workspace_id),
            eve_character_id: self.eve_character_id,
            eve_character_name: self.eve_character_name,
        }
    }
}

fn map_sqlx_error(error: sqlx::Error) -> AuthError {
    AuthError::Persistence(error.to_string())
}

fn map_json_error(error: serde_json::Error) -> AuthError {
    AuthError::Persistence(error.to_string())
}

/// How long a consumed or expired pending authorization is kept before the
/// purge drops it -- long enough to look at a failed sign-in afterwards.
const PENDING_AUTHORIZATION_RETENTION_HOURS: i64 = 24;

/// Rows removed by one `PgAuthMaintenance::purge_expired` pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AuthPurgeOutcome {
    pub sessions: u64,
    pub login_authorizations: u64,
    pub link_authorizations: u64,
}

/// Deletes auth rows nothing reads anymore: expired sessions, and pending
/// login / character-link authorizations past their 10-minute window (plus
/// a retention margin). Nothing else ever deletes them, and an anonymous
/// caller can create a pending login row per request.
#[derive(Clone)]
pub struct PgAuthMaintenance {
    pool: PgPool,
}

impl PgAuthMaintenance {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn purge_expired(&self, now: DateTime<Utc>) -> Result<AuthPurgeOutcome, AuthError> {
        let cutoff = now - chrono::Duration::hours(PENDING_AUTHORIZATION_RETENTION_HOURS);
        let sessions = sqlx::query("DELETE FROM sessions WHERE expires_at <= $1")
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?
            .rows_affected();
        let login_authorizations =
            sqlx::query("DELETE FROM login_oauth_pending_authorizations WHERE created_at < $1")
                .bind(cutoff)
                .execute(&self.pool)
                .await
                .map_err(map_sqlx_error)?
                .rows_affected();
        let link_authorizations =
            sqlx::query("DELETE FROM eve_oauth_pending_authorizations WHERE expires_at < $1")
                .bind(cutoff)
                .execute(&self.pool)
                .await
                .map_err(map_sqlx_error)?
                .rows_affected();
        Ok(AuthPurgeOutcome {
            sessions,
            login_authorizations,
            link_authorizations,
        })
    }
}
