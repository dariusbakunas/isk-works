//! EVE client CSV market exports: multipart upload -> preview / import,
//! plus reads over stored import batches and a batch's raw order book.

use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use iskworks_core::{
    MarketError, MarketImportBatch, MarketImportBatchId, MarketImportPreview, MarketImportResult,
    MarketOrderBook, MarketUpload, MAX_MARKET_BATCH_BYTES,
};
use serde::Deserialize;

use crate::{workspace_context, ApiError, AppState};

async fn read_market_uploads(mut multipart: Multipart) -> Result<Vec<MarketUpload>, ApiError> {
    let mut files = Vec::new();
    let mut observed_at = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| MarketError::InvalidUpload(error.to_string()))?
    {
        if field.name() == Some("observedAt") {
            let value = field
                .text()
                .await
                .map_err(|error| MarketError::InvalidUpload(error.to_string()))?;
            observed_at = Some(
                chrono::DateTime::parse_from_rfc3339(value.trim())
                    .map_err(|_| {
                        MarketError::InvalidUpload(
                            "observedAt must be an RFC 3339 timestamp.".to_string(),
                        )
                    })?
                    .with_timezone(&chrono::Utc),
            );
            continue;
        }
        let Some(filename) = field.file_name().map(str::to_string) else {
            continue;
        };
        let content = field
            .bytes()
            .await
            .map_err(|error| MarketError::InvalidUpload(error.to_string()))?;
        files.push(MarketUpload {
            filename,
            content: content.to_vec(),
            user_observed_at: None,
        });
    }
    if let Some(observed_at) = observed_at {
        for file in &mut files {
            file.user_observed_at = Some(observed_at);
        }
    }
    Ok(files)
}

async fn preview_market_exports(
    State(state): State<AppState>,
    multipart: Multipart,
) -> Result<Json<MarketImportPreview>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let uploads = read_market_uploads(multipart).await?;
    Ok(Json(
        state
            .market_service()?
            .preview_import(workspace_id, uploads)
            .await?,
    ))
}

async fn import_market_exports(
    State(state): State<AppState>,
    multipart: Multipart,
) -> Result<(StatusCode, Json<MarketImportResult>), ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let uploads = read_market_uploads(multipart).await?;
    let result = state
        .market_service()?
        .import(workspace_id, uploads)
        .await?;
    let status = if result.batch.is_some() {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(result)))
}

async fn list_market_imports(
    State(state): State<AppState>,
) -> Result<Json<Vec<MarketImportBatch>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .market_repository()?
            .list_imports(workspace_id)
            .await?,
    ))
}

async fn get_market_import(
    State(state): State<AppState>,
    Path(batch_id): Path<uuid::Uuid>,
) -> Result<Json<MarketImportBatch>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .market_repository()?
            .get_import(workspace_id, MarketImportBatchId(batch_id))
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarketOrderBookQuery {
    type_id: i64,
    location_id: i64,
    batch_id: Option<uuid::Uuid>,
}

async fn get_market_order_book(
    State(state): State<AppState>,
    Query(query): Query<MarketOrderBookQuery>,
) -> Result<Json<MarketOrderBook>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .market_repository()?
            .get_order_book(
                workspace_id,
                query.type_id,
                query.location_id,
                query.batch_id.map(MarketImportBatchId),
            )
            .await?,
    ))
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/industry/market-imports",
            get(list_market_imports).post(import_market_exports),
        )
        .route(
            "/api/industry/market-imports/preview",
            post(preview_market_exports),
        )
        .route(
            "/api/industry/market-imports/:batch_id",
            get(get_market_import),
        )
        .route(
            "/api/industry/market-observations/order-book",
            get(get_market_order_book),
        )
        // Multipart batches of EVE market exports; `import_parse` enforces
        // the per-file and per-batch caps on the parsed content. Overrides
        // the app-wide default set in `build_router`.
        .layer(DefaultBodyLimit::max(MAX_MARKET_BATCH_BYTES + 1024 * 1024))
}
