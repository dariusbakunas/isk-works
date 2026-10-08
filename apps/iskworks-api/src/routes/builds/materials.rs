//! HTTP glue for the Build Materials aggregate route (the whole-plan
//! inventory allocator).
//!
//! `POST /api/builds/:build_id/materials` -- POST, not GET, because the
//! aggregate must reflect the editor's current *unsaved* planning overlay,
//! which the client sends in the body (the same
//! [`PreviewBuildPlanCommand`] the build-plan preview and the Build Graph
//! take). The persisted Build is only the root's identity; its stored
//! planning state is not consulted for topology.
//!
//! The handler does request extraction, `workspace_context`, one
//! `BuildMaterialsCoordinator` call, and `Json` wrapping. All tree
//! construction / per-node revision folding / allocation lives in
//! `iskworks_app::BuildMaterialsCoordinator` ->
//! `IndustryService::project_build_tree_revisions` ->
//! `iskworks_core::build_materials::aggregate_build_materials`;
//! `BuildMaterialsError` fans back to `ApiError` via `crate::error`.

use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use iskworks_app::BuildMaterialsSummary;
use iskworks_core::{BuildId, PreviewBuildPlanCommand};

use crate::{workspace_context, ApiError, AppState};

/// The request body is the existing live-planning DTO
/// [`PreviewBuildPlanCommand`] -- the same overlay the build-plan preview and
/// the Build Graph take. The path `:build_id` is authoritative for which
/// Build is projected; the coordinator overwrites `command.build_id` with it.
/// This route never mutates the Build, inventory, or orders.
async fn build_materials(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(command): Json<PreviewBuildPlanCommand>,
) -> Result<Json<BuildMaterialsSummary>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let coordinator = state.build_materials_coordinator()?;
    Ok(Json(
        coordinator
            .materials(workspace_id, owner_id, BuildId(build_id), command, false)
            .await?,
    ))
}

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/api/builds/:build_id/materials", post(build_materials))
}
