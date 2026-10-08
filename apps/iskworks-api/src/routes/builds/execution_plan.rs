//! HTTP glue for the Execution Plan projection route.
//!
//! `POST /api/builds/:build_id/execution-plan` -- POST, not GET, because the
//! execution plan must reflect the editor's current *unsaved* planning
//! overlay, which the client sends in the body -- the same contract
//! `/materials` and `/graph` already use. The persisted Build is only the
//! root's identity; its stored planning state is not consulted for
//! topology.
//!
//! The handler does request extraction, `workspace_context`, one
//! `ExecutionPlanCoordinator` call, and `Json` wrapping. All orchestration
//! lives in `iskworks_app::ExecutionPlanCoordinator` (reuses
//! `BuildMaterialsCoordinator` as-is, never a second planning walk);
//! `ExecutionPlanError` fans back to `ApiError` via `crate::error`.
//!
//! Read-only despite POST: no Build mutation, no inventory mutation, no
//! reservation, no Epic creation, no ticket creation. An ordinary incomplete
//! cost projection (a missing price, a missing facility) is never an HTTP
//! failure -- the response succeeds with `complete: false` and typed
//! `warnings`; only a genuine `BuildMaterialsError` (the same conditions
//! `/materials`/`/graph` already treat as failures) becomes one.

use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use iskworks_core::execution_plan::ExecutionPlanProjection;
use iskworks_core::{BuildId, PreviewBuildPlanCommand};

use crate::{workspace_context, ApiError, AppState};

/// The request body is the existing live-planning DTO
/// [`PreviewBuildPlanCommand`] -- the same overlay the build-plan preview,
/// `/materials`, and `/graph` take. The path `:build_id` is authoritative
/// for which Build is projected; the coordinator overwrites
/// `command.build_id` with it. This route never mutates the Build,
/// inventory, or orders.
async fn build_execution_plan(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(command): Json<PreviewBuildPlanCommand>,
) -> Result<Json<ExecutionPlanProjection>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let coordinator = state.execution_plan_coordinator()?;
    Ok(Json(
        coordinator
            .execution_plan(workspace_id, owner_id, BuildId(build_id), command)
            .await?,
    ))
}

pub(super) fn router() -> Router<AppState> {
    Router::new().route(
        "/api/builds/:build_id/execution-plan",
        post(build_execution_plan),
    )
}
