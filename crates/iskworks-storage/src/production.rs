use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_core::order::{InventoryAllocationId, OrderId, TicketId, TicketStatus};
use iskworks_core::{
    BuildCoverageReport, BuildId, EsiHoldingContributor, EsiHoldings, EsiObservation,
    InventoryBalance, InventoryItemKey, InventoryReservation, InventoryReservationSource,
    MaterialCostQuality, MaterialCoverage, Money, OrderReservationStatus, OwnerId, ProductionError,
    ProductionRepository, QuantityCoverageState, SetEsiHoldingReconciliationInclusion, WorkspaceId,
};
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres};
use uuid::Uuid;

/// The single source of truth for "what does ESI currently observe, per
/// owner" -- shared as one SQL fragment (not re-derived) between
/// `list_esi_observations` (the Inventory row/list summary) and
/// `esi_holdings` (the discrepancy drill-down modal's per-character/
/// location breakdown), so the two can never disagree about which rows
/// count. Carries `location_id`/`location_flag`/character identity through
/// even though `list_esi_observations`'s own `SELECT` doesn't use them --
/// unused output columns don't change which row `DISTINCT ON` picks or
/// what the dedup/exclusion `WHERE` admits, so this stays exactly one
/// piece of dedup/scope logic instead of two queries that happen to agree
/// today.
///
/// Deduped by physical item (`source_item_id`) so a corp hangar seen by
/// two connected characters isn't double-counted; excludes fitted/
/// singleton assets and blueprint copies (`is_blueprint_copy` is nullable
/// -- `IS NOT TRUE` is the null-safe "not a confirmed BPC", since `= false`
/// would silently drop every non-blueprint row once it hit SQL's
/// three-valued logic); scoped across every connected character's active
/// complete snapshot regardless of connection status, per `observed_at`
/// staying visible for staleness rather than filtering disconnected
/// characters out. Binds `$1` workspace_id, `$2` owner_id.
const ESI_OBSERVATION_DEDUP_CTE: &str = r#"
WITH scoped_snapshots AS (
    SELECT s.id, s.observed_at, c.id AS connection_id, c.eve_character_id, c.character_name
    FROM eve_connections c
    JOIN esi_asset_snapshots s
      ON s.connection_id = c.id AND s.active AND s.status = 'complete'
    WHERE c.workspace_id = $1 AND c.owner_id = $2
),
deduped AS (
    SELECT DISTINCT ON (o.source_item_id)
        o.source_item_id, o.type_id, o.quantity, o.location_id, o.location_type,
        o.location_flag, snapshot.id AS snapshot_id, snapshot.observed_at,
        snapshot.connection_id, snapshot.eve_character_id, snapshot.character_name
    FROM esi_asset_observations o
    JOIN scoped_snapshots snapshot ON snapshot.id = o.snapshot_id
    WHERE o.is_singleton = false AND o.is_blueprint_copy IS NOT TRUE
    ORDER BY o.source_item_id, snapshot.observed_at DESC
)
"#;

#[derive(Clone)]
pub struct PgProductionRepository {
    pool: PgPool,
}

impl PgProductionRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn build_context<'e, E>(
        executor: E,
        workspace_id: WorkspaceId,
        build_id: BuildId,
    ) -> Result<BuildContext, ProductionError>
    where
        E: sqlx::Executor<'e, Database = Postgres>,
    {
        sqlx::query_as::<_, BuildContext>(
            "SELECT owner_id, revision, runs, recipe_fingerprint FROM builds WHERE workspace_id = $1 AND id = $2",
        )
        .bind(workspace_id.0)
        .bind(build_id.0)
        .fetch_optional(executor)
        .await
        .map_err(map_sqlx)?
        .ok_or(ProductionError::BuildNotFound)
    }

    async fn materials<'e, E>(
        executor: E,
        build_id: BuildId,
    ) -> Result<Vec<MaterialRow>, ProductionError>
    where
        E: sqlx::Executor<'e, Database = Postgres>,
    {
        sqlx::query_as::<_, MaterialRow>(
            r#"
            SELECT type_id, captured_name, quantity_per_run, sort_order
            FROM build_recipe_materials
            WHERE build_id = $1
            ORDER BY type_id
            "#,
        )
        .bind(build_id.0)
        .fetch_all(executor)
        .await
        .map_err(map_sqlx)
    }

    /// Builds hold no material reservations, so
    /// `reserved_for_this_build`/`reserved_by_other_builds` are always `0`
    /// here: everything else on `MaterialCoverage`
    /// (owned quantity, cost, coverage state) only ever needed real
    /// inventory, never a reservation table, so it stays fully live.
    async fn coverage_with<'e, E>(
        executor: E,
        workspace_id: WorkspaceId,
        build_id: BuildId,
    ) -> Result<BuildCoverageReport, ProductionError>
    where
        E: sqlx::Executor<'e, Database = Postgres> + Copy,
    {
        let build = Self::build_context(executor, workspace_id, build_id).await?;
        let runs = as_u64(build.runs)?;
        let materials = Self::materials(executor, build_id).await?;
        let connection_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM eve_connections WHERE workspace_id = $1 AND owner_id = $2)",
        )
        .bind(workspace_id.0)
        .bind(build.owner_id)
        .fetch_one(executor)
        .await
        .map_err(map_sqlx)?;
        let mut lines = Vec::with_capacity(materials.len());
        for material in materials {
            let required = material.required(runs)?;
            let balance =
                load_balance(executor, workspace_id, OwnerId(build.owner_id), &material).await?;
            let here = 0u64;
            let elsewhere = 0u64;
            let owned = balance.quantity;
            let unreserved = owned.saturating_sub(here.saturating_add(elsewhere));
            let available = here.saturating_add(unreserved);
            let covered = required.min(available);
            let missing = required.saturating_sub(covered);
            let additional = required.saturating_sub(here).min(unreserved);
            let quality = inventory_quality(
                executor,
                workspace_id,
                OwnerId(build.owner_id),
                material.type_id,
                &balance,
            )
            .await?;
            let average = resolvable_average(&balance)?;
            let projected = if balance.quantity >= required {
                average
                    .map(|value| value.checked_mul_quantity(required))
                    .transpose()
                    .map_err(|_| ProductionError::ArithmeticOverflow)?
            } else {
                None
            };
            let observation = load_observation(
                executor,
                workspace_id,
                OwnerId(build.owner_id),
                material.type_id,
            )
            .await?;
            let difference = observation.as_ref().map(|row| {
                row.quantity
                    .saturating_sub(i64::try_from(owned).unwrap_or(i64::MAX))
            });
            let mut warnings = Vec::new();
            if let Some(delta) = difference {
                if delta > 0 {
                    warnings.push(format!(
                        "EVE currently reports {delta} more units than ISK Works inventory. These units are not reservable until they are recorded through an Inventory Event."
                    ));
                } else if delta < 0 {
                    warnings.push(format!(
                        "ISK Works accounts for {} more units than EVE currently reports. Review the discrepancy before relying on the physical quantity.",
                        delta.unsigned_abs()
                    ));
                }
            }
            lines.push(MaterialCoverage {
                type_id: material.type_id,
                type_name: material.captured_name,
                sort_order: as_u32(material.sort_order)?,
                required_quantity: required,
                accounted_owned_quantity: owned,
                reserved_for_this_build: here,
                reserved_by_other_builds: elsewhere,
                unreserved_available_quantity: unreserved,
                available_to_this_build: available,
                reservable_additional_quantity: additional,
                covered_quantity: covered,
                missing_quantity: missing,
                average_historical_unit_cost: average,
                projected_historical_cost: projected,
                cost_quality: if balance.quantity < required {
                    MaterialCostQuality::Unresolved
                } else {
                    quality
                },
                quantity_coverage_state: quantity_state(owned, here, covered, required),
                esi_observed_quantity: observation
                    .as_ref()
                    .map(|row| as_u64(row.quantity))
                    .transpose()?,
                esi_reconciliation_difference: difference,
                esi_observed_at: observation.map(|row| row.observed_at),
                explanation: format!(
                    "Accounted {owned}; available to this Build {available}; required {required}."
                ),
                warnings,
                inventory_revision: balance.revision,
            });
        }
        lines.sort_by_key(|line| line.sort_order);
        let complete_quantity = lines.iter().all(|line| line.missing_quantity == 0);
        let complete_cost = lines
            .iter()
            .all(|line| line.accounted_owned_quantity >= line.required_quantity);
        let mut warnings = Vec::new();
        if !connection_exists {
            warnings.push("No EVE connection is linked to this Owner. Inventory coverage remains fully available from ISK Works accounting records.".to_string());
        } else if lines.iter().all(|line| line.esi_observed_at.is_none()) {
            warnings.push("No complete ESI asset snapshot is available for this Owner. Coverage uses ISK Works accounting inventory only.".to_string());
        }
        Ok(BuildCoverageReport {
            build_id,
            owner_id: OwnerId(build.owner_id),
            build_revision: as_u64(build.revision)?,
            recipe_fingerprint: build.recipe_fingerprint,
            runs,
            complete_quantity_coverage: complete_quantity,
            complete_cost_coverage: complete_cost,
            material_lines: lines,
            warnings,
        })
    }
}

#[async_trait]
impl ProductionRepository for PgProductionRepository {
    async fn coverage(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
    ) -> Result<BuildCoverageReport, ProductionError> {
        Self::coverage_with(&self.pool, workspace_id, build_id).await
    }

    /// Total quantity of `type_id` currently held by an active
    /// `inventory_allocations` row (an `order::OrderRequirement` or
    /// `order::TicketPrerequisite` claim not yet released/consumed) for
    /// this owner -- the Inventory page's "Reserved"/"Available" columns
    /// and the Build worksheet's create-candidate coverage preview both
    /// read this via `AppState::reserved_quantity`.
    async fn reserved_quantity(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        type_id: i64,
    ) -> Result<u64, ProductionError> {
        let reserved: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(quantity), 0)::bigint FROM inventory_allocations \
             WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3 \
             AND released_at IS NULL AND consumed_at IS NULL",
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .bind(type_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx)?;
        u64::try_from(reserved)
            .map_err(|_| ProductionError::Persistence("negative reserved quantity".to_string()))
    }

    /// `reserved_quantity`'s sum for every listed type in one grouped
    /// query; a type with no active allocation is absent.
    async fn reserved_quantities(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, u64>, ProductionError> {
        if type_ids.is_empty() {
            return Ok(std::collections::BTreeMap::new());
        }
        let rows: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT type_id, COALESCE(SUM(quantity), 0)::bigint FROM inventory_allocations \
             WHERE workspace_id = $1 AND owner_id = $2 AND type_id = ANY($3) \
             AND released_at IS NULL AND consumed_at IS NULL \
             GROUP BY type_id",
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .bind(type_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;
        rows.into_iter()
            .map(|(type_id, reserved)| {
                u64::try_from(reserved)
                    .map(|reserved| (type_id, reserved))
                    .map_err(|_| {
                        ProductionError::Persistence("negative reserved quantity".to_string())
                    })
            })
            .collect()
    }

    /// Every active `inventory_allocations` row for this type, each
    /// resolved to its owning Order or Ticket in one query -- an
    /// `order_requirement_id` allocation joins straight through to its
    /// Order (`order_requirements.order_id`), and a `ticket_prerequisite_id`
    /// allocation joins straight through to the ticket that needs the
    /// material (`ticket_prerequisites.ticket_id`). The `inventory_allocations_exactly_one_owner`
    /// CHECK constraint plus `ON DELETE CASCADE` from both `orders` and
    /// `tickets` mean exactly one side resolves for every row in practice.
    async fn list_reservations(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        type_id: i64,
    ) -> Result<Vec<InventoryReservation>, ProductionError> {
        let rows = sqlx::query_as::<_, ReservationRow>(
            r#"
            SELECT
                allocation.id,
                allocation.quantity,
                allocation.created_at,
                req_order.id AS order_id,
                req_order.display_name AS order_display_name,
                req_order.started_at AS order_started_at,
                req_order.completed_at AS order_completed_at,
                req_order.canceled_at AS order_canceled_at,
                prereq_ticket.id AS ticket_id,
                prereq_ticket.display_id AS ticket_display_id,
                prereq_ticket.status AS ticket_status
            FROM inventory_allocations allocation
            LEFT JOIN order_requirements req ON req.id = allocation.order_requirement_id
            LEFT JOIN orders req_order ON req_order.id = req.order_id
            LEFT JOIN ticket_prerequisites prereq ON prereq.id = allocation.ticket_prerequisite_id
            LEFT JOIN tickets prereq_ticket ON prereq_ticket.id = prereq.ticket_id
            WHERE allocation.workspace_id = $1
              AND allocation.owner_id = $2
              AND allocation.type_id = $3
              AND allocation.released_at IS NULL
              AND allocation.consumed_at IS NULL
            ORDER BY allocation.created_at ASC
            "#,
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .bind(type_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        rows.into_iter().map(reservation_from_row).collect()
    }

    /// See `ESI_OBSERVATION_DEDUP_CTE` for the scope/dedup/exclusion rules
    /// this shares with `esi_holdings`.
    async fn list_esi_observations(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
    ) -> Result<BTreeMap<i64, EsiObservation>, ProductionError> {
        let sql = format!(
            "{ESI_OBSERVATION_DEDUP_CTE} \
             SELECT d.type_id, SUM(d.quantity)::bigint AS quantity, \
                    SUM(CASE WHEN exclusion.type_id IS NOT NULL THEN d.quantity ELSE 0 END)::bigint AS ignored_quantity, \
                    MAX(d.observed_at) AS observed_at \
             FROM deduped d \
             JOIN esi_asset_hierarchy resolved \
               ON resolved.snapshot_id=d.snapshot_id AND resolved.source_item_id=d.source_item_id \
             LEFT JOIN inventory_reconciliation_exclusions exclusion \
               ON exclusion.workspace_id=$1 AND exclusion.owner_id=$2 AND exclusion.type_id=d.type_id \
              AND exclusion.eve_character_id=d.eve_character_id AND exclusion.effective_location_id=resolved.effective_location_id \
             GROUP BY d.type_id"
        );
        let rows = sqlx::query_as::<_, EsiObservationRow>(&sql)
            .bind(workspace_id.0)
            .bind(owner_id.0)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx)?;

        let mut observations = BTreeMap::new();
        for row in rows {
            observations.insert(
                row.type_id,
                EsiObservation {
                    quantity: as_u64(row.quantity)?,
                    ignored_quantity: as_u64(row.ignored_quantity)?,
                    included_quantity: as_u64(row.quantity - row.ignored_quantity)?,
                    observed_at: row.observed_at,
                },
            );
        }
        Ok(observations)
    }

    /// See `ESI_OBSERVATION_DEDUP_CTE`. `observed_quantity`/`observed_at`
    /// are computed in Rust from these exact same contributor rows (not a
    /// separate `SUM`/`MAX` query) so the drill-down's total is
    /// *structurally* guaranteed to equal the sum of what it shows, and to
    /// agree with `list_esi_observations`'s figure for the same type_id.
    async fn esi_holdings(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        type_id: i64,
    ) -> Result<EsiHoldings, ProductionError> {
        // `d.location_id` is the *raw* ESI location: for an item stashed in a
        // ship's cargo hold or a container, that's the ship/container's own
        // item id, not a station -- resolvable against neither
        // `market_location_names` nor `sde_npc_stations`. Join the snapshot's
        // persisted hierarchy (`esi_asset_hierarchy`, the same resolution the
        // Assets browser reads) for the terminal station/structure/system.
        let sql = format!(
            "{ESI_OBSERVATION_DEDUP_CTE}, \
             target AS ( \
                 SELECT * FROM deduped WHERE type_id = $3 \
             ) \
             SELECT d.connection_id, d.eve_character_id, d.character_name, \
                    resolved.effective_location_id AS location_id, \
                    COALESCE(loc.location_name, station.name_en) AS location_name, \
                    MIN(d.location_flag) AS location_flag, \
                    SUM(d.quantity)::bigint AS quantity, MAX(d.observed_at) AS observed_at, \
                    BOOL_OR(exclusion.type_id IS NOT NULL) AS ignored_for_reconciliation \
             FROM target d \
             JOIN esi_asset_hierarchy resolved \
               ON resolved.snapshot_id = d.snapshot_id AND resolved.source_item_id = d.source_item_id \
             LEFT JOIN inventory_reconciliation_exclusions exclusion \
               ON exclusion.workspace_id=$1 AND exclusion.owner_id=$2 AND exclusion.type_id=d.type_id \
              AND exclusion.eve_character_id=d.eve_character_id AND exclusion.effective_location_id=resolved.effective_location_id \
             LEFT JOIN market_location_names loc \
               ON loc.workspace_id = $1 AND loc.location_id = resolved.effective_location_id \
             LEFT JOIN sde_imports import ON import.active \
             LEFT JOIN sde_npc_stations station \
               ON station.import_id = import.id AND station.station_id = resolved.effective_location_id \
             GROUP BY d.connection_id, d.eve_character_id, d.character_name, \
                      resolved.effective_location_id, loc.location_name, station.name_en \
             ORDER BY quantity DESC"
        );
        let rows = sqlx::query_as::<_, EsiHoldingRow>(&sql)
            .bind(workspace_id.0)
            .bind(owner_id.0)
            .bind(type_id)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx)?;

        let mut observed_quantity: u64 = 0;
        let mut ignored_quantity: u64 = 0;
        let mut observed_at: Option<DateTime<Utc>> = None;
        let mut contributors = Vec::with_capacity(rows.len());
        for row in rows {
            let quantity = as_u64(row.quantity)?;
            observed_quantity = observed_quantity
                .checked_add(quantity)
                .ok_or(ProductionError::ArithmeticOverflow)?;
            if row.ignored_for_reconciliation {
                ignored_quantity = ignored_quantity
                    .checked_add(quantity)
                    .ok_or(ProductionError::ArithmeticOverflow)?;
            }
            observed_at = Some(match observed_at {
                Some(current) => current.max(row.observed_at),
                None => row.observed_at,
            });
            contributors.push(EsiHoldingContributor {
                connection_id: row.connection_id,
                eve_character_id: row.eve_character_id,
                character_name: row.character_name,
                location_id: row.location_id,
                location_name: row.location_name,
                location_flag: row.location_flag,
                quantity,
                ignored_for_reconciliation: row.ignored_for_reconciliation,
            });
        }
        Ok(EsiHoldings {
            type_id,
            observed_quantity,
            ignored_quantity,
            included_quantity: observed_quantity.saturating_sub(ignored_quantity),
            observed_at,
            contributors,
        })
    }

    async fn set_esi_holding_reconciliation_inclusion(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: SetEsiHoldingReconciliationInclusion,
    ) -> Result<EsiHoldings, ProductionError> {
        let current = self
            .esi_holdings(workspace_id, owner_id, command.type_id)
            .await?;
        if !current.contributors.iter().any(|row| {
            row.eve_character_id == command.eve_character_id
                && row.location_id == command.effective_location_id
        }) {
            return Err(ProductionError::ReconciliationContributorChanged);
        }
        if command.included {
            sqlx::query("DELETE FROM inventory_reconciliation_exclusions WHERE workspace_id=$1 AND owner_id=$2 AND type_id=$3 AND eve_character_id=$4 AND effective_location_id=$5")
                .bind(workspace_id.0).bind(owner_id.0).bind(command.type_id).bind(command.eve_character_id).bind(command.effective_location_id)
                .execute(&self.pool).await.map_err(map_sqlx)?;
        } else {
            sqlx::query("INSERT INTO inventory_reconciliation_exclusions (workspace_id,owner_id,type_id,eve_character_id,effective_location_id,created_at) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING")
                .bind(workspace_id.0).bind(owner_id.0).bind(command.type_id).bind(command.eve_character_id).bind(command.effective_location_id).bind(crate::db_now())
                .execute(&self.pool).await.map_err(map_sqlx)?;
        }
        self.esi_holdings(workspace_id, owner_id, command.type_id)
            .await
    }
}

async fn load_balance<'e, E>(
    executor: E,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    material: &MaterialRow,
) -> Result<InventoryBalance, ProductionError>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let row = sqlx::query_as::<_, BalanceProjection>(
        r#"
        SELECT quantity, total_historical_cost, revision, last_activity_at
        FROM inventory_balances
        WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3
        "#,
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(material.type_id)
    .fetch_optional(executor)
    .await
    .map_err(map_sqlx)?;
    balance_from_row(row, workspace_id, owner_id, material)
}

fn balance_from_row(
    row: Option<BalanceProjection>,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    material: &MaterialRow,
) -> Result<InventoryBalance, ProductionError> {
    let key = InventoryItemKey {
        workspace_id,
        owner_id,
        type_id: material.type_id,
    };
    let Some(row) = row else {
        return Ok(InventoryBalance::empty(key, material.captured_name.clone()));
    };
    Ok(InventoryBalance {
        key,
        type_name: material.captured_name.clone(),
        quantity: as_u64(row.quantity)?,
        total_historical_cost: Money(row.total_historical_cost),
        average_unit_cost: None,
        revision: as_u64(row.revision)?,
        last_activity_at: Some(row.last_activity_at),
    })
}

fn resolvable_average(balance: &InventoryBalance) -> Result<Option<Money>, ProductionError> {
    if balance.quantity == 0 {
        return Ok(None);
    }
    let divisor = Decimal::from_i128_with_scale(i128::from(balance.quantity), 0);
    let mut average = balance
        .total_historical_cost
        .0
        .checked_div(divisor)
        .ok_or(ProductionError::ArithmeticOverflow)?;
    average.rescale(4);
    Ok(Some(Money(average)))
}

async fn inventory_quality<'e, E>(
    executor: E,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    type_id: i64,
    balance: &InventoryBalance,
) -> Result<MaterialCostQuality, ProductionError>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    if balance.quantity == 0 {
        return Ok(MaterialCostQuality::Unresolved);
    }
    if balance.total_historical_cost == Money::zero() {
        return Ok(MaterialCostQuality::ZeroCost);
    }
    let estimated: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM inventory_events WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3 AND cost_quality = 'estimated' AND quantity_delta > 0)",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(type_id)
    .fetch_one(executor)
    .await
    .map_err(map_sqlx)?;
    Ok(if estimated {
        MaterialCostQuality::Estimated
    } else {
        MaterialCostQuality::Known
    })
}

async fn load_observation<'e, E>(
    executor: E,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    type_id: i64,
) -> Result<Option<ObservationRow>, ProductionError>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, ObservationRow>(
        r#"
        SELECT COALESCE(SUM(o.quantity), 0)::bigint AS quantity, s.observed_at
        FROM eve_connections c
        JOIN esi_asset_snapshots s ON s.connection_id = c.id AND s.active AND s.status = 'complete'
        JOIN esi_asset_observations o ON o.snapshot_id = s.id AND o.type_id = $3
        WHERE c.workspace_id = $1 AND c.owner_id = $2
        GROUP BY s.id, s.observed_at
        ORDER BY s.observed_at DESC LIMIT 1
        "#,
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(type_id)
    .fetch_optional(executor)
    .await
    .map_err(map_sqlx)
}

fn quantity_state(owned: u64, here: u64, covered: u64, required: u64) -> QuantityCoverageState {
    if here == required {
        QuantityCoverageState::Reserved
    } else if here > 0 {
        QuantityCoverageState::ReservedPartially
    } else if owned == 0 {
        QuantityCoverageState::NoInventory
    } else if covered == required {
        QuantityCoverageState::Covered
    } else if covered > 0 {
        QuantityCoverageState::PartiallyCovered
    } else {
        QuantityCoverageState::Missing
    }
}

fn as_u64(value: i64) -> Result<u64, ProductionError> {
    u64::try_from(value).map_err(|_| ProductionError::ArithmeticOverflow)
}

fn as_u32(value: i32) -> Result<u32, ProductionError> {
    u32::try_from(value).map_err(|_| ProductionError::ArithmeticOverflow)
}

fn map_sqlx(error: sqlx::Error) -> ProductionError {
    ProductionError::Persistence(error.to_string())
}

fn reservation_from_row(row: ReservationRow) -> Result<InventoryReservation, ProductionError> {
    let source =
        if let (Some(order_id), Some(display_name)) = (row.order_id, row.order_display_name) {
            InventoryReservationSource::Order {
                order_id: OrderId(order_id),
                display_name,
                status: order_reservation_status(
                    row.order_started_at,
                    row.order_completed_at,
                    row.order_canceled_at,
                ),
            }
        } else if let (Some(ticket_id), Some(display_id), Some(status)) = (
            row.ticket_id,
            row.ticket_display_id,
            row.ticket_status.as_deref(),
        ) {
            InventoryReservationSource::Ticket {
                ticket_id: TicketId(ticket_id),
                display_id,
                status: ticket_status_from_str(status)?,
            }
        } else {
            return Err(ProductionError::Persistence(
                "inventory allocation resolved to neither an Order nor a Ticket".to_string(),
            ));
        };
    Ok(InventoryReservation {
        allocation_id: InventoryAllocationId(row.id),
        quantity: as_u64(row.quantity)?,
        created_at: row.created_at,
        source,
    })
}

fn order_reservation_status(
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    canceled_at: Option<DateTime<Utc>>,
) -> OrderReservationStatus {
    if canceled_at.is_some() {
        OrderReservationStatus::Canceled
    } else if completed_at.is_some() {
        OrderReservationStatus::Complete
    } else if started_at.is_some() {
        OrderReservationStatus::InProgress
    } else {
        OrderReservationStatus::NotStarted
    }
}

fn ticket_status_from_str(value: &str) -> Result<TicketStatus, ProductionError> {
    match value {
        "todo" => Ok(TicketStatus::Todo),
        "in_progress" => Ok(TicketStatus::InProgress),
        "complete" => Ok(TicketStatus::Complete),
        "canceled" => Ok(TicketStatus::Canceled),
        other => Err(ProductionError::Persistence(format!(
            "unknown ticket status {other}"
        ))),
    }
}

#[derive(sqlx::FromRow)]
struct BuildContext {
    owner_id: Uuid,
    revision: i64,
    runs: i64,
    recipe_fingerprint: String,
}

#[derive(sqlx::FromRow)]
struct MaterialRow {
    type_id: i64,
    captured_name: String,
    quantity_per_run: i64,
    sort_order: i32,
}

impl MaterialRow {
    fn required(&self, runs: u64) -> Result<u64, ProductionError> {
        as_u64(self.quantity_per_run)?
            .checked_mul(runs)
            .ok_or(ProductionError::ArithmeticOverflow)
    }
}

#[derive(sqlx::FromRow)]
struct BalanceProjection {
    quantity: i64,
    total_historical_cost: Decimal,
    revision: i64,
    last_activity_at: DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct ObservationRow {
    quantity: i64,
    observed_at: DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct EsiObservationRow {
    type_id: i64,
    quantity: i64,
    ignored_quantity: i64,
    observed_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct EsiHoldingRow {
    connection_id: Uuid,
    eve_character_id: i64,
    character_name: String,
    location_id: i64,
    location_name: Option<String>,
    location_flag: String,
    quantity: i64,
    observed_at: DateTime<Utc>,
    ignored_for_reconciliation: bool,
}

#[derive(sqlx::FromRow)]
struct ReservationRow {
    id: Uuid,
    quantity: i64,
    created_at: DateTime<Utc>,
    order_id: Option<Uuid>,
    order_display_name: Option<String>,
    order_started_at: Option<DateTime<Utc>>,
    order_completed_at: Option<DateTime<Utc>>,
    order_canceled_at: Option<DateTime<Utc>>,
    ticket_id: Option<Uuid>,
    ticket_display_id: Option<String>,
    ticket_status: Option<String>,
}

#[cfg(test)]
mod reservation_tests;

#[cfg(test)]
mod esi_observation_tests;
