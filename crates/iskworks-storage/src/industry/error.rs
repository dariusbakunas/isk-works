use super::*;

pub(super) fn map_error(error: sqlx::Error) -> IndustryError {
    IndustryError::Persistence(error.to_string())
}

pub(super) fn i64_from_u64(value: u64) -> Result<i64, IndustryError> {
    i64::try_from(value)
        .map_err(|_| IndustryError::Persistence("integer exceeds database range".to_string()))
}

pub(super) fn u64_from_i64(value: i64) -> Result<u64, IndustryError> {
    u64::try_from(value)
        .map_err(|_| IndustryError::Persistence("invalid negative stored integer".to_string()))
}
