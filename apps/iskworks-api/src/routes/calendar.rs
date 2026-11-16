use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use iskworks_app::{CalendarMilestone, CalendarRange};
use iskworks_core::calendar_entry::CalendarEntryInput;
use iskworks_core::{ConnectedCharacterId, StoreError};
use serde::Deserialize;
use uuid::Uuid;

use crate::{workspace_context, ApiError, AppState};

#[derive(Debug, Deserialize)]
struct CalendarQuery {
    from: String,
    to: String,
}

/// A custom entry as the web sends it; `occursAt` is the UTC instant the
/// client computed from the wall-clock time in `timeZone`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalendarEntryRequest {
    occurs_at: DateTime<Utc>,
    time_zone: String,
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    connection_id: Option<Uuid>,
}

impl From<CalendarEntryRequest> for CalendarEntryInput {
    fn from(request: CalendarEntryRequest) -> Self {
        Self {
            occurs_at: request.occurs_at,
            time_zone: request.time_zone,
            title: request.title,
            description: request.description.unwrap_or_default(),
            connection_id: request.connection_id.map(ConnectedCharacterId),
        }
    }
}

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/calendar", get(get_calendar))
        .route("/api/calendar/entries", post(create_entry))
        .route(
            "/api/calendar/entries/:id",
            put(update_entry).delete(delete_entry),
        )
}

async fn create_entry(
    State(state): State<AppState>,
    Json(request): Json<CalendarEntryRequest>,
) -> Result<(StatusCode, Json<CalendarMilestone>), ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let entry = state
        .calendar_entries_service()?
        .create(workspace_id, request.into())
        .await?;
    Ok((StatusCode::CREATED, Json(entry)))
}

async fn update_entry(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<CalendarEntryRequest>,
) -> Result<Json<CalendarMilestone>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .calendar_entries_service()?
            .update(workspace_id, id, request.into())
            .await?,
    ))
}

async fn delete_entry(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    state
        .calendar_entries_service()?
        .delete(workspace_id, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_calendar(
    State(state): State<AppState>,
    Query(query): Query<CalendarQuery>,
) -> Result<Json<Vec<CalendarMilestone>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let range = parse_range(query)?;
    Ok(Json(
        state.calendar_service()?.range(workspace_id, range).await?,
    ))
}

fn parse_range(query: CalendarQuery) -> Result<CalendarRange, ApiError> {
    let parse = |value: &str, field: &str| {
        value.parse::<DateTime<Utc>>().map_err(|_| {
            ApiError::from(StoreError::Validation(format!(
                "{field} must be an RFC3339 timestamp"
            )))
        })
    };
    let from = parse(&query.from, "from")?;
    let to = parse(&query.to, "to")?;
    CalendarRange::new(from, to)
        .map_err(|_| ApiError::from(StoreError::Validation("from must be before to".to_string())))
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::{parse_range, CalendarQuery};

    #[test]
    fn parses_absolute_rfc3339_query_into_a_half_open_range() {
        let result = parse_range(CalendarQuery {
            from: "2026-10-01T00:00:00Z".to_string(),
            to: "2026-11-01T00:00:00Z".to_string(),
        });
        let Ok(range) = result else {
            panic!("valid RFC3339 range should parse");
        };

        assert_eq!(
            range.from(),
            Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).single().unwrap()
        );
        assert_eq!(
            range.to(),
            Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).single().unwrap()
        );
    }

    #[test]
    fn rejects_malformed_or_non_increasing_ranges() {
        assert!(parse_range(CalendarQuery {
            from: "not-a-date".to_string(),
            to: "2026-11-01T00:00:00Z".to_string(),
        })
        .is_err());
        assert!(parse_range(CalendarQuery {
            from: "2026-11-01T00:00:00Z".to_string(),
            to: "2026-10-01T00:00:00Z".to_string(),
        })
        .is_err());
    }
}
