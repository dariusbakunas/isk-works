use super::*;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecordAcquisitionRequest {
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
    let input = RecordAcquisitionInput {
        idempotency_key: request.idempotency_key,
        quantity: request.quantity,
        unit_cost,
        location_note: request.location_note.trim().to_string(),
        note: request.note.trim().to_string(),
        effective_at: request.effective_at.unwrap_or_else(Utc::now),
    };
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
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecordProductionResponse {
    recording: TicketInventoryRecording,
    summary: TicketRecordingSummary,
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
) -> Result<(StatusCode, Json<RecordProductionResponse>), ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    if request.runs_completed == 0 {
        return Err(OrderError::InvalidQuantity.into());
    }
    if request.inputs.iter().any(|line| line.quantity == 0) {
        return Err(OrderError::InvalidQuantity.into());
    }
    // `Money::parse` rejects a negative value, so `installationCost >= 0`
    // (and `0`) fall out for free.
    let installation_cost = iskworks_core::Money::parse(request.installation_cost.trim())?;
    let input = RecordProductionInput {
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
    };
    let outcome = state
        .order_repository()?
        .record_ticket_production(
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
        Json(RecordProductionResponse {
            recording: outcome.recording,
            summary: outcome.summary,
        }),
    ))
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
