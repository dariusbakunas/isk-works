//! A frozen Epic as an [`ExecutionPlanProjection`] -- the exact shape the
//! Build's Plan view already renders -- so selecting an Epic changes the
//! Plan's *data*, never its layout.
//!
//! Built purely from the Epic's frozen version-3 operations and
//! requirements: one display node (and one occurrence) per operation,
//! staged by the frozen operation DAG, each with its own frozen
//! requirements; every `Buy` requirement still to source becomes an
//! acquisition line. Nothing is re-planned. Fields the freeze did not keep
//! stay empty rather than invented: no facility, no production methods, no
//! unresolved placeholders, no logistics, and typed cost warnings are
//! omitted (the freeze kept them only as text).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::*;
use crate::build_materials::{MaterialBoundaryResolution, MaterialRowStrategy};
use crate::execution_plan::{
    AcquisitionConsumerRef, AcquisitionLine, ExecutionConsumerRef, ExecutionNode,
    ExecutionOccurrence, ExecutionPlanProjection, ExecutionRequirement, ExecutionStage,
};

/// Inventory the projection reads alongside the frozen plan.
#[derive(Debug, Clone, Default)]
pub struct FrozenPlanInventory {
    /// Free stock per type (`physical - every active reservation`).
    pub free_by_type: BTreeMap<i64, u64>,
    /// Stock other Epics hold per type (excludes this Epic's own).
    pub reserved_elsewhere_by_type: BTreeMap<i64, u64>,
}

fn resolution(kind: RequirementKind) -> MaterialBoundaryResolution {
    match kind {
        RequirementKind::Buy => MaterialBoundaryResolution::Buy,
        RequirementKind::Build => MaterialBoundaryResolution::Build,
        RequirementKind::React => MaterialBoundaryResolution::Reaction,
    }
}

fn dependency_key(requirement: &OrderRequirement) -> String {
    requirement
        .dependency_id
        .clone()
        .unwrap_or_else(|| format!("req:{}", requirement.id.0))
}

/// The frozen cost of a requirement's fresh (to-buy) portion, when known.
fn fresh_cost(requirement: &OrderRequirement) -> Option<Money> {
    let total = requirement.estimated_line_total?;
    let reused = match requirement.reused_line_total {
        Some(reused) => reused,
        None if requirement.reused_quantity == 0 => Money::zero(),
        None => return None,
    };
    Some(Money(total.0 - reused.0))
}

fn per_unit(total: Option<Money>, quantity: u64) -> Option<Money> {
    let total = total?;
    (quantity > 0).then(|| Money(total.0 / rust_decimal::Decimal::from(quantity)))
}

/// A build id for a frozen operation whose Build was since deleted.
fn build_or_nil(build_id: Option<BuildId>) -> BuildId {
    build_id.unwrap_or(BuildId(Uuid::nil()))
}

/// Project a version-3 Epic's frozen plan into the Plan view's shape.
///
/// # Errors
///
/// [`OrderError::CorruptProductionGraph`] if the frozen relation is not a
/// single-rooted DAG (see [`derive_operation_dag`]).
pub fn project_frozen_execution_plan(
    operations: &[PlanOperation],
    requirements: &[OrderRequirement],
    blueprint_names: &BTreeMap<i64, String>,
    inventory: &FrozenPlanInventory,
    generated_at: DateTime<Utc>,
) -> Result<ExecutionPlanProjection, OrderError> {
    let dag = derive_operation_dag(
        operations
            .iter()
            .map(|operation| operation.occurrence_key.as_str()),
        requirements.iter().filter_map(|requirement| {
            Some(FrozenDemandEdge {
                consumer: requirement.operation_occurrence_key.as_deref()?,
                producer: requirement.child_occurrence_key.as_deref()?,
                dependency_id: requirement.dependency_id.as_deref(),
            })
        }),
    )?;
    let by_key: BTreeMap<&str, &PlanOperation> = operations
        .iter()
        .map(|operation| (operation.occurrence_key.as_str(), operation))
        .collect();
    let stage_of = |key: &str| dag.stages.get(key).copied().unwrap_or_default();
    let build_of = |key: Option<&str>| {
        build_or_nil(
            key.and_then(|key| by_key.get(key))
                .and_then(|op| op.build_id),
        )
    };

    let mut nodes = Vec::with_capacity(operations.len());
    let mut occurrences = Vec::with_capacity(operations.len());
    for key in &dag.order {
        let Some(operation) = by_key.get(key.as_str()).copied() else {
            continue;
        };
        let stage = stage_of(key);
        let is_root = *key == dag.root_occurrence_key;
        let served: Vec<&OrderRequirement> = requirements
            .iter()
            .filter(|requirement| requirement.child_occurrence_key.as_deref() == Some(key))
            .collect();
        let required: u64 = served.iter().map(|r| r.required_quantity).sum();
        let reused: u64 = served.iter().map(|r| r.reused_quantity).sum();
        let demand: u64 = served.iter().map(|r| r.fresh_quantity).sum();
        let consumers: Vec<ExecutionConsumerRef> = served
            .iter()
            .filter_map(|requirement| {
                let consumer = requirement.operation_occurrence_key.clone()?;
                Some(ExecutionConsumerRef {
                    node_id: consumer.clone(),
                    occurrence_id: consumer.clone(),
                    quantity: requirement.fresh_quantity,
                    build_id: build_of(Some(consumer.as_str())),
                    dependency_id: dependency_key(requirement),
                    fulfillment_scope: requirement.fulfillment_scope,
                    required_quantity: requirement.required_quantity,
                    planned_inventory_quantity: requirement.reused_quantity,
                })
            })
            .collect();
        let own_requirements: Vec<ExecutionRequirement> = requirements
            .iter()
            .filter(|requirement| requirement.operation_occurrence_key.as_deref() == Some(key))
            .map(|requirement| ExecutionRequirement {
                type_id: requirement.type_id,
                type_name: requirement.captured_name.clone(),
                required_quantity: requirement.required_quantity,
                planned_inventory_quantity: requirement.reused_quantity,
                shortage_quantity: requirement.fresh_quantity,
                fulfillment_scope: requirement.fulfillment_scope,
                resolution: resolution(requirement.kind),
                dependency_id: dependency_key(requirement),
                producer_build_id: requirement.source_build_id,
                producer_node_id: requirement.child_occurrence_key.clone(),
            })
            .collect();
        let retained_surplus_quantity = operation.surplus_quantity.unwrap_or(0);
        let retained_surplus_cost = if operation.complete {
            operation.surplus_retained_basis
        } else {
            None
        };
        let unit_production_cost =
            per_unit(operation.total_production_cost, operation.produced_quantity);
        let build_id = build_or_nil(operation.build_id);

        nodes.push(ExecutionNode {
            id: key.clone(),
            output_type_id: operation.product_type_id,
            output_type_name: operation.product_name.clone(),
            activity: operation.activity,
            stage,
            occurrence_ids: vec![key.clone()],
            facility_id: None,
            facility_name: None,
            effective_me: operation.evidence.effective_me,
            effective_te: operation.evidence.effective_te,
            required_quantity: required,
            planned_inventory_quantity: reused,
            production_demand: demand,
            projected_output: operation.produced_quantity,
            projected_runs: operation.runs,
            retained_surplus_quantity,
            retained_surplus_cost,
            material_component_cost: operation.material_component_cost,
            own_installation_cost: operation.own_installation_cost,
            total_production_cost: operation.total_production_cost,
            cost_complete: operation.complete,
            consumers,
            production_methods: Vec::new(),
            unit_production_cost,
            available_quantity: inventory
                .free_by_type
                .get(&operation.product_type_id)
                .copied()
                .unwrap_or(0),
        });
        occurrences.push(ExecutionOccurrence {
            id: key.clone(),
            node_id: key.clone(),
            build_id,
            revision: 0,
            is_root,
            stage,
            activity: operation.activity,
            output_type_id: operation.product_type_id,
            output_type_name: operation.product_name.clone(),
            blueprint_or_formula_type_id: operation.blueprint_or_formula_type_id,
            blueprint_or_formula_name: blueprint_names
                .get(&operation.blueprint_or_formula_type_id)
                .cloned()
                .unwrap_or_default(),
            facility_id: None,
            facility_name: None,
            effective_me: operation.evidence.effective_me,
            effective_te: operation.evidence.effective_te,
            blueprint_selection: None,
            projected_runs: operation.runs,
            projected_output: operation.produced_quantity,
            required_quantity: required,
            planned_inventory_quantity: reused,
            production_demand: demand,
            retained_surplus_quantity,
            retained_surplus_cost,
            material_component_cost: operation.material_component_cost,
            own_installation_cost: operation.own_installation_cost,
            total_production_cost: operation.total_production_cost,
            unit_production_cost,
            cost_complete: operation.complete,
            requirements: own_requirements,
        });
    }

    let mut stage_nodes: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for node in &nodes {
        stage_nodes
            .entry(node.stage)
            .or_default()
            .push(node.id.clone());
    }
    let stages = stage_nodes
        .into_iter()
        .map(|(index, node_ids)| ExecutionStage { index, node_ids })
        .collect();

    let mut edges: Vec<crate::execution_plan::ExecutionEdge> = requirements
        .iter()
        .filter_map(|requirement| {
            Some(crate::execution_plan::ExecutionEdge {
                from: requirement.child_occurrence_key.clone()?,
                to: requirement.operation_occurrence_key.clone()?,
            })
        })
        .collect();
    edges.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
    edges.dedup();

    Ok(ExecutionPlanProjection {
        root_node_id: dag.root_occurrence_key.clone(),
        stages,
        nodes,
        edges,
        occurrences,
        acquisitions: frozen_acquisitions(requirements, &build_of, inventory),
        unresolved: Vec::new(),
        complete: operations.iter().all(|operation| operation.complete),
        warnings: Vec::new(),
        generated_at,
        logistics: crate::logistics::LogisticsPlan::default(),
    })
}

/// Every `Buy` requirement with fresh quantity to source, grouped by type.
fn frozen_acquisitions(
    requirements: &[OrderRequirement],
    build_of: &dyn Fn(Option<&str>) -> BuildId,
    inventory: &FrozenPlanInventory,
) -> Vec<AcquisitionLine> {
    let mut by_type: BTreeMap<i64, Vec<&OrderRequirement>> = BTreeMap::new();
    for requirement in requirements {
        if requirement.kind == RequirementKind::Buy && requirement.fresh_quantity > 0 {
            by_type
                .entry(requirement.type_id)
                .or_default()
                .push(requirement);
        }
    }
    by_type
        .into_iter()
        .map(|(type_id, rows)| {
            let mut consumers: Vec<AcquisitionConsumerRef> = rows
                .iter()
                .map(|requirement| {
                    let key = requirement
                        .operation_occurrence_key
                        .clone()
                        .unwrap_or_default();
                    let cost = fresh_cost(requirement);
                    AcquisitionConsumerRef {
                        node_id: key.clone(),
                        occurrence_id: key.clone(),
                        quantity: requirement.fresh_quantity,
                        build_id: build_of(Some(key.as_str())),
                        dependency_id: dependency_key(requirement),
                        fulfillment_scope: requirement.fulfillment_scope,
                        required_quantity: requirement.required_quantity,
                        planned_inventory_quantity: requirement.reused_quantity,
                        fresh_cost: cost,
                        fresh_unit_price: per_unit(cost, requirement.fresh_quantity),
                    }
                })
                .collect();
            consumers.sort_by(|a, b| {
                (a.node_id.as_str(), a.occurrence_id.as_str())
                    .cmp(&(b.node_id.as_str(), b.occurrence_id.as_str()))
            });
            let fresh_cost = consumers
                .iter()
                .map(|consumer| consumer.fresh_cost)
                .try_fold(Money::zero(), |total, cost| Some(Money(total.0 + cost?.0)));
            let mut unit_prices: Vec<Money> = consumers
                .iter()
                .filter_map(|consumer| consumer.fresh_unit_price)
                .collect();
            unit_prices.sort_by_key(|price| price.0);
            unit_prices.dedup();
            AcquisitionLine {
                type_id,
                type_name: rows[0].captured_name.clone(),
                required_quantity: rows.iter().map(|r| r.required_quantity).sum(),
                planned_inventory_quantity: rows.iter().map(|r| r.reused_quantity).sum(),
                shortage_quantity: rows.iter().map(|r| r.fresh_quantity).sum(),
                available_quantity: inventory.free_by_type.get(&type_id).copied().unwrap_or(0),
                reserved_quantity: inventory
                    .reserved_elsewhere_by_type
                    .get(&type_id)
                    .copied()
                    .unwrap_or(0),
                source_strategy: MaterialRowStrategy::Buy,
                consumers,
                production_methods: Vec::new(),
                fresh_cost,
                fresh_unit_price: (unit_prices.len() == 1).then(|| unit_prices[0]),
                fresh_price_stale: rows.iter().any(|requirement| {
                    requirement
                        .price_evidence
                        .as_ref()
                        .is_some_and(|evidence| evidence.fresh_price_stale)
                }),
            }
        })
        .collect()
}

/// What an Epic holds and has done against one line of its plan.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EpicStockProgress {
    /// Active reservations: held, not yet used.
    pub reserved: u64,
    /// Used by recordings.
    pub consumed: u64,
    /// `required - reserved - consumed`, floored at `0`.
    pub remaining_need: u64,
}

impl EpicStockProgress {
    fn add(&mut self, other: EpicStockProgress) {
        self.reserved += other.reserved;
        self.consumed += other.consumed;
        self.remaining_need += other.remaining_need;
    }
}

/// One frozen operation's ticket and the stock held for the requirements
/// it serves (its output reserved for its consumers).
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EpicNodeProgress {
    pub ticket_id: Option<TicketId>,
    pub ticket_display_id: Option<String>,
    pub ticket_status: Option<TicketStatus>,
    pub output: EpicStockProgress,
}

/// The Epic-only facts the Plan view overlays on
/// [`project_frozen_execution_plan`]'s nodes and acquisition lines.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EpicPlanOverlay {
    pub order_id: OrderId,
    pub display_name: String,
    pub source_build_revision: u64,
    /// By `ExecutionNode::id` (the operation's occurrence key).
    pub nodes: BTreeMap<String, EpicNodeProgress>,
    /// By acquisition `type_id`: Σ over the Epic's `Buy` requirements.
    pub acquisitions: BTreeMap<i64, EpicStockProgress>,
}

fn stock_progress(
    requirement: &OrderRequirement,
    totals: &std::collections::HashMap<OrderRequirementId, RequirementReservationTotals>,
) -> EpicStockProgress {
    let line = epic_coverage_line(
        requirement.id,
        requirement.required_quantity,
        totals.get(&requirement.id).copied(),
        0,
    );
    EpicStockProgress {
        reserved: line.reserved,
        consumed: line.consumed,
        remaining_need: line.remaining_need,
    }
}

#[must_use]
pub fn epic_plan_overlay(
    order: &Order,
    operations: &[PlanOperation],
    requirements: &[OrderRequirement],
    totals: &std::collections::HashMap<OrderRequirementId, RequirementReservationTotals>,
    tickets: &[Ticket],
) -> EpicPlanOverlay {
    let nodes = operations
        .iter()
        .map(|operation| {
            let key = operation.occurrence_key.as_str();
            let ticket = tickets.iter().find(|ticket| {
                ticket.occurrence_key.as_deref() == Some(key)
                    && ticket.status != TicketStatus::Canceled
            });
            let mut output = EpicStockProgress::default();
            for requirement in requirements
                .iter()
                .filter(|requirement| requirement.child_occurrence_key.as_deref() == Some(key))
            {
                output.add(stock_progress(requirement, totals));
            }
            (
                key.to_string(),
                EpicNodeProgress {
                    ticket_id: ticket.map(|ticket| ticket.id),
                    ticket_display_id: ticket.map(|ticket| ticket.display_id.clone()),
                    ticket_status: ticket.map(|ticket| ticket.status),
                    output,
                },
            )
        })
        .collect();
    let mut acquisitions: BTreeMap<i64, EpicStockProgress> = BTreeMap::new();
    for requirement in requirements
        .iter()
        .filter(|requirement| requirement.kind == RequirementKind::Buy)
    {
        acquisitions
            .entry(requirement.type_id)
            .or_default()
            .add(stock_progress(requirement, totals));
    }
    EpicPlanOverlay {
        order_id: order.id,
        display_name: order.display_name.clone(),
        source_build_revision: order.source_build_revision,
        nodes,
        acquisitions,
    }
}

#[cfg(test)]
mod tests;
