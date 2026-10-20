//! Facts for Inventory › Consumption: recorded consumption per UTC day and
//! type. A reversed event, or one posted by a since-reverted recording, no
//! longer counts.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use iskworks_core::{
    ConsumptionDay, ConsumptionFacts, ConsumptionTypeFact, InventoryConsumptionRepository,
};

use super::*;

/// Shared `FROM`/`WHERE` for live consumption events of workspace `$1`.
const LIVE_CONSUMPTION: &str = "FROM inventory_events e \
     LEFT JOIN ticket_inventory_recordings tr ON tr.id = e.ticket_inventory_recording_id \
     WHERE e.workspace_id = $1 AND e.event_kind = 'consumption' \
       AND tr.reverted_at IS NULL \
       AND NOT EXISTS (SELECT 1 FROM inventory_events r WHERE r.reverses_event_id = e.id)";

fn quantity(value: i64) -> Result<u64, InventoryError> {
    u64::try_from(value).map_err(|_| InventoryError::Persistence("negative consumption".into()))
}

impl PgInventoryRepository {
    async fn consumption_days(
        &self,
        workspace_id: WorkspaceId,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<ConsumptionDay>, InventoryError> {
        let rows: Vec<(NaiveDate, i64, i64, Decimal)> = sqlx::query_as(&format!(
            "SELECT (e.effective_at AT TIME ZONE 'UTC')::date AS day, e.type_id, \
               SUM(-e.quantity_delta)::bigint, SUM(-e.total_cost_delta) \
             {LIVE_CONSUMPTION} \
               AND (e.effective_at AT TIME ZONE 'UTC')::date BETWEEN $2 AND $3 \
             GROUP BY day, e.type_id"
        ))
        .bind(workspace_id.0)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;
        rows.into_iter()
            .map(|(day, type_id, consumed, value)| {
                Ok(ConsumptionDay {
                    day,
                    type_id,
                    quantity: quantity(consumed)?,
                    value: Money(value),
                })
            })
            .collect()
    }

    /// Every consumed type's first consumption and a captured name.
    async fn consumed_types(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<(i64, NaiveDate, String)>, InventoryError> {
        sqlx::query_as(&format!(
            "SELECT e.type_id, MIN((e.effective_at AT TIME ZONE 'UTC')::date), \
               MAX(e.captured_name) \
             {LIVE_CONSUMPTION} \
             GROUP BY e.type_id"
        ))
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)
    }

    async fn on_hand_by_type(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<HashMap<i64, u64>, InventoryError> {
        let rows: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT type_id, SUM(quantity)::bigint FROM inventory_balances \
             WHERE workspace_id = $1 GROUP BY type_id",
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;
        rows.into_iter()
            .map(|(type_id, on_hand)| Ok((type_id, quantity(on_hand)?)))
            .collect()
    }
}

#[async_trait]
impl InventoryConsumptionRepository for PgInventoryRepository {
    async fn consumption_facts(
        &self,
        workspace_id: WorkspaceId,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<ConsumptionFacts, InventoryError> {
        let (days, consumed, on_hand) = tokio::try_join!(
            self.consumption_days(workspace_id, from, to),
            self.consumed_types(workspace_id),
            self.on_hand_by_type(workspace_id),
        )?;
        let first_consumed: HashMap<i64, NaiveDate> = consumed
            .iter()
            .map(|(type_id, first, _)| (*type_id, *first))
            .collect();
        let names: BTreeMap<i64, String> = consumed
            .into_iter()
            .map(|(type_id, _, name)| (type_id, name))
            .collect();
        let types = crate::material_types::material_types(&self.pool, names)
            .await?
            .into_iter()
            .map(|(type_id, name, category)| ConsumptionTypeFact {
                type_id,
                name,
                category,
                first_consumed: first_consumed[&type_id],
                on_hand: on_hand.get(&type_id).copied().unwrap_or(0),
            })
            .collect();
        Ok(ConsumptionFacts { days, types })
    }
}

#[cfg(test)]
mod tests;
