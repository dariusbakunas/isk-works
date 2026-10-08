use super::*;

/// `GET /api/orders/:order_id/coverage`: the Epic's live inventory state
/// per frozen requirement -- what it holds (`reserved`), what recordings
/// used (`consumed`), what it still needs, and the free stock that could
/// cover that now. The Plan view's Epic mode joins this with the Epic's
/// detail (`GET /api/orders/:id`) by requirement id.
pub(super) async fn get_order_coverage(
    State(state): State<AppState>,
    Path(order_id): Path<uuid::Uuid>,
) -> Result<Json<EpicCoverageResponse>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let order_id = OrderId(order_id);
    let repository = state.order_repository()?;
    let order = repository.get_order(workspace_id, order_id).await?;
    let requirements = repository.list_order_requirements(order_id).await?;
    let totals: HashMap<OrderRequirementId, RequirementReservationTotals> = repository
        .requirement_reservation_totals(workspace_id, order_id)
        .await?
        .into_iter()
        .map(|totals| (totals.requirement_id, totals))
        .collect();

    let inventory = state.inventory_repository()?;
    let reserved = inventory
        .active_reservations(workspace_id, order.owner_id)
        .await?;
    let free: HashMap<i64, u64> = inventory
        .list_balances(workspace_id, order.owner_id)
        .await?
        .into_iter()
        .map(|balance| {
            let held = reserved.get(&balance.key.type_id).copied().unwrap_or(0);
            (balance.key.type_id, balance.quantity.saturating_sub(held))
        })
        .collect();

    let lines = requirements
        .iter()
        .map(|requirement| {
            epic_coverage_line(
                requirement.id,
                requirement.required_quantity,
                totals.get(&requirement.id).copied(),
                free.get(&requirement.type_id).copied().unwrap_or(0),
            )
        })
        .collect();
    Ok(Json(EpicCoverageResponse { order_id, lines }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EpicCoverageResponse {
    order_id: OrderId,
    lines: Vec<EpicCoverageLine>,
}
