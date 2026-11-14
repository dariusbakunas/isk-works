//! Planetary › Plans: PI plans saved as snapshots, and their set-up
//! checklist.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use iskworks_app::{SavePlanRequestDto, SavedPlanDto, SavedPlanSummaryDto, SavedPlanUpdateDto};
use uuid::Uuid;

use crate::{workspace_context, ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/planetary/plans", get(list).post(save))
        .route(
            "/api/planetary/plans/:id",
            get(open).patch(update).delete(delete),
        )
}

async fn list(State(state): State<AppState>) -> Result<Json<Vec<SavedPlanSummaryDto>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .pi_saved_plans_service()?
            .saved_plans(workspace_id)
            .await?,
    ))
}

async fn save(
    State(state): State<AppState>,
    Json(request): Json<SavePlanRequestDto>,
) -> Result<(StatusCode, Json<SavedPlanDto>), ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let saved = state
        .pi_saved_plans_service()?
        .save(workspace_id, request)
        .await?;
    Ok((StatusCode::CREATED, Json(saved)))
}

async fn open(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SavedPlanDto>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .pi_saved_plans_service()?
            .saved_plan(workspace_id, id)
            .await?,
    ))
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(update): Json<SavedPlanUpdateDto>,
) -> Result<Json<SavedPlanDto>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .pi_saved_plans_service()?
            .update(workspace_id, id, update)
            .await?,
    ))
}

async fn delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    state
        .pi_saved_plans_service()?
        .delete(workspace_id, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
