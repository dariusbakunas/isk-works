use super::*;

pub(super) fn facility_response(error: FacilityError) -> (StatusCode, ErrorBody) {
    let (status, code) = facility_error_status(&error);
    (
        status,
        ErrorBody {
            code,
            message: error.to_string(),
            fields: None,
            retryable: matches!(error, FacilityError::Persistence(_)),
            correlation_id: None,
        },
    )
}

pub(super) fn opportunity_response(error: OpportunityError) -> (StatusCode, ErrorBody) {
    let (status, code, retryable) = match &error {
        OpportunityError::Industry(IndustryError::PriceSourceNotFound)
        | OpportunityError::Market(MarketError::PriceSourceNotFound) => {
            (StatusCode::NOT_FOUND, "price_source_not_found", false)
        }
        OpportunityError::Industry(IndustryError::RevisionConflict)
        | OpportunityError::Facility(FacilityError::RevisionConflict) => {
            (StatusCode::CONFLICT, "revision_conflict", false)
        }
        OpportunityError::Industry(IndustryError::Facility(FacilityError::RevisionConflict)) => {
            (StatusCode::CONFLICT, "facility_revision_conflict", false)
        }
        OpportunityError::Industry(IndustryError::Facility(FacilityError::Archived))
        | OpportunityError::Facility(FacilityError::Archived) => {
            (StatusCode::CONFLICT, "facility_archived", false)
        }
        OpportunityError::Industry(IndustryError::Facility(FacilityError::NotFound))
        | OpportunityError::Facility(FacilityError::NotFound) => {
            (StatusCode::NOT_FOUND, "facility_not_found", false)
        }
        OpportunityError::Industry(IndustryError::Facility(
            FacilityError::Validation(_) | FacilityError::ArithmeticOverflow,
        ))
        | OpportunityError::Facility(
            FacilityError::Validation(_) | FacilityError::ArithmeticOverflow,
        ) => (
            StatusCode::BAD_REQUEST,
            "facility_calculation_failed",
            false,
        ),
        OpportunityError::Blueprint(
            BlueprintError::SelectionRequired
            | BlueprintError::ManualKindUnknown
            | BlueprintError::InvalidMaterialEfficiency
            | BlueprintError::InvalidTimeEfficiency
            | BlueprintError::CopyRunsRequired
            | BlueprintError::CopyRunsInsufficient { .. },
        ) => (StatusCode::BAD_REQUEST, "validation_failed", false),
        OpportunityError::StaticData(_)
        | OpportunityError::AdjustedPrices(_)
        | OpportunityError::Industry(IndustryError::Persistence(_))
        | OpportunityError::Market(MarketError::Persistence(_))
        | OpportunityError::Industry(IndustryError::Facility(FacilityError::Persistence(_)))
        | OpportunityError::Facility(FacilityError::Persistence(_)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "opportunity_evaluation_unavailable",
            true,
        ),
        _ => (
            StatusCode::BAD_REQUEST,
            "opportunity_evaluation_failed",
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
