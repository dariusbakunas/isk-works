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

/// 409 body when recording would use stock other Epics reserved: per short
/// item, what it needs, what the ticket's Epic holds, what's free, and who
/// holds the rest -- enough for "Take N from EP-x?".
#[derive(Debug, Serialize)]
struct InsufficientAvailableEnvelope {
    error: InsufficientAvailableBody,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InsufficientAvailableBody {
    code: &'static str,
    message: &'static str,
    retryable: bool,
    shortages: Vec<ShortageView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ShortageView {
    type_id: i64,
    type_name: String,
    needed: u64,
    own: u64,
    free: u64,
    holders: Vec<HolderView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HolderView {
    order_id: iskworks_core::order::OrderId,
    display_name: String,
    quantity: u64,
}

async fn insufficient_available_response(
    state: &AppState,
    workspace_id: iskworks_core::WorkspaceId,
    ticket_id: iskworks_core::order::TicketId,
    shortages: Vec<iskworks_core::order::AvailabilityShortage>,
) -> Result<axum::response::Response, ApiError> {
    let repository = state.order_repository()?;
    let names: HashMap<i64, String> = repository
        .list_ticket_prerequisites(ticket_id)
        .await?
        .into_iter()
        .map(|prerequisite| (prerequisite.type_id, prerequisite.captured_name))
        .collect();
    let mut views = Vec::with_capacity(shortages.len());
    for shortage in shortages {
        let mut holders = Vec::with_capacity(shortage.holders.len());
        for holder in shortage.holders {
            let display_name = match repository.get_order(workspace_id, holder.order_id).await {
                Ok(order) => order.display_name,
                Err(_) => "Another reservation".to_string(),
            };
            holders.push(HolderView {
                order_id: holder.order_id,
                display_name,
                quantity: holder.quantity,
            });
        }
        views.push(ShortageView {
            type_id: shortage.type_id,
            type_name: names.get(&shortage.type_id).cloned().unwrap_or_default(),
            needed: shortage.needed,
            own: shortage.own,
            free: shortage.free,
            holders,
        });
    }
    Ok((
        StatusCode::CONFLICT,
        Json(InsufficientAvailableEnvelope {
            error: InsufficientAvailableBody {
                code: "insufficient_available",
                message: "Other Epics have reserved stock this recording needs.",
                retryable: false,
                shortages: views,
            },
        }),
    )
        .into_response())
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
) -> Result<axum::response::Response, ApiError> {
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
        take_from: request
            .take_from
            .into_iter()
            .map(|take| TakeFrom {
                order_id: iskworks_core::order::OrderId(take.order_id),
                type_id: take.type_id,
            })
            .collect(),
    };
    let ticket_id = iskworks_core::order::TicketId(ticket_id);
    let outcome = match state
        .order_repository()?
        .record_ticket_production(workspace_id, ticket_id, input)
        .await
    {
        Ok(outcome) => outcome,
        Err(OrderError::InsufficientAvailable(shortages)) => {
            return insufficient_available_response(&state, workspace_id, ticket_id, shortages)
                .await;
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
