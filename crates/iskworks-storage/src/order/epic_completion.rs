//! Epic auto-completion inside the ticket write that finishes the root.
//!
//! `complete_ticket`, `set_ticket_status` / `update_ticket` and
//! `record_ticket_production` call [`complete_epic_if_finished`] in their
//! own transaction, before committing. The rule itself is core's
//! `epic_completes_after`; this module only loads its inputs and applies
//! the completion, so the ticket write and the Epic completion commit or
//! roll back together.

use super::*;
use iskworks_core::order::{epic_completes_after, may_complete_epic};

/// Completes `ticket_id`'s Epic when the ticket, as written so far in `tx`,
/// is its finished root. A no-op for any other ticket.
pub(super) async fn complete_epic_if_finished(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: WorkspaceId,
    ticket_id: TicketId,
    now: DateTime<Utc>,
) -> Result<(), OrderError> {
    let ticket = sqlx::query_as::<_, TicketRow>(
        "SELECT id, workspace_id, owner_id, display_id, kind, type_id, captured_name, \
         quantity, order_id, source_build_id, notes, assignee_character_id, status, estimated_unit_cost, estimated_line_total, \
         actual_unit_cost, actual_line_total, market_region_id, market_location_id, \
         price_source_id, acquisition_run_id, acquired_quantity, \
         execution_snapshot, created_at, updated_at, completed_at, archived_at, \
         occurrence_key, parent_ticket_id, produced_quantity, material_component_cost, \
         own_installation_cost, total_production_cost, plan_evidence, merged_into_ticket_id \
         FROM tickets WHERE workspace_id = $1 AND id = $2",
    )
    .bind(workspace_id.0)
    .bind(ticket_id.0)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_error)?
    .ok_or(OrderError::TicketNotFound)?
    .into_ticket()?;
    if !may_complete_epic(&ticket) {
        return Ok(());
    }
    let Some(order_id) = ticket.order_id else {
        return Ok(());
    };
    // Locks the Epic, so a concurrent cancel or completion waits for this
    // transaction and then sees the result.
    let order = sqlx::query_as::<_, OrderRow>(
        "SELECT id, workspace_id, owner_id, source_build_id, source_build_revision, \
         display_name, runs, recipe_fingerprint, price_snapshot_id, estimated_material_cost, \
         expected_revenue, estimated_margin, missing_price_count, created_at, updated_at, \
         started_at, completed_at, canceled_at, archived_at, planning_snapshot_version \
         FROM orders WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(workspace_id.0)
    .bind(order_id.0)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_error)?
    .ok_or(OrderError::OrderNotFound)?
    .into_order()?;
    let recorded_runs = recording_queries::recorded_runs_sum(tx, ticket_id).await?;
    if epic_completes_after(&ticket, &order, recorded_runs) {
        complete_open_order(tx, workspace_id, order_id, now).await?;
    }
    Ok(())
}

/// Completes an open Epic and releases its leftover reservations. `false`
/// when it was already completed or canceled.
pub(super) async fn complete_open_order(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: WorkspaceId,
    order_id: OrderId,
    now: DateTime<Utc>,
) -> Result<bool, OrderError> {
    let result = sqlx::query(
        "UPDATE orders SET started_at = COALESCE(started_at, $1), completed_at = $1, \
         updated_at = $1 \
         WHERE workspace_id = $2 AND id = $3 \
         AND completed_at IS NULL AND canceled_at IS NULL",
    )
    .bind(now)
    .bind(workspace_id.0)
    .bind(order_id.0)
    .execute(&mut **tx)
    .await
    .map_err(map_error)?;
    if result.rows_affected() == 0 {
        return Ok(false);
    }
    super::reservations::release_order_allocations(tx, order_id, now).await?;
    Ok(true)
}
