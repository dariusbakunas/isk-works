//! The Jobs page: running industry jobs across the workspace's characters,
//! and the finished-job history ISK Works has stored.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{Duration, NaiveDate, Utc};
use iskworks_app::{ActiveJobs, JobHistoryView};
use iskworks_core::InventoryError;
use serde::Deserialize;

use crate::{workspace_context, ApiError, AppState};

const DEFAULT_HISTORY_DAYS: i64 = 30;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/jobs", get(get_active_jobs))
        .route("/api/jobs/history", get(get_job_history))
}

async fn get_active_jobs(State(state): State<AppState>) -> Result<Json<ActiveJobs>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .jobs_service()?
            .active(workspace_id, Utc::now())
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
struct HistoryParams {
    /// Inclusive `YYYY-MM-DD`; defaults to 30 days ending `to`.
    from: Option<String>,
    /// Inclusive `YYYY-MM-DD`; defaults to today (UTC).
    to: Option<String>,
}

fn parse_date(value: Option<&str>, name: &str) -> Result<Option<NaiveDate>, InventoryError> {
    value
        .filter(|value| !value.is_empty())
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| InventoryError::Validation(format!("{name} must be YYYY-MM-DD")))
        })
        .transpose()
}

async fn get_job_history(
    State(state): State<AppState>,
    Query(params): Query<HistoryParams>,
) -> Result<Json<JobHistoryView>, ApiError> {
    let now = Utc::now();
    let to = parse_date(params.to.as_deref(), "to")?.unwrap_or_else(|| now.date_naive());
    let from = parse_date(params.from.as_deref(), "from")?
        .unwrap_or(to - Duration::days(DEFAULT_HISTORY_DAYS - 1));
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .jobs_service()?
            .history(workspace_id, from, to, now)
            .await?,
    ))
}
