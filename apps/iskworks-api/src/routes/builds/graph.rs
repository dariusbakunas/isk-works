//! HTTP glue for the Build Graph projection route.
//!
//! `POST /api/builds/:build_id/graph` -- POST, not GET, because the graph
//! must reflect the editor's current *unsaved* planning overlay, which the
//! client sends in the body. The persisted Build is only the root's
//! identity; its stored planning state is not consulted for topology.
//!
//! The handler does request extraction, `workspace_context`, one
//! `BuildGraphCoordinator` call, and `Json` wrapping. All hierarchy
//! orchestration lives in `iskworks_app::BuildGraphCoordinator` (it
//! wraps `BuildMaterialsCoordinator` and `iskworks_core::build_graph::
//! project_live_build_graph`, never `IndustryService::build_graph`);
//! `BuildGraphError` fans back to `ApiError` via `crate::error`.

use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use iskworks_core::build_graph::BuildGraphProjection;
use iskworks_core::{BuildId, PreviewBuildPlanCommand};

use crate::{workspace_context, ApiError, AppState};

/// The request body is the existing live-planning DTO
/// [`PreviewBuildPlanCommand`] -- the same overlay the build-plan preview
/// takes. The path `:build_id` is authoritative for which Build is graphed;
/// the coordinator overwrites `command.build_id` with it. `/graph` never
/// mutates the Build.
async fn build_graph(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(command): Json<PreviewBuildPlanCommand>,
) -> Result<Json<BuildGraphProjection>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let coordinator = state.build_graph_coordinator()?;
    Ok(Json(
        coordinator
            .graph(workspace_id, owner_id, BuildId(build_id), command)
            .await?,
    ))
}

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/api/builds/:build_id/graph", post(build_graph))
}
