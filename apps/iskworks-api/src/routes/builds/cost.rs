//! HTTP glue for the allocation-aware production-cost projection.
//!
//! `POST /api/builds/:build_id/cost-projection` -- POST, not GET, because the
//! projection must reflect the editor's current *unsaved* planning overlay
//! (the same [`PreviewBuildPlanCommand`] the build-plan preview, Build Graph,
//! and Materials view take).
//!
//! This endpoint exposes `BuildCostProjection` directly so it can be
//! inspected standalone. `POST /build-plans/preview` and
//! `POST /build-plans/candidate-preview` are powered by the same projection
//! (`BuildMaterialsCoordinator::preview_plan_with_planning_cost`
//! / `candidate_preview_with_planning_cost`, both built on
//! `build_cost_projection_for_overlay` -- the same code this route calls),
//! but this route is the only place the **full** `BuildCostProjection`
//! reaches the API: `boundaries`
//! (per-type inventory/fresh/child cost split, in traversal order) and the
//! full per-operation `warnings` list are not carried by `BuildPlanRevision`
//! or `CreateBuildPlanPreview`, whose `material_lines` only get the
//! summarized `planning_evidence`. Useful for QA/debugging
//! against the merged response, and as the natural home for a future
//! Worksheet "cost breakdown" detail view without a new endpoint. The handler
//! is fully read-only: one `BuildMaterialsCoordinator::build_cost_projection`
//! call, which performs one `list_balances`, one bulk adjusted-price
//! resolution, and a pure bottom-up cost pass -- no Build / inventory / order
//! mutation.

use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use iskworks_core::build_cost::BuildCostProjection;
use iskworks_core::{BuildId, PreviewBuildPlanCommand};

use crate::{workspace_context, ApiError, AppState};

async fn build_cost_projection(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(command): Json<PreviewBuildPlanCommand>,
) -> Result<Json<BuildCostProjection>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let coordinator = state.build_materials_coordinator()?;
    Ok(Json(
        coordinator
            .build_cost_projection(workspace_id, owner_id, BuildId(build_id), command)
            .await?,
    ))
}

pub(super) fn router() -> Router<AppState> {
    Router::new().route(
        "/api/builds/:build_id/cost-projection",
        post(build_cost_projection),
    )
}
