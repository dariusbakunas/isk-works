//! Inventory › Runway (and, later, Consumption): read-only analytics over
//! the inventory ledger and the Epics that draw on it.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use iskworks_core::{
    build_runway, InventoryError, InventoryRunway, MaterialCategory, RunwayFilter,
};
use serde::Deserialize;

use crate::{workspace_context, ApiError, AppState};

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/api/inventory/runway", get(get_runway))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunwayParams {
    category: Option<String>,
}

impl RunwayParams {
    fn filter(&self) -> Result<RunwayFilter, InventoryError> {
        let category = self
            .category
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(str::parse::<MaterialCategory>)
            .transpose()
            .map_err(InventoryError::Validation)?;
        Ok(RunwayFilter { category })
    }
}

async fn get_runway(
    State(state): State<AppState>,
    Query(params): Query<RunwayParams>,
) -> Result<Json<InventoryRunway>, ApiError> {
    let filter = params.filter()?;
    let (workspace_id, _) = workspace_context(&state).await?;
    let facts = state
        .inventory_runway_repository()?
        .runway_facts(workspace_id)
        .await?;
    Ok(Json(build_runway(&facts, &filter)))
}
