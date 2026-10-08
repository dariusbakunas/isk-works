use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_core::{AuthError, InviteAdminRepository, InviteId, InviteSummary, NewInvite};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone)]
pub struct PgInviteRepository {
    pool: PgPool,
}

impl PgInviteRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Operator tooling: record a new invite. Returns its id; the caller
    /// already holds (and prints, once) the plaintext.
    pub async fn create_invite(&self, new_invite: NewInvite) -> Result<InviteId, AuthError> {
        let id = InviteId::new();
        sqlx::query(
            r#"
            INSERT INTO invite_codes (id, code_hash, max_uses, expires_at, note, code_ciphertext)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(id.0)
        .bind(new_invite.code_hash)
        .bind(new_invite.max_uses)
        .bind(new_invite.expires_at)
        .bind(new_invite.note)
        .bind(new_invite.code_ciphertext)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(id)
    }

    /// Operator tooling: newest first. Never includes `code_hash`.
    pub async fn list_invites(&self) -> Result<Vec<InviteSummary>, AuthError> {
        let rows = sqlx::query_as::<_, InviteRow>(
            r#"
            SELECT id, created_at, expires_at, disabled_at, max_uses, use_count, note,
                   (code_ciphertext IS NOT NULL) AS revealable
            FROM invite_codes
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(rows.into_iter().map(InviteRow::into_summary).collect())
    }

    /// Operator tooling: disable an invite so it can no longer be redeemed.
    /// Idempotent — an already-disabled invite keeps its original
    /// `disabled_at`. The row is never deleted (minimal audit history).
    /// Returns `false` if no such invite exists.
    pub async fn disable_invite(&self, id: InviteId) -> Result<bool, AuthError> {
        let result = sqlx::query(
            r#"
            UPDATE invite_codes
               SET disabled_at = COALESCE(disabled_at, now())
             WHERE id = $1
            "#,
        )
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected() > 0)
    }

    /// Admin: permanently delete an invite. The pending-login reference to it
    /// is nulled by the FK (`ON DELETE SET NULL`), so an in-flight login that
    /// pre-validated this invite is rejected at the authoritative consume.
    /// Returns `false` if no such invite exists.
    pub async fn delete_invite(&self, id: InviteId) -> Result<bool, AuthError> {
        let result = sqlx::query("DELETE FROM invite_codes WHERE id = $1")
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(result.rows_affected() > 0)
    }

    /// Admin reveal: the encrypted code for `id`, if one was stored.
    pub async fn invite_code_ciphertext(&self, id: InviteId) -> Result<Option<String>, AuthError> {
        let row = sqlx::query_scalar::<_, Option<String>>(
            "SELECT code_ciphertext FROM invite_codes WHERE id = $1",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(row.flatten())
    }

    /// Login-start UX check: resolve a normalized-code hash to an invite id
    /// only if the invite is currently redeemable (not disabled, not
    /// expired, uses remaining). This is advisory — the authoritative check
    /// is the atomic consume inside provisioning (see
    /// `consume_invite_in_tx`) — so a race that exhausts the invite between
    /// here and the callback is caught there, not here.
    pub async fn find_redeemable_invite_id(
        &self,
        code_hash: &str,
    ) -> Result<Option<InviteId>, AuthError> {
        let row = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id
            FROM invite_codes
            WHERE code_hash = $1
              AND disabled_at IS NULL
              AND (expires_at IS NULL OR expires_at > now())
              AND use_count < max_uses
            "#,
        )
        .bind(code_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(row.map(InviteId))
    }
}

#[async_trait]
impl InviteAdminRepository for PgInviteRepository {
    async fn create_invite(&self, new_invite: NewInvite) -> Result<InviteId, AuthError> {
        Self::create_invite(self, new_invite).await
    }

    async fn list_invites(&self) -> Result<Vec<InviteSummary>, AuthError> {
        Self::list_invites(self).await
    }

    async fn disable_invite(&self, id: InviteId) -> Result<bool, AuthError> {
        Self::disable_invite(self, id).await
    }

    async fn delete_invite(&self, id: InviteId) -> Result<bool, AuthError> {
        Self::delete_invite(self, id).await
    }

    async fn invite_code_ciphertext(&self, id: InviteId) -> Result<Option<String>, AuthError> {
        Self::invite_code_ciphertext(self, id).await
    }
}

/// Atomically consume one use of `invite_id` within an in-flight provisioning
/// transaction. A single conditional `UPDATE ... RETURNING` — no
/// check-then-increment — so two concurrent redemptions of a `max_uses = 1`
/// invite serialize on the row lock and exactly one succeeds; the other sees
/// zero rows and gets `AuthError::InviteRejected`, which rolls the whole
/// provisioning back.
pub(crate) async fn consume_invite_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    invite_id: InviteId,
) -> Result<(), AuthError> {
    let consumed = sqlx::query_scalar::<_, Uuid>(
        r#"
        UPDATE invite_codes
           SET use_count = use_count + 1
         WHERE id = $1
           AND disabled_at IS NULL
           AND (expires_at IS NULL OR expires_at > now())
           AND use_count < max_uses
        RETURNING id
        "#,
    )
    .bind(invite_id.0)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_sqlx_error)?;

    match consumed {
        Some(_) => Ok(()),
        None => Err(AuthError::InviteRejected),
    }
}

#[derive(sqlx::FromRow)]
struct InviteRow {
    id: Uuid,
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    disabled_at: Option<DateTime<Utc>>,
    max_uses: i32,
    use_count: i32,
    note: Option<String>,
    revealable: bool,
}

impl InviteRow {
    fn into_summary(self) -> InviteSummary {
        InviteSummary {
            id: InviteId(self.id),
            created_at: self.created_at,
            expires_at: self.expires_at,
            disabled_at: self.disabled_at,
            max_uses: self.max_uses,
            use_count: self.use_count,
            note: self.note,
            revealable: self.revealable,
        }
    }
}

fn map_sqlx_error(error: sqlx::Error) -> AuthError {
    AuthError::Persistence(error.to_string())
}
