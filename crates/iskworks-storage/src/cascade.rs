//! Shared FK-safe teardown for `tickets` / `orders` -- the deliberate
//! `ON DELETE RESTRICT` foreign keys that "freeze" a plan's sourcing (see
//! `migrations/202608230004_freeze_requirement_and_prerequisite_sourcing.sql`
//! and `202608230002_orders_and_tickets.sql`) mean these rows can never be
//! removed by a plain `DELETE`. `delete_build(force)`, `delete_order` and
//! `delete_ticket` all need the same ordered cleanup, so it lives here once.

use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Delete `tickets` by id, first clearing every `RESTRICT` child that would
/// block them:
///
/// * `order_requirement_fulfillments` / `ticket_prerequisite_fulfillments`
///   that point at the ticket,
/// * an `acquisition_runs` row a ticket solely owns (its items cascade),
///
/// `ticket_prerequisites` (and their own cascaded children) go automatically.
/// `ticket_inventory_recordings` deliberately survive: their `ticket_id` is
/// immutable historical provenance, not a live FK or owned child.
/// No-op on an empty slice.
pub(crate) async fn teardown_tickets(
    tx: &mut Transaction<'_, Postgres>,
    ticket_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    if ticket_ids.is_empty() {
        return Ok(());
    }
    sqlx::query("DELETE FROM order_requirement_fulfillments WHERE ticket_id = ANY($1)")
        .bind(ticket_ids)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "DELETE FROM ticket_prerequisite_fulfillments WHERE fulfilling_ticket_id = ANY($1)",
    )
    .bind(ticket_ids)
    .execute(&mut **tx)
    .await?;
    // An acquisition run is the execution vehicle of one acquisition ticket;
    // drop it (its `acquisition_run_items` cascade) when no *other* ticket
    // still references it.
    sqlx::query(
        r#"
        DELETE FROM acquisition_runs ar
        WHERE ar.id IN (
                SELECT acquisition_run_id FROM tickets
                WHERE id = ANY($1) AND acquisition_run_id IS NOT NULL
            )
          AND NOT EXISTS (
                SELECT 1 FROM tickets other
                WHERE other.acquisition_run_id = ar.id AND other.id <> ALL($1)
            )
        "#,
    )
    .bind(ticket_ids)
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM tickets WHERE id = ANY($1)")
        .bind(ticket_ids)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
