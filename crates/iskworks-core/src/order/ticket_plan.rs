//! Pure rules for turning a frozen Epic snapshot into tickets: root output
//! quantity, the frozen inventory-reuse split of a material line, a ticket's
//! prerequisite rows, and the run count a Build-backed ticket is planned for.

use super::*;
use crate::{BuildId, FulfillmentScope, IndustryError, Money, PlannedMaterialLine};

/// The root Manufacturing/Reaction ticket's output quantity: discrete-run
/// semantics, `runs x quantity_per_run`. This *is* the root operation, so
/// there is no downstream required-quantity rounding to apply -- 3 runs of a
/// recipe that yields 500 per run is a ticket producing exactly 1500.
pub fn planned_root_output(runs: u64, quantity_per_run: u64) -> Result<u64, IndustryError> {
    runs.checked_mul(quantity_per_run)
        .ok_or(IndustryError::MoneyOverflow)
}

/// The frozen Model-B inventory-reuse split for one worksheet `line` of a
/// coverage-aware snapshot (`calculate_epic_snapshot`). `line.reused_quantity`
/// is `Some(_)` iff the line was in the coverage map, i.e. iff it is
/// `Missing`-scoped (the coordinator excludes explicitly `Full`-scoped
/// type_ids). `Some(0)` is a real `Missing` state -- selected, nothing
/// available yet. Returns `(scope, reused_quantity, reused_line_total)`.
pub fn frozen_reuse(line: &PlannedMaterialLine) -> (FulfillmentScope, u64, Option<Money>) {
    match line.reused_quantity {
        Some(reused) => (FulfillmentScope::Missing, reused, line.reused_line_total),
        None => (FulfillmentScope::Full, 0, None),
    }
}

/// [`NewOrderRequirement`] -> [`NewTicketPrerequisite`]: the same frozen
/// row, duplicated onto a ticket with a new id -- the convention every
/// production ticket's prerequisites already followed.
pub fn requirement_to_prerequisite(requirement: &NewOrderRequirement) -> NewTicketPrerequisite {
    NewTicketPrerequisite {
        id: TicketPrerequisiteId::new(),
        type_id: requirement.type_id,
        captured_name: requirement.captured_name.clone(),
        kind: requirement.kind,
        source_build_id: requirement.source_build_id,
        required_quantity: requirement.required_quantity,
        fulfillment_scope: requirement.fulfillment_scope,
        reused_quantity: requirement.reused_quantity,
        estimated_unit_cost: requirement.estimated_unit_cost,
        estimated_line_total: requirement.estimated_line_total,
        reused_line_total: requirement.reused_line_total,
        operation_occurrence_key: requirement.operation_occurrence_key.clone(),
        child_occurrence_key: requirement.child_occurrence_key.clone(),
        inventory_unit_basis: requirement.inventory_unit_basis,
        child_produced_quantity: requirement.child_produced_quantity,
        child_consumed_quantity: requirement.child_consumed_quantity,
        child_surplus_quantity: requirement.child_surplus_quantity,
        child_surplus_retained_basis: requirement.child_surplus_retained_basis,
        child_consumed_cost: requirement.child_consumed_cost,
        dependency_id: requirement.dependency_id.clone(),
        price_evidence: requirement.price_evidence.clone(),
    }
}

pub fn intended_build_backed_runs(
    plan_root: Option<BuildId>,
    build_id: BuildId,
    persisted_runs: u64,
    requested_runs: Option<u64>,
) -> Result<u64, IndustryError> {
    if plan_root.is_some_and(|root| root != build_id) && requested_runs.is_none() {
        return Err(IndustryError::Validation(
            "Runs are required for a production step whose quantity is derived from a top-level Build plan."
                .to_string(),
        ));
    }
    Ok(requested_runs.unwrap_or(persisted_runs))
}

/// [`OrderRequirement`] (already persisted under its Epic) ->
/// [`NewTicketPrerequisite`]: the same frozen row mirrored onto a ticket
/// created later, exactly as [`requirement_to_prerequisite`] mirrors it at
/// Epic creation.
#[must_use]
pub fn persisted_requirement_to_prerequisite(
    requirement: &OrderRequirement,
) -> NewTicketPrerequisite {
    NewTicketPrerequisite {
        id: TicketPrerequisiteId::new(),
        type_id: requirement.type_id,
        captured_name: requirement.captured_name.clone(),
        kind: requirement.kind,
        source_build_id: requirement.source_build_id,
        required_quantity: requirement.required_quantity,
        fulfillment_scope: requirement.fulfillment_scope,
        reused_quantity: requirement.reused_quantity,
        estimated_unit_cost: requirement.estimated_unit_cost,
        estimated_line_total: requirement.estimated_line_total,
        reused_line_total: requirement.reused_line_total,
        operation_occurrence_key: requirement.operation_occurrence_key.clone(),
        child_occurrence_key: requirement.child_occurrence_key.clone(),
        inventory_unit_basis: requirement.inventory_unit_basis,
        child_produced_quantity: requirement.child_produced_quantity,
        child_consumed_quantity: requirement.child_consumed_quantity,
        child_surplus_quantity: requirement.child_surplus_quantity,
        child_surplus_retained_basis: requirement.child_surplus_retained_basis,
        child_consumed_cost: requirement.child_consumed_cost,
        dependency_id: requirement.dependency_id.clone(),
        price_evidence: requirement.price_evidence.clone(),
    }
}

/// The ticket for one frozen operation of a version-3 Epic, built from its
/// persisted rows -- the same ticket Create Epic would have made for it:
/// the operation's product, quantity and costs, its own requirements as
/// prerequisites, and its occurrence key (which is what links recordings
/// and reservations to it). `parent_ticket_id` is left for storage to
/// resolve from the parent operation's ticket.
///
/// Its execution snapshot -- what the recording form prefills from -- is
/// the frozen operation's: runs, installation cost evidence and material
/// value. The freeze kept no blueprint, facility or duration, so those are
/// empty (the root ticket Create Epic made carries the fuller root
/// snapshot and market scope; a root ticket made here has neither extra).
#[must_use]
pub fn operation_ticket(
    order: &Order,
    operation: &PlanOperation,
    requirements: &[OrderRequirement],
) -> NewTicket {
    let prerequisites = requirements
        .iter()
        .filter(|requirement| {
            requirement.operation_occurrence_key.as_deref()
                == Some(operation.occurrence_key.as_str())
        })
        .map(persisted_requirement_to_prerequisite)
        .collect();
    NewTicket {
        id: TicketId::new(),
        workspace_id: order.workspace_id,
        owner_id: order.owner_id,
        order_id: Some(order.id),
        kind: match operation.activity {
            crate::build_materials::MaterialActivity::Manufacturing => TicketKind::Manufacturing,
            crate::build_materials::MaterialActivity::Reaction => TicketKind::Reaction,
        },
        type_id: Some(operation.product_type_id),
        captured_name: operation.product_name.clone(),
        quantity: Some(operation.produced_quantity),
        source_build_id: operation.build_id,
        estimated_unit_cost: None,
        estimated_line_total: operation.material_component_cost,
        market_region_id: None,
        market_location_id: None,
        price_source_id: None,
        notes: String::new(),
        assignee_character_id: None,
        execution_snapshot: Some(operation_execution_snapshot(operation)),
        prerequisites,
        occurrence_key: Some(operation.occurrence_key.clone()),
        parent_ticket_id: None,
        produced_quantity: Some(operation.produced_quantity),
        material_component_cost: operation.material_component_cost,
        own_installation_cost: operation.own_installation_cost,
        total_production_cost: operation.total_production_cost,
        plan_evidence: Some(operation.evidence.clone()),
    }
}

/// The intended plan for one frozen operation, in the shape a ticket's
/// execution snapshot takes: its runs, the frozen installation-cost
/// evidence (total = the operation's own installation cost) and material
/// value.
#[must_use]
pub fn operation_execution_snapshot(operation: &PlanOperation) -> crate::TaskExecutionSnapshot {
    let installation_cost = operation
        .evidence
        .installation
        .as_ref()
        .map(|installation| crate::InstallationCostBreakdown {
            complete: installation.complete,
            estimated_item_value: installation.estimated_item_value,
            system_cost_index: installation
                .system_cost_index
                .as_deref()
                .and_then(|value| value.parse().ok()),
            unmodified_system_index_cost: installation.unmodified_system_index_cost,
            job_cost_reduction_percent: installation
                .job_cost_reduction_percent
                .parse()
                .unwrap_or_default(),
            system_index_cost: installation.system_index_cost,
            facility_tax: installation.facility_tax,
            scc_surcharge: installation.scc_surcharge,
            alliance_surcharge: installation.alliance_surcharge,
            fixed_supplemental_cost: installation.fixed_supplemental_cost,
            total: operation.own_installation_cost,
            warnings: Vec::new(),
            formula_version: installation.formula_version.clone(),
        });
    crate::TaskExecutionSnapshot {
        runs: operation.runs,
        blueprint: None,
        facility: None,
        duration_seconds: None,
        installation_cost,
        material_value: operation.material_component_cost,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planned_root_output_is_runs_times_output_per_run() {
        // Discrete-run semantics: no downstream required-quantity rounding.
        // `.ok()` because `ApiError` is deliberately not `Debug`.
        assert_eq!(planned_root_output(1, 1).ok(), Some(1));
        assert_eq!(planned_root_output(10, 1).ok(), Some(10));
        assert_eq!(planned_root_output(1, 500).ok(), Some(500));
        assert_eq!(planned_root_output(3, 500).ok(), Some(1_500));
    }

    #[test]
    fn planned_root_output_rejects_an_overflowing_product() {
        assert!(planned_root_output(u64::MAX, 2).is_err());
    }

    #[test]
    fn canonical_descendant_ticket_runs_must_be_explicit() {
        let root = BuildId::new();
        let producer = BuildId::new();
        assert!(intended_build_backed_runs(Some(root), producer, 1, None,).is_err());
        assert_eq!(
            intended_build_backed_runs(Some(root), producer, 1, Some(12),).ok(),
            Some(12),
        );
        assert_eq!(
            intended_build_backed_runs(Some(root), root, 2, None).ok(),
            Some(2),
        );
    }
}
