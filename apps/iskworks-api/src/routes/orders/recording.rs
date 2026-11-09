use super::*;
use crate::error::{HolderView, InsufficientAvailableDetail, ShortageView};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecordAcquisitionRequest {
    /// A preview ignores it.
    idempotency_key: uuid::Uuid,
    quantity: u64,
    #[serde(default)]
    unit_cost: Option<String>,
    #[serde(default)]
    location_note: String,
    #[serde(default)]
    note: String,
    #[serde(default)]
    effective_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecordAcquisitionResponse {
    recording: TicketInventoryRecording,
    summary: TicketRecordingSummary,
}

fn acquisition_input(
    request: RecordAcquisitionRequest,
) -> Result<RecordAcquisitionInput, ApiError> {
    if request.quantity == 0 {
        return Err(OrderError::InvalidQuantity.into());
    }
    let unit_cost = request
        .unit_cost
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(iskworks_core::Money::parse)
        .transpose()?;
    Ok(RecordAcquisitionInput {
        idempotency_key: request.idempotency_key,
        quantity: request.quantity,
        unit_cost,
        location_note: request.location_note.trim().to_string(),
        note: request.note.trim().to_string(),
        effective_at: request.effective_at.unwrap_or_else(Utc::now),
    })
}

/// What recording this acquisition would do to inventory -- the same body
/// as `record-acquisition`, nothing recorded. `idempotencyKey` is ignored.
pub(super) async fn preview_ticket_acquisition_route(
    State(state): State<AppState>,
    Path(ticket_id): Path<uuid::Uuid>,
    Json(request): Json<RecordAcquisitionRequest>,
) -> Result<Json<iskworks_core::order::RecordingPreview>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let input = acquisition_input(request)?;
    Ok(Json(
        state
            .order_repository()?
            .preview_ticket_acquisition(workspace_id, TicketId(ticket_id), input)
            .await?,
    ))
}

/// Explicit "I acquired this" recording -- the first explicit
/// inventory-recording action.
/// Posts exactly one `Purchase` for `quantity` and records immutable
/// provenance, in one transaction. **Does not** change the ticket's
/// status, cancel/complete/start it, cascade dependents, or touch any
/// Build/Order/allocation. Idempotent on `idempotencyKey`: a replay posts
/// nothing and returns `200` with the prior recording (`201` on first
/// post).
pub(super) async fn record_ticket_acquisition_route(
    State(state): State<AppState>,
    Path(ticket_id): Path<uuid::Uuid>,
    Json(request): Json<RecordAcquisitionRequest>,
) -> Result<(StatusCode, Json<RecordAcquisitionResponse>), ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let input = acquisition_input(request)?;
    let outcome = state
        .order_repository()?
        .record_ticket_acquisition(
            workspace_id,
            iskworks_core::order::TicketId(ticket_id),
            input,
        )
        .await?;
    let status = if outcome.created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        Json(RecordAcquisitionResponse {
            recording: outcome.recording,
            summary: outcome.summary,
        }),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecordProductionOutputRequest {
    type_id: i64,
    quantity: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecordProductionInputRequest {
    type_id: i64,
    quantity: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecordProductionRequest {
    /// A preview ignores it.
    idempotency_key: uuid::Uuid,
    runs_completed: u64,
    output: RecordProductionOutputRequest,
    inputs: Vec<RecordProductionInputRequest>,
    installation_cost: String,
    #[serde(default)]
    location_note: String,
    #[serde(default)]
    note: String,
    #[serde(default)]
    effective_at: Option<chrono::DateTime<Utc>>,
    /// Epics the user agreed to take reserved stock from, per type, after
    /// a `409 insufficient_available`.
    #[serde(default)]
    take_from: Vec<TakeFromRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TakeFromRequest {
    order_id: uuid::Uuid,
    type_id: i64,
}

/// `ApiError::InsufficientAvailable` for `shortages`, with each item's
/// name and each holding Epic's display name (one read for all of them).
async fn insufficient_available(
    state: &AppState,
    workspace_id: iskworks_core::WorkspaceId,
    ticket_id: iskworks_core::order::TicketId,
    shortages: Vec<iskworks_core::order::AvailabilityShortage>,
) -> ApiError {
    match shortage_views(state, workspace_id, ticket_id, shortages).await {
        Ok(shortages) => ApiError::InsufficientAvailable(InsufficientAvailableDetail { shortages }),
        Err(error) => error,
    }
}

async fn shortage_views(
    state: &AppState,
    workspace_id: iskworks_core::WorkspaceId,
    ticket_id: iskworks_core::order::TicketId,
    shortages: Vec<iskworks_core::order::AvailabilityShortage>,
) -> Result<Vec<ShortageView>, ApiError> {
    let repository = state.order_repository()?;
    let names: HashMap<i64, String> = repository
        .list_ticket_prerequisites(ticket_id)
        .await?
        .into_iter()
        .map(|prerequisite| (prerequisite.type_id, prerequisite.captured_name))
        .collect();
    let holder_ids: Vec<iskworks_core::order::OrderId> = shortages
        .iter()
        .flat_map(|shortage| shortage.holders.iter().map(|holder| holder.order_id))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let holder_names: HashMap<iskworks_core::order::OrderId, String> = repository
        .get_orders(workspace_id, &holder_ids)
        .await?
        .into_iter()
        .map(|order| (order.id, order.display_name))
        .collect();
    Ok(shortages
        .into_iter()
        .map(|shortage| ShortageView {
            type_id: shortage.type_id,
            type_name: names.get(&shortage.type_id).cloned().unwrap_or_default(),
            needed: shortage.needed,
            own: shortage.own,
            free: shortage.free,
            holders: shortage
                .holders
                .into_iter()
                .map(|holder| HolderView {
                    order_id: holder.order_id,
                    // A reservation always names an Epic in this workspace;
                    // the fallback only covers one deleted meanwhile.
                    display_name: holder_names
                        .get(&holder.order_id)
                        .cloned()
                        .unwrap_or_else(|| "Another reservation".to_string()),
                    quantity: holder.quantity,
                })
                .collect(),
        })
        .collect())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecordProductionResponse {
    recording: TicketInventoryRecording,
    summary: TicketRecordingSummary,
}

fn production_input(request: RecordProductionRequest) -> Result<RecordProductionInput, ApiError> {
    if request.runs_completed == 0 {
        return Err(OrderError::InvalidQuantity.into());
    }
    if request.inputs.iter().any(|line| line.quantity == 0) {
        return Err(OrderError::InvalidQuantity.into());
    }
    // `Money::parse` rejects a negative value, so `installationCost >= 0`
    // (and `0`) fall out for free.
    let installation_cost = iskworks_core::Money::parse(request.installation_cost.trim())?;
    Ok(RecordProductionInput {
        idempotency_key: request.idempotency_key,
        runs_completed: request.runs_completed,
        output_type_id: request.output.type_id,
        output_quantity: request.output.quantity,
        inputs: request
            .inputs
            .into_iter()
            .map(|line| RecordProductionInputLine {
                type_id: line.type_id,
                quantity: line.quantity,
            })
            .collect(),
        installation_cost,
        location_note: request.location_note.trim().to_string(),
        note: request.note.trim().to_string(),
        effective_at: request.effective_at.unwrap_or_else(Utc::now),
        take_from: request
            .take_from
            .into_iter()
            .map(|take| TakeFrom {
                order_id: iskworks_core::order::OrderId(take.order_id),
                type_id: take.type_id,
            })
            .collect(),
    })
}

/// What recording this production would do to inventory -- the same body
/// as `record-production`, nothing recorded. `idempotencyKey` is ignored;
/// other Epics' reservations answer the same `409 insufficient_available`.
pub(super) async fn preview_ticket_production_route(
    State(state): State<AppState>,
    Path(ticket_id): Path<uuid::Uuid>,
    Json(request): Json<RecordProductionRequest>,
) -> Result<axum::response::Response, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let input = production_input(request)?;
    let ticket_id = TicketId(ticket_id);
    match state
        .order_repository()?
        .preview_ticket_production(workspace_id, ticket_id, input)
        .await
    {
        Ok(preview) => Ok(Json(preview).into_response()),
        Err(OrderError::InsufficientAvailable(shortages)) => {
            Err(insufficient_available(&state, workspace_id, ticket_id, shortages).await)
        }
        Err(error) => Err(error.into()),
    }
}

/// Explicit "I produced this" recording. In one transaction:
/// one `Consumption` per input at the material's current inventory
/// weighted average, then -- when `output.quantity > 0` -- one
/// `ProductionOutput` whose basis is `Σ consumed basis + installationCost`
/// (`Known` quality). **Does not** change the ticket's status, cascade
/// dependents, or touch any Build/Order/allocation. Idempotent on
/// `idempotencyKey` (`200` on replay, `201` on first post).
pub(super) async fn record_ticket_production_route(
    State(state): State<AppState>,
    Path(ticket_id): Path<uuid::Uuid>,
    Json(request): Json<RecordProductionRequest>,
) -> Result<axum::response::Response, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let input = production_input(request)?;
    let ticket_id = iskworks_core::order::TicketId(ticket_id);
    let repository = state.order_repository()?;
    let outcome = match repository
        .record_ticket_production(workspace_id, ticket_id, input)
        .await
    {
        Ok(outcome) => outcome,
        Err(OrderError::InsufficientAvailable(shortages)) => {
            return Err(insufficient_available(&state, workspace_id, ticket_id, shortages).await);
        }
        Err(error) => return Err(error.into()),
    };
    let status = if outcome.created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        Json(RecordProductionResponse {
            recording: outcome.recording,
            summary: outcome.summary,
        }),
    )
        .into_response())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RevertTicketInventoryRecordingResponse {
    recording: TicketInventoryRecording,
    summary: TicketRecordingSummary,
    recordings: Vec<TicketInventoryRecording>,
}

pub(super) async fn revert_ticket_inventory_recording_route(
    State(state): State<AppState>,
    Path((ticket_id, recording_id)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> Result<Json<RevertTicketInventoryRecordingResponse>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let repository = state.order_repository()?;
    let outcome = repository
        .revert_ticket_inventory_recording(
            workspace_id,
            TicketId(ticket_id),
            iskworks_core::order::TicketInventoryRecordingId(recording_id),
        )
        .await?;
    let recordings = repository
        .list_ticket_inventory_recordings(TicketId(ticket_id))
        .await?;
    let recording = recordings
        .iter()
        .find(|recording| recording.id.0 == recording_id)
        .cloned()
        .ok_or(OrderError::RecordingNotFound)?;
    Ok(Json(RevertTicketInventoryRecordingResponse {
        recording,
        summary: outcome.summary,
        recordings,
    }))
}
