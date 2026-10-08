//! Manual price-list (`PriceSource`) CRUD plus per-item price
//! upsert/remove. Operates through the Industry repository/service, not
//! market-observation storage; kept under `routes/market/` for now.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use iskworks_core::{
    CreatePriceSourceCommand, IndustryError, PriceInput, PriceSource, PriceSourceId,
    UpdatePriceSourceCommand,
};
use serde::Deserialize;

use crate::{workspace_context, ApiError, AppState};

async fn list_price_sources(
    State(state): State<AppState>,
) -> Result<Json<Vec<PriceSource>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .industry_repository()?
            .list_price_sources(workspace_id)
            .await?,
    ))
}

async fn create_price_source(
    State(state): State<AppState>,
    Json(command): Json<CreatePriceSourceCommand>,
) -> Result<(StatusCode, Json<PriceSource>), ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let source = state
        .industry_service()?
        .create_price_source(workspace_id, command)
        .await?;
    Ok((StatusCode::CREATED, Json(source)))
}

async fn get_price_source(
    State(state): State<AppState>,
    Path(source_id): Path<uuid::Uuid>,
) -> Result<Json<PriceSource>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .industry_repository()?
            .get_price_source(workspace_id, PriceSourceId(source_id))
            .await?,
    ))
}

async fn update_price_source(
    State(state): State<AppState>,
    Path(source_id): Path<uuid::Uuid>,
    Json(command): Json<UpdatePriceSourceCommand>,
) -> Result<Json<PriceSource>, ApiError> {
    if command.name.trim().is_empty() {
        return Err(IndustryError::Validation("Price Source name is required.".to_string()).into());
    }
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .industry_repository()?
            .update_price_source(workspace_id, PriceSourceId(source_id), command)
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpsertPriceItemRequest {
    expected_revision: u64,
    type_name: String,
    price: String,
    #[serde(default)]
    note: String,
}

async fn upsert_price_item(
    State(state): State<AppState>,
    Path((source_id, type_id)): Path<(uuid::Uuid, i64)>,
    Json(request): Json<UpsertPriceItemRequest>,
) -> Result<Json<PriceSource>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let items = state
        .industry_service()?
        .parse_price_items(vec![PriceInput {
            type_id,
            type_name: request.type_name,
            price: request.price,
            note: request.note,
        }])
        .await?;
    Ok(Json(
        state
            .industry_repository()?
            .upsert_price_items(
                workspace_id,
                PriceSourceId(source_id),
                request.expected_revision,
                items,
            )
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
struct ExpectedRevisionRequest {
    expected_revision: u64,
}

async fn remove_price_item(
    State(state): State<AppState>,
    Path((source_id, type_id)): Path<(uuid::Uuid, i64)>,
    Query(request): Query<ExpectedRevisionRequest>,
) -> Result<Json<PriceSource>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .industry_repository()?
            .remove_price_item(
                workspace_id,
                PriceSourceId(source_id),
                type_id,
                request.expected_revision,
            )
            .await?,
    ))
}

async fn delete_price_source(
    State(state): State<AppState>,
    Path(source_id): Path<uuid::Uuid>,
    Query(request): Query<ExpectedRevisionRequest>,
) -> Result<StatusCode, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    state
        .industry_repository()?
        .delete_price_source(
            workspace_id,
            PriceSourceId(source_id),
            request.expected_revision,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/price-sources",
            get(list_price_sources).post(create_price_source),
        )
        .route(
            "/api/price-sources/:source_id",
            get(get_price_source)
                .put(update_price_source)
                .delete(delete_price_source),
        )
        .route(
            "/api/price-sources/:source_id/items/:type_id",
            put(upsert_price_item).delete(remove_price_item),
        )
}
