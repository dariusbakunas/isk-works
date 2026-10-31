//! Planetary › Planner: planets around a home system, the readings the
//! player enters for them, and the product ranking.

use std::collections::BTreeMap;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use iskworks_app::{
    DensityWriteDto, PlanetSettingsWriteDto, PlannerCharacterSkillsDto, PlannerPlanetsDto,
    PlannerRankingDto, PlannerScenarioDto,
};
use serde::Deserialize;

use super::planetary::register_coverage;
use crate::{workspace_context, ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/planetary/planner/planets", get(planets))
        .route("/api/planetary/planner/characters", get(characters))
        .route("/api/planetary/planner/densities", put(put_densities))
        .route(
            "/api/planetary/planner/planet-settings",
            put(put_planet_settings),
        )
        .route("/api/planetary/planner/rank", post(rank))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanetsQuery {
    system_id: i64,
    #[serde(default)]
    radius: u8,
}

async fn planets(
    State(state): State<AppState>,
    Query(query): Query<PlanetsQuery>,
) -> Result<Json<PlannerPlanetsDto>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .pi_planner_service()?
            .planets(workspace_id, query.system_id, query.radius)
            .await?,
    ))
}

async fn characters(
    State(state): State<AppState>,
) -> Result<Json<Vec<PlannerCharacterSkillsDto>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state.pi_planner_service()?.characters(workspace_id).await?,
    ))
}

async fn put_densities(
    State(state): State<AppState>,
    Json(writes): Json<Vec<DensityWriteDto>>,
) -> Result<StatusCode, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    state
        .pi_planner_service()?
        .save_densities(workspace_id, writes)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn put_planet_settings(
    State(state): State<AppState>,
    Json(writes): Json<Vec<PlanetSettingsWriteDto>>,
) -> Result<StatusCode, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    state
        .pi_planner_service()?
        .save_planet_settings(workspace_id, writes)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn rank(
    State(state): State<AppState>,
    Json(scenario): Json<PlannerScenarioDto>,
) -> Result<Json<PlannerRankingDto>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let scope = state.workspace_default_market_scope(workspace_id).await?;
    let ranking = state
        .pi_planner_service()?
        .rank(workspace_id, scope, scenario)
        .await?;
    // Every product, ranked or not: an unpriced one is out of reach only
    // until the market worker covers it.
    let items: BTreeMap<i64, String> = ranking
        .ranked
        .iter()
        .map(|product| (product.type_id, product.name.clone()))
        .chain(
            ranking
                .out_of_reach
                .iter()
                .map(|product| (product.type_id, product.name.clone())),
        )
        .collect();
    register_coverage(&state, workspace_id, scope, items);
    Ok(Json(ranking))
}
