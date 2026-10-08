//! Ordinary Build resource CRUD and library reads: list / open / create /
//! update / rename / delete a Build, producer create-or-reuse, blueprint
//! observations, and the production-coverage read. All plain routing over
//! the Industry / Production repositories and `IndustryService`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use iskworks_core::{
    Build, BuildBlueprintSettingsPatch, BuildFacilitySettingsPatch, BuildId,
    BuildPricingSettingsPatch, CreateBuildCommand, DescendantConfigurationMember,
    DescendantProductionConfigurationRequest, PreviewBuildPlanCommand, RenameBuildCommand,
    UpdateBuildCommand,
};
use serde::{Deserialize, Serialize};

use crate::{workspace_context, ApiError, AppState};

async fn list_builds(State(state): State<AppState>) -> Result<Json<Vec<Build>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .industry_repository()?
            .list_builds(workspace_id)
            .await?,
    ))
}

async fn create_build(
    State(state): State<AppState>,
    Json(command): Json<CreateBuildCommand>,
) -> Result<(StatusCode, Json<Build>), ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let build = state
        .industry_service()?
        .create_draft(workspace_id, owner_id, command)
        .await?;
    Ok((StatusCode::CREATED, Json(build)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlueprintObservationQuery {
    blueprint_type_id: i64,
}

async fn list_blueprint_observations(
    State(state): State<AppState>,
    Query(query): Query<BlueprintObservationQuery>,
) -> Result<Json<Vec<iskworks_core::BlueprintObservation>>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    Ok(Json(
        state
            .industry_repository()?
            .list_blueprint_observations(workspace_id, owner_id, query.blueprint_type_id)
            .await?,
    ))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildDetailResponse {
    #[serde(flatten)]
    build: Build,
    plan_root_build_id: Option<BuildId>,
    plan_root_build_name: Option<String>,
}

async fn get_build(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
) -> Result<Json<BuildDetailResponse>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let build = state
        .industry_repository()?
        .get_build(workspace_id, BuildId(build_id))
        .await?;
    let plan_root_build_id = state
        .industry_service()?
        .plan_root_of(workspace_id, build.id)
        .await?;
    let plan_root_build_name = match plan_root_build_id {
        Some(root) if root == build.id => Some(build.name.clone()),
        Some(root) => Some(
            state
                .industry_repository()?
                .get_build(workspace_id, root)
                .await?
                .name,
        ),
        None => None,
    };
    Ok(Json(BuildDetailResponse {
        build,
        plan_root_build_id,
        plan_root_build_name,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateLinkedBuildCommand {
    component_type_id: i64,
}

async fn create_linked_build(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(command): Json<CreateLinkedBuildCommand>,
) -> Result<Json<Build>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let build = state
        .industry_service()?
        .create_or_reuse_linked_build(
            workspace_id,
            owner_id,
            BuildId(build_id),
            command.component_type_id,
        )
        .await?;
    Ok(Json(build))
}

async fn update_build(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(command): Json<UpdateBuildCommand>,
) -> Result<Json<Build>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let build = state
        .industry_service()?
        .update_draft(workspace_id, BuildId(build_id), command)
        .await?;
    Ok(Json(build))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetComponentResolutionCommand {
    component_type_id: i64,
    recipe: iskworks_core::RecipeSelection,
    expected_revision: u64,
}

/// Build-ID-addressable sourcing mutation for the Graph: resolve one of
/// `build_id`'s direct components to BUILD (BUY -> BUILD on a nested
/// acquisition node targets the Build that *owns* the requirement, never
/// the root). Resolving the producer is a separate follow-up call to
/// `.../linked-builds`.
async fn set_component_resolution(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(command): Json<SetComponentResolutionCommand>,
) -> Result<Json<Build>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let build = state
        .industry_service()?
        .set_component_resolution(
            workspace_id,
            BuildId(build_id),
            command.component_type_id,
            command.recipe,
            command.expected_revision,
        )
        .await?;
    Ok(Json(build))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClearComponentResolutionQuery {
    expected_revision: u64,
}

/// BUILD -> BUY on one of `build_id`'s direct components. Only that demand
/// edge changes; the producer is kept (detached when nothing else uses it).
async fn clear_component_resolution(
    State(state): State<AppState>,
    Path((build_id, component_type_id)): Path<(uuid::Uuid, i64)>,
    Query(query): Query<ClearComponentResolutionQuery>,
) -> Result<Json<Build>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let build = state
        .industry_service()?
        .clear_component_resolution(
            workspace_id,
            BuildId(build_id),
            component_type_id,
            query.expected_revision,
        )
        .await?;
    Ok(Json(build))
}

/// Build-ID-addressable "common settings" mutations. Each patches exactly
/// one facet of `build_id`'s own `draft_planning.input` (blueprint selection,
/// the recipe-appropriate facility slot, or the pricing configuration),
/// reusing the worksheet editor's persistence + normalisation path.
/// The unified inspector uses these to edit a producer Build in place; the
/// root Build keeps going through the worksheet editor.
async fn set_build_blueprint_selection(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(patch): Json<BuildBlueprintSettingsPatch>,
) -> Result<Json<Build>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let build = state
        .industry_service()?
        .set_build_blueprint_selection(workspace_id, BuildId(build_id), patch)
        .await?;
    Ok(Json(build))
}

async fn set_build_facility(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(patch): Json<BuildFacilitySettingsPatch>,
) -> Result<Json<Build>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let build = state
        .industry_service()?
        .set_build_facility(workspace_id, BuildId(build_id), patch)
        .await?;
    Ok(Json(build))
}

async fn set_build_pricing(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(patch): Json<BuildPricingSettingsPatch>,
) -> Result<Json<Build>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let build = state
        .industry_service()?
        .set_build_pricing(workspace_id, BuildId(build_id), patch)
        .await?;
    Ok(Json(build))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DescendantProductionConfigurationBody {
    /// The live, unsaved planning overlay -- the same body shape
    /// `/materials`/`/graph`/`/execution-plan` already take. Re-derives the
    /// *current* plan immediately before mutating, so a stale inspector can
    /// never apply an edit to a membership set that no longer matches
    /// reality (see `DescendantProductionConfigurationCoordinator::update`'s
    /// own doc comment).
    command: PreviewBuildPlanCommand,
    /// The canonical descendant Builds the target production operation
    /// currently represents, each with the revision the client last
    /// observed. Every member must still belong to exactly one current
    /// operation, or the request is rejected as stale -- see
    /// `ApiError::DescendantOperationMembershipStale`.
    members: Vec<DescendantConfigurationMember>,
    #[serde(flatten)]
    request: DescendantProductionConfigurationRequest,
}

/// Stages inspector's atomic, multi-Build descendant-configuration edit
/// (see `crates/iskworks-app`'s `DescendantProductionConfigurationCoordinator`
/// for the full contract). `:build_id` is the root Build (path-authoritative,
/// mirrors `/execution-plan`) -- none of `members` may be the root itself;
/// root configuration stays owned by Worksheet's own `PUT /api/builds/:id`.
///
/// Applies one facility or blueprint/formula-selection patch to every
/// member atomically.
async fn update_descendant_production_configuration(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(body): Json<DescendantProductionConfigurationBody>,
) -> Result<Json<Vec<Build>>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let builds = state
        .descendant_production_configuration_coordinator()?
        .update(
            workspace_id,
            owner_id,
            BuildId(build_id),
            body.command,
            body.members,
            body.request,
        )
        .await?;
    Ok(Json(builds))
}

async fn rename_build(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(command): Json<RenameBuildCommand>,
) -> Result<Json<Build>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .industry_service()?
            .rename_build(workspace_id, BuildId(build_id), command)
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedRevisionRequest {
    expected_revision: u64,
    /// Retained for compatibility with older clients. It never authorizes
    /// deletion of Epics, Tickets, or inventory history.
    #[serde(default)]
    force: bool,
}

async fn delete_build(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Query(request): Query<ExpectedRevisionRequest>,
) -> Result<StatusCode, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    state
        .industry_repository()?
        .delete_build(
            workspace_id,
            BuildId(build_id),
            request.expected_revision,
            request.force,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_build_coverage(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
) -> Result<Json<iskworks_core::BuildCoverageReport>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .production_repository()?
            .coverage(workspace_id, BuildId(build_id))
            .await?,
    ))
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/industry/blueprints/observations",
            get(list_blueprint_observations),
        )
        .route("/api/builds", get(list_builds).post(create_build))
        .route(
            "/api/builds/:build_id",
            get(get_build).put(update_build).delete(delete_build),
        )
        .route("/api/builds/:build_id/name", patch(rename_build))
        .route(
            "/api/builds/:build_id/linked-builds",
            post(create_linked_build),
        )
        .route(
            "/api/builds/:build_id/component-resolutions",
            post(set_component_resolution),
        )
        .route(
            "/api/builds/:build_id/component-resolutions/:component_type_id",
            delete(clear_component_resolution),
        )
        .route(
            "/api/builds/:build_id/blueprint-selection",
            patch(set_build_blueprint_selection),
        )
        .route("/api/builds/:build_id/facility", patch(set_build_facility))
        .route("/api/builds/:build_id/pricing", patch(set_build_pricing))
        .route(
            "/api/builds/:build_id/descendant-production-configuration",
            patch(update_descendant_production_configuration),
        )
        .route("/api/builds/:build_id/coverage", get(get_build_coverage))
}
