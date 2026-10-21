//! The Market Orders page: the workspace characters' open orders, judged
//! against the market books ISK Works keeps.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use iskworks_app::MarketOrdersOverview;

use crate::{workspace_context, ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/market-orders", get(get_market_orders))
        .route("/api/market-orders/refresh", post(refresh_market_orders))
}

async fn get_market_orders(
    State(state): State<AppState>,
) -> Result<Json<MarketOrdersOverview>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    Ok(Json(
        state
            .market_orders_service()?
            .overview(workspace_id, owner_id, Utc::now())
            .await?,
    ))
}

/// Moves the books the page shows to the front of the refresh queue; the
/// worker fetches them shortly after.
async fn refresh_market_orders(State(state): State<AppState>) -> Result<StatusCode, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    state
        .market_orders_service()?
        .prioritize_books(workspace_id, Utc::now())
        .await?;
    Ok(StatusCode::ACCEPTED)
}
