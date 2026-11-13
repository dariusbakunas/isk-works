use super::*;

/// A store port's error, independent of the inventory ledger. Validation
/// and out-of-range input are the caller's to fix; a missing record or a
/// stale revision are curated outcomes; invalid stored data and storage
/// failures are internal (redacted by `internal_failure`).
pub(super) fn store_response(error: StoreError) -> (StatusCode, ErrorBody) {
    let (status, code, retryable) = match &error {
        StoreError::Validation(_) | StoreError::OutOfRange => {
            (StatusCode::BAD_REQUEST, "validation_failed", false)
        }
        StoreError::NotFound => (StatusCode::NOT_FOUND, "record_not_found", false),
        StoreError::Conflict => (StatusCode::CONFLICT, "revision_conflict", false),
        StoreError::InvalidData | StoreError::Persistence(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_unavailable",
            true,
        ),
    };
    (status, ErrorBody::new(code, error.to_string(), retryable))
}
