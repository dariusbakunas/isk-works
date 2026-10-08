use super::*;

/// `GET /api/orders/:order_id/execution-plan`: a version-3 Epic's frozen
/// plan in the exact shape the Build's Plan view renders
/// (`ExecutionPlanProjection`), plus the Epic-only overlay (each step's
/// ticket, what the Epic holds and has used). Selecting an Epic swaps the
/// Plan's data, not its layout.
pub(super) async fn get_order_execution_plan(
    State(state): State<AppState>,
    Path(order_id): Path<uuid::Uuid>,
) -> Result<Json<EpicExecutionPlanResponse>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let order_id = OrderId(order_id);
    let repository = state.order_repository()?;
    let order = repository.get_order(workspace_id, order_id).await?;
    if order.planning_snapshot_version < 3 {
        return Err(OrderError::FrozenPlanUnavailable.into());
    }
    let operations = repository.list_order_plan_operations(order_id).await?;
    let requirements = repository.list_order_requirements(order_id).await?;
    let tickets = repository
        .list_tickets_for_order(workspace_id, order_id)
        .await?;
    let totals: HashMap<OrderRequirementId, RequirementReservationTotals> = repository
        .requirement_reservation_totals(workspace_id, order_id)
        .await?
        .into_iter()
        .map(|totals| (totals.requirement_id, totals))
        .collect();

    // Free stock, and what *other* Epics hold (this Epic's own active
    // reservations are its overlay, not someone else's claim).
    let inventory = state.inventory_repository()?;
    let reserved = inventory
        .active_reservations(workspace_id, order.owner_id)
        .await?;
    let mut own_reserved: BTreeMap<i64, u64> = BTreeMap::new();
    for requirement in &requirements {
        if let Some(totals) = totals.get(&requirement.id) {
            *own_reserved.entry(requirement.type_id).or_insert(0) += totals.reserved;
        }
    }
    let free_by_type: BTreeMap<i64, u64> = inventory
        .list_balances(workspace_id, order.owner_id)
        .await?
        .into_iter()
        .map(|balance| {
            let held = reserved.get(&balance.key.type_id).copied().unwrap_or(0);
            (balance.key.type_id, balance.quantity.saturating_sub(held))
        })
        .collect();
    let reserved_elsewhere_by_type = reserved
        .iter()
        .map(|(&type_id, &held)| {
            let own = own_reserved.get(&type_id).copied().unwrap_or(0);
            (type_id, held.saturating_sub(own))
        })
        .filter(|(_, held)| *held > 0)
        .collect();

    let blueprint_ids: Vec<i64> = operations
        .iter()
        .map(|operation| operation.blueprint_or_formula_type_id)
        .collect();
    // Names are display-only: a missing SDE leaves them blank.
    let blueprint_names = state
        .sde_repository
        .type_names(&blueprint_ids)
        .await
        .unwrap_or_default();

    let plan = project_frozen_execution_plan(
        &operations,
        &requirements,
        &blueprint_names,
        &FrozenPlanInventory {
            free_by_type,
            reserved_elsewhere_by_type,
        },
        Utc::now(),
    )?;
    let epic = epic_plan_overlay(&order, &operations, &requirements, &totals, &tickets);
    Ok(Json(EpicExecutionPlanResponse { plan, epic }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EpicExecutionPlanResponse {
    plan: iskworks_core::execution_plan::ExecutionPlanProjection,
    epic: EpicPlanOverlay,
}

/// `POST /api/orders/:order_id/reserve`: the Epic's "Reserve inventory"
/// action. Reserves what free stock allows toward each requirement's frozen
/// reuse (e.g. after restoring an archived Epic) and reports the rest.
pub(super) async fn reserve_order_inventory(
    State(state): State<AppState>,
    Path(order_id): Path<uuid::Uuid>,
) -> Result<Json<ReserveInventoryResponse>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let order_id = OrderId(order_id);
    let repository = state.order_repository()?;
    let plan = repository
        .reserve_order_inventory(workspace_id, order_id)
        .await?;
    let names: BTreeMap<i64, String> = repository
        .list_order_requirements(order_id)
        .await?
        .into_iter()
        .map(|requirement| (requirement.type_id, requirement.captured_name))
        .collect();
    let mut reserved: BTreeMap<i64, u64> = BTreeMap::new();
    for reservation in &plan.reservations {
        *reserved.entry(reservation.type_id).or_insert(0) += reservation.quantity;
    }
    Ok(Json(ReserveInventoryResponse {
        reserved: reserved
            .into_iter()
            .map(|(type_id, quantity)| ReservedLine {
                type_id,
                type_name: names.get(&type_id).cloned().unwrap_or_default(),
                quantity,
            })
            .collect(),
        shortfalls: plan.shortfalls,
    }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReserveInventoryResponse {
    /// Newly reserved, per type.
    reserved: Vec<ReservedLine>,
    /// Types free stock couldn't fully cover.
    shortfalls: Vec<ReservationShortfall>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReservedLine {
    type_id: i64,
    type_name: String,
    quantity: u64,
}
