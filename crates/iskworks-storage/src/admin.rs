use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_core::{AdminUserSummary, AdminUsersRepository, AuthError, UserId};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct PgAdminRepository {
    pool: PgPool,
}

impl PgAdminRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const USER_SUMMARY_SQL: &str = r#"
    SELECT u.id,
           u.eve_character_id,
           u.eve_character_name,
           u.created_at,
           u.last_login_at,
           u.disabled_at,
           COALESCE(c.total, 0) AS character_count,
           COALESCE(c.attention, 0) AS characters_needing_attention,
           COALESCE(s.active, 0) AS active_session_count
    FROM users u
    LEFT JOIN (
        SELECT workspace_id,
               COUNT(*) AS total,
               COUNT(*) FILTER (WHERE status <> 'connected') AS attention
        FROM eve_connections
        WHERE disconnected_at IS NULL
        GROUP BY workspace_id
    ) c ON c.workspace_id = u.workspace_id
    LEFT JOIN (
        SELECT user_id, COUNT(*) AS active
        FROM sessions
        WHERE expires_at > now()
        GROUP BY user_id
    ) s ON s.user_id = u.id
"#;

#[derive(sqlx::FromRow)]
struct AdminUserRow {
    id: Uuid,
    eve_character_id: i64,
    eve_character_name: String,
    created_at: DateTime<Utc>,
    last_login_at: DateTime<Utc>,
    disabled_at: Option<DateTime<Utc>>,
    character_count: i64,
    characters_needing_attention: i64,
    active_session_count: i64,
}

impl AdminUserRow {
    fn into_summary(self) -> AdminUserSummary {
        AdminUserSummary {
            user_id: UserId(self.id),
            eve_character_id: self.eve_character_id,
            eve_character_name: self.eve_character_name,
            created_at: self.created_at,
            last_login_at: self.last_login_at,
            character_count: self.character_count,
            characters_needing_attention: self.characters_needing_attention,
            active_session_count: self.active_session_count,
            disabled_at: self.disabled_at,
        }
    }
}

const ORPHANED_WORKSPACES_IDS_SQL: &str = r#"
    SELECT w.id FROM workspaces w
     WHERE w.claimed_at IS NOT NULL
       AND NOT EXISTS (SELECT 1 FROM users u WHERE u.workspace_id = w.id)
"#;

const ORPHANED_WORKSPACES_COUNT_SQL: &str = r#"
    SELECT COUNT(*) FROM workspaces w
     WHERE w.claimed_at IS NOT NULL
       AND NOT EXISTS (SELECT 1 FROM users u WHERE u.workspace_id = w.id)
"#;

fn map_error(error: sqlx::Error) -> AuthError {
    AuthError::Persistence(error.to_string())
}

#[async_trait]
impl AdminUsersRepository for PgAdminRepository {
    async fn list_users(&self) -> Result<Vec<AdminUserSummary>, AuthError> {
        let rows = sqlx::query_as::<_, AdminUserRow>(&format!(
            "{USER_SUMMARY_SQL} ORDER BY u.last_login_at DESC, u.created_at DESC"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?;
        Ok(rows.into_iter().map(AdminUserRow::into_summary).collect())
    }

    async fn find_user(&self, id: UserId) -> Result<Option<AdminUserSummary>, AuthError> {
        let row = sqlx::query_as::<_, AdminUserRow>(&format!("{USER_SUMMARY_SQL} WHERE u.id = $1"))
            .bind(id.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_error)?;
        Ok(row.map(AdminUserRow::into_summary))
    }

    async fn set_user_disabled(&self, id: UserId, disabled: bool) -> Result<bool, AuthError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let updated = sqlx::query(
            r#"
            UPDATE users
               SET disabled_at = CASE WHEN $2 THEN COALESCE(disabled_at, now()) END
             WHERE id = $1
            "#,
        )
        .bind(id.0)
        .bind(disabled)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?
        .rows_affected();
        if updated == 0 {
            return Ok(false);
        }
        if disabled {
            sqlx::query("DELETE FROM sessions WHERE user_id = $1")
                .bind(id.0)
                .execute(&mut *tx)
                .await
                .map_err(map_error)?;
        }
        tx.commit().await.map_err(map_error)?;
        Ok(true)
    }

    async fn delete_user(&self, id: UserId) -> Result<bool, AuthError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let workspace_id: Option<Uuid> =
            sqlx::query_scalar("SELECT workspace_id FROM users WHERE id = $1 FOR UPDATE")
                .bind(id.0)
                .fetch_optional(&mut *tx)
                .await
                .map_err(map_error)?;
        let Some(workspace_id) = workspace_id else {
            return Ok(false);
        };
        // Drop the sessions first so nothing can resolve mid-erase.
        sqlx::query("DELETE FROM sessions WHERE user_id = $1")
            .bind(id.0)
            .execute(&mut *tx)
            .await
            .map_err(map_error)?;
        crate::erase::erase_workspace_in_tx(&mut tx, workspace_id)
            .await
            .map_err(map_error)?;
        tx.commit().await.map_err(map_error)?;
        Ok(true)
    }

    async fn count_orphaned_workspaces(&self) -> Result<i64, AuthError> {
        sqlx::query_scalar(ORPHANED_WORKSPACES_COUNT_SQL)
            .fetch_one(&self.pool)
            .await
            .map_err(map_error)
    }

    async fn erase_orphaned_workspaces(&self) -> Result<i64, AuthError> {
        let ids: Vec<Uuid> = sqlx::query_scalar(ORPHANED_WORKSPACES_IDS_SQL)
            .fetch_all(&self.pool)
            .await
            .map_err(map_error)?;
        let mut erased = 0;
        for workspace_id in ids {
            let mut tx = self.pool.begin().await.map_err(map_error)?;
            // Re-check inside the transaction: a user could have claimed it
            // since the id list was read.
            let still_orphaned: Option<Uuid> = sqlx::query_scalar(&format!(
                "{ORPHANED_WORKSPACES_IDS_SQL} AND w.id = $1 FOR UPDATE OF w"
            ))
            .bind(workspace_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_error)?;
            if still_orphaned.is_none() {
                continue;
            }
            if crate::erase::erase_workspace_in_tx(&mut tx, workspace_id)
                .await
                .map_err(map_error)?
            {
                erased += 1;
            }
            tx.commit().await.map_err(map_error)?;
        }
        Ok(erased)
    }
}
