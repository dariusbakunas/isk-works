use std::collections::BTreeMap;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use iskworks_app::PlanetaryOverview;
use iskworks_core::planetary::PlanetaryPreferences;
use iskworks_core::{MarketCoverageRegistration, MarketScope, WorkspaceId};

use crate::{workspace_context, ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new().route("/api/planetary", get(overview)).route(
        "/api/planetary/preferences",
        get(get_preferences).put(put_preferences),
    )
}

async fn overview(State(state): State<AppState>) -> Result<Json<PlanetaryOverview>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let scope = state.workspace_default_market_scope(workspace_id).await?;
    let overview = state
        .planetary_service()?
        .overview(workspace_id, scope)
        .await?;
    register_commodity_coverage(&state, workspace_id, scope, &overview);
    Ok(Json(overview))
}

/// Keeps every PI commodity on the page covered by the market worker in the
/// valuation scope (and refreshes the uncovered ones now), the same way Build
/// preview requests prices for its materials. Fire-and-forget: a slow or
/// failing ESI fetch must not hold the page, and the next load picks prices up.
fn register_commodity_coverage(
    state: &AppState,
    workspace_id: WorkspaceId,
    scope: MarketScope,
    overview: &PlanetaryOverview,
) {
    let mut items = BTreeMap::new();
    for planet in overview.characters.iter().flat_map(|c| &c.planets) {
        for export in &planet.exports {
            items.insert(export.type_id, export.name.clone());
        }
        for import in &planet.imports {
            items.insert(import.type_id, import.name.clone());
        }
        for item in planet.storage.iter().flat_map(|storage| &storage.contents) {
            items.insert(item.type_id, item.name.clone());
        }
    }
    if items.is_empty() {
        return;
    }
    let (Ok(market), Ok(public_market)) =
        (state.market_repository(), state.public_market_service())
    else {
        return;
    };
    tokio::spawn(async move {
        let source_id = match market
            .ensure_esi_price_source_for_scope(workspace_id, scope)
            .await
        {
            Ok(source_id) => source_id,
            Err(error) => {
                tracing::warn!(%error, "planetary market coverage registration failed");
                return;
            }
        };
        let coverage = items
            .into_iter()
            .map(|(type_id, type_name)| MarketCoverageRegistration { type_id, type_name })
            .collect();
        if let Err(error) = public_market
            .register_and_refresh(workspace_id, source_id, coverage)
            .await
        {
            tracing::warn!(%error, "planetary market coverage refresh failed");
        }
    });
}

async fn get_preferences(
    State(state): State<AppState>,
) -> Result<Json<PlanetaryPreferences>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state.planetary_service()?.preferences(workspace_id).await?,
    ))
}

async fn put_preferences(
    State(state): State<AppState>,
    Json(preferences): Json<PlanetaryPreferences>,
) -> Result<Json<PlanetaryPreferences>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .planetary_service()?
            .save_preferences(workspace_id, preferences)
            .await?,
    ))
}
