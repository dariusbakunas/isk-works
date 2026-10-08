use super::*;

pub(super) fn order_response(error: OrderError) -> (StatusCode, ErrorBody) {
    let (status, code, retryable) = match &error {
        OrderError::OrderNotFound => (StatusCode::NOT_FOUND, "order_not_found", false),
        OrderError::OrderRequirementNotFound => {
            (StatusCode::NOT_FOUND, "order_requirement_not_found", false)
        }
        OrderError::TicketReferenceNotFound => {
            (StatusCode::NOT_FOUND, "ticket_reference_not_found", false)
        }
        OrderError::TicketNotFound => (StatusCode::NOT_FOUND, "ticket_not_found", false),
        OrderError::TicketPrerequisiteNotFound => (
            StatusCode::NOT_FOUND,
            "ticket_prerequisite_not_found",
            false,
        ),
        OrderError::InvalidQuantity => (StatusCode::BAD_REQUEST, "invalid_quantity", false),
        OrderError::InvalidTicketTitle => (StatusCode::BAD_REQUEST, "invalid_ticket_title", false),
        OrderError::TicketKindDoesNotMatchBuildRecipe => (
            StatusCode::BAD_REQUEST,
            "ticket_kind_does_not_match_build_recipe",
            false,
        ),
        OrderError::TicketTypeMismatch => (StatusCode::BAD_REQUEST, "ticket_type_mismatch", false),
        OrderError::OrderNotStartable => (StatusCode::CONFLICT, "order_not_startable", false),
        OrderError::OrderNotCompletable => (StatusCode::CONFLICT, "order_not_completable", false),
        OrderError::TicketNotStartable => (StatusCode::CONFLICT, "ticket_not_startable", false),
        OrderError::TicketNotCompletable => (StatusCode::CONFLICT, "ticket_not_completable", false),
        OrderError::OrderNotCancelable => (StatusCode::CONFLICT, "order_not_cancelable", false),
        OrderError::OrderNotArchivable => (StatusCode::CONFLICT, "order_not_archivable", false),
        OrderError::OrderNotRestorable => (StatusCode::CONFLICT, "order_not_restorable", false),
        OrderError::TicketNotCancelable => (StatusCode::CONFLICT, "ticket_not_cancelable", false),
        OrderError::TicketNotArchivable => (StatusCode::CONFLICT, "ticket_not_archivable", false),
        OrderError::TicketNotRestorable => (StatusCode::CONFLICT, "ticket_not_restorable", false),
        OrderError::ArithmeticOverflow => (StatusCode::BAD_REQUEST, "arithmetic_overflow", false),
        // Create Epic answers a shortfall with its own drift body (fresh
        // preview included); this is the generic fallback.
        OrderError::ReservationShortfall(_) => {
            (StatusCode::CONFLICT, "reservation_shortfall", true)
        }
        OrderError::OrderNotReservable => (StatusCode::CONFLICT, "order_not_reservable", false),
        OrderError::FrozenPlanUnavailable => {
            (StatusCode::CONFLICT, "frozen_plan_unavailable", false)
        }
        OrderError::AcquisitionRunNotFound => {
            (StatusCode::NOT_FOUND, "acquisition_run_not_found", false)
        }
        OrderError::AcquisitionRunEmpty => {
            (StatusCode::BAD_REQUEST, "acquisition_run_empty", false)
        }
        OrderError::TicketNotBatchable => (StatusCode::CONFLICT, "ticket_not_batchable", false),
        OrderError::AcquisitionRunCrossesIncompatibleLocation => (
            StatusCode::CONFLICT,
            "acquisition_run_crosses_incompatible_location",
            false,
        ),
        OrderError::AcquisitionRunNotReady => {
            (StatusCode::CONFLICT, "acquisition_run_not_ready", false)
        }
        OrderError::AcquisitionRunNotInProgress => (
            StatusCode::CONFLICT,
            "acquisition_run_not_in_progress",
            false,
        ),
        OrderError::CostRequired => (StatusCode::BAD_REQUEST, "cost_required", false),
        OrderError::RecordingRequiresAcquisitionTicket => (
            StatusCode::CONFLICT,
            "recording_requires_acquisition_ticket",
            false,
        ),
        OrderError::RecordingNotAllowedForBatchedTicket => (
            StatusCode::CONFLICT,
            "recording_not_allowed_for_batched_ticket",
            false,
        ),
        OrderError::RecordingRequiresProductionTicket => (
            StatusCode::CONFLICT,
            "recording_requires_production_ticket",
            false,
        ),
        OrderError::RecordingOutputTypeMismatch => (
            StatusCode::CONFLICT,
            "recording_output_type_mismatch",
            false,
        ),
        OrderError::RecordingInputNotAPrerequisite => (
            StatusCode::CONFLICT,
            "recording_input_not_a_prerequisite",
            false,
        ),
        OrderError::RecordingNotFound => (StatusCode::NOT_FOUND, "recording_not_found", false),
        OrderError::RecordingAlreadyReversed => {
            (StatusCode::CONFLICT, "recording_already_reversed", false)
        }
        OrderError::RecordingEvidenceInvalid => {
            (StatusCode::CONFLICT, "recording_evidence_invalid", false)
        }
        OrderError::RecordingReversalInvalid => {
            (StatusCode::CONFLICT, "recording_reversal_invalid", false)
        }
        OrderError::InsufficientInventory => {
            (StatusCode::CONFLICT, "insufficient_inventory", false)
        }
        OrderError::Persistence(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_unavailable",
            true,
        ),
        OrderError::CorruptProductionGraph { .. } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "corrupt_production_graph",
            false,
        ),
        OrderError::FrozenCostNotConserved { .. } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "frozen_cost_not_conserved",
            false,
        ),
        OrderError::FrozenProducerTicketUnavailable { .. } => (
            StatusCode::CONFLICT,
            "frozen_producer_ticket_unavailable",
            false,
        ),
    };
    (
        status,
        ErrorBody {
            code,
            message: error.to_string(),
            fields: None,
            retryable,
            correlation_id: None,
        },
    )
}
