//! Pure reduction from the allocation-aware live planning
//! projection (`VerificationOperationInput` / `VerificationBoundaryInput` /
//! `BuildCostProjection` -- the same data `crate::build_graph::project_live_build_graph`
//! presents as Graph) into the frozen whole-tree Epic snapshot
//! (`NewPlanOperation` / `NewOrderRequirement`). Mirrors
//! `project_live_build_graph`'s join-map pattern exactly (`op_by_index`,
//! `op_cost_by_index`, `boundary_cost_by_traversal`,
//! `child_op_by_parent_and_type`), so the frozen tree matches the live Graph
//! node-for-node -- this is a second *presentation* of the same one walk,
//! never a second planning engine.
//!
//! Deliberately produces no ticket rows -- `NewPlanTicket` generation
//! (sizing, parent-ticket linkage, prerequisite duplication) is an
//! application-layer concern (`iskworks-app`), not a pure planning
//! transform, and needs display-only data (SDE names, etc.) this module
//! doesn't have.
//!
//! **Known data-model limitation** (reported, not fixed here): a
//! fully-covered `Build`/`Reaction` boundary (`child_op_index` unresolved,
//! see [`MaterialBoundaryResolution::Build`]/`Reaction`'s own doc) has no
//! walked child operation, and this module's only inputs are the *walked*
//! operations/boundaries -- so `NewOrderRequirement::source_build_id` is
//! `None` for that case even though the boundary's own sourcing intent
//! (`kind`) is `Build`/`React`. Recovering it would need a second, live
//! data source outside the one shared walk, which this module deliberately
//! avoids. A frozen `OrderRequirement` with `kind:
//! Build/React` and `source_build_id: None` reads as "sourced by
//! production, but Epic creation could not resolve which specific Build" --
//! correct, if less informative than the root-only freeze for this one
//! (fully-covered) case. (A canonical freeze recovers it from the
//! boundary's own `producer_build_id`, see below.)
//!
//! ## Canonical producers (planning snapshot version 3)
//!
//! A canonical root's walk plans **one** `VerificationOperationInput` per
//! producer, listing every demand edge it serves in `incoming`. The
//! freeze freezes that operation
//! exactly once and each served boundary as its own requirement, all
//! naming the same `child_occurrence_key`:
//!
//! * operation-level evidence (runs, output, cost, installation, ME/TE,
//!   warnings, and aggregate `consumed_quantity` / `surplus_quantity` /
//!   `surplus_retained_basis`) is frozen once on the operation;
//! * requirement-level evidence (consumer, scope, required, planned
//!   inventory use, shortage, `child_consumed_quantity`, the consumer's own
//!   `child_consumed_cost` share, `dependency_id`) is frozen per edge;
//! * `parent_occurrence_key` is set only when an operation has exactly one
//!   consuming operation -- never an arbitrary pick among several;
//! * the operation DAG is the requirement relation
//!   `child_occurrence_key -> operation_occurrence_key`, re-derivable at any
//!   time by [`derive_operation_dag`] (deterministic stages).
//!
//! The canonical freeze validates the graph first and refuses a corrupt one
//! with [`OrderError::CorruptProductionGraph`], and asserts exact money
//! conservation per producer (`total = sum(consumed shares) + retained
//! surplus basis`, [`OrderError::FrozenCostNotConserved`]). Every new Epic
//! freezes as version 3; older version-1/2 Epics are still read, never
//! written.

use std::collections::{BTreeMap, BTreeSet};

use rust_decimal::Decimal;

use crate::build_cost::{
    BoundaryCostProjection, BuildCostProjection, CostWarning, OperationCostProjection,
};
use crate::build_materials::{
    InventoryBasisEntry, MaterialBoundaryResolution, VerificationBoundaryInput,
    VerificationOperationInput,
};
use crate::{BuildId, Money, RecipeSelection};

use super::{
    NewOrderRequirement, NewPlanOperation, OrderError, OrderRequirementId,
    PlanInstallationEvidence, PlanOperationEvidence, PlanOperationId, PlanRequirementEvidence,
    RequirementKind,
};

/// The occurrence-key prefix of the one root operation of any whole-tree
/// freeze.
pub const ROOT_OCCURRENCE_PREFIX: &str = "root:";

/// [`freeze_order_plan`]'s output: every active operation (root + every
/// non-fully-covered Build/Reaction descendant), and every requirement at
/// any depth. Both lists are keyed for the caller by `occurrence_key` /
/// `operation_occurrence_key` respectively -- no separate index needed, the
/// caller already has `NewPlanOperation::occurrence_key` on every row.
/// `operations[0]` is always the root.
#[derive(Debug, Clone, PartialEq)]
pub struct FrozenPlan {
    pub operations: Vec<NewPlanOperation>,
    pub requirements: Vec<NewOrderRequirement>,
}

/// Reduce one allocation-aware walk's operations/boundaries/cost into a
/// [`FrozenPlan`]. `inventory_basis` is the same single `list_balances`
/// snapshot slice `BuildMaterialsSummary::inventory_basis` already carries
/// (captured by the same walk) -- used only for
/// `NewOrderRequirement::inventory_unit_basis` (a per-`type_id` weighted
/// average, identical across every boundary of that type, so a flat map
/// suffices).
///
/// # Errors
///
/// [`OrderError::CorruptProductionGraph`] for a structurally inconsistent
/// operation graph and [`OrderError::FrozenCostNotConserved`] if a
/// producer's cost shares do not conserve its total.
pub fn freeze_order_plan(
    operations: &[VerificationOperationInput],
    boundaries: &[VerificationBoundaryInput],
    cost: &BuildCostProjection,
    inventory_basis: &[InventoryBasisEntry],
) -> Result<FrozenPlan, OrderError> {
    let op_by_index: BTreeMap<u32, &VerificationOperationInput> =
        operations.iter().map(|op| (op.op_index, op)).collect();
    let op_cost_by_index: BTreeMap<u32, &OperationCostProjection> =
        cost.operations.iter().map(|op| (op.op_index, op)).collect();
    let boundary_cost_by_traversal: BTreeMap<u32, &BoundaryCostProjection> = cost
        .boundaries
        .iter()
        .map(|boundary| (boundary.traversal_index, boundary))
        .collect();
    let inventory_unit_basis: BTreeMap<i64, Option<Money>> = inventory_basis
        .iter()
        .map(|entry| (entry.type_id, entry.unit_basis.map(Money)))
        .collect();
    let joins = Joins {
        op_by_index: &op_by_index,
        op_cost_by_index: &op_cost_by_index,
        boundary_cost_by_traversal: &boundary_cost_by_traversal,
        inventory_unit_basis: &inventory_unit_basis,
    };

    freeze_canonical(operations, boundaries, &joins)
}

/// The read-only join maps both freeze paths share.
struct Joins<'a> {
    op_by_index: &'a BTreeMap<u32, &'a VerificationOperationInput>,
    op_cost_by_index: &'a BTreeMap<u32, &'a OperationCostProjection>,
    boundary_cost_by_traversal: &'a BTreeMap<u32, &'a BoundaryCostProjection>,
    inventory_unit_basis: &'a BTreeMap<i64, Option<Money>>,
}

/// Operation-level aggregate consumption/surplus, canonical only.
#[derive(Clone, Copy, Default)]
struct OperationSurplus {
    consumed_quantity: Option<u64>,
    surplus_quantity: Option<u64>,
    surplus_retained_basis: Option<Money>,
}

/// The canonical (version-3) reduction -- see this module's own doc.
fn freeze_canonical(
    operations: &[VerificationOperationInput],
    boundaries: &[VerificationBoundaryInput],
    joins: &Joins<'_>,
) -> Result<FrozenPlan, OrderError> {
    let graph = validate_canonical_graph(operations, boundaries)?;

    // Root first (callers rely on `operations[0]`), then the walk's own
    // deterministic op_index order.
    let mut ordered: Vec<&VerificationOperationInput> = operations.iter().collect();
    ordered.sort_by_key(|op| (op.op_index != graph.root_op_index, op.op_index));

    let mut new_operations = Vec::with_capacity(ordered.len());
    for op in ordered {
        let consumers = graph
            .consumer_ops_by_producer
            .get(&op.op_index)
            .cloned()
            .unwrap_or_default();
        // Exactly one consuming operation: a genuine parent. Fan-in: `None`
        // -- never an arbitrary consumer; the DAG lives on the requirements.
        let parent_occurrence_key = match consumers.len() {
            1 => consumers
                .first()
                .and_then(|consumer| joins.op_by_index.get(consumer))
                .map(|consumer| consumer.graph_node_id.clone()),
            _ => None,
        };
        let surplus = if op.op_index == graph.root_op_index {
            OperationSurplus::default()
        } else {
            operation_surplus(op, joins)?
        };
        new_operations.push(freeze_operation(op, joins, parent_occurrence_key, surplus));
    }

    let new_requirements: Vec<NewOrderRequirement> = boundaries
        .iter()
        .filter_map(|boundary| {
            let child_op_index = graph
                .child_op_by_traversal
                .get(&boundary.traversal_index)
                .copied();
            freeze_requirement(boundary, joins, child_op_index)
        })
        .collect();

    assert_cost_conservation(&new_operations, &new_requirements)?;
    // The frozen rows themselves must reproduce one acyclic, single-root
    // DAG -- validates exactly what a later reader will re-derive.
    derive_operation_dag(
        new_operations.iter().map(|op| op.occurrence_key.as_str()),
        new_requirements.iter().filter_map(|requirement| {
            Some(FrozenDemandEdge {
                consumer: requirement.operation_occurrence_key.as_deref()?,
                producer: requirement.child_occurrence_key.as_deref()?,
                dependency_id: requirement.dependency_id.as_deref(),
            })
        }),
    )?;

    Ok(FrozenPlan {
        operations: new_operations,
        requirements: new_requirements,
    })
}

/// The validated canonical operation graph.
struct CanonicalGraph {
    root_op_index: u32,
    /// Consuming boundary `traversal_index` -> producer `op_index`.
    child_op_by_traversal: BTreeMap<u32, u32>,
    /// Producer `op_index` -> every distinct consuming `op_index`.
    consumer_ops_by_producer: BTreeMap<u32, BTreeSet<u32>>,
}

fn corrupt(detail: impl Into<String>) -> OrderError {
    OrderError::CorruptProductionGraph {
        detail: detail.into(),
    }
}

fn validate_canonical_graph(
    operations: &[VerificationOperationInput],
    boundaries: &[VerificationBoundaryInput],
) -> Result<CanonicalGraph, OrderError> {
    let mut op_indices = BTreeSet::new();
    let mut occurrence_keys = BTreeSet::new();
    for op in operations {
        if !op_indices.insert(op.op_index) {
            return Err(corrupt(format!(
                "duplicate operation index {}",
                op.op_index
            )));
        }
        if !occurrence_keys.insert(op.graph_node_id.as_str()) {
            return Err(corrupt(format!(
                "operation {} is planned more than once",
                op.graph_node_id
            )));
        }
    }
    let mut boundary_by_traversal: BTreeMap<u32, &VerificationBoundaryInput> = BTreeMap::new();
    for boundary in boundaries {
        if boundary_by_traversal
            .insert(boundary.traversal_index, boundary)
            .is_some()
        {
            return Err(corrupt(format!(
                "duplicate requirement traversal index {}",
                boundary.traversal_index
            )));
        }
        if !op_indices.contains(&boundary.op_index) {
            return Err(corrupt(format!(
                "requirement {} belongs to unknown operation {}",
                boundary.traversal_index, boundary.op_index
            )));
        }
    }

    let mut child_op_by_traversal: BTreeMap<u32, u32> = BTreeMap::new();
    let mut consumer_ops_by_producer: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    let mut roots = Vec::new();
    for op in operations {
        if op.incoming.is_empty() {
            roots.push(op);
            continue;
        }
        for demand in &op.incoming {
            if demand.consumer_op_index == op.op_index {
                return Err(corrupt(format!("{} consumes itself", op.graph_node_id)));
            }
            if !op_indices.contains(&demand.consumer_op_index) {
                return Err(corrupt(format!(
                    "{} serves unknown consuming operation {}",
                    op.graph_node_id, demand.consumer_op_index
                )));
            }
            let boundary = boundary_by_traversal
                .get(&demand.traversal_index)
                .ok_or_else(|| {
                    corrupt(format!(
                        "{} serves unknown requirement {}",
                        op.graph_node_id, demand.traversal_index
                    ))
                })?;
            if boundary.op_index != demand.consumer_op_index
                || boundary.type_id != op.product_type_id
            {
                return Err(corrupt(format!(
                    "{} demand edge {} does not match its requirement",
                    op.graph_node_id, demand.dependency_id
                )));
            }
            if let Some(other) = child_op_by_traversal.insert(demand.traversal_index, op.op_index) {
                return Err(corrupt(format!(
                    "requirement {} is served by two producers ({other} and {})",
                    demand.traversal_index, op.op_index
                )));
            }
            consumer_ops_by_producer
                .entry(op.op_index)
                .or_default()
                .insert(demand.consumer_op_index);
        }
    }
    let root = match roots.as_slice() {
        [root] if root.graph_node_id.starts_with(ROOT_OCCURRENCE_PREFIX) => *root,
        [] => return Err(corrupt("the plan has no root operation")),
        [other] => {
            return Err(corrupt(format!(
                "{} consumes nothing but is not the root",
                other.graph_node_id
            )))
        }
        _ => {
            return Err(corrupt(format!(
                "the plan has {} operations that serve no demand",
                roots.len()
            )))
        }
    };

    // Every production-resolved requirement with a shortage must be served
    // by exactly one walked producer -- otherwise its demand vanished.
    for boundary in boundaries {
        let produced = matches!(
            boundary.resolution,
            MaterialBoundaryResolution::Build | MaterialBoundaryResolution::Reaction
        );
        if produced
            && boundary.api_shortage > 0
            && !child_op_by_traversal.contains_key(&boundary.traversal_index)
        {
            return Err(corrupt(format!(
                "requirement {} ({}) has an unserved production shortage",
                boundary.traversal_index, boundary.type_name
            )));
        }
    }

    Ok(CanonicalGraph {
        root_op_index: root.op_index,
        child_op_by_traversal,
        consumer_ops_by_producer,
    })
}

/// Aggregate consumption across every demand edge `op` serves, its one
/// surplus, and that surplus's retained basis (the one non-zero owner
/// share the cost projection assigns, summed so no edge is privileged).
fn operation_surplus(
    op: &VerificationOperationInput,
    joins: &Joins<'_>,
) -> Result<OperationSurplus, OrderError> {
    let produced = op.node_runs.saturating_mul(op.output_per_run);
    let mut consumed: u64 = 0;
    let mut retained = Some(Decimal::ZERO);
    for demand in &op.incoming {
        let bcost = joins
            .boundary_cost_by_traversal
            .get(&demand.traversal_index)
            .ok_or_else(|| {
                corrupt(format!(
                    "requirement {} has no cost projection",
                    demand.traversal_index
                ))
            })?;
        consumed = consumed
            .checked_add(bcost.child_consumed_quantity)
            .ok_or_else(|| corrupt(format!("{} consumption overflows", op.graph_node_id)))?;
        retained = match (retained, bcost.child_surplus_retained_basis) {
            (Some(sum), Some(share)) => sum.checked_add(share.0),
            _ => None,
        };
    }
    if consumed > produced {
        return Err(corrupt(format!(
            "{} produces {produced} but its consumers need {consumed}",
            op.graph_node_id
        )));
    }
    Ok(OperationSurplus {
        consumed_quantity: Some(consumed),
        surplus_quantity: Some(produced - consumed),
        surplus_retained_basis: retained.map(Money),
    })
}

/// `total_production_cost == sum(child_consumed_cost) + surplus_retained_basis`
/// for every frozen producer whose cost is fully known -- exactly, at
/// Money scale. An incomplete cost (any `None`) is tolerated, never
/// zero-substituted.
fn assert_cost_conservation(
    operations: &[NewPlanOperation],
    requirements: &[NewOrderRequirement],
) -> Result<(), OrderError> {
    for op in operations {
        let (Some(total), Some(retained)) = (op.total_production_cost, op.surplus_retained_basis)
        else {
            continue;
        };
        let shares: Option<Decimal> = requirements
            .iter()
            .filter(|requirement| {
                requirement.child_occurrence_key.as_deref() == Some(op.occurrence_key.as_str())
            })
            .try_fold(Decimal::ZERO, |sum, requirement| {
                sum.checked_add(requirement.child_consumed_cost?.0)
            });
        let Some(shares) = shares else {
            continue;
        };
        if shares.checked_add(retained.0) != Some(total.0) {
            return Err(OrderError::FrozenCostNotConserved {
                occurrence_key: op.occurrence_key.clone(),
            });
        }
    }
    Ok(())
}

fn freeze_operation(
    op: &VerificationOperationInput,
    joins: &Joins<'_>,
    parent_occurrence_key: Option<String>,
    surplus: OperationSurplus,
) -> NewPlanOperation {
    let opcost = joins.op_cost_by_index.get(&op.op_index).copied();

    let installation = op.facility_id.and(opcost).map(|c| {
        let installation_cost = &c.own_installation;
        PlanInstallationEvidence {
            estimated_item_value: installation_cost.eiv,
            eiv_missing_type_ids: installation_cost.eiv_missing_type_ids.clone(),
            system_cost_index: installation_cost.system_cost_index.map(|v| v.to_string()),
            job_cost_reduction_percent: installation_cost.job_cost_reduction_percent.to_string(),
            facility_tax_percent: installation_cost.facility_tax_percent.to_string(),
            scc_surcharge_percent: installation_cost.scc_surcharge_percent.to_string(),
            alliance_surcharge_percent: installation_cost.alliance_surcharge_percent.to_string(),
            fixed_supplemental_cost: installation_cost.fixed_supplemental_cost,
            unmodified_system_index_cost: installation_cost.unmodified_system_index_cost,
            system_index_cost: installation_cost.system_index_cost,
            facility_tax: installation_cost.facility_tax,
            scc_surcharge: installation_cost.scc_surcharge,
            alliance_surcharge: installation_cost.alliance_surcharge,
            complete: installation_cost.complete,
            formula_version: installation_cost.formula_version.clone(),
            facility_profile_id: installation_cost.facility_profile_id,
            facility_profile_revision: installation_cost.facility_profile_revision,
        }
    });

    let warnings = opcost
        .map(|c| c.warnings.iter().map(describe_cost_warning).collect())
        .unwrap_or_default();

    let (
        produced_quantity,
        material_component_cost,
        own_installation_cost,
        total_production_cost,
        complete,
    ) = match opcost {
        Some(c) => (
            c.produced_quantity,
            c.material_component_cost,
            c.own_installation.total,
            c.total_production_cost,
            c.complete,
        ),
        None => (
            op.node_runs.saturating_mul(op.output_per_run),
            None,
            None,
            None,
            false,
        ),
    };

    NewPlanOperation {
        id: PlanOperationId::new(),
        occurrence_key: op.graph_node_id.clone(),
        parent_occurrence_key,
        build_id: op.build_id,
        activity: op.activity,
        runs: op.node_runs,
        persisted_runs: op.persisted_runs,
        product_type_id: op.product_type_id,
        product_name: op.product_name.clone(),
        output_per_run: op.output_per_run,
        produced_quantity,
        blueprint_or_formula_type_id: op.blueprint_or_formula_type_id,
        material_component_cost,
        own_installation_cost,
        total_production_cost,
        complete,
        consumed_quantity: surplus.consumed_quantity,
        surplus_quantity: surplus.surplus_quantity,
        surplus_retained_basis: surplus.surplus_retained_basis,
        evidence: PlanOperationEvidence {
            effective_me: op.me,
            effective_te: op.te,
            job_count: op.job_count,
            recipe_currency: op.recipe_currency,
            installation,
            warnings,
        },
    }
}

/// One boundary -> one frozen requirement. `canonical` additionally
/// freezes the consumer's own `child_consumed_cost` share and the demand
/// edge's `dependency_id`, and recovers a fully-covered row's producer
/// identity from the boundary's own `producer_build_id`.
fn freeze_requirement(
    boundary: &VerificationBoundaryInput,
    joins: &Joins<'_>,
    child_op_index: Option<u32>,
) -> Option<NewOrderRequirement> {
    let operation_occurrence_key = joins
        .op_by_index
        .get(&boundary.op_index)?
        .graph_node_id
        .clone();
    let bcost = joins
        .boundary_cost_by_traversal
        .get(&boundary.traversal_index)
        .copied();

    let child_op = child_op_index
        .and_then(|index| joins.op_by_index.get(&index))
        .copied();
    let child_occurrence_key = child_op.map(|op| op.graph_node_id.clone());

    let (kind, mut source_build_id) = requirement_kind(boundary, child_op);
    if source_build_id.is_none() && kind != RequirementKind::Buy {
        source_build_id = boundary.producer_build_id;
    }

    let (
        child_produced_quantity,
        child_consumed_quantity,
        child_surplus_quantity,
        child_surplus_retained_basis,
        child_consumed_cost,
    ) = match (bcost, &child_occurrence_key) {
        (Some(c), Some(_)) => (
            Some(c.child_produced_quantity),
            Some(c.child_consumed_quantity),
            Some(c.child_surplus_quantity),
            c.child_surplus_retained_basis,
            c.child_consumed_cost,
        ),
        _ => (None, None, None, None, None),
    };

    let required_quantity = bcost.map_or(boundary.api_required, |c| c.required_quantity);
    let reused_quantity = bcost.map_or(boundary.api_planned_use, |c| c.inventory_quantity);
    let estimated_line_total = bcost.and_then(|c| c.requirement_cost);
    let estimated_unit_cost = estimated_line_total.map(|total| {
        if required_quantity > 0 {
            Money(total.0 / Decimal::from(required_quantity))
        } else {
            total
        }
    });

    let price_evidence = matches!(
        boundary.resolution,
        MaterialBoundaryResolution::Buy | MaterialBoundaryResolution::Unresolved
    )
    .then(|| PlanRequirementEvidence {
        fresh_price_selection: boundary.fresh_price_selection,
        fresh_price_note: boundary.fresh_price_note.clone(),
        fresh_price_stale: boundary.fresh_price_stale,
        market_region_id: boundary.market_region_id,
        market_location_id: boundary.market_location_id,
    });

    Some(NewOrderRequirement {
        id: OrderRequirementId::new(),
        type_id: boundary.type_id,
        captured_name: boundary.type_name.clone(),
        kind,
        source_build_id,
        required_quantity,
        fulfillment_scope: boundary.scope,
        reused_quantity,
        estimated_unit_cost,
        estimated_line_total,
        // `BoundaryCostProjection::inventory_cost` is `Some(Money::zero())`
        // (never `None`) when nothing was reused -- the cost layer's own
        // "zero reused, zero cost" convention. `OrderRequirement::reused_line_total`
        // predates that layer and means "no reused portion at all" as
        // `None` (see its own doc); reconcile here rather than leaking
        // a spurious `Some(0.0000)` into a `Full`-scoped or zero-reuse row.
        reused_line_total: (reused_quantity > 0)
            .then(|| bcost.and_then(|c| c.inventory_cost))
            .flatten(),
        operation_occurrence_key: Some(operation_occurrence_key),
        child_occurrence_key,
        // Matches `reused_line_total`: `None` unless this boundary
        // actually drew on inventory -- the doc's own "basis
        // actually used for `reused_quantity`", not merely "the
        // type's basis happens to be known."
        inventory_unit_basis: (reused_quantity > 0)
            .then(|| {
                joins
                    .inventory_unit_basis
                    .get(&boundary.type_id)
                    .copied()
                    .flatten()
            })
            .flatten(),
        child_produced_quantity,
        child_consumed_quantity,
        child_surplus_quantity,
        child_surplus_retained_basis,
        child_consumed_cost,
        dependency_id: (!boundary.dependency_id.is_empty()).then(|| boundary.dependency_id.clone()),
        price_evidence,
    })
}

/// `kind` / `source_build_id` for one boundary -- see this module's own doc
/// for the fully-covered-child data-model limitation.
fn requirement_kind(
    boundary: &VerificationBoundaryInput,
    child_op: Option<&VerificationOperationInput>,
) -> (RequirementKind, Option<BuildId>) {
    match boundary.resolution {
        MaterialBoundaryResolution::Buy => (RequirementKind::Buy, None),
        MaterialBoundaryResolution::Build => {
            (RequirementKind::Build, child_op.map(|op| op.build_id))
        }
        MaterialBoundaryResolution::Reaction => {
            (RequirementKind::React, child_op.map(|op| op.build_id))
        }
        MaterialBoundaryResolution::Unresolved => {
            let kind = match boundary.intended_recipe {
                Some(RecipeSelection::Reaction { .. }) => RequirementKind::React,
                _ => RequirementKind::Build,
            };
            (kind, None)
        }
    }
}

/// One frozen demand edge: requirement row `consumer` (its
/// `operation_occurrence_key`) is served by operation `producer` (its
/// `child_occurrence_key`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrozenDemandEdge<'a> {
    pub consumer: &'a str,
    pub producer: &'a str,
    pub dependency_id: Option<&'a str>,
}

/// One producer -> consumer operation dependency, with every demand edge
/// (requirement) it carries.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationDependency {
    pub producer_occurrence_key: String,
    pub consumer_occurrence_key: String,
    pub dependency_ids: Vec<String>,
}

/// A frozen plan's operation DAG, re-derived from its requirement rows.
/// Deterministic: `stages[key]` is the longest producer chain beneath an
/// operation (`0` = consumes no produced operation), and `order` lists
/// every operation by `(stage, occurrence_key)` -- the reproducible build
/// order. Readiness is deliberately not modelled here.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationDag {
    pub root_occurrence_key: String,
    pub stages: BTreeMap<String, u32>,
    pub order: Vec<String>,
    pub dependencies: Vec<OperationDependency>,
}

/// Derive the operation DAG of a frozen whole-tree plan (version 2 or 3)
/// from its requirement relation.
///
/// # Errors
///
/// [`OrderError::CorruptProductionGraph`] if an edge names an unknown
/// operation, an operation serves itself, the relation has a cycle, or it
/// has anything but exactly one root (an operation nothing consumes).
pub fn derive_operation_dag<'a>(
    operation_keys: impl IntoIterator<Item = &'a str>,
    demands: impl IntoIterator<Item = FrozenDemandEdge<'a>>,
) -> Result<OperationDag, OrderError> {
    let keys: BTreeSet<&str> = operation_keys.into_iter().collect();
    let mut dependency_ids: BTreeMap<(&str, &str), Vec<String>> = BTreeMap::new();
    for demand in demands {
        for key in [demand.consumer, demand.producer] {
            if !keys.contains(key) {
                return Err(corrupt(format!(
                    "demand edge names unknown operation {key}"
                )));
            }
        }
        if demand.consumer == demand.producer {
            return Err(corrupt(format!("{} consumes itself", demand.producer)));
        }
        let ids = dependency_ids
            .entry((demand.producer, demand.consumer))
            .or_default();
        if let Some(id) = demand.dependency_id {
            ids.push(id.to_string());
        }
    }

    let mut consumers_of: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for &(producer, consumer) in dependency_ids.keys() {
        consumers_of.entry(producer).or_default().insert(consumer);
    }

    let roots: Vec<&str> = keys
        .iter()
        .copied()
        .filter(|key| !consumers_of.contains_key(key))
        .collect();
    let root = match roots.as_slice() {
        [root] => (*root).to_string(),
        [] => return Err(corrupt("the operation graph has no root (it is cyclic)")),
        _ => {
            return Err(corrupt(format!(
                "the operation graph has {} roots",
                roots.len()
            )))
        }
    };

    // The one authoritative stage algorithm (`crate::production_stage`).
    let stages: BTreeMap<String, u32> = crate::production_stage::production_stages(
        keys.iter().copied(),
        dependency_ids.keys().copied(),
    )
    .map_err(|_| corrupt("the operation graph has a cycle"))?
    .into_iter()
    .map(|(key, stage)| (key.to_string(), stage))
    .collect();

    let mut order: Vec<String> = stages.keys().cloned().collect();
    order.sort_by(|a, b| stages[a].cmp(&stages[b]).then_with(|| a.cmp(b)));
    let dependencies = dependency_ids
        .into_iter()
        .map(|((producer, consumer), mut ids)| {
            ids.sort();
            OperationDependency {
                producer_occurrence_key: producer.to_string(),
                consumer_occurrence_key: consumer.to_string(),
                dependency_ids: ids,
            }
        })
        .collect();

    Ok(OperationDag {
        root_occurrence_key: root,
        stages,
        order,
        dependencies,
    })
}

/// A human-readable rendering of one `CostWarning`, for
/// `PlanOperationEvidence::warnings` -- audit-only text, never
/// re-interpreted, same convention as the live Worksheet/Graph's own
/// warning presentation.
fn describe_cost_warning(warning: &CostWarning) -> String {
    match warning {
        CostWarning::MissingFreshPrice { type_id, .. } => {
            format!("missing fresh price for type {type_id}")
        }
        CostWarning::MissingInventoryBasis { type_id, .. } => {
            format!("missing inventory basis for type {type_id}")
        }
        CostWarning::MissingAdjustedPrice { type_ids, .. } => {
            format!("missing adjusted price for {} material(s)", type_ids.len())
        }
        CostWarning::MissingSystemCostIndex { .. } => "missing system cost index".to_string(),
        CostWarning::NoFacilitySelected { .. } => "no facility selected".to_string(),
        CostWarning::UnresolvedBuild { type_id, .. } => {
            format!("unresolved build for type {type_id}")
        }
        CostWarning::ChildCostIncomplete { child_op_index, .. } => {
            format!("child operation {child_op_index} has an incomplete cost")
        }
        CostWarning::StaleFreshPrice { type_id, .. } => {
            format!("stale fresh price for type {type_id}")
        }
        CostWarning::ArithmeticOverflow { op_index } => {
            format!("arithmetic overflow costing operation {op_index}")
        }
    }
}

#[cfg(test)]
mod tests;
