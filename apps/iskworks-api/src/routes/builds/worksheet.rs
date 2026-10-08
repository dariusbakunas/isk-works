//! HTTP adapter for the read-only saved-Build Worksheet projection.

use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use iskworks_core::build_worksheet::BuildWorksheetProjection;
use iskworks_core::{BuildId, PreviewBuildPlanCommand};
use serde::Deserialize;

use crate::{workspace_context, ApiError, AppState};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildWorksheetRequest {
    command: PreviewBuildPlanCommand,
    #[serde(default)]
    focused_producer_id: Option<BuildId>,
    /// Defaults to direct materials only; `true` adds downstream dependencies.
    #[serde(default)]
    include_downstream: bool,
}

async fn build_worksheet(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(request): Json<BuildWorksheetRequest>,
) -> Result<Json<BuildWorksheetProjection>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let coordinator = state.build_worksheet_coordinator()?;
    Ok(Json(
        coordinator
            .worksheet(
                workspace_id,
                owner_id,
                BuildId(build_id),
                request.command,
                request.focused_producer_id,
                request.include_downstream,
            )
            .await?,
    ))
}

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/api/builds/:build_id/worksheet", post(build_worksheet))
}
