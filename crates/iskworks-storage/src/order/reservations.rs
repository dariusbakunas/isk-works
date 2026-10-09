use super::*;

use std::collections::{BTreeMap, BTreeSet};

use iskworks_core::order::{
    plan_input_consumption, AllocationUse, ConsumptionPlan, HeldAllocation, InputAvailability,
    InventoryAllocationId, PlannedReservation, TakeFrom,
};

/// Locks the `inventory_balances` rows of `type_ids` (`FOR UPDATE`, sorted
/// `type_id` order -- the lock order every multi-balance inventory writer
/// uses) and returns each type's free stock under that lock:
/// `physical - Σ active allocations`, floored at `0`. A type with no
/// balance row is `0` free.
///
/// The lock is what makes reservation race-safe: a concurrent writer
/// reserving the same type waits here, then sees the first writer's
/// committed allocations in its own `SUM`.
pub(super) async fn lock_free_stock(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    type_ids: impl IntoIterator<Item = i64>,
) -> Result<BTreeMap<i64, u64>, OrderError> {
    let mut type_ids: Vec<i64> = type_ids.into_iter().collect();
    type_ids.sort_unstable();
    type_ids.dedup();
    let mut physical: BTreeMap<i64, u64> = BTreeMap::new();
    for &type_id in &type_ids {
        let quantity: Option<i64> = sqlx::query_scalar(
            "SELECT quantity FROM inventory_balances \
             WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3 FOR UPDATE",
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .bind(type_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(map_error)?;
        physical.insert(type_id, u64_from_i64(quantity.unwrap_or(0))?);
    }
    if type_ids.is_empty() {
        return Ok(physical);
    }
    let reserved: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT type_id, SUM(quantity)::bigint FROM inventory_allocations \
         WHERE workspace_id = $1 AND owner_id = $2 AND type_id = ANY($3) \
         AND released_at IS NULL AND consumed_at IS NULL \
         GROUP BY type_id",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(&type_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(map_error)?;
    for (type_id, reserved) in reserved {
        let reserved = u64_from_i64(reserved)?;
        if let Some(free) = physical.get_mut(&type_id) {
            *free = free.saturating_sub(reserved);
        }
    }
    Ok(physical)
}

/// Inserts one active allocation per planned reservation, owned by its
/// requirement.
pub(super) async fn insert_requirement_allocations(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    reason: AllocationReason,
    reservations: &[PlannedReservation],
    now: DateTime<Utc>,
) -> Result<(), OrderError> {
    for reservation in reservations {
        sqlx::query(
            "INSERT INTO inventory_allocations \
             (id, workspace_id, owner_id, type_id, quantity, order_requirement_id, created_at, reason) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(Uuid::new_v4())
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .bind(reservation.type_id)
        .bind(i64_from_u64(reservation.quantity)?)
        .bind(reservation.requirement_id.0)
        .bind(now)
        .bind(reason.as_str())
        .execute(&mut **tx)
        .await
        .map_err(map_error)?;
    }
    Ok(())
}

/// Releases every active allocation owned by `order_id`'s requirements
/// (`released_at = now`): the claim is abandoned, nothing was used, so no
/// ledger event is posted and no balance changes. Consumed rows stay as
/// history. Returns how many rows were released.
pub(super) async fn release_order_allocations(
    tx: &mut Transaction<'_, Postgres>,
    order_id: OrderId,
    now: DateTime<Utc>,
) -> Result<u64, OrderError> {
    let result = sqlx::query(
        "UPDATE inventory_allocations SET released_at = $1 \
         WHERE released_at IS NULL AND consumed_at IS NULL \
         AND order_requirement_id IN (SELECT id FROM order_requirements WHERE order_id = $2)",
    )
    .bind(now)
    .bind(order_id.0)
    .execute(&mut **tx)
    .await
    .map_err(map_error)?;
    Ok(result.rows_affected())
}

/// `OrderRepository::reserve_order_inventory`, inside the caller's
/// transaction.
pub(super) async fn top_up_order_reservations(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: WorkspaceId,
    order_id: OrderId,
    now: DateTime<Utc>,
) -> Result<CappedReservationPlan, OrderError> {
    // Lock the Epic so a concurrent cancel/archive can't interleave.
    let order: Option<OrderReservationRow> = sqlx::query_as(
        "SELECT owner_id, planning_snapshot_version, canceled_at, archived_at \
         FROM orders WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(workspace_id.0)
    .bind(order_id.0)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_error)?;
    let order = order.ok_or(OrderError::OrderNotFound)?;
    if order.planning_snapshot_version < 3 {
        return Err(OrderError::FrozenPlanUnavailable);
    }
    if order.canceled_at.is_some() || order.archived_at.is_some() {
        return Err(OrderError::OrderNotReservable);
    }
    let owner_id = OwnerId(order.owner_id);

    let requirement_rows: Vec<RequirementReservationRow> = sqlx::query_as(
        "SELECT id, type_id, reused_quantity, operation_occurrence_key, \
                    child_occurrence_key, dependency_id \
             FROM order_requirements WHERE order_id = $1",
    )
    .bind(order_id.0)
    .fetch_all(&mut **tx)
    .await
    .map_err(map_error)?;
    let operation_keys: Vec<String> =
        sqlx::query_scalar("SELECT occurrence_key FROM order_plan_operations WHERE order_id = $1")
            .bind(order_id.0)
            .fetch_all(&mut **tx)
            .await
            .map_err(map_error)?;
    let dag = derive_operation_dag(
        operation_keys.iter().map(String::as_str),
        requirement_rows.iter().filter_map(|row| {
            Some(FrozenDemandEdge {
                consumer: row.operation_occurrence_key.as_deref()?,
                producer: row.child_occurrence_key.as_deref()?,
                dependency_id: row.dependency_id.as_deref(),
            })
        }),
    )?;

    // Balances first (the shared lock order), then what each requirement
    // already holds, measured under that lock.
    let free = lock_free_stock(
        tx,
        workspace_id,
        owner_id,
        requirement_rows
            .iter()
            .filter(|row| row.reused_quantity > 0)
            .map(|row| row.type_id),
    )
    .await?;
    let held: BTreeMap<Uuid, i64> = sqlx::query_as::<_, (Uuid, i64)>(
        "SELECT order_requirement_id, SUM(quantity)::bigint FROM inventory_allocations \
         WHERE released_at IS NULL \
         AND order_requirement_id IN (SELECT id FROM order_requirements WHERE order_id = $1) \
         GROUP BY order_requirement_id",
    )
    .bind(order_id.0)
    .fetch_all(&mut **tx)
    .await
    .map_err(map_error)?
    .into_iter()
    .collect();

    let mut needs = Vec::with_capacity(requirement_rows.len());
    for row in &requirement_rows {
        let want = u64_from_i64(row.reused_quantity)?
            .saturating_sub(u64_from_i64(held.get(&row.id).copied().unwrap_or(0))?);
        needs.push(ReservationNeed {
            requirement_id: OrderRequirementId(row.id),
            type_id: row.type_id,
            operation_occurrence_key: row.operation_occurrence_key.as_deref(),
            quantity: want,
        });
    }
    let plan = plan_capped_reservations(&needs, &dag.stages, &free);
    insert_requirement_allocations(
        tx,
        workspace_id,
        owner_id,
        AllocationReason::EpicTopUp,
        &plan.reservations,
        now,
    )
    .await?;
    Ok(plan)
}

#[derive(sqlx::FromRow)]
struct OrderReservationRow {
    owner_id: Uuid,
    planning_snapshot_version: i16,
    canceled_at: Option<DateTime<Utc>>,
    archived_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct RequirementReservationRow {
    id: Uuid,
    type_id: i64,
    reused_quantity: i64,
    operation_occurrence_key: Option<String>,
    child_occurrence_key: Option<String>,
    dependency_id: Option<String>,
}

/// One active allocation of a type, with the Epic that owns it.
#[derive(sqlx::FromRow)]
struct HeldRow {
    id: Uuid,
    order_id: Option<Uuid>,
    quantity: i64,
    created_at: DateTime<Utc>,
}

/// Plans where each input type of a production recording draws from (see
/// `order::plan_input_consumption`). Call with every touched balance
/// already locked: free stock is measured here, before anything is
/// consumed, and every allocation of the input types is locked (`FOR
/// UPDATE`, by id). `order_id` / `occurrence_key` identify the consuming
/// ticket's own requirements; a ticket outside an Epic has none.
///
/// # Errors
///
/// [`OrderError::InsufficientInventory`] if a type is short with no Epic
/// holding any of it (the stock isn't there); otherwise
/// [`OrderError::InsufficientAvailable`] with every short type.
pub(super) async fn plan_recording_draws(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    consumer: Option<(OrderId, &str)>,
    quantities: &BTreeMap<i64, u64>,
    take_from: &[TakeFrom],
) -> Result<BTreeMap<i64, ConsumptionPlan>, OrderError> {
    let mut plans = BTreeMap::new();
    let mut shortages = Vec::new();
    for (&type_id, &quantity) in quantities {
        let physical: Option<i64> = sqlx::query_scalar(
            "SELECT quantity FROM inventory_balances \
             WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3",
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .bind(type_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(map_error)?;
        let held: Vec<HeldRow> = sqlx::query_as(
            "SELECT a.id, r.order_id, a.quantity, a.created_at \
             FROM inventory_allocations a \
             LEFT JOIN order_requirements r ON r.id = a.order_requirement_id \
             WHERE a.workspace_id = $1 AND a.owner_id = $2 AND a.type_id = $3 \
             AND a.released_at IS NULL AND a.consumed_at IS NULL \
             ORDER BY a.id FOR UPDATE OF a",
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .bind(type_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(map_error)?;
        let own_ids: BTreeSet<Uuid> = match consumer {
            Some((order_id, occurrence_key)) => sqlx::query_scalar(
                "SELECT a.id FROM inventory_allocations a \
                 JOIN order_requirements r ON r.id = a.order_requirement_id \
                 WHERE r.order_id = $1 AND r.operation_occurrence_key = $2 AND a.type_id = $3 \
                 AND a.released_at IS NULL AND a.consumed_at IS NULL",
            )
            .bind(order_id.0)
            .bind(occurrence_key)
            .bind(type_id)
            .fetch_all(&mut **tx)
            .await
            .map_err(map_error)?
            .into_iter()
            .collect(),
            None => BTreeSet::new(),
        };

        let active: u64 = held
            .iter()
            .map(|row| u64_from_i64(row.quantity))
            .sum::<Result<u64, _>>()?;
        let free_before = u64_from_i64(physical.unwrap_or(0))?.saturating_sub(active);
        // Serving order: oldest reservation first.
        let mut ordered: Vec<&HeldRow> = held.iter().collect();
        ordered.sort_by_key(|row| (row.created_at, row.id));
        let to_held = |row: &HeldRow| -> Result<HeldAllocation, OrderError> {
            Ok(HeldAllocation {
                id: InventoryAllocationId(row.id),
                // A legacy ticket-owned row has no Epic; it can't be named
                // in `take_from` (nil never matches a real Epic).
                order_id: OrderId(row.order_id.unwrap_or(Uuid::nil())),
                quantity: u64_from_i64(row.quantity)?,
            })
        };
        let own: Vec<HeldAllocation> = ordered
            .iter()
            .filter(|row| own_ids.contains(&row.id))
            .map(|row| to_held(row))
            .collect::<Result<_, _>>()?;
        let others: Vec<HeldAllocation> = ordered
            .iter()
            .filter(|row| !own_ids.contains(&row.id))
            .map(|row| to_held(row))
            .collect::<Result<_, _>>()?;
        let permitted: BTreeSet<OrderId> = take_from
            .iter()
            .filter(|take| take.type_id == type_id)
            .map(|take| take.order_id)
            .collect();

        match plan_input_consumption(
            InputAvailability {
                type_id,
                quantity,
                own: &own,
                free_before,
                others: &others,
            },
            &permitted,
        ) {
            Ok(plan) => {
                plans.insert(type_id, plan);
            }
            Err(shortage) => shortages.push(shortage),
        }
    }
    if shortages.is_empty() {
        Ok(plans)
    } else if shortages.iter().any(|shortage| shortage.holders.is_empty()) {
        // No Epic holds what's missing: the stock simply isn't there.
        Err(OrderError::InsufficientInventory)
    } else {
        Err(OrderError::InsufficientAvailable(shortages))
    }
}

/// Applies planned draws for `recording_id`: own reservations become
/// consumed by it; taken reservations are released from their Epic (the
/// taken stock is then consumed like free stock). A partial use splits the
/// row: the used part keeps the row, the remainder becomes a new active
/// row with the same owner, reason and source.
pub(super) async fn apply_recording_draws(
    tx: &mut Transaction<'_, Postgres>,
    plans: &BTreeMap<i64, ConsumptionPlan>,
    recording_id: TicketInventoryRecordingId,
    now: DateTime<Utc>,
) -> Result<(), OrderError> {
    for plan in plans.values() {
        for draw in &plan.own {
            split_off_remainder(tx, draw, now).await?;
            sqlx::query(
                "UPDATE inventory_allocations \
                 SET quantity = $2, consumed_at = $3, consumed_by_recording_id = $4 \
                 WHERE id = $1",
            )
            .bind(draw.id.0)
            .bind(i64_from_u64(draw.used)?)
            .bind(now)
            .bind(recording_id.0)
            .execute(&mut **tx)
            .await
            .map_err(map_error)?;
        }
        for draw in &plan.taken {
            split_off_remainder(tx, draw, now).await?;
            sqlx::query(
                "UPDATE inventory_allocations SET quantity = $2, released_at = $3 WHERE id = $1",
            )
            .bind(draw.id.0)
            .bind(i64_from_u64(draw.used)?)
            .bind(now)
            .execute(&mut **tx)
            .await
            .map_err(map_error)?;
        }
    }
    Ok(())
}

async fn split_off_remainder(
    tx: &mut Transaction<'_, Postgres>,
    draw: &AllocationUse,
    now: DateTime<Utc>,
) -> Result<(), OrderError> {
    if draw.remainder == 0 {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO inventory_allocations \
         (id, workspace_id, owner_id, type_id, quantity, order_requirement_id, \
          ticket_prerequisite_id, created_at, reason, source_recording_id) \
         SELECT $2, workspace_id, owner_id, type_id, $3, order_requirement_id, \
                ticket_prerequisite_id, $4, reason, source_recording_id \
         FROM inventory_allocations WHERE id = $1",
    )
    .bind(draw.id.0)
    .bind(Uuid::new_v4())
    .bind(i64_from_u64(draw.remainder)?)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(map_error)?;
    Ok(())
}

/// Reverting a recording: every reservation it consumed becomes active
/// again (the reversal returns that stock). Reservations taken from other
/// Epics are not handed back to them -- the returned stock is free.
/// No free-stock check: a reversal corrects the ledger to what physically
/// happened, even if that leaves stock over-reserved.
pub(super) async fn unconsume_recording_allocations(
    tx: &mut Transaction<'_, Postgres>,
    recording_id: TicketInventoryRecordingId,
) -> Result<(), OrderError> {
    sqlx::query(
        "UPDATE inventory_allocations SET consumed_at = NULL, consumed_by_recording_id = NULL \
         WHERE consumed_by_recording_id = $1",
    )
    .bind(recording_id.0)
    .execute(&mut **tx)
    .await
    .map_err(map_error)?;
    Ok(())
}
