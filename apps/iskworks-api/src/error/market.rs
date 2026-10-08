use super::*;

pub(super) fn finance_response(error: FinanceError) -> (StatusCode, ErrorBody) {
    let (status, code, retryable) = match &error {
        FinanceError::Validation(_) => (StatusCode::BAD_REQUEST, "validation_failed", false),
        FinanceError::NotFound => (StatusCode::NOT_FOUND, "finance_record_not_found", false),
        FinanceError::Persistence(_) => (
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

pub(super) fn market_response(error: MarketError) -> (StatusCode, ErrorBody) {
    let (status, code, retryable) = match &error {
        MarketError::InvalidUpload(_) => (
            StatusCode::BAD_REQUEST,
            "market_import_limit_exceeded",
            false,
        ),
        MarketError::Empty => (StatusCode::BAD_REQUEST, "market_export_empty", false),
        MarketError::TooLarge => (
            StatusCode::PAYLOAD_TOO_LARGE,
            "market_export_too_large",
            false,
        ),
        MarketError::InvalidHeader(_) => (
            StatusCode::BAD_REQUEST,
            "market_export_invalid_header",
            false,
        ),
        MarketError::InvalidRow { .. } => {
            (StatusCode::BAD_REQUEST, "market_export_invalid_row", false)
        }
        MarketError::MixedIdentity(_) => (
            StatusCode::BAD_REQUEST,
            "market_export_mixed_identity",
            false,
        ),
        MarketError::ConflictingOrder(_) => (
            StatusCode::BAD_REQUEST,
            "market_export_conflicting_order",
            false,
        ),
        MarketError::UnknownType => (StatusCode::BAD_REQUEST, "market_export_unknown_type", false),
        MarketError::ImportNotFound => (StatusCode::NOT_FOUND, "market_import_not_found", false),
        MarketError::PriceSourceNotFound => (
            StatusCode::NOT_FOUND,
            "market_price_source_not_found",
            false,
        ),
        MarketError::PriceSourceArchived => {
            (StatusCode::CONFLICT, "market_price_source_archived", false)
        }
        MarketError::RevisionConflict => (
            StatusCode::CONFLICT,
            "market_price_source_revision_conflict",
            false,
        ),
        MarketError::OrdersUnavailable => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "market_orders_unavailable",
            false,
        ),
        MarketError::InsufficientVolume => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "market_volume_insufficient",
            false,
        ),
        MarketError::StaleObservations => {
            (StatusCode::CONFLICT, "market_observations_stale", false)
        }
        MarketError::InvalidEsiOrder(_) => {
            (StatusCode::BAD_GATEWAY, "esi_market_order_invalid", false)
        }
        MarketError::RefreshFailed(_) => (StatusCode::BAD_GATEWAY, "market_refresh_failed", true),
        MarketError::Persistence(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_unavailable",
            true,
        ),
        MarketError::Validation(_) => (StatusCode::BAD_REQUEST, "validation_failed", false),
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
