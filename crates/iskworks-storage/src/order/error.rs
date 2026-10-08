use super::*;

pub(super) fn map_error(error: sqlx::Error) -> OrderError {
    OrderError::Persistence(error.to_string())
}

/// A Postgres `unique_violation` (SQLSTATE 23505) -- the DB-authoritative
/// backstop for `(ticket_id, idempotency_key)` when two duplicate
/// `record_ticket_acquisition` requests race past the ticket row lock.
pub(super) fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505")
    )
}

pub(super) fn i64_from_u64(value: u64) -> Result<i64, OrderError> {
    i64::try_from(value)
        .map_err(|_| OrderError::Persistence("integer exceeds database range".to_string()))
}

pub(super) fn u64_from_i64(value: i64) -> Result<u64, OrderError> {
    u64::try_from(value)
        .map_err(|_| OrderError::Persistence("invalid negative stored integer".to_string()))
}
