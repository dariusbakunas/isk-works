use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use iskworks_app::{CalendarMilestone, CalendarRange};
use iskworks_core::InventoryError;
use serde::Deserialize;

use crate::{workspace_context, ApiError, AppState};

#[derive(Debug, Deserialize)]
struct CalendarQuery {
    from: String,
    to: String,
}

pub(crate) fn router() -> Router<AppState> {
    Router::new().route("/api/calendar", get(get_calendar))
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
            ApiError::Inventory(InventoryError::Validation(format!(
                "{field} must be an RFC3339 timestamp"
            )))
        })
    };
    let from = parse(&query.from, "from")?;
    let to = parse(&query.to, "to")?;
    CalendarRange::new(from, to).map_err(|_| {
        ApiError::Inventory(InventoryError::Validation(
            "from must be before to".to_string(),
        ))
    })
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
