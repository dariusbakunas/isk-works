use super::*;

pub(super) fn to_u64(value: i64) -> Result<u64, StoreError> {
    value.try_into().map_err(|_| StoreError::InvalidData)
}

pub(super) fn invalid_projection(_field: &str) -> StoreError {
    StoreError::InvalidData
}

pub(super) fn map_sqlx(error: sqlx::Error) -> StoreError {
    crate::pool_metrics::observe_error(&error);
    StoreError::Persistence(error.to_string())
}

pub(super) fn map_json(error: serde_json::Error) -> StoreError {
    StoreError::Persistence(error.to_string())
}
