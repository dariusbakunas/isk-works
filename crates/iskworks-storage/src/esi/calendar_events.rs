//! In-game calendar events per connection (the `calendar` character source).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_core::calendar_event::{
    CalendarEventRepository, CalendarEventResponse, CharacterCalendarEvent, WorkspaceCalendarEvent,
};
use iskworks_core::{ConnectedCharacterId, InventoryError, WorkspaceId};
use uuid::Uuid;

use super::{map_sqlx, PgEsiRepository};

const EVENT_COLUMNS: &str = "e.connection_id, e.event_id, e.event_date, e.title, e.importance, \
     e.response, e.duration_minutes, e.owner_id, e.owner_name, e.owner_type, e.text, e.observed_at";

#[derive(sqlx::FromRow)]
struct EventRow {
    connection_id: Uuid,
    event_id: i64,
    event_date: DateTime<Utc>,
    title: String,
    importance: i64,
    response: String,
    duration_minutes: i64,
    owner_id: Option<i64>,
    owner_name: Option<String>,
    owner_type: Option<String>,
    text: String,
    observed_at: DateTime<Utc>,
}

impl EventRow {
    fn into_domain(self) -> Result<CharacterCalendarEvent, InventoryError> {
        let response = CalendarEventResponse::from_db_str(&self.response).ok_or_else(|| {
            InventoryError::Persistence(format!("unknown calendar response {}", self.response))
        })?;
        Ok(CharacterCalendarEvent {
            connection_id: ConnectedCharacterId(self.connection_id),
            event_id: self.event_id,
            event_date: self.event_date,
            title: self.title,
            importance: self.importance,
            response,
            duration_minutes: self.duration_minutes,
            owner_id: self.owner_id,
            owner_name: self.owner_name,
            owner_type: self.owner_type,
            text: self.text,
            observed_at: self.observed_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct WorkspaceEventRow {
    #[sqlx(flatten)]
    event: EventRow,
    eve_character_id: i64,
    character_name: String,
}

#[async_trait]
impl CalendarEventRepository for PgEsiRepository {
    async fn calendar_events(
        &self,
        connection_id: ConnectedCharacterId,
    ) -> Result<Vec<CharacterCalendarEvent>, InventoryError> {
        sqlx::query_as::<_, EventRow>(&format!(
            "SELECT {EVENT_COLUMNS} FROM character_calendar_events e
             WHERE e.connection_id = $1 ORDER BY e.event_date, e.event_id"
        ))
        .bind(connection_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        .into_iter()
        .map(EventRow::into_domain)
        .collect()
    }

    async fn workspace_calendar_event_copies(
        &self,
        workspace_id: WorkspaceId,
        event_ids: &[i64],
    ) -> Result<Vec<CharacterCalendarEvent>, InventoryError> {
        sqlx::query_as::<_, EventRow>(&format!(
            "SELECT {EVENT_COLUMNS} FROM character_calendar_events e
             JOIN eve_connections c ON c.id = e.connection_id
             WHERE c.workspace_id = $1 AND e.event_id = ANY($2)
             ORDER BY e.event_id, e.observed_at DESC"
        ))
        .bind(workspace_id.0)
        .bind(event_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        .into_iter()
        .map(EventRow::into_domain)
        .collect()
    }

    async fn replace_calendar_events(
        &self,
        connection_id: ConnectedCharacterId,
        events: &[CharacterCalendarEvent],
    ) -> Result<(), InventoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        let seen: Vec<i64> = events.iter().map(|event| event.event_id).collect();
        sqlx::query(
            "DELETE FROM character_calendar_events
             WHERE connection_id = $1 AND event_id <> ALL($2)",
        )
        .bind(connection_id.0)
        .bind(&seen)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        for event in events {
            sqlx::query(
                r#"INSERT INTO character_calendar_events (
                     connection_id, event_id, event_date, title, importance, response,
                     duration_minutes, owner_id, owner_name, owner_type, text, observed_at
                   ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
                   ON CONFLICT (connection_id, event_id) DO UPDATE SET
                     event_date = EXCLUDED.event_date,
                     title = EXCLUDED.title,
                     importance = EXCLUDED.importance,
                     response = EXCLUDED.response,
                     duration_minutes = EXCLUDED.duration_minutes,
                     owner_id = EXCLUDED.owner_id,
                     owner_name = EXCLUDED.owner_name,
                     owner_type = EXCLUDED.owner_type,
                     text = EXCLUDED.text,
                     observed_at = EXCLUDED.observed_at"#,
            )
            .bind(connection_id.0)
            .bind(event.event_id)
            .bind(event.event_date)
            .bind(&event.title)
            .bind(event.importance)
            .bind(event.response.as_db_str())
            .bind(event.duration_minutes)
            .bind(event.owner_id)
            .bind(&event.owner_name)
            .bind(&event.owner_type)
            .bind(&event.text)
            .bind(event.observed_at)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;
        }
        tx.commit().await.map_err(map_sqlx)
    }

    async fn workspace_calendar_events(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<WorkspaceCalendarEvent>, InventoryError> {
        sqlx::query_as::<_, WorkspaceEventRow>(&format!(
            "SELECT {EVENT_COLUMNS}, c.eve_character_id, c.character_name
             FROM character_calendar_events e
             JOIN eve_connections c ON c.id = e.connection_id
             WHERE c.workspace_id = $1 AND c.disconnected_at IS NULL
             ORDER BY e.event_date, e.event_id, c.character_name"
        ))
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        .into_iter()
        .map(|row| {
            Ok(WorkspaceCalendarEvent {
                event: row.event.into_domain()?,
                eve_character_id: row.eve_character_id,
                character_name: row.character_name,
            })
        })
        .collect()
    }
}
