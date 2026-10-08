//! Acquisition Run batching for standalone `order::Ticket`s. Uses the
//! entity-agnostic `acquisition_runs`/`acquisition_run_items` tables and the
//! `AcquisitionRun`/`AcquisitionRunItem`/`AcquisitionRunId` domain types
//! (crate-root `iskworks_core`); member `order::Ticket`s are returned
//! separately rather than embedded in the run itself.
//!
//! `OrderRepository`'s trait methods for these operations
//! (`crates/iskworks-core/src/order/repository.rs`) delegate straight into
//! the free functions here; they can't live in a second
//! `impl OrderRepository for PgOrderRepository` block (Rust forbids two
//! impls of the same trait for the same type), so `repository_impl.rs`'s
//! impl block keeps one-line bodies calling into this module.

use super::inventory_posting::{post_purchase_in_transaction, InventoryPostingLine};
use super::*;

#[derive(sqlx::FromRow)]
struct AcquisitionRunRow {
    id: Uuid,
    workspace_id: Uuid,
    owner_id: Uuid,
    display_id: String,
    name: String,
    kind: String,
    status: String,
    market_region_id: Option<i64>,
    market_location_id: Option<i64>,
    price_source_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
}

fn acquisition_run_kind_from_str(
    value: &str,
) -> Result<iskworks_core::AcquisitionRunKind, OrderError> {
    match value {
        "acquisition" => Ok(iskworks_core::AcquisitionRunKind::Acquisition),
        other => Err(OrderError::Persistence(format!(
            "unknown acquisition run kind {other}"
        ))),
    }
}

fn acquisition_run_status_from_str(
    value: &str,
) -> Result<iskworks_core::AcquisitionRunStatus, OrderError> {
    match value {
        "ready" => Ok(iskworks_core::AcquisitionRunStatus::Ready),
        "in_progress" => Ok(iskworks_core::AcquisitionRunStatus::InProgress),
        "complete" => Ok(iskworks_core::AcquisitionRunStatus::Complete),
        other => Err(OrderError::Persistence(format!(
            "unknown acquisition run status {other}"
        ))),
    }
}

impl AcquisitionRunRow {
    fn into_run(self) -> Result<AcquisitionRun, OrderError> {
        Ok(AcquisitionRun {
            id: AcquisitionRunId(self.id),
            workspace_id: WorkspaceId(self.workspace_id),
            owner_id: OwnerId(self.owner_id),
            display_id: self.display_id,
            name: self.name,
            kind: acquisition_run_kind_from_str(&self.kind)?,
            status: acquisition_run_status_from_str(&self.status)?,
            market_region_id: self.market_region_id,
            market_location_id: self.market_location_id,
            price_source_id: self.price_source_id.map(PriceSourceId),
            created_at: self.created_at,
            updated_at: self.updated_at,
            started_at: self.started_at,
            completed_at: self.completed_at,
        })
    }
}

async fn load_order_acquisition_run(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    run_id: AcquisitionRunId,
) -> Result<AcquisitionRun, OrderError> {
    sqlx::query_as::<_, AcquisitionRunRow>(
        "SELECT id, workspace_id, owner_id, display_id, name, kind, status, \
         market_region_id, market_location_id, price_source_id, \
         created_at, updated_at, started_at, completed_at \
         FROM acquisition_runs WHERE id = $1 AND workspace_id = $2",
    )
    .bind(run_id.0)
    .bind(workspace_id.0)
    .fetch_optional(pool)
    .await
    .map_err(map_error)?
    .ok_or(OrderError::AcquisitionRunNotFound)?
    .into_run()
}

/// One ticket's batching-compatibility identity: a
/// resolved market scope takes priority, falling back to `price_source_id`
/// only for a manually-priced ticket (which has no scope at all). Two
/// tickets are only ever compatible if they resolve to the exact same key
/// -- a market-scoped ticket can never group with a manually-priced one,
/// even by coincidence, since they're different `BatchKey` variants.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum BatchKey {
    Scope(i64, Option<i64>),
    ManualSource(Uuid),
}

/// Locks every candidate ticket (scoped to this caller) so a concurrent
/// create can't also claim one, and resolves each one's `BatchKey` in the
/// same query (a standalone ticket carries its own scope/`price_source_id`,
/// so no join is needed).
pub(super) async fn create_order_acquisition_run(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    name: Option<String>,
    ticket_ids: Vec<TicketId>,
) -> Result<AcquisitionRun, OrderError> {
    if ticket_ids.is_empty() {
        return Err(OrderError::AcquisitionRunEmpty);
    }
    let mut tx = pool.begin().await.map_err(map_error)?;

    let raw_ids: Vec<Uuid> = ticket_ids.iter().map(|id| id.0).collect();
    #[allow(clippy::type_complexity)]
    let rows: Vec<(
        Uuid,
        String,
        String,
        bool,
        Option<i64>,
        Option<i64>,
        Option<Uuid>,
    )> = sqlx::query_as(
        "SELECT id, kind, status, (acquisition_run_id IS NOT NULL), \
             market_region_id, market_location_id, price_source_id \
             FROM tickets WHERE id = ANY($1) AND workspace_id = $2 AND owner_id = $3 \
             FOR UPDATE",
    )
    .bind(&raw_ids)
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .fetch_all(&mut *tx)
    .await
    .map_err(map_error)?;

    if rows.len() != raw_ids.len() {
        return Err(OrderError::TicketNotFound);
    }

    let mut shared_key: Option<BatchKey> = None;
    for (_, kind, status, batched, market_region_id, market_location_id, price_source_id) in &rows {
        if kind != "acquisition" || status != "todo" || *batched {
            return Err(OrderError::TicketNotBatchable);
        }
        let key = match market_region_id {
            Some(region_id) => BatchKey::Scope(*region_id, *market_location_id),
            None => match price_source_id {
                Some(source_id) => BatchKey::ManualSource(*source_id),
                None => return Err(OrderError::TicketNotBatchable),
            },
        };
        match shared_key {
            None => shared_key = Some(key),
            Some(existing) if existing != key => {
                return Err(OrderError::AcquisitionRunCrossesIncompatibleLocation);
            }
            Some(_) => {}
        }
    }
    // Safe: `rows` is non-empty (checked above) and every element either
    // sets `shared_key` or returns early.
    let (market_region_id, market_location_id, price_source_id) =
        match shared_key.ok_or(OrderError::AcquisitionRunEmpty)? {
            BatchKey::Scope(region_id, location_id) => (Some(region_id), location_id, None),
            BatchKey::ManualSource(source_id) => (None, None, Some(source_id)),
        };

    let run_id = Uuid::new_v4();
    let display_id: String = sqlx::query_scalar(
        "SELECT 'ACQ-' || lpad(nextval('acquisition_run_display_id_seq')::text, 4, '0')",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(map_error)?;
    let name = name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| display_id.clone());
    let now = crate::db_now();

    sqlx::query(
        "INSERT INTO acquisition_runs \
         (id, workspace_id, owner_id, display_id, name, kind, status, \
          market_region_id, market_location_id, price_source_id, \
          created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, 'acquisition', 'ready', $6, $7, $8, $9, $9)",
    )
    .bind(run_id)
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(&display_id)
    .bind(&name)
    .bind(market_region_id)
    .bind(market_location_id)
    .bind(price_source_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(map_error)?;

    sqlx::query(
        "UPDATE tickets SET acquisition_run_id = $1, acquired_quantity = 0 WHERE id = ANY($2)",
    )
    .bind(run_id)
    .bind(&raw_ids)
    .execute(&mut *tx)
    .await
    .map_err(map_error)?;

    tx.commit().await.map_err(map_error)?;
    load_order_acquisition_run(pool, workspace_id, AcquisitionRunId(run_id)).await
}

pub(super) async fn list_order_acquisition_runs(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
) -> Result<Vec<AcquisitionRun>, OrderError> {
    let rows = sqlx::query_as::<_, AcquisitionRunRow>(
        "SELECT id, workspace_id, owner_id, display_id, name, kind, status, \
         market_region_id, market_location_id, price_source_id, \
         created_at, updated_at, started_at, completed_at \
         FROM acquisition_runs WHERE workspace_id = $1 AND owner_id = $2 ORDER BY updated_at DESC",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .fetch_all(pool)
    .await
    .map_err(map_error)?;
    rows.into_iter().map(AcquisitionRunRow::into_run).collect()
}

pub(super) async fn get_order_acquisition_run(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    run_id: AcquisitionRunId,
) -> Result<AcquisitionRun, OrderError> {
    load_order_acquisition_run(pool, workspace_id, run_id).await
}

pub(super) async fn list_order_acquisition_run_tickets(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    run_id: AcquisitionRunId,
) -> Result<Vec<Ticket>, OrderError> {
    let rows = sqlx::query_as::<_, TicketRow>(
        "SELECT id, workspace_id, owner_id, display_id, kind, type_id, captured_name, \
         quantity, order_id, source_build_id, notes, assignee_character_id, status, estimated_unit_cost, estimated_line_total, \
         actual_unit_cost, actual_line_total, market_region_id, market_location_id, \
         price_source_id, acquisition_run_id, \
         acquired_quantity, execution_snapshot, created_at, updated_at, archived_at, \
         occurrence_key, parent_ticket_id, produced_quantity, material_component_cost, \
         own_installation_cost, total_production_cost, plan_evidence \
         FROM tickets WHERE acquisition_run_id = $1 AND workspace_id = $2 ORDER BY display_id",
    )
    .bind(run_id.0)
    .bind(workspace_id.0)
    .fetch_all(pool)
    .await
    .map_err(map_error)?;
    rows.into_iter().map(TicketRow::into_ticket).collect()
}

pub(super) async fn list_order_acquisition_run_items(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    run_id: AcquisitionRunId,
) -> Result<Vec<AcquisitionRunItem>, OrderError> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM acquisition_runs WHERE id = $1 AND workspace_id = $2",
    )
    .bind(run_id.0)
    .bind(workspace_id.0)
    .fetch_optional(pool)
    .await
    .map_err(map_error)?
    .ok_or(OrderError::AcquisitionRunNotFound)?;

    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT type_id, acquired_quantity FROM acquisition_run_items \
         WHERE acquisition_run_id = $1 ORDER BY type_id",
    )
    .bind(run_id.0)
    .fetch_all(pool)
    .await
    .map_err(map_error)?;
    rows.into_iter()
        .map(|(type_id, acquired_quantity)| {
            Ok(AcquisitionRunItem {
                type_id,
                acquired_quantity: u64_from_i64(acquired_quantity)?,
            })
        })
        .collect()
}

/// No preview/confirm step: Order status is derived, so starting a Run
/// locks nothing (see this function's own doc on the `OrderRepository`
/// trait). Starts the Run only -- member Tickets' workflow status is
/// user-controlled and never touched by Run lifecycle (a batched Ticket
/// advances through its lanes on the Board like any other).
pub(super) async fn start_order_acquisition_run(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    run_id: AcquisitionRunId,
) -> Result<AcquisitionRun, OrderError> {
    let mut tx = pool.begin().await.map_err(map_error)?;

    let status: String = sqlx::query_scalar(
        "SELECT status FROM acquisition_runs WHERE id = $1 AND workspace_id = $2 AND owner_id = $3 FOR UPDATE",
    )
    .bind(run_id.0)
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_error)?
    .ok_or(OrderError::AcquisitionRunNotFound)?;
    if status != "ready" {
        return Err(OrderError::AcquisitionRunNotReady);
    }

    let now = crate::db_now();
    sqlx::query(
        "UPDATE acquisition_runs SET status = 'in_progress', started_at = $1, updated_at = $1 \
         WHERE id = $2",
    )
    .bind(now)
    .bind(run_id.0)
    .execute(&mut *tx)
    .await
    .map_err(map_error)?;

    tx.commit().await.map_err(map_error)?;
    load_order_acquisition_run(pool, workspace_id, run_id).await
}

/// Upserts each item's real, uncapped acquired total (never a delta) and
/// distributes it across member tickets' `acquired_quantity`, capped per
/// ticket at its own `quantity` (a standalone Acquisition ticket's
/// `quantity` already *is* its fresh/outstanding need -- the reused-vs-fresh
/// split happened one level up when its `OrderRequirement`/
/// `TicketPrerequisite` was created) and capped in total at the type's
/// summed demand. Posts no inventory event and completes no ticket --
/// purely informational execution state.
pub(super) async fn record_order_acquisition_progress(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    run_id: AcquisitionRunId,
    items: Vec<AcquisitionProgressUpdate>,
) -> Result<AcquisitionRun, OrderError> {
    let mut tx = pool.begin().await.map_err(map_error)?;

    let status: String = sqlx::query_scalar(
        "SELECT status FROM acquisition_runs WHERE id = $1 AND workspace_id = $2 FOR UPDATE",
    )
    .bind(run_id.0)
    .bind(workspace_id.0)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_error)?
    .ok_or(OrderError::AcquisitionRunNotFound)?;
    if status != "in_progress" {
        return Err(OrderError::AcquisitionRunNotInProgress);
    }

    let now = crate::db_now();
    for update in items {
        sqlx::query(
            "INSERT INTO acquisition_run_items \
               (id, acquisition_run_id, type_id, acquired_quantity, updated_at) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (acquisition_run_id, type_id) DO UPDATE SET \
               acquired_quantity = EXCLUDED.acquired_quantity, updated_at = EXCLUDED.updated_at",
        )
        .bind(Uuid::new_v4())
        .bind(run_id.0)
        .bind(update.type_id)
        .bind(i64_from_u64(update.acquired_quantity)?)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;

        let tickets: Vec<(Uuid, i64)> = sqlx::query_as(
            "SELECT id, quantity FROM tickets \
             WHERE acquisition_run_id = $1 AND type_id = $2 \
             ORDER BY display_id FOR UPDATE",
        )
        .bind(run_id.0)
        .bind(update.type_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_error)?;

        let mut total_needed: u64 = 0;
        for (_, quantity) in &tickets {
            total_needed += u64_from_i64(*quantity)?;
        }
        let mut remaining = update.acquired_quantity.min(total_needed);
        for (ticket_id, quantity) in &tickets {
            let need = u64_from_i64(*quantity)?;
            let allocated = remaining.min(need);
            remaining -= allocated;
            sqlx::query("UPDATE tickets SET acquired_quantity = $1 WHERE id = $2")
                .bind(i64_from_u64(allocated)?)
                .bind(ticket_id)
                .execute(&mut *tx)
                .await
                .map_err(map_error)?;
        }
    }

    sqlx::query("UPDATE acquisition_runs SET updated_at = $1 WHERE id = $2")
        .bind(now)
        .bind(run_id.0)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;

    tx.commit().await.map_err(map_error)?;
    load_order_acquisition_run(pool, workspace_id, run_id).await
}

/// The delivery transaction -- see the trait doc comment on
/// `OrderRepository::complete_order_acquisition_run`. This is an explicit
/// recording/accounting action: for each distinct `type_id` it posts
/// exactly one `Purchase` event for the *full* recorded acquired total
/// (surplus included), allocates that total across member tickets via
/// `allocate_acquisition_delivery` and records each member's own
/// `acquired_quantity`, and marks the Run itself `complete`. It does
/// **not** touch any member Ticket's *workflow* `status`, and it does not
/// recompute any dependent Ticket's status -- a fully-delivered member
/// stays in whatever lane the user put it in (its recording just becomes
/// `Recorded`), and any ticket that depends on it advances only when the
/// user moves it.
pub(super) async fn complete_order_acquisition_run(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    run_id: AcquisitionRunId,
) -> Result<AcquisitionRun, OrderError> {
    let mut tx = pool.begin().await.map_err(map_error)?;

    let run_row: Option<(String, String)> = sqlx::query_as(
        "SELECT status, display_id FROM acquisition_runs \
         WHERE id = $1 AND workspace_id = $2 AND owner_id = $3 FOR UPDATE",
    )
    .bind(run_id.0)
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_error)?;
    let (status, run_display_id) = run_row.ok_or(OrderError::AcquisitionRunNotFound)?;
    if status != "in_progress" {
        return Err(OrderError::AcquisitionRunNotInProgress);
    }

    // Locked and read in deterministic (display_id) order -- the same
    // order `allocate_acquisition_delivery` fills against.
    let members: Vec<(Uuid, i64, String, i64, Option<Decimal>)> = sqlx::query_as(
        "SELECT id, type_id, captured_name, quantity, estimated_unit_cost \
         FROM tickets WHERE acquisition_run_id = $1 ORDER BY display_id FOR UPDATE",
    )
    .bind(run_id.0)
    .fetch_all(&mut *tx)
    .await
    .map_err(map_error)?;

    type MemberInfo = (Uuid, String, u64, Option<Money>);
    let mut by_type: std::collections::BTreeMap<i64, Vec<MemberInfo>> =
        std::collections::BTreeMap::new();
    for (ticket_id, type_id, captured_name, quantity, estimated_unit_cost) in members {
        by_type.entry(type_id).or_default().push((
            ticket_id,
            captured_name,
            u64_from_i64(quantity)?,
            estimated_unit_cost.map(Money),
        ));
    }

    let acquired_rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT type_id, acquired_quantity FROM acquisition_run_items WHERE acquisition_run_id = $1",
    )
    .bind(run_id.0)
    .fetch_all(&mut *tx)
    .await
    .map_err(map_error)?;
    let mut acquired_by_type: std::collections::HashMap<i64, u64> =
        std::collections::HashMap::new();
    for (type_id, acquired_quantity) in acquired_rows {
        acquired_by_type.insert(type_id, u64_from_i64(acquired_quantity)?);
    }

    let now = crate::db_now();
    for (type_id, tickets) in &by_type {
        let acquired_total = acquired_by_type.get(type_id).copied().unwrap_or(0);
        let type_name = tickets[0].1.clone();

        // The current inventory average is this type's cost-resolution
        // hierarchy fallback (step 3), shared across every ticket in this
        // group since they're all the same `type_id` -- resolved once,
        // not re-queried per ticket.
        let current_average_row: Option<(i64, Decimal)> = sqlx::query_as(
            "SELECT quantity, total_historical_cost FROM inventory_balances \
             WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3",
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .bind(*type_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?;
        let current_average = match current_average_row {
            Some((existing_quantity, total_historical_cost)) if existing_quantity > 0 => {
                let mut average = total_historical_cost
                    / Decimal::from_i128_with_scale(i128::from(existing_quantity), 0);
                average.rescale(4);
                Some(Money(average))
            }
            _ => None,
        };

        if acquired_total > 0 {
            let costs: Vec<(Option<Money>, u64)> = tickets
                .iter()
                .map(|(_, _, quantity, cost)| (*cost, *quantity))
                .collect();
            let unit_cost = weighted_unit_cost(&costs, current_average)
                .map_err(|error| OrderError::Persistence(error.to_string()))?;
            post_purchase_in_transaction(
                &mut tx,
                InventoryPostingLine {
                    key: InventoryItemKey {
                        workspace_id,
                        owner_id,
                        type_id: *type_id,
                    },
                    type_name: &type_name,
                    quantity: acquired_total,
                    source_reference: &run_display_id,
                },
                None,
                unit_cost,
                now,
                now,
            )
            .await?;
        }

        let alloc_members: Vec<(TicketId, u64)> = tickets
            .iter()
            .map(|(ticket_id, _, quantity, _)| (TicketId(*ticket_id), *quantity))
            .collect();
        let (allocations, _surplus) = allocate_acquisition_delivery(&alloc_members, acquired_total);

        for (ticket_id, allocated) in allocations {
            debug_assert!(
                tickets.iter().any(|(id, ..)| *id == ticket_id.0),
                "allocation only ever names a ticket from this same type group"
            );
            // Delivery-progress accounting only -- `acquired_quantity` is
            // "how much of this Ticket's demand the Run brought back". The
            // Ticket's workflow `status` is never touched here; the user
            // moves the card when they consider the work done.
            sqlx::query("UPDATE tickets SET acquired_quantity = $1, updated_at = $2 WHERE id = $3")
                .bind(i64_from_u64(allocated)?)
                .bind(now)
                .bind(ticket_id.0)
                .execute(&mut *tx)
                .await
                .map_err(map_error)?;
        }
    }

    sqlx::query(
        "UPDATE acquisition_runs SET status = 'complete', completed_at = $1, updated_at = $1 \
         WHERE id = $2",
    )
    .bind(now)
    .bind(run_id.0)
    .execute(&mut *tx)
    .await
    .map_err(map_error)?;

    tx.commit().await.map_err(map_error)?;
    load_order_acquisition_run(pool, workspace_id, run_id).await
}
