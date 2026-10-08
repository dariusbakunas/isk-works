use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use iskworks_app::AutomaticEiv;
use iskworks_core::{
    BuildPlan, BuildPlanningError, CapturedReactionFormula, CapturedRecipe, IndustryError,
    ReactionPlan, RecipeSelection,
};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, ReactionFormulaSearchResult, TypeSearchResult,
};
use serde::{Deserialize, Serialize};

use crate::{ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/sde", get(get_sde_status))
        .route("/api/blueprints/search", get(search_blueprints))
        .route(
            "/api/reaction-formulas/search",
            get(search_reaction_formulas),
        )
        .route("/api/types/search", get(search_types))
        .route("/api/structure-types/search", get(search_structure_types))
        .route("/api/structure-rigs/search", get(search_structure_rigs))
        .route(
            "/api/structure-types/:type_id/manufacturing-modifiers",
            get(structure_manufacturing_modifiers),
        )
        .route(
            "/api/structure-rigs/:type_id/manufacturing-modifiers",
            get(rig_manufacturing_modifiers),
        )
        .route("/api/reaction-rigs/search", get(search_reaction_rigs))
        .route(
            "/api/reaction-rigs/:type_id/reaction-modifiers",
            get(reaction_rig_modifiers),
        )
        .route(
            "/api/universe/solar-systems/search",
            get(search_solar_systems),
        )
        .route(
            "/api/universe/npc-stations/search",
            get(search_npc_stations),
        )
        .route("/api/blueprints/:blueprint_type_id/plan", get(plan_build))
        .route(
            "/api/reaction-formulas/:reaction_formula_type_id/plan",
            get(plan_reaction),
        )
        .route(
            "/api/blueprints/:blueprint_type_id/estimated-item-value",
            get(get_blueprint_automatic_eiv),
        )
        .route(
            "/api/reaction-formulas/:reaction_formula_type_id/estimated-item-value",
            get(get_reaction_formula_automatic_eiv),
        )
        .route(
            "/api/recipes/for-product/:product_type_id",
            get(recipe_for_product),
        )
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SdeStatusResponse {
    pub configured: bool,
    pub active: Option<ActiveSde>,
}

async fn get_sde_status(
    State(state): State<AppState>,
) -> Result<Json<SdeStatusResponse>, ApiError> {
    let active = state.sde_repository.active_sde().await?;
    Ok(Json(SdeStatusResponse {
        configured: active.is_some(),
        active,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintSearchQuery {
    pub q: String,
    pub limit: Option<u32>,
    pub structure_type_id: Option<i64>,
}

async fn search_blueprints(
    State(state): State<AppState>,
    Query(query): Query<BlueprintSearchQuery>,
) -> Result<Json<Vec<BlueprintSearchResult>>, ApiError> {
    Ok(Json(
        state
            .sde_repository
            .search_manufacturing_blueprints(
                &query.q,
                crate::routes::clamp_search_limit(query.limit, 20),
            )
            .await?,
    ))
}

async fn search_reaction_formulas(
    State(state): State<AppState>,
    Query(query): Query<BlueprintSearchQuery>,
) -> Result<Json<Vec<ReactionFormulaSearchResult>>, ApiError> {
    Ok(Json(
        state
            .sde_repository
            .search_reaction_formulas(&query.q, crate::routes::clamp_search_limit(query.limit, 20))
            .await?,
    ))
}

async fn search_types(
    State(state): State<AppState>,
    Query(query): Query<BlueprintSearchQuery>,
) -> Result<Json<Vec<TypeSearchResult>>, ApiError> {
    Ok(Json(
        state
            .sde_repository
            .search_types(&query.q, crate::routes::clamp_search_limit(query.limit, 20))
            .await?,
    ))
}

async fn search_structure_types(
    State(state): State<AppState>,
    Query(query): Query<BlueprintSearchQuery>,
) -> Result<Json<Vec<TypeSearchResult>>, ApiError> {
    Ok(Json(
        state
            .sde_repository
            .search_structure_types(&query.q, crate::routes::clamp_search_limit(query.limit, 20))
            .await?,
    ))
}

async fn search_structure_rigs(
    State(state): State<AppState>,
    Query(query): Query<BlueprintSearchQuery>,
) -> Result<Json<Vec<TypeSearchResult>>, ApiError> {
    Ok(Json(
        state
            .sde_repository
            .search_structure_rigs(
                &query.q,
                crate::routes::clamp_search_limit(query.limit, 20),
                query.structure_type_id,
            )
            .await?,
    ))
}

async fn structure_manufacturing_modifiers(
    State(state): State<AppState>,
    Path(type_id): Path<i64>,
) -> Result<Json<iskworks_sde::StructureManufacturingModifiers>, ApiError> {
    state
        .sde_repository
        .structure_manufacturing_modifiers(type_id)
        .await?
        .map(Json)
        .ok_or(ApiError::StaticDataEntryNotFound)
}

/// Reverse recipe lookup: does anything produce this product? `null` (not a
/// 404) when nothing does -- that's the expected case for a genuine raw
/// material, not an error.
async fn recipe_for_product(
    State(state): State<AppState>,
    Path(product_type_id): Path<i64>,
) -> Result<Json<Option<RecipeSelection>>, ApiError> {
    if let Some(blueprint_type_id) = state
        .sde_repository
        .manufacturing_blueprint_for_product(product_type_id)
        .await?
    {
        return Ok(Json(Some(RecipeSelection::Manufacturing {
            blueprint_type_id,
        })));
    }
    if let Some(reaction_formula_type_id) = state
        .sde_repository
        .reaction_formula_for_product(product_type_id)
        .await?
    {
        return Ok(Json(Some(RecipeSelection::Reaction {
            reaction_formula_type_id,
        })));
    }
    Ok(Json(None))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RigModifierQuery {
    security_class: String,
    structure_type_id: Option<i64>,
}

async fn rig_manufacturing_modifiers(
    State(state): State<AppState>,
    Path(type_id): Path<i64>,
    Query(query): Query<RigModifierQuery>,
) -> Result<Json<iskworks_sde::RigManufacturingModifiers>, ApiError> {
    state
        .sde_repository
        .rig_manufacturing_modifiers(type_id, &query.security_class, query.structure_type_id)
        .await?
        .map(Json)
        .ok_or(ApiError::StaticDataEntryNotFound)
}

async fn search_reaction_rigs(
    State(state): State<AppState>,
    Query(query): Query<BlueprintSearchQuery>,
) -> Result<Json<Vec<TypeSearchResult>>, ApiError> {
    Ok(Json(
        state
            .sde_repository
            .search_reaction_rigs(
                &query.q,
                crate::routes::clamp_search_limit(query.limit, 20),
                query.structure_type_id,
            )
            .await?,
    ))
}

async fn reaction_rig_modifiers(
    State(state): State<AppState>,
    Path(type_id): Path<i64>,
    Query(query): Query<RigModifierQuery>,
) -> Result<Json<iskworks_sde::ReactionRigModifiers>, ApiError> {
    state
        .sde_repository
        .reaction_rig_modifiers(type_id, &query.security_class, query.structure_type_id)
        .await?
        .map(Json)
        .ok_or(ApiError::StaticDataEntryNotFound)
}

async fn search_solar_systems(
    State(state): State<AppState>,
    Query(query): Query<BlueprintSearchQuery>,
) -> Result<Json<Vec<iskworks_sde::SolarSystemSearchResult>>, ApiError> {
    Ok(Json(
        state
            .sde_repository
            .search_solar_systems(&query.q, crate::routes::clamp_search_limit(query.limit, 12))
            .await?,
    ))
}

async fn search_npc_stations(
    State(state): State<AppState>,
    Query(query): Query<BlueprintSearchQuery>,
) -> Result<Json<Vec<iskworks_sde::NpcStationSearchResult>>, ApiError> {
    Ok(Json(
        state
            .sde_repository
            .search_npc_stations(&query.q, crate::routes::clamp_search_limit(query.limit, 50))
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct BuildPlanQuery {
    pub runs: Option<u64>,
}

async fn plan_build(
    State(state): State<AppState>,
    Path(blueprint_type_id): Path<i64>,
    Query(query): Query<BuildPlanQuery>,
) -> Result<Json<BuildPlan>, ApiError> {
    Ok(Json(
        state
            .build_planning_service()
            .plan(blueprint_type_id, query.runs.unwrap_or(1))
            .await?,
    ))
}

async fn plan_reaction(
    State(state): State<AppState>,
    Path(reaction_formula_type_id): Path<i64>,
    Query(query): Query<BuildPlanQuery>,
) -> Result<Json<ReactionPlan>, ApiError> {
    Ok(Json(
        state
            .reaction_planning_service()
            .plan(reaction_formula_type_id, query.runs.unwrap_or(1))
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
struct AutomaticEivQuery {
    runs: u64,
}

async fn get_blueprint_automatic_eiv(
    State(state): State<AppState>,
    Path(blueprint_type_id): Path<i64>,
    Query(query): Query<AutomaticEivQuery>,
) -> Result<Json<AutomaticEiv>, ApiError> {
    if !(1..=1_000_000).contains(&query.runs) {
        return Err(BuildPlanningError::InvalidRuns.into());
    }
    let active = state
        .sde_repository
        .active_sde()
        .await?
        .ok_or(IndustryError::NoActiveSde)?;
    let recipe = state
        .sde_repository
        .manufacturing_recipe(blueprint_type_id)
        .await?
        .ok_or(IndustryError::BlueprintNotFound)?;
    let captured = CapturedRecipe::capture(active.import_id, active.source_version, recipe)?;
    Ok(Json(
        state
            .esi_service()?
            .automatic_eiv(&captured.materials, query.runs)
            .await?,
    ))
}

async fn get_reaction_formula_automatic_eiv(
    State(state): State<AppState>,
    Path(reaction_formula_type_id): Path<i64>,
    Query(query): Query<AutomaticEivQuery>,
) -> Result<Json<AutomaticEiv>, ApiError> {
    if !(1..=1_000_000).contains(&query.runs) {
        return Err(BuildPlanningError::InvalidRuns.into());
    }
    let active = state
        .sde_repository
        .active_sde()
        .await?
        .ok_or(IndustryError::NoActiveSde)?;
    let formula = state
        .sde_repository
        .reaction_formula(reaction_formula_type_id)
        .await?
        .ok_or(IndustryError::ReactionFormulaNotFound)?;
    let captured =
        CapturedReactionFormula::capture(active.import_id, active.source_version, formula)?;
    Ok(Json(
        state
            .esi_service()?
            .automatic_eiv(&captured.materials, query.runs)
            .await?,
    ))
}
