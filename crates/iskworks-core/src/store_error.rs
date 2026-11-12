//! The error a non-inventory repository port returns: input it rejects, a
//! record it doesn't have, a concurrent change, stored data it can't read,
//! a value out of range, or a storage failure. Inventory has its own richer `InventoryError`;
//! a `StoreError` converts into it without losing its kind.

use thiserror::Error;

use crate::InventoryError;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StoreError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("record was not found")]
    NotFound,
    #[error("record changed while this form was open")]
    Conflict,
    /// A stored value is inconsistent with its row.
    #[error("stored data is invalid")]
    InvalidData,
    /// A value doesn't fit its column, or arithmetic on it overflowed.
    #[error("value is out of the supported range")]
    OutOfRange,
    #[error("persistence failed: {0}")]
    Persistence(String),
}

impl From<StoreError> for InventoryError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Validation(message) => Self::Validation(message),
            StoreError::NotFound => Self::ItemNotFound,
            StoreError::Conflict => Self::RevisionConflict,
            StoreError::InvalidData => Self::InvalidProjection,
            StoreError::OutOfRange => Self::ArithmeticOverflow,
            StoreError::Persistence(message) => Self::Persistence(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_into_inventory_error_by_kind() {
        let cases = [
            (
                StoreError::Validation("bad".into()),
                "validation failed: bad",
            ),
            (StoreError::NotFound, "inventory item was not found"),
            (
                StoreError::Conflict,
                "inventory changed while this form was open",
            ),
            (
                StoreError::InvalidData,
                "inventory projection is inconsistent",
            ),
            (
                StoreError::OutOfRange,
                "inventory arithmetic exceeds the supported range",
            ),
            (
                StoreError::Persistence("down".into()),
                "persistence failed: down",
            ),
        ];
        for (store, inventory) in cases {
            assert_eq!(InventoryError::from(store).to_string(), inventory);
        }
    }
}
