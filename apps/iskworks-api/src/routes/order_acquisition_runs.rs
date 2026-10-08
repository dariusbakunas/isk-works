//! Acquisition Run batching for standalone `order::Ticket`s.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use iskworks_core::order::{Ticket, TicketId};
use iskworks_core::{
    AcquisitionProgressUpdate, AcquisitionRun, AcquisitionRunId, AcquisitionRunItem,
};
use serde::{Deserialize, Serialize};

use crate::{workspace_context, ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/acquisition-runs",
            post(create_order_acquisition_run).get(list_order_acquisition_runs),
        )
        .route(
            "/api/acquisition-runs/:run_id",
            get(get_order_acquisition_run),
        )
        .route(
            "/api/acquisition-runs/:run_id/start",
            post(start_order_acquisition_run),
        )
        .route(
            "/api/acquisition-runs/:run_id/items",
            patch(record_order_acquisition_progress),
        )
        .route(
            "/api/acquisition-runs/:run_id/complete",
            post(complete_order_acquisition_run),
        )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateOrderAcquisitionRunRequest {
    name: Option<String>,
    ticket_ids: Vec<uuid::Uuid>,
}

async fn create_order_acquisition_run(
    State(state): State<AppState>,
    Json(body): Json<CreateOrderAcquisitionRunRequest>,
) -> Result<(StatusCode, Json<AcquisitionRun>), ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let run = state
        .order_repository()?
        .create_order_acquisition_run(
            workspace_id,
            owner_id,
            body.name,
            body.ticket_ids.into_iter().map(TicketId).collect(),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(run)))
}

async fn list_order_acquisition_runs(
    State(state): State<AppState>,
) -> Result<Json<Vec<AcquisitionRun>>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    Ok(Json(
        state
            .order_repository()?
            .list_order_acquisition_runs(workspace_id, owner_id)
            .await?,
    ))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderAcquisitionRunDetailResponse {
    #[serde(flatten)]
    run: AcquisitionRun,
    tickets: Vec<Ticket>,
    /// The Run's real, uncapped per-type acquired totals -- may exceed the
    /// matching tickets' summed `quantity` (over-acquisition). Ticket
    /// `acquiredQuantity` stays capped to that ticket's own demand; this is
    /// what the drawer shows as the actual "Acquired" number.
    items: Vec<AcquisitionRunItem>,
}

async fn get_order_acquisition_run(
    State(state): State<AppState>,
    Path(run_id): Path<uuid::Uuid>,
) -> Result<Json<OrderAcquisitionRunDetailResponse>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let run_id = AcquisitionRunId(run_id);
    let repository = state.order_repository()?;
    let run = repository
        .get_order_acquisition_run(workspace_id, run_id)
        .await?;
    let tickets = repository
        .list_order_acquisition_run_tickets(workspace_id, run_id)
        .await?;
    let items = repository
        .list_order_acquisition_run_items(workspace_id, run_id)
        .await?;
    Ok(Json(OrderAcquisitionRunDetailResponse {
        run,
        tickets,
        items,
    }))
}

/// No preview/confirm step: Order status is derived, so starting a Run
/// locks nothing and there is nothing to warn about -- see
/// `OrderRepository::start_order_acquisition_run`'s own doc.
async fn start_order_acquisition_run(
    State(state): State<AppState>,
    Path(run_id): Path<uuid::Uuid>,
) -> Result<Json<AcquisitionRun>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    Ok(Json(
        state
            .order_repository()?
            .start_order_acquisition_run(workspace_id, owner_id, AcquisitionRunId(run_id))
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordOrderAcquisitionProgressItem {
    type_id: i64,
    acquired_quantity: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordOrderAcquisitionProgressRequest {
    items: Vec<RecordOrderAcquisitionProgressItem>,
}

async fn record_order_acquisition_progress(
    State(state): State<AppState>,
    Path(run_id): Path<uuid::Uuid>,
    Json(body): Json<RecordOrderAcquisitionProgressRequest>,
) -> Result<Json<AcquisitionRun>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let items: Vec<AcquisitionProgressUpdate> = body
        .items
        .into_iter()
        .map(|item| AcquisitionProgressUpdate {
            type_id: item.type_id,
            acquired_quantity: item.acquired_quantity,
        })
        .collect();
    Ok(Json(
        state
            .order_repository()?
            .record_order_acquisition_progress(workspace_id, AcquisitionRunId(run_id), items)
            .await?,
    ))
}

async fn complete_order_acquisition_run(
    State(state): State<AppState>,
    Path(run_id): Path<uuid::Uuid>,
) -> Result<Json<AcquisitionRun>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    Ok(Json(
        state
            .order_repository()?
            .complete_order_acquisition_run(workspace_id, owner_id, AcquisitionRunId(run_id))
            .await?,
    ))
}
