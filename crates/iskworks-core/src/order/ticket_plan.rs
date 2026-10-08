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
