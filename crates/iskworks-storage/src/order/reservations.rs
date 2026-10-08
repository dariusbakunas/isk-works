use super::*;

use std::collections::BTreeMap;

use iskworks_core::order::PlannedReservation;

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
