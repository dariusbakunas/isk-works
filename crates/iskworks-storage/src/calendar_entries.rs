//! Custom Calendar entries, per workspace.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_core::calendar_entry::{
    foreign_character, CalendarEntry, CalendarEntryCharacter, CalendarEntryInput,
    CalendarEntryRepository,
};
use iskworks_core::{ConnectedCharacterId, StoreError, WorkspaceId};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct PgCalendarEntryRepository {
    pool: PgPool,
}

impl PgCalendarEntryRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn check_character(
        &self,
        workspace_id: WorkspaceId,
        connection_id: Option<ConnectedCharacterId>,
    ) -> Result<(), StoreError> {
        let Some(connection_id) = connection_id else {
            return Ok(());
        };
        let found: Option<i32> =
            sqlx::query_scalar("SELECT 1 FROM eve_connections WHERE id = $1 AND workspace_id = $2")
                .bind(connection_id.0)
                .bind(workspace_id.0)
                .fetch_optional(&self.pool)
                .await
                .map_err(persistence)?;
        found.map(|_| ()).ok_or_else(foreign_character)
    }

    async fn entry(
        &self,
        workspace_id: WorkspaceId,
        id: Uuid,
    ) -> Result<CalendarEntry, StoreError> {
        let row: Option<EntryRow> = sqlx::query_as(&format!(
            "SELECT {ENTRY_COLUMNS} {ENTRY_FROM} WHERE e.workspace_id = $1 AND e.id = $2"
        ))
        .bind(workspace_id.0)
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(persistence)?;
        row.map(Into::into).ok_or(StoreError::NotFound)
    }
}

fn persistence(error: sqlx::Error) -> StoreError {
    StoreError::Persistence(error.to_string())
}

const ENTRY_COLUMNS: &str = "e.id, e.occurs_at, e.time_zone, e.title, e.description,
    e.created_at, e.updated_at, c.id AS connection_id, c.eve_character_id, c.character_name";
const ENTRY_FROM: &str = "FROM calendar_custom_entries e
    LEFT JOIN eve_connections c ON c.id = e.connection_id";

#[derive(sqlx::FromRow)]
struct EntryRow {
    id: Uuid,
    occurs_at: DateTime<Utc>,
    time_zone: String,
    title: String,
    description: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    connection_id: Option<Uuid>,
    eve_character_id: Option<i64>,
    character_name: Option<String>,
}

impl From<EntryRow> for CalendarEntry {
    fn from(row: EntryRow) -> Self {
        let character = match (row.connection_id, row.eve_character_id, row.character_name) {
            (Some(connection_id), Some(eve_character_id), Some(character_name)) => {
                Some(CalendarEntryCharacter {
                    connection_id: ConnectedCharacterId(connection_id),
                    eve_character_id,
                    character_name,
                })
            }
            _ => None,
        };
        Self {
            id: row.id,
            occurs_at: row.occurs_at,
            time_zone: row.time_zone,
            title: row.title,
            description: row.description,
            character,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[async_trait]
impl CalendarEntryRepository for PgCalendarEntryRepository {
    async fn calendar_entries(
        &self,
        workspace_id: WorkspaceId,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<CalendarEntry>, StoreError> {
        let rows: Vec<EntryRow> = sqlx::query_as(&format!(
            "SELECT {ENTRY_COLUMNS} {ENTRY_FROM}
             WHERE e.workspace_id = $1 AND e.occurs_at >= $2 AND e.occurs_at < $3
             ORDER BY e.occurs_at, e.id"
        ))
        .bind(workspace_id.0)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(persistence)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn create_calendar_entry(
        &self,
        workspace_id: WorkspaceId,
        input: CalendarEntryInput,
    ) -> Result<CalendarEntry, StoreError> {
        self.check_character(workspace_id, input.connection_id)
            .await?;
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO calendar_custom_entries
                 (workspace_id, connection_id, occurs_at, time_zone, title, description)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING id",
        )
        .bind(workspace_id.0)
        .bind(input.connection_id.map(|id| id.0))
        .bind(input.occurs_at)
        .bind(&input.time_zone)
        .bind(&input.title)
        .bind(&input.description)
        .fetch_one(&self.pool)
        .await
        .map_err(persistence)?;
        self.entry(workspace_id, id).await
    }

    async fn update_calendar_entry(
        &self,
        workspace_id: WorkspaceId,
        id: Uuid,
        input: CalendarEntryInput,
    ) -> Result<CalendarEntry, StoreError> {
        self.check_character(workspace_id, input.connection_id)
            .await?;
        let updated = sqlx::query(
            "UPDATE calendar_custom_entries
             SET connection_id = $3, occurs_at = $4, time_zone = $5, title = $6,
                 description = $7, updated_at = clock_timestamp()
             WHERE workspace_id = $1 AND id = $2",
        )
        .bind(workspace_id.0)
        .bind(id)
        .bind(input.connection_id.map(|id| id.0))
        .bind(input.occurs_at)
        .bind(&input.time_zone)
        .bind(&input.title)
        .bind(&input.description)
        .execute(&self.pool)
        .await
        .map_err(persistence)?
        .rows_affected();
        if updated == 0 {
            return Err(StoreError::NotFound);
        }
        self.entry(workspace_id, id).await
    }

    async fn delete_calendar_entry(
        &self,
        workspace_id: WorkspaceId,
        id: Uuid,
    ) -> Result<(), StoreError> {
        let deleted =
            sqlx::query("DELETE FROM calendar_custom_entries WHERE workspace_id = $1 AND id = $2")
                .bind(workspace_id.0)
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(persistence)?
                .rows_affected();
        if deleted == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}
