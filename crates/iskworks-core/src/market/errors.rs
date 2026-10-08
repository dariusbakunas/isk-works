//! `MarketError` and its serializable projection.

use thiserror::Error;

use super::types::MarketImportProblem;

#[derive(Debug, Error)]
pub enum MarketError {
    #[error("{0}")]
    InvalidUpload(String),
    #[error("market export is empty")]
    Empty,
    #[error("market export exceeds the upload limit")]
    TooLarge,
    #[error("market export header is invalid: {0}")]
    InvalidHeader(String),
    #[error("market export row {row} column {column} is invalid: {message}")]
    InvalidRow {
        row: u32,
        column: String,
        message: String,
    },
    #[error("market export contains mixed {0}")]
    MixedIdentity(&'static str),
    #[error("market export contains conflicting values for order {0}")]
    ConflictingOrder(i64),
    #[error("market export item does not exist in the active SDE")]
    UnknownType,
    #[error("market import was not found")]
    ImportNotFound,
    #[error("market price source was not found")]
    PriceSourceNotFound,
    #[error("market price source is archived")]
    PriceSourceArchived,
    #[error("market source changed since it was loaded")]
    RevisionConflict,
    #[error("no compatible market orders are available")]
    OrdersUnavailable,
    #[error("available market volume does not cover the requested quantity")]
    InsufficientVolume,
    #[error("market observations are stale")]
    StaleObservations,
    #[error("ESI market order is invalid: {0}")]
    InvalidEsiOrder(String),
    #[error("market price refresh failed: {0}")]
    RefreshFailed(String),
    #[error("market persistence failed: {0}")]
    Persistence(String),
    #[error("{0}")]
    Validation(String),
}

impl MarketError {
    #[must_use]
    pub fn problem(&self) -> MarketImportProblem {
        let (code, row, column) = match self {
            Self::Empty => ("market_export_empty", None, None),
            Self::TooLarge => ("market_export_too_large", None, None),
            Self::InvalidHeader(_) => ("market_export_invalid_header", None, None),
            Self::InvalidRow { row, column, .. } => (
                "market_export_invalid_row",
                Some(*row),
                Some(column.clone()),
            ),
            Self::MixedIdentity(_) => ("market_export_mixed_identity", None, None),
            Self::ConflictingOrder(_) => ("market_export_conflicting_order", None, None),
            Self::UnknownType => ("market_export_unknown_type", None, None),
            Self::InvalidUpload(_) => ("market_import_limit_exceeded", None, None),
            _ => ("market_import_failed", None, None),
        };
        MarketImportProblem {
            code: code.to_string(),
            message: self.to_string(),
            row,
            column,
        }
    }
}
