use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::order::{InventoryAllocationId, OrderId, TicketId, TicketStatus};
use crate::{BuildId, Money, OwnerId, WorkspaceId};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QuantityCoverageState {
    NoInventory,
    Missing,
    PartiallyCovered,
    Covered,
    ReservedPartially,
    Reserved,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MaterialCostQuality {
    Known,
    Estimated,
    ZeroCost,
    Unresolved,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialCoverage {
    pub type_id: i64,
    pub type_name: String,
    pub sort_order: u32,
    pub required_quantity: u64,
    pub accounted_owned_quantity: u64,
    pub reserved_for_this_build: u64,
    pub reserved_by_other_builds: u64,
    pub unreserved_available_quantity: u64,
    pub available_to_this_build: u64,
    pub reservable_additional_quantity: u64,
    pub covered_quantity: u64,
    pub missing_quantity: u64,
    pub average_historical_unit_cost: Option<Money>,
    pub projected_historical_cost: Option<Money>,
    pub cost_quality: MaterialCostQuality,
    pub quantity_coverage_state: QuantityCoverageState,
    pub esi_observed_quantity: Option<u64>,
    pub esi_reconciliation_difference: Option<i64>,
    pub esi_observed_at: Option<DateTime<Utc>>,
    pub explanation: String,
    pub warnings: Vec<String>,
    pub inventory_revision: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildCoverageReport {
    pub build_id: BuildId,
    pub owner_id: OwnerId,
    pub build_revision: u64,
    pub recipe_fingerprint: String,
    pub runs: u64,
    pub complete_quantity_coverage: bool,
    pub complete_cost_coverage: bool,
    pub material_lines: Vec<MaterialCoverage>,
    pub warnings: Vec<String>,
}

/// A coarse Order lifecycle summary for the Inventory page's Reservations
/// tab -- deliberately **not** the real `order::OrderStatus`
/// (Blocked/Ready/InProgress/Complete/Canceled -- an Epic-lifecycle enum),
/// which requires re-deriving
/// every requirement's fulfillment state via `order::derive_requirement_state`
/// and `order::compute_order_rollup`, each needing its own fulfilling-ticket
/// lookups. Recomputing that per reservation row here would either
/// duplicate that derivation or make this listing an N+1 query storm.
/// This reads only the Order's own three lifecycle timestamps, which is
/// enough to tell "reserved and idle" from "actively moving" from "done" --
/// the "Navigate to order" link is where a user goes for the real status.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OrderReservationStatus {
    NotStarted,
    InProgress,
    Complete,
    Canceled,
}

/// Who is holding a reservation against physical inventory -- resolved
/// from `order::AllocationOwner`'s `OrderRequirement`/`TicketPrerequisite`
/// id into something the Inventory page can display and link to directly,
/// without the caller needing a second round trip per row.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum InventoryReservationSource {
    Order {
        order_id: OrderId,
        display_name: String,
        status: OrderReservationStatus,
    },
    /// A `TicketPrerequisite` allocation resolves directly to the ticket
    /// that *needs* the material (`ticket_prerequisites.ticket_id`), not
    /// to whatever fulfills it -- that's the ticket a shortfall blocks.
    /// Unlike `OrderReservationStatus` above, `TicketStatus` is read
    /// straight off the `tickets.status` column -- a purely user-controlled
    /// organizational value, nothing this listing has to recompute.
    Ticket {
        ticket_id: TicketId,
        display_id: String,
        status: TicketStatus,
    },
}

/// One active (`released_at IS NULL AND consumed_at IS NULL`)
/// `inventory_allocations` row, enriched with enough of its owning
/// Order/Ticket to render and link to from the Inventory page's
/// Reservations tab -- see `ProductionRepository::list_reservations`.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryReservation {
    pub allocation_id: InventoryAllocationId,
    pub quantity: u64,
    pub created_at: DateTime<Utc>,
    pub source: InventoryReservationSource,
}

/// What ESI currently reports for one EVE type, for one owner -- the
/// Inventory page's ESI observation column/section. `quantity` is already
/// deduped by physical item and excludes fitted/singleton assets and
/// blueprint copies (Inventory only accounts for fungible stock); it is
/// summed across every connected character regardless of connection
/// status, so `observed_at` (the freshest contributing snapshot) is what
/// tells a reader whether this is current or stale -- see
/// `ProductionRepository::list_esi_observations`.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EsiObservation {
    pub quantity: u64,
    pub ignored_quantity: u64,
    pub included_quantity: u64,
    pub observed_at: DateTime<Utc>,
}

/// One (character, resolved location) group contributing to an
/// `EsiObservation`'s total -- the Inventory ESI discrepancy drill-down.
/// `location_name` is `None` when `location_id` isn't in the locally
/// cached `market_location_names` table yet (most commonly because it's
/// actually a container/ship item id, not a station or structure --
/// `esi_asset_observations.location_id` is only resolvable when
/// `location_type` is `station`/`structure`); the caller falls back to
/// "Unknown location {id}", never a synchronous lookup.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EsiHoldingContributor {
    pub connection_id: Uuid,
    pub eve_character_id: i64,
    pub character_name: String,
    pub location_id: i64,
    pub location_name: Option<String>,
    pub location_flag: String,
    pub quantity: u64,
    pub ignored_for_reconciliation: bool,
}

/// The full breakdown behind one `EsiObservation` -- every contributor
/// summing to exactly `observed_quantity` by construction (both are
/// computed from the same query result in
/// `ProductionRepository::esi_holdings`, itself sharing its dedup/
/// exclusion CTE text with `list_esi_observations` so the two can never
/// drift apart). `contributors` is empty (not an error) when ESI has
/// never observed this type for this owner.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EsiHoldings {
    pub type_id: i64,
    pub observed_quantity: u64,
    pub ignored_quantity: u64,
    pub included_quantity: u64,
    pub observed_at: Option<DateTime<Utc>>,
    pub contributors: Vec<EsiHoldingContributor>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetEsiHoldingReconciliationInclusion {
    pub type_id: i64,
    pub eve_character_id: i64,
    pub effective_location_id: i64,
    pub included: bool,
}

/// Read-only material coverage for the Build worksheet's create-candidate
/// preview: `coverage` and `reserved_quantity`. Reservation/production/
/// completion (`reserve`, `start`, `complete`, ...) as stateful operations
/// on `Build` itself don't exist here -- that lifecycle belongs to
/// `order::OrderRepository` (`Order`/`Ticket`/`inventory_allocations`), not
/// this trait. `Build` stays a mutable planning model with no lifecycle of
/// its own.
#[async_trait]
pub trait ProductionRepository: Send + Sync {
    async fn coverage(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
    ) -> Result<BuildCoverageReport, ProductionError>;
    /// Sum of `quantity` across every currently-active (`released_at IS
    /// NULL AND consumed_at IS NULL`) `inventory_allocations` row for this
    /// `(workspace_id, owner_id, type_id)` -- the standing Order/Ticket
    /// allocation ledger, not a Plan-era concept. Read by the Inventory
    /// page's "Reserved"/"Available" columns and the Build worksheet's
    /// create-candidate coverage preview, both via `AppState::reserved_quantity`.
    async fn reserved_quantity(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        type_id: i64,
    ) -> Result<u64, ProductionError>;
    /// `reserved_quantity` for many types in one read, keyed by `type_id`;
    /// a type with nothing reserved may be absent (read it as 0). The
    /// Inventory list's batched form. Defaults to one `reserved_quantity`
    /// per type.
    async fn reserved_quantities(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, u64>, ProductionError> {
        let mut reserved = std::collections::BTreeMap::new();
        for &type_id in type_ids {
            reserved.insert(
                type_id,
                self.reserved_quantity(workspace_id, owner_id, type_id)
                    .await?,
            );
        }
        Ok(reserved)
    }
    /// Every currently-active `inventory_allocations` row for this
    /// `(workspace_id, owner_id, type_id)`, each resolved to its owning
    /// Order or Ticket -- the Inventory page's Reservations tab. Ordered
    /// oldest-first (`created_at ASC`) so the tab reads as a queue.
    async fn list_reservations(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        type_id: i64,
    ) -> Result<Vec<InventoryReservation>, ProductionError>;
    /// Every EVE type currently ESI-observed for this owner, keyed by
    /// `type_id` -- the Inventory page's row-level Match/Difference badge
    /// and the inspector's ESI Observation section, including types with
    /// no `inventory_balances` row at all (a brand-new ESI-observed type
    /// the route layer surfaces as a zero-owned "phantom" row). Absence
    /// from the returned map means "no ESI data for this type", which
    /// callers must not coerce to an observed quantity of zero -- ESI
    /// never emits a zero-quantity asset row, so there is no such thing
    /// as a real observed zero, only "observed" or "not observed".
    async fn list_esi_observations(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
    ) -> Result<std::collections::BTreeMap<i64, EsiObservation>, ProductionError>;
    /// The Inventory ESI discrepancy drill-down: every (character,
    /// location) holding contributing to this one type's observed
    /// quantity. Must share `list_esi_observations`'s exact scope/dedup/
    /// exclusion semantics for one type_id -- implementations should
    /// literally reuse the same CTE text, not a hand-rewritten copy, so
    /// `observed_quantity` here can never drift from what
    /// `list_esi_observations` reports for the same type.
    async fn esi_holdings(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        type_id: i64,
    ) -> Result<EsiHoldings, ProductionError>;

    async fn set_esi_holding_reconciliation_inclusion(
        &self,
        _workspace_id: WorkspaceId,
        _owner_id: OwnerId,
        _command: SetEsiHoldingReconciliationInclusion,
    ) -> Result<EsiHoldings, ProductionError> {
        Err(ProductionError::Persistence(
            "ESI reconciliation policy persistence is unavailable.".into(),
        ))
    }
}

pub fn project_candidate_coverage(
    current: &BuildCoverageReport,
    runs: u64,
    requirements: &[crate::PlannedMaterialLine],
) -> Result<BuildCoverageReport, ProductionError> {
    let current_by_type = current
        .material_lines
        .iter()
        .map(|line| (line.type_id, line))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut lines = Vec::with_capacity(requirements.len());
    for (sort_order, requirement) in requirements.iter().enumerate() {
        let current_line =
            current_by_type
                .get(&requirement.type_id)
                .ok_or(ProductionError::Persistence(
                    "Candidate material is absent from current coverage.".into(),
                ))?;
        let required = requirement.total_quantity;
        let here = current_line.reserved_for_this_build;
        let elsewhere = current_line.reserved_by_other_builds;
        let owned = current_line.accounted_owned_quantity;
        let unreserved = owned.saturating_sub(here.saturating_add(elsewhere));
        let available = here.saturating_add(unreserved);
        let covered = required.min(available);
        let missing = required.saturating_sub(covered);
        let additional = required.saturating_sub(here).min(unreserved);
        let projected_historical_cost = if current_line.accounted_owned_quantity >= required {
            current_line
                .average_historical_unit_cost
                .map(|cost| cost.checked_mul_quantity(required))
                .transpose()
                .map_err(|_| ProductionError::ArithmeticOverflow)?
        } else {
            None
        };
        let quantity_coverage_state = if owned == 0 {
            QuantityCoverageState::NoInventory
        } else if here >= required {
            QuantityCoverageState::Reserved
        } else if here > 0 {
            QuantityCoverageState::ReservedPartially
        } else if covered >= required {
            QuantityCoverageState::Covered
        } else if covered > 0 {
            QuantityCoverageState::PartiallyCovered
        } else {
            QuantityCoverageState::Missing
        };
        let mut line = (*current_line).clone();
        line.type_name = requirement.type_name.clone();
        line.sort_order =
            u32::try_from(sort_order).map_err(|_| ProductionError::ArithmeticOverflow)?;
        line.required_quantity = required;
        line.unreserved_available_quantity = unreserved;
        line.available_to_this_build = available;
        line.reservable_additional_quantity = additional;
        line.covered_quantity = covered;
        line.missing_quantity = missing;
        line.projected_historical_cost = projected_historical_cost;
        line.cost_quality = if current_line.accounted_owned_quantity >= required {
            current_line.cost_quality
        } else {
            MaterialCostQuality::Unresolved
        };
        line.quantity_coverage_state = quantity_coverage_state;
        line.explanation = format!(
            "Accounted {owned}; reserved here {here}; reserved elsewhere {elsewhere}; available to this Build {available}; required {required}."
        );
        lines.push(line);
    }
    let complete_quantity_coverage = lines.iter().all(|line| line.missing_quantity == 0);
    let complete_cost_coverage = lines
        .iter()
        .all(|line| line.accounted_owned_quantity >= line.required_quantity);
    Ok(BuildCoverageReport {
        build_id: current.build_id,
        owner_id: current.owner_id,
        build_revision: current.build_revision,
        recipe_fingerprint: current.recipe_fingerprint.clone(),
        runs,
        complete_quantity_coverage,
        complete_cost_coverage,
        material_lines: lines,
        warnings: current.warnings.clone(),
    })
}

pub fn project_create_coverage(
    build_id: BuildId,
    owner_id: OwnerId,
    recipe_fingerprint: &str,
    runs: u64,
    requirements: &[crate::PlannedMaterialLine],
    balances: &[crate::InventoryBalance],
    reservations: &std::collections::BTreeMap<i64, u64>,
) -> Result<BuildCoverageReport, ProductionError> {
    let balances_by_type = balances
        .iter()
        .map(|balance| (balance.key.type_id, balance))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut lines = Vec::with_capacity(requirements.len());
    for (sort_order, requirement) in requirements.iter().enumerate() {
        let balance = balances_by_type.get(&requirement.type_id).copied();
        let owned = balance.map_or(0, |item| item.quantity);
        let elsewhere = reservations.get(&requirement.type_id).copied().unwrap_or(0);
        let available = owned.saturating_sub(elsewhere);
        let required = requirement.total_quantity;
        let covered = required.min(available);
        let missing = required.saturating_sub(covered);
        let average = balance.and_then(|item| item.average_unit_cost);
        let projected_historical_cost = if owned >= required {
            average
                .map(|cost| cost.checked_mul_quantity(required))
                .transpose()
                .map_err(|_| ProductionError::ArithmeticOverflow)?
        } else {
            None
        };
        let quality = if owned < required {
            MaterialCostQuality::Unresolved
        } else if balance.is_some_and(|item| item.total_historical_cost == Money::zero()) {
            MaterialCostQuality::ZeroCost
        } else {
            MaterialCostQuality::Known
        };
        let quantity_coverage_state = if owned == 0 {
            QuantityCoverageState::NoInventory
        } else if covered >= required {
            QuantityCoverageState::Covered
        } else if covered > 0 {
            QuantityCoverageState::PartiallyCovered
        } else {
            QuantityCoverageState::Missing
        };
        lines.push(MaterialCoverage {
            type_id: requirement.type_id,
            type_name: requirement.type_name.clone(),
            sort_order: u32::try_from(sort_order)
                .map_err(|_| ProductionError::ArithmeticOverflow)?,
            required_quantity: required,
            accounted_owned_quantity: owned,
            reserved_for_this_build: 0,
            reserved_by_other_builds: elsewhere,
            unreserved_available_quantity: available,
            available_to_this_build: available,
            reservable_additional_quantity: covered,
            covered_quantity: covered,
            missing_quantity: missing,
            average_historical_unit_cost: average,
            projected_historical_cost,
            cost_quality: quality,
            quantity_coverage_state,
            esi_observed_quantity: None,
            esi_reconciliation_difference: None,
            esi_observed_at: None,
            explanation: format!(
                "Accounted {owned}; reserved elsewhere {elsewhere}; available to this Build {available}; required {required}."
            ),
            warnings: Vec::new(),
            inventory_revision: balance.map_or(0, |item| item.revision),
        });
    }
    let complete_quantity_coverage = lines.iter().all(|line| line.missing_quantity == 0);
    let complete_cost_coverage = lines
        .iter()
        .all(|line| line.accounted_owned_quantity >= line.required_quantity);
    Ok(BuildCoverageReport {
        build_id,
        owner_id,
        build_revision: 1,
        recipe_fingerprint: recipe_fingerprint.to_string(),
        runs,
        complete_quantity_coverage,
        complete_cost_coverage,
        material_lines: lines,
        warnings: Vec::new(),
    })
}

#[cfg(test)]
mod candidate_coverage_tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn candidate_requirement_recomputes_shortage_and_historical_cost() {
        let current = BuildCoverageReport {
            build_id: BuildId::new(),
            owner_id: OwnerId::new(),
            build_revision: 7,
            recipe_fingerprint: "recipe".into(),
            runs: 1,
            complete_quantity_coverage: true,
            complete_cost_coverage: true,
            material_lines: vec![MaterialCoverage {
                type_id: 34,
                type_name: "Tritanium".into(),
                sort_order: 0,
                required_quantity: 28_800,
                accounted_owned_quantity: 27_500,
                reserved_for_this_build: 0,
                reserved_by_other_builds: 0,
                unreserved_available_quantity: 27_500,
                available_to_this_build: 27_500,
                reservable_additional_quantity: 27_500,
                covered_quantity: 27_500,
                missing_quantity: 1_300,
                average_historical_unit_cost: Some(Money::parse("4.0000").unwrap()),
                projected_historical_cost: None,
                cost_quality: MaterialCostQuality::Known,
                quantity_coverage_state: QuantityCoverageState::PartiallyCovered,
                esi_observed_quantity: None,
                esi_reconciliation_difference: None,
                esi_observed_at: None,
                explanation: String::new(),
                warnings: vec![],
                inventory_revision: 2,
            }],
            warnings: vec![],
        };
        let requirements = vec![crate::PlannedMaterialLine {
            type_id: 34,
            type_name: "Tritanium".into(),
            quantity_per_run: 32_000,
            total_quantity: 27_075,
            unit_price: Some(Money::parse("4.1000").unwrap()),
            line_total: Some(Money::parse("111007.5000").unwrap()),
            missing: false,
            contributions: Vec::new(),
            is_build_resolved: false,
            installation_cost: None,
            reused_quantity: None,
            missing_quantity: None,
            reused_line_total: None,
            planning_evidence: None,
        }];

        let projected = project_candidate_coverage(&current, 1, &requirements).unwrap();

        assert_eq!(projected.material_lines[0].missing_quantity, 0);
        assert_eq!(
            projected.material_lines[0]
                .projected_historical_cost
                .unwrap()
                .0
                .to_string(),
            "108300.0000"
        );
        assert!(projected.complete_quantity_coverage);
        assert!(projected.complete_cost_coverage);
    }

    #[test]
    fn create_requirement_projects_unreserved_inventory_and_historical_cost() {
        let workspace_id = WorkspaceId::new();
        let owner_id = OwnerId::new();
        let balances = vec![crate::InventoryBalance {
            key: crate::InventoryItemKey {
                workspace_id,
                owner_id,
                type_id: 34,
            },
            type_name: "Tritanium".into(),
            quantity: 1_200,
            total_historical_cost: Money::parse("6000").unwrap(),
            average_unit_cost: Some(Money::parse("5").unwrap()),
            revision: 9,
            last_activity_at: None,
        }];
        let requirements = vec![crate::PlannedMaterialLine {
            type_id: 34,
            type_name: "Tritanium".into(),
            quantity_per_run: 1_000,
            total_quantity: 1_000,
            unit_price: Some(Money::parse("4").unwrap()),
            line_total: Some(Money::parse("4000").unwrap()),
            missing: false,
            contributions: Vec::new(),
            is_build_resolved: false,
            installation_cost: None,
            reused_quantity: None,
            missing_quantity: None,
            reused_line_total: None,
            planning_evidence: None,
        }];
        let reservations = BTreeMap::from([(34, 300)]);

        let coverage = project_create_coverage(
            BuildId::new(),
            owner_id,
            "recipe",
            1,
            &requirements,
            &balances,
            &reservations,
        )
        .unwrap();

        let line = &coverage.material_lines[0];
        assert_eq!(line.reserved_for_this_build, 0);
        assert_eq!(line.reserved_by_other_builds, 300);
        assert_eq!(line.available_to_this_build, 900);
        assert_eq!(line.covered_quantity, 900);
        assert_eq!(line.missing_quantity, 100);
        assert_eq!(
            line.projected_historical_cost,
            Some(Money::parse("5000").unwrap())
        );
        assert!(!coverage.complete_quantity_coverage);
        assert!(coverage.complete_cost_coverage);
    }
}

#[derive(Debug, Error)]
pub enum ProductionError {
    #[error("build was not found")]
    BuildNotFound,
    #[error("inventory arithmetic exceeds the supported range")]
    ArithmeticOverflow,
    #[error("the ESI holding changed while reconciliation policy was being updated")]
    ReconciliationContributorChanged,
    #[error("persistence failed: {0}")]
    Persistence(String),
}

pub fn required_quantity(per_run: u64, runs: u64) -> Result<u64, ProductionError> {
    per_run
        .checked_mul(runs)
        .ok_or(ProductionError::ArithmeticOverflow)
}
