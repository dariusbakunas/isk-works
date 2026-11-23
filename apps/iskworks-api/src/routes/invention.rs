use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use iskworks_app::{InventionProductSearch, InventionQuoteCommand, InventionQuoteResponse};
use serde::Deserialize;

use crate::routes::fittings::register_price_coverage;
use crate::{workspace_context, ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/invention/products", get(search_products))
        .route("/api/invention/quote", post(quote))
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    #[serde(default)]
    query: String,
}

/// Items whose blueprint can be invented, matched on the item's name.
async fn search_products(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<InventionProductSearch>, ApiError> {
    let _ = workspace_context(&state).await?;
    Ok(Json(
        state
            .invention_service()?
            .search_products(&params.query)
            .await?,
    ))
}

/// "No decryptor" and every decryptor compared for one product. A read: it
/// is a `POST` only for its body. The types it priced are registered with
/// the market worker, so a missing price is there on a later load.
async fn quote(
    State(state): State<AppState>,
    Json(command): Json<InventionQuoteCommand>,
) -> Result<Json<InventionQuoteResponse>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let scope = state.workspace_default_market_scope(workspace_id).await?;
    let mut response = state
        .invention_service()?
        .quote(workspace_id, scope, command, Utc::now())
        .await?;
    register_price_coverage(
        &state,
        workspace_id,
        scope,
        std::mem::take(&mut response.priced_types),
    );
    Ok(Json(response))
}
