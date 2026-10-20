//! Facts for Inventory › Runway: open Epics' `Buy` demand and the stock that
//! can meet it. Set-based reads only; the arithmetic is
//! `iskworks_core::build_runway`.

use std::collections::BTreeMap;

use iskworks_core::{
    InventoryError, InventoryRunwayRepository, RunwayEpic, RunwayEventConsumption, RunwayFacts,
    RunwayRequirement, RunwayStockPool, RunwayTypeFact,
};

use super::*;

/// The Epics Runway counts: live (not completed, canceled or archived)
/// version-3+ Epics (same rule as reservations' `LIVE`). Frozen v1/v2 Epics stay readable elsewhere but carry no
/// reservations to net against.
const OPEN_EPIC: &str = "o.completed_at IS NULL AND o.canceled_at IS NULL \
     AND o.archived_at IS NULL AND o.planning_snapshot_version >= 3";

fn persistence(error: sqlx::Error) -> InventoryError {
    InventoryError::Persistence(error.to_string())
}

fn quantity(value: i64, what: &str) -> Result<u64, InventoryError> {
    u64::try_from(value).map_err(|_| InventoryError::Persistence(format!("negative {what}")))
}

impl PgOrderRepository {
    async fn runway_epics(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<RunwayEpic>, InventoryError> {
        let rows: Vec<(Uuid, Uuid, String, DateTime<Utc>)> = sqlx::query_as(&format!(
            "SELECT o.id, o.owner_id, o.display_name, o.created_at FROM orders o \
             WHERE o.workspace_id = $1 AND {OPEN_EPIC}"
        ))
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(persistence)?;
        Ok(rows
            .into_iter()
            .map(|(id, owner_id, display_name, created_at)| RunwayEpic {
                order_id: OrderId(id),
                owner_id: OwnerId(owner_id),
                display_name,
                created_at,
            })
            .collect())
    }

    /// Buy requirements with their allocation totals, plus each type's
    /// captured name (the fallback when the active SDE lacks the type).
    async fn runway_requirements(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<(Vec<RunwayRequirement>, BTreeMap<i64, String>), InventoryError> {
        #[allow(clippy::type_complexity)]
        let rows: Vec<(Uuid, i64, String, i64, i64, i64, Option<Decimal>)> =
            sqlx::query_as(&format!(
                "SELECT r.order_id, r.type_id, r.captured_name, r.required_quantity, \
                   COALESCE(SUM(a.quantity) FILTER \
                     (WHERE a.released_at IS NULL AND a.consumed_at IS NULL), 0)::bigint, \
                   COALESCE(SUM(a.quantity) FILTER \
                     (WHERE a.released_at IS NULL AND a.consumed_at IS NOT NULL), 0)::bigint, \
                   r.estimated_unit_cost \
                 FROM order_requirements r \
                 JOIN orders o ON o.id = r.order_id \
                 LEFT JOIN inventory_allocations a ON a.order_requirement_id = r.id \
                 WHERE o.workspace_id = $1 AND r.kind = 'buy' AND {OPEN_EPIC} \
                 GROUP BY r.id"
            ))
            .bind(workspace_id.0)
            .fetch_all(&self.pool)
            .await
            .map_err(persistence)?;
        let mut names = BTreeMap::new();
        let requirements = rows
            .into_iter()
            .map(
                |(order_id, type_id, name, required, reserved, consumed, unit_cost)| {
                    names.entry(type_id).or_insert(name);
                    Ok(RunwayRequirement {
                        order_id: OrderId(order_id),
                        type_id,
                        required: quantity(required, "required quantity")?,
                        reserved: quantity(reserved, "reserved quantity")?,
                        consumed: quantity(consumed, "consumed quantity")?,
                        unit_cost: unit_cost.map(Money),
                    })
                },
            )
            .collect::<Result<_, InventoryError>>()?;
        Ok((requirements, names))
    }

    /// Consumption posted by open Epics' tickets. A reverted recording or a
    /// reversed event no longer counts.
    async fn runway_event_consumption(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<RunwayEventConsumption>, InventoryError> {
        let rows: Vec<(Uuid, i64, i64)> = sqlx::query_as(&format!(
            "SELECT o.id, e.type_id, SUM(-e.quantity_delta)::bigint \
             FROM inventory_events e \
             JOIN ticket_inventory_recordings tr ON tr.id = e.ticket_inventory_recording_id \
             JOIN tickets t ON t.id = tr.ticket_id \
             JOIN orders o ON o.id = t.order_id \
             WHERE e.workspace_id = $1 AND e.event_kind = 'consumption' \
               AND tr.reverted_at IS NULL \
               AND NOT EXISTS (SELECT 1 FROM inventory_events r WHERE r.reverses_event_id = e.id) \
               AND {OPEN_EPIC} \
             GROUP BY o.id, e.type_id"
        ))
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(persistence)?;
        rows.into_iter()
            .map(|(order_id, type_id, consumed)| {
                Ok(RunwayEventConsumption {
                    order_id: OrderId(order_id),
                    type_id,
                    quantity: quantity(consumed, "consumed quantity")?,
                })
            })
            .collect()
    }

    async fn runway_pools(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<RunwayStockPool>, InventoryError> {
        let rows: Vec<(Uuid, i64, i64, i64)> = sqlx::query_as(
            "SELECT owner_id, type_id, SUM(on_hand)::bigint, SUM(reserved)::bigint FROM ( \
               SELECT owner_id, type_id, quantity AS on_hand, 0 AS reserved \
               FROM inventory_balances WHERE workspace_id = $1 \
               UNION ALL \
               SELECT owner_id, type_id, 0, quantity FROM inventory_allocations \
               WHERE workspace_id = $1 AND released_at IS NULL AND consumed_at IS NULL \
             ) stock GROUP BY owner_id, type_id",
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(persistence)?;
        rows.into_iter()
            .map(|(owner_id, type_id, on_hand, reserved)| {
                Ok(RunwayStockPool {
                    owner_id: OwnerId(owner_id),
                    type_id,
                    on_hand: quantity(on_hand, "on-hand quantity")?,
                    active_reserved: quantity(reserved, "reserved quantity")?,
                })
            })
            .collect()
    }
}

#[async_trait]
impl InventoryRunwayRepository for PgOrderRepository {
    async fn runway_facts(&self, workspace_id: WorkspaceId) -> Result<RunwayFacts, InventoryError> {
        let (epics, (requirements, names), event_consumption, pools) = tokio::try_join!(
            self.runway_epics(workspace_id),
            self.runway_requirements(workspace_id),
            self.runway_event_consumption(workspace_id),
            self.runway_pools(workspace_id),
        )?;
        let types = crate::material_types::material_types(&self.pool, names)
            .await?
            .into_iter()
            .map(|(type_id, name, category)| RunwayTypeFact {
                type_id,
                name,
                category,
            })
            .collect();
        Ok(RunwayFacts {
            epics,
            requirements,
            event_consumption,
            pools,
            types,
        })
    }
}
