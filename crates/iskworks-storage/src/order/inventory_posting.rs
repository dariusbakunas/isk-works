use super::*;

/// The item, quantity and source reference every inventory posting below
/// records.
pub(super) struct InventoryPostingLine<'a> {
    pub(super) key: InventoryItemKey,
    pub(super) type_name: &'a str,
    pub(super) quantity: u64,
    pub(super) source_reference: &'a str,
}

/// Posts a real `Consumption` event for `quantity` of `type_id`, locking
/// the balance first (same technique `allocate_in_transaction`/
/// `commit_plan` use) so the cost computed below reflects the row this
/// same transaction is about to update, not a stale read from outside it.
///
/// Draws the full quantity at the balance's current weighted average --
/// every accounted unit already has a cost basis, so there is no
/// known/unknown split to reason about here. Returns the event id (for a
/// caller that links provenance) and the **positive** cost basis actually
/// removed (`abs(total_cost_delta)`), which `record_ticket_production`
/// sums into the production batch basis. Rejects `quantity` greater than
/// the on-hand balance with `InsufficientInventory` -- negative inventory
/// is never allowed.
pub(super) async fn post_consumption_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    line: InventoryPostingLine<'_>,
    now: DateTime<Utc>,
) -> Result<(InventoryEventId, Decimal), OrderError> {
    let InventoryPostingLine {
        key:
            InventoryItemKey {
                workspace_id,
                owner_id,
                type_id,
            },
        type_name,
        quantity,
        source_reference,
    } = line;
    let row: Option<(i64, i64, Decimal)> = sqlx::query_as(
        "SELECT revision, quantity, total_historical_cost \
         FROM inventory_balances WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3 \
         FOR UPDATE",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(type_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_error)?;
    let (expected_revision, existing_quantity, total_historical_cost) = match row {
        Some((revision, existing_quantity, total_historical_cost)) => (
            u64_from_i64(revision)?,
            u64_from_i64(existing_quantity)?,
            total_historical_cost,
        ),
        None => (0, 0, Decimal::ZERO),
    };

    if quantity > existing_quantity {
        return Err(OrderError::InsufficientInventory);
    }

    // Consumption always draws the full quantity at the current weighted
    // average -- there is no known/unknown split to reason about, every
    // accounted unit already has a cost basis.
    let consumed_cost = if existing_quantity == 0 {
        Decimal::ZERO
    } else if quantity == existing_quantity {
        total_historical_cost
    } else {
        let average =
            total_historical_cost / Decimal::from_i128_with_scale(i128::from(existing_quantity), 0);
        let mut cost = average * Decimal::from_i128_with_scale(i128::from(quantity), 0);
        cost.rescale(4);
        cost
    };
    // `Decimal` preserves sign on zero, so negating a zero `cost` (a fully
    // zero-cost balance being consumed) would otherwise produce a
    // "-0.0000" that `apply_inventory_event` correctly rejects as
    // inconsistent (`is_sign_negative()` is true even though the value is
    // numerically zero) -- guard explicitly rather than let a real
    // zero-cost consumption look invalid.
    let total_cost_delta = if consumed_cost.is_zero() {
        MoneyDelta::zero()
    } else {
        MoneyDelta(-consumed_cost)
    };

    let quantity_delta = -i64_from_u64(quantity)?;
    let event_id = InventoryEventId::new();
    let posting = InventoryPosting {
        id: event_id,
        key: InventoryItemKey {
            workspace_id,
            owner_id,
            type_id,
        },
        type_name: type_name.to_string(),
        kind: InventoryEventKind::Consumption,
        quantity_delta,
        total_cost_delta,
        unit_cost: None,
        cost_quality: CostInputQuality::Known,
        source_reference: source_reference.to_string(),
        note: String::new(),
        effective_at: now,
        recorded_at: now,
        expected_revision,
        reverses_event_id: None,
    };
    PgInventoryRepository::post_in_transaction(tx, &posting)
        .await
        .map_err(|error| OrderError::Persistence(error.to_string()))?;
    Ok((event_id, consumed_cost))
}

/// Posts one `ProductionOutput` event for `quantity` of `type_id` with an
/// explicit `total_cost_delta` (the production batch basis) -- unlike
/// `InventoryService::preview_production_output` this does not reconstruct
/// the amount from a per-unit string, so no rounding drift enters the
/// authoritative `total_historical_cost`. Same balance-lock discipline as
/// `post_purchase_in_transaction`. Returns the event id.
pub(super) async fn post_production_output_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    line: InventoryPostingLine<'_>,
    total_cost_delta: MoneyDelta,
    unit_cost: Option<Money>,
    cost_quality: CostInputQuality,
    effective_at: DateTime<Utc>,
    recorded_at: DateTime<Utc>,
) -> Result<InventoryEventId, OrderError> {
    let InventoryPostingLine {
        key:
            InventoryItemKey {
                workspace_id,
                owner_id,
                type_id,
            },
        type_name,
        quantity,
        source_reference,
    } = line;
    let expected_revision: i64 = sqlx::query_scalar(
        "SELECT revision FROM inventory_balances \
         WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3 FOR UPDATE",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(type_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_error)?
    .unwrap_or(0);

    let event_id = InventoryEventId::new();
    let posting = InventoryPosting {
        id: event_id,
        key: InventoryItemKey {
            workspace_id,
            owner_id,
            type_id,
        },
        type_name: type_name.to_string(),
        kind: InventoryEventKind::ProductionOutput,
        quantity_delta: i64_from_u64(quantity)?,
        total_cost_delta,
        unit_cost,
        cost_quality,
        source_reference: source_reference.to_string(),
        note: String::new(),
        effective_at,
        recorded_at,
        expected_revision: u64_from_i64(expected_revision)?,
        reverses_event_id: None,
    };
    PgInventoryRepository::post_in_transaction(tx, &posting)
        .await
        .map_err(|error| OrderError::Persistence(error.to_string()))?;
    Ok(event_id)
}

/// Posts a real `Purchase` event for `quantity` of `type_id` -- the
/// Acquisition-ticket-completion equivalent of `complete_order`'s
/// Consumption posting. Same locking discipline as
/// `post_acquisition_delivery` (`industry/acquisition_run.rs`), which this
/// mirrors closely.
///
/// Cost is resolved through the approved hierarchy, never left unknown or
/// silently zeroed: (1) `actual_unit_cost` when present -> `Known`; (2)
/// otherwise the ticket/run's frozen `estimated_unit_cost` when present ->
/// `Estimated`; (3) otherwise the type's current inventory weighted
/// average when one exists -> `Estimated`; (4) otherwise the completion is
/// rejected with `OrderError::CostRequired`. This is intentionally
/// transitional -- once wallet-transaction reconciliation can supply a
/// real `actual_unit_cost` for a purchase fill, the `Known` path becomes
/// the normal case instead of the rare one.
/// Returns the id of the `Purchase` event it posted so a caller (e.g.
/// `record_ticket_acquisition`) can link provenance to it; the run/order
/// completion callers ignore it.
pub(super) async fn post_purchase_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    line: InventoryPostingLine<'_>,
    actual_unit_cost: Option<Money>,
    estimated_unit_cost: Option<Money>,
    effective_at: DateTime<Utc>,
    recorded_at: DateTime<Utc>,
) -> Result<InventoryEventId, OrderError> {
    let InventoryPostingLine {
        key:
            InventoryItemKey {
                workspace_id,
                owner_id,
                type_id,
            },
        type_name,
        quantity,
        source_reference,
    } = line;
    let row: Option<(i64, i64, Decimal)> = sqlx::query_as(
        "SELECT revision, quantity, total_historical_cost \
         FROM inventory_balances WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3 \
         FOR UPDATE",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(type_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_error)?;
    let (expected_revision, current_average) = match row {
        Some((revision, existing_quantity, total_historical_cost)) => {
            let existing_quantity = u64_from_i64(existing_quantity)?;
            let average = if existing_quantity > 0 {
                let mut average = total_historical_cost
                    / Decimal::from_i128_with_scale(i128::from(existing_quantity), 0);
                average.rescale(4);
                Some(Money(average))
            } else {
                None
            };
            (u64_from_i64(revision)?, average)
        }
        None => (0, None),
    };

    let (unit_cost, cost_quality) = match (actual_unit_cost, estimated_unit_cost, current_average) {
        (Some(cost), ..) => (cost, CostInputQuality::Known),
        (None, Some(cost), _) => (cost, CostInputQuality::Estimated),
        (None, None, Some(cost)) => (cost, CostInputQuality::Estimated),
        (None, None, None) => return Err(OrderError::CostRequired),
    };

    let quantity_delta = i64_from_u64(quantity)?;
    let total_cost_delta = MoneyDelta(
        unit_cost
            .checked_mul_quantity(quantity)
            .map_err(|error| OrderError::Persistence(error.to_string()))?
            .0,
    );
    let event_id = InventoryEventId::new();
    let posting = InventoryPosting {
        id: event_id,
        key: InventoryItemKey {
            workspace_id,
            owner_id,
            type_id,
        },
        type_name: type_name.to_string(),
        kind: InventoryEventKind::Purchase,
        quantity_delta,
        total_cost_delta,
        unit_cost: Some(unit_cost),
        cost_quality,
        source_reference: source_reference.to_string(),
        note: String::new(),
        effective_at,
        recorded_at,
        expected_revision,
        reverses_event_id: None,
    };
    PgInventoryRepository::post_in_transaction(tx, &posting)
        .await
        .map_err(|error| OrderError::Persistence(error.to_string()))?;
    Ok(event_id)
}
