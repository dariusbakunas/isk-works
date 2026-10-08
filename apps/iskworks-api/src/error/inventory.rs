use super::*;

pub(super) fn inventory_response(error: InventoryError) -> (StatusCode, ErrorBody) {
    let (status, code, retryable) = match &error {
        InventoryError::Validation(_)
        | InventoryError::InvalidMoney
        | InventoryError::AcknowledgementRequired(_)
        | InventoryError::NegativeBalance
        | InventoryError::IdentityMismatch
        | InventoryError::InvalidProjection
        | InventoryError::ArithmeticOverflow => {
            (StatusCode::BAD_REQUEST, "validation_failed", false)
        }
        InventoryError::ItemNotFound => (StatusCode::NOT_FOUND, "inventory_item_not_found", false),
        InventoryError::EventNotFound => {
            (StatusCode::NOT_FOUND, "inventory_event_not_found", false)
        }
        InventoryError::RevisionConflict => (StatusCode::CONFLICT, "revision_conflict", false),
        InventoryError::OpeningBalanceAlreadyExists
        | InventoryError::LatestEventOnly
        | InventoryError::AlreadyReversed => {
            (StatusCode::CONFLICT, "invalid_inventory_state", false)
        }
        InventoryError::BuildConsumptionNotReversible => (
            StatusCode::CONFLICT,
            "build_consumption_not_reversible",
            false,
        ),
        InventoryError::BuildOutputNotReversible => {
            (StatusCode::CONFLICT, "build_output_not_reversible", false)
        }
        InventoryError::Persistence(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_unavailable",
            true,
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

pub(super) fn production_response(error: ProductionError) -> (StatusCode, ErrorBody) {
    let (status, code, retryable) = match &error {
        ProductionError::BuildNotFound => (StatusCode::NOT_FOUND, "build_not_found", false),
        ProductionError::ArithmeticOverflow => {
            (StatusCode::BAD_REQUEST, "arithmetic_overflow", false)
        }
        ProductionError::ReconciliationContributorChanged => (
            StatusCode::CONFLICT,
            "reconciliation_contributor_changed",
            false,
        ),
        ProductionError::Persistence(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_unavailable",
            true,
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
