//! The bare ticket writes behind `TicketStore::update_ticket`, run in
//! the caller's transaction.

use super::*;

/// A metadata write. See `TicketMetadataUpdate` for the three-value
/// omitted/clear/set semantics of each field. Writes only the fields
/// present, never `status`, recordings, the execution snapshot, the source
/// Build, requirement fulfillments or Acquisition Run membership. An empty
/// update writes nothing.
pub(super) async fn write_ticket_metadata(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: WorkspaceId,
    ticket_id: TicketId,
    update: TicketMetadataUpdate,
    now: DateTime<Utc>,
) -> Result<(), OrderError> {
    if update.captured_name.is_none()
        && update.notes.is_none()
        && update.assignee_character_id.is_none()
    {
        return Ok(());
    }
    let mut query = sqlx::QueryBuilder::<Postgres>::new("UPDATE tickets SET updated_at = ");
    query.push_bind(now);
    if let Some(captured_name) = update.captured_name {
        query.push(", captured_name = ").push_bind(captured_name);
    }
    if let Some(notes) = update.notes {
        query.push(", notes = ").push_bind(notes);
    }
    if let Some(assignee_character_id) = update.assignee_character_id {
        query
            .push(", assignee_character_id = ")
            .push_bind(assignee_character_id.map(|id| id.0));
    }
    query
        .push(" WHERE workspace_id = ")
        .push_bind(workspace_id.0);
    query.push(" AND id = ").push_bind(ticket_id.0);
    let result = query.build().execute(&mut **tx).await.map_err(map_error)?;
    if result.rows_affected() == 0 {
        return Err(OrderError::TicketNotFound);
    }
    Ok(())
}

/// A bare workflow-status write: `status`, `completed_at` and `updated_at`.
/// `completed_at` is stamped on entering Complete, kept on a repeated
/// Complete write and cleared on leaving it. Every transition is legal
/// (unlike `start_ticket`/`cancel_ticket`), so Board drags move both ways.
/// A same-status write still matches its row.
pub(super) async fn write_ticket_status(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: WorkspaceId,
    ticket_id: TicketId,
    status: TicketStatus,
    now: DateTime<Utc>,
) -> Result<(), OrderError> {
    let result = sqlx::query(
        // The right-hand `status` is the pre-update value.
        "UPDATE tickets SET status = $1, updated_at = $2, \
         completed_at = CASE WHEN $1 <> 'complete' THEN NULL \
                             WHEN status = 'complete' THEN completed_at \
                             ELSE $2 END \
         WHERE workspace_id = $3 AND id = $4",
    )
    .bind(ticket_status_str(status))
    .bind(now)
    .bind(workspace_id.0)
    .bind(ticket_id.0)
    .execute(&mut **tx)
    .await
    .map_err(map_error)?;
    if result.rows_affected() == 0 {
        return Err(OrderError::TicketNotFound);
    }
    Ok(())
}
