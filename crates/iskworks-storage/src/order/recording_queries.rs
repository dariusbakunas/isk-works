use super::*;

/// The `(ticket_id, idempotency_key)` recording, if it already exists --
/// the fast path for an idempotent `record_ticket_acquisition` replay.
pub(super) async fn find_ticket_recording_by_key(
    tx: &mut Transaction<'_, Postgres>,
    ticket_id: TicketId,
    idempotency_key: Uuid,
) -> Result<Option<TicketInventoryRecording>, OrderError> {
    sqlx::query_as::<_, TicketInventoryRecordingRow>(
        "SELECT id, ticket_id, kind, recorded_quantity, runs_completed, \
         installation_cost, output_type_id, output_quantity, location_note, note, recorded_at, reverted_at \
         FROM ticket_inventory_recordings WHERE ticket_id = $1 AND idempotency_key = $2",
    )
    .bind(ticket_id.0)
    .bind(idempotency_key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_error)?
    .map(TicketInventoryRecordingRow::into_recording)
    .transpose()
}

/// `SUM(recorded_quantity)` across every acquisition recording of
/// `ticket_id` (production rows have `recorded_quantity NULL` and are
/// skipped) -- the `recorded` input to `derive_recording_summary`.
pub(super) async fn recorded_quantity_sum(
    tx: &mut Transaction<'_, Postgres>,
    ticket_id: TicketId,
) -> Result<u64, OrderError> {
    let sum: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(recorded_quantity), 0)::bigint \
         FROM ticket_inventory_recordings WHERE ticket_id = $1 AND reverted_at IS NULL",
    )
    .bind(ticket_id.0)
    .fetch_one(&mut **tx)
    .await
    .map_err(map_error)?;
    u64_from_i64(sum)
}

/// `SUM(runs_completed)` across every production recording of `ticket_id`.
pub(super) async fn recorded_runs_sum(
    tx: &mut Transaction<'_, Postgres>,
    ticket_id: TicketId,
) -> Result<u64, OrderError> {
    let sum: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(runs_completed), 0)::bigint \
         FROM ticket_inventory_recordings WHERE ticket_id = $1 AND kind = 'production' \
         AND reverted_at IS NULL",
    )
    .bind(ticket_id.0)
    .fetch_one(&mut **tx)
    .await
    .map_err(map_error)?;
    u64_from_i64(sum)
}

/// The planned run count frozen on a Manufacturing/Reaction ticket, if a
/// plan was captured. `Ok(None)` when `execution_snapshot IS NULL` (the
/// current ticket-creation path does not populate it) -- callers default
/// `requested` to the recorded amount in that case. A present-but-corrupt
/// snapshot is a `Persistence` error, same convention as `into_ticket`.
pub(super) fn execution_snapshot_runs(
    value: &Option<serde_json::Value>,
) -> Result<Option<u64>, OrderError> {
    match value {
        None => Ok(None),
        Some(json) => {
            let snapshot: TaskExecutionSnapshot =
                serde_json::from_value(json.clone()).map_err(|error| {
                    OrderError::Persistence(format!("invalid stored execution snapshot: {error}"))
                })?;
            Ok(Some(snapshot.runs))
        }
    }
}
