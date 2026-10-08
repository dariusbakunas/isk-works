//! HTTP glue for the build-plan preview routes.
//!
//! `preview_build_plan` and `preview_create_build_plan_candidate` do only
//! request extraction, `workspace_context`, and one
//! `BuildPreviewCoordinator` call (the orchestration lives in
//! `iskworks_app::build_preview`; `BuildPreviewError` is fanned back to
//! `ApiError` by `crate::error`). `preview_component_expansion` and
//! `get_automatic_eiv` are standalone reads that share no orchestration and
//! stay route-level.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use iskworks_app::AutomaticEiv;
use iskworks_core::{
    BuildId, ComponentExpansion, PreviewBuildPlanCommand, PreviewComponentExpansionCommand,
};
use serde::Deserialize;

use crate::{workspace_context, ApiError, AppState};

async fn preview_build_plan(
    State(state): State<AppState>,
    Json(command): Json<PreviewBuildPlanCommand>,
) -> Result<Json<iskworks_core::BuildPlanRevision>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let coordinator = state.build_materials_coordinator()?;
    // The allocation-aware planning-cost model (`BuildCostProjection`)
    // merged onto the revision -- see
    // `BuildMaterialsCoordinator::preview_plan_with_planning_cost`. The
    // `BuildCostProjection` itself is discarded here (the response DTO is
    // `BuildPlanRevision`); `POST /builds/:id/cost-projection`
    // exposes it directly when the full evidence is needed.
    let (revision, _planning) = coordinator
        .preview_plan_with_planning_cost(workspace_id, owner_id, command)
        .await?;
    // `_planning` is `None` only for a brand-new, never-saved Build (no
    // allocation-aware tree to walk yet -- see
    // `BuildPreviewCoordinator::preview_unsaved_plan`'s doc comment).
    Ok(Json(revision))
}

async fn preview_component_expansion(
    State(state): State<AppState>,
    Json(command): Json<PreviewComponentExpansionCommand>,
) -> Result<Json<ComponentExpansion>, ApiError> {
    Ok(Json(
        state
            .component_expansion_service()
            .expand(
                command.root,
                command.runs,
                &command.resolutions,
                // This standalone preview has no IndustryRepository, so it
                // cannot resolve an ObservedAsset blueprint selection --
                // ME/TE is not reflected here, a deliberate limitation.
                &std::collections::BTreeMap::new(),
                // `PreviewComponentExpansionCommand` has no build_id and no
                // fulfillment_scopes -- this standalone preview always
                // behaves as `Full` scope, same deliberate limitation as
                // ME/TE above.
                &std::collections::BTreeMap::new(),
            )
            .await?,
    ))
}

async fn preview_create_build_plan_candidate(
    State(state): State<AppState>,
    Json(command): Json<PreviewBuildPlanCommand>,
) -> Result<Json<iskworks_core::CreateBuildPlanPreview>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let coordinator = state.build_materials_coordinator()?;
    Ok(Json(
        coordinator
            .candidate_preview_with_planning_cost(workspace_id, owner_id, command)
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
struct AutomaticEivQuery {
    runs: u64,
}

async fn get_automatic_eiv(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Query(query): Query<AutomaticEivQuery>,
) -> Result<Json<AutomaticEiv>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let build = state
        .industry_repository()?
        .get_build(workspace_id, BuildId(build_id))
        .await?;
    Ok(Json(
        state
            .esi_service()?
            .automatic_eiv(build.recipe.materials(), query.runs)
            .await?,
    ))
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/builds/:build_id/estimated-item-value",
            get(get_automatic_eiv),
        )
        .route("/api/build-plans/preview", post(preview_build_plan))
        .route(
            "/api/build-plans/component-expansion-preview",
            post(preview_component_expansion),
        )
        .route(
            "/api/build-plans/candidate-preview",
            post(preview_create_build_plan_candidate),
        )
}
