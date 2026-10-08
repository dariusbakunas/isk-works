//! Full erase of one workspace and everything in it.
//!
//! Most workspace-scoped tables cascade from `workspaces`, but a web of
//! deliberate `ON DELETE RESTRICT` / `NO ACTION` foreign keys (the inventory
//! ledger, ESI history, orders/tickets, market snapshots; see also
//! `cascade.rs`) means a plain `DELETE FROM workspaces` can never succeed.
//! This deletes child-first, explicitly, in one transaction.
//!
//! Safety properties:
//! * All-or-nothing. If any table still references a row we try to delete
//!   (e.g. a future migration adds a RESTRICT link this list does not know
//!   about), the final delete fails and the whole transaction rolls back --
//!   nothing is half-erased.
//! * Every statement is scoped to this workspace's id (directly, or through
//!   its parent rows), so no other tenant's data is touched.
//! * `erase_guard_tests` in `tests.rs` walks the live schema and fails when a
//!   new RESTRICT/NO ACTION link or FK-less uuid column is not accounted for.

use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::cascade::teardown_tickets;

/// Every table this teardown deletes from explicitly (cascades from these are
/// implicit and not listed). Used by the schema guard test.
#[cfg(test)]
pub(crate) const EXPLICIT_DELETES: &[&str] = &[
    "tickets",
    "order_requirement_fulfillments",
    "ticket_prerequisite_fulfillments",
    "acquisition_runs",
    "orders",
    "production_dependencies",
    "builds",
    "inventory_event_sources",
    "inventory_events",
    "ticket_inventory_recordings",
    "inventory_balances",
    "inventory_allocations",
    "inventory_reconciliation_exclusions",
    "esi_asset_snapshots",
    "esi_wallet_balances",
    "esi_wallet_transactions",
    "esi_sync_runs",
    "blueprint_observations",
    "eve_oauth_pending_authorizations",
    "eve_connections",
    "market_price_snapshot_observations",
    "market_price_snapshot_lines",
    "market_price_snapshots",
    "market_import_file_observations",
    "market_import_files",
    "market_price_source_configs",
    "market_source_coverage",
    "market_order_observations",
    "market_observation_batches",
    "market_import_batches",
    "price_sources",
    "users",
    "owners",
    "workspaces",
];

async fn run(
    tx: &mut Transaction<'_, Postgres>,
    sql: &str,
    workspace_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(sql)
        .bind(workspace_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Erase `workspace_id` and all of its data, including its user (and that
/// user's sessions). Returns `false` if the workspace does not exist.
pub(crate) async fn erase_workspace_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM workspaces WHERE id = $1 FOR UPDATE")
            .bind(workspace_id)
            .fetch_optional(&mut **tx)
            .await?;
    if exists.is_none() {
        return Ok(false);
    }

    // Price snapshots are an immutable audit trail; this transaction (and
    // only this one, `SET LOCAL`) is allowed to delete them. See
    // `migrations/202610030001_allow_workspace_erase_of_price_snapshots.sql`.
    sqlx::query("SELECT set_config('iskworks.erase_workspace', 'on', true)")
        .execute(&mut **tx)
        .await?;

    // Captured up front: recordings have no FK to a ticket or workspace, only
    // inventory events point at them.
    let ticket_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM tickets WHERE workspace_id = $1")
            .bind(workspace_id)
            .fetch_all(&mut **tx)
            .await?;
    let recording_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT ticket_inventory_recording_id
          FROM inventory_events
         WHERE workspace_id = $1 AND ticket_inventory_recording_id IS NOT NULL
        "#,
    )
    .bind(workspace_id)
    .fetch_all(&mut **tx)
    .await?;

    // Orders, tickets, builds.
    teardown_tickets(tx, &ticket_ids).await?;
    run(
        tx,
        "DELETE FROM orders WHERE workspace_id = $1",
        workspace_id,
    )
    .await?;
    run(
        tx,
        "DELETE FROM acquisition_runs WHERE workspace_id = $1",
        workspace_id,
    )
    .await?;
    run(
        tx,
        "DELETE FROM production_dependencies WHERE workspace_id = $1",
        workspace_id,
    )
    .await?;
    run(
        tx,
        "DELETE FROM builds WHERE workspace_id = $1",
        workspace_id,
    )
    .await?;

    // Inventory ledger. Sources point at events and at ESI wallet
    // transactions, so they go first.
    run(
        tx,
        "DELETE FROM inventory_event_sources WHERE workspace_id = $1",
        workspace_id,
    )
    .await?;
    run(
        tx,
        "DELETE FROM inventory_events WHERE workspace_id = $1",
        workspace_id,
    )
    .await?;
    sqlx::query(
        "DELETE FROM ticket_inventory_recordings WHERE id = ANY($1) OR ticket_id = ANY($2)",
    )
    .bind(&recording_ids)
    .bind(&ticket_ids)
    .execute(&mut **tx)
    .await?;
    run(
        tx,
        "DELETE FROM inventory_balances WHERE workspace_id = $1",
        workspace_id,
    )
    .await?;
    run(
        tx,
        "DELETE FROM inventory_allocations WHERE workspace_id = $1",
        workspace_id,
    )
    .await?;
    run(
        tx,
        "DELETE FROM inventory_reconciliation_exclusions WHERE workspace_id = $1",
        workspace_id,
    )
    .await?;

    // ESI: observations hang off connections and sync runs (both RESTRICT).
    for table in [
        "esi_asset_snapshots",
        "esi_wallet_balances",
        "esi_wallet_transactions",
    ] {
        run(
            tx,
            &format!(
                "DELETE FROM {table} WHERE connection_id IN \
                 (SELECT id FROM eve_connections WHERE workspace_id = $1)"
            ),
            workspace_id,
        )
        .await?;
    }
    run(
        tx,
        "DELETE FROM esi_sync_runs WHERE workspace_id = $1",
        workspace_id,
    )
    .await?;
    run(
        tx,
        "DELETE FROM blueprint_observations WHERE workspace_id = $1",
        workspace_id,
    )
    .await?;
    run(
        tx,
        "DELETE FROM eve_oauth_pending_authorizations WHERE workspace_id = $1",
        workspace_id,
    )
    .await?;
    // Refresh tokens cascade from the connection.
    run(
        tx,
        "DELETE FROM eve_connections WHERE workspace_id = $1",
        workspace_id,
    )
    .await?;

    // Market data and price sources.
    for table in [
        "market_price_snapshot_observations",
        "market_price_snapshot_lines",
    ] {
        run(
            tx,
            &format!(
                "DELETE FROM {table} WHERE market_price_snapshot_id IN \
                 (SELECT id FROM market_price_snapshots WHERE workspace_id = $1)"
            ),
            workspace_id,
        )
        .await?;
    }
    for table in [
        "market_price_snapshots",
        "market_import_file_observations",
        "market_import_files",
        "market_price_source_configs",
        "market_source_coverage",
        "market_order_observations",
        "market_observation_batches",
        "market_import_batches",
        "price_sources",
    ] {
        run(
            tx,
            &format!("DELETE FROM {table} WHERE workspace_id = $1"),
            workspace_id,
        )
        .await?;
    }

    // The account itself (sessions cascade), then the owner/workspace pair.
    // `workspaces.owner_id -> owners` is the one deferrable constraint: owners
    // go first, the workspace row right after, checked at commit.
    run(
        tx,
        "DELETE FROM users WHERE workspace_id = $1",
        workspace_id,
    )
    .await?;
    sqlx::query("SET CONSTRAINTS ALL DEFERRED")
        .execute(&mut **tx)
        .await?;
    run(
        tx,
        "DELETE FROM owners WHERE workspace_id = $1",
        workspace_id,
    )
    .await?;
    run(tx, "DELETE FROM workspaces WHERE id = $1", workspace_id).await?;
    Ok(true)
}
