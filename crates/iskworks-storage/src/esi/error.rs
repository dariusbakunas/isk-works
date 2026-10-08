use super::*;

pub(super) fn to_u64(value: i64) -> Result<u64, InventoryError> {
    value
        .try_into()
        .map_err(|_| InventoryError::InvalidProjection)
}

pub(super) fn invalid_projection(_field: &str) -> InventoryError {
    InventoryError::InvalidProjection
}

pub(super) fn map_sqlx(error: sqlx::Error) -> InventoryError {
    InventoryError::Persistence(error.to_string())
}

pub(super) fn map_json(error: serde_json::Error) -> InventoryError {
    InventoryError::Persistence(error.to_string())
}
