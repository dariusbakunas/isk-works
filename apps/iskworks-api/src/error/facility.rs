use super::*;

pub(super) fn facility_response(error: FacilityError) -> (StatusCode, ErrorBody) {
    let (status, code) = facility_error_status(&error);
    (
        status,
        ErrorBody::new(
            code,
            error.to_string(),
            matches!(error, FacilityError::Persistence(_)),
        ),
    )
}

pub(super) fn facility_error_status(error: &FacilityError) -> (StatusCode, &'static str) {
    match error {
        FacilityError::NotFound => (StatusCode::NOT_FOUND, "facility_not_found"),
        FacilityError::Archived => (StatusCode::CONFLICT, "facility_archived"),
        FacilityError::RevisionConflict => (StatusCode::CONFLICT, "facility_revision_conflict"),
        FacilityError::Validation(_) | FacilityError::ArithmeticOverflow => {
            (StatusCode::BAD_REQUEST, "facility_calculation_failed")
        }
        FacilityError::Persistence(_) => {
            (StatusCode::SERVICE_UNAVAILABLE, "persistence_unavailable")
        }
    }
}
