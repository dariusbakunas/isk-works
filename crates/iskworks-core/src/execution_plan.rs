//! A pure, occurrence-preserving projection of the
//! existing allocation-aware Build planning evidence into a staged production
//! dependency graph.
//!
//! Architectural precedent: [`crate::build_graph::project_live_build_graph`]
//! and `crate::order::freeze::freeze_order_plan` are both pure, sync,
//! I/O-free consumers of exactly the same three inputs this module reads --
//! [`VerificationOperationInput`], [`VerificationBoundaryInput`], and
//! [`BuildCostProjection`] (the one allocation-aware walk
//! `IndustryService::project_build_materials` already ran, cost-enriched by
//! `crate::build_cost::project_build_cost`). This module is a **third** such
//! consumer. It performs **no** tree walk, **no** SDE/inventory/market I/O,
//! **no** inventory allocation, **no** projected-run recalculation, and
//! **no** Build mutation -- see the module-level invariants below.
//!
//! ## The central structural fact
//!
//! The underlying planning topology is the plan's producer DAG: each
//! [`VerificationOperationInput`] is one producer, and its `incoming` lists
//! every demand edge it serves (one producer, many consumers); an operation
//! with no `incoming` is linked to its single consumer by `parent_op_index`.
//! The "Execution DAG" this module derives is not a change to that
//! topology; it is the same edges read in the opposite direction
//! (prerequisite producer -> consuming operation), with a **display-only**
//! node layer ([`ExecutionNode`]) over the occurrences -- one node per
//! operation.
//!
//! ## Non-goals (never do these here)
//!
//! * Walk recipes, query the SDE, query inventory, or resolve market prices.
//! * Allocate inventory, or re-run [`crate::build_materials::PlanningInventory`].
//! * Recompute projected runs for a group (`ceil(Σ demand / output_per_run)`
//!   is **not** the same as `Σ ceil(demand_i / output_per_run)` -- see
//!   `tests::x_nonlinear_run_aggregation_counterexample`). A grouped node's
//!   `projected_runs`/`projected_output` are **sums of already-computed
//!   authoritative per-occurrence values**, display evidence only, never fed
//!   back into any calculation.
//! * Reroute surplus between occurrences, or merge/optimize production jobs.
//! * Mutate a `Build`, or claim readiness/workflow state (`Ready`, `Blocked`,
//!   `Complete`, ...) -- this module answers dependency *order* only.
//!
//! Every aggregate field this module produces is a sum or reorganization of
//! fields already computed by [`crate::build_cost::project_build_cost`] and
//! the allocation-aware walk that produced its inputs.
//!
//! ## Warnings
//!
//! This module deliberately does not introduce a second warning taxonomy.
//! [`crate::build_graph::GraphWarning`] is presentation-shaped for one
//! specific tree-rendering consumer (Graph); pulling it into this pure core
//! module would create a dependency on a module documented as a presentation
//! layer for no real gain, since every `GraphWarningCode` this module could
//! need is already a straightforward translation of a [`CostWarning`] the
//! [`BuildCostProjection`] input already carries. [`ExecutionPlanProjection::warnings`]
//! therefore reuses [`CostWarning`] verbatim -- the narrowest already-shared
//! abstraction, with no new dependency edge.
//!
//! ## Acquisition scope decision
//!
//! To avoid maintaining a second, independently-derived aggregation next to
//! [`crate::build_materials::AggregateMaterialLine`] (the Materials tab's
//! whole-tree per-`type_id` rollup, which also folds same-type
//! `Build`/`Reaction`-resolved contributions into the same row and tracks a
//! `Mixed` strategy), this module reuses what the coordinator already has:
//! [`NodeMaterialAllocation`] (`BuildMaterialsSummary::node_allocations`) and
//! [`AggregateMaterialLine`] (`BuildMaterialsSummary::rows`) from the *same*
//! planning call, computed by the authoritative `MaterialsAccumulator` --
//! zero extra computation. This module takes both as inputs and:
//!
//! * derives `acquisitions` from `node_allocations` (per-node, per-type,
//!   each row already carrying its own `resolution`), filtered to
//!   `resolution == Buy` -- **never** from the whole-tree `AggregateMaterialLine`
//!   rollup directly, because a `Mixed`-strategy row's `shortage_quantity`
//!   sums Buy shortage *and* Build/Reaction production shortage together;
//!   using it unfiltered would misclassify a production shortfall as
//!   something to purchase (see `tests::acquisitions_never_conflate_mixed_source_shortage_with_buy_shortage`).
//! * looks up each acquisition's `available_quantity` from
//!   `material_lines` by `type_id` -- that figure (the original
//!   `PlanningInventory` seed) is identical regardless of source strategy,
//!   so reading it from the whole-tree rollup is safe and avoids yet
//!   another aggregation of the same fact.
//! * exposes `source_strategy` (from the same `material_lines` row) purely
//!   as informational context -- `Mixed` on an acquisition line means "this
//!   type is also produced elsewhere in the tree," never a claim about the
//!   acquisition amount itself, which is always Buy-only.
//!
//! `node_allocations`/`material_lines` are **not** recomputed, walked, or
//! re-derived here -- passed straight through from the same evidence the
//! coordinator already holds.
//!
//! ## Acquisition consumer provenance (Stages polish pass)
//!
//! Each `AcquisitionLine` also carries `consumers: Vec<AcquisitionConsumerRef>`
//! -- "which production occurrence(s) need this purchase, and how much."
//! Every `Buy`-resolution `NodeMaterialAllocation` already identifies its
//! own owning node (`graph_node_id`) and its own `shortage_quantity`; this
//! is a straight re-key of that same evidence (`graph_node_id` -> the
//! occurrence's `ExecutionNode.id`, via the same `occ_by_op` map every
//! other consumer edge already uses), never a second planning walk, a
//! recipe re-expansion, or a re-allocation. The quantity shown is each
//! occurrence's own `shortage_quantity` (its external shortfall for that
//! type) -- not the gross requirement -- because that is the one figure
//! that provably reconciles: `Σ consumers[].quantity ==
//! AcquisitionLine::shortage_quantity` by construction, since both are
//! folded from the exact same per-occurrence contributions. No proportional
//! split is invented anywhere in this module.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

use crate::build_cost::{
    BoundaryCostProjection, BuildCostProjection, CostWarning, OperationCostProjection,
};
use crate::build_materials::{
    AggregateMaterialLine, MaterialActivity, MaterialBoundaryResolution, MaterialRowStrategy,
    NodeMaterialAllocation, VerificationBoundaryInput, VerificationOperationInput,
};
use crate::industry::{BuildId, Money, RecipeSelection};

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

/// The full result of [`project_execution_plan`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPlanProjection {
    /// The [`ExecutionNode::id`] of the group containing the root operation
    /// (the occurrence with no parent). Empty only for a degenerate empty
    /// input.
    pub root_node_id: String,
    /// Topological production stages, ascending, **sparse** -- a stage index
    /// with no node at it is omitted rather than emitted empty (a node's
    /// only prerequisite can skip several stage numbers).
    pub stages: Vec<ExecutionStage>,
    /// Every display node, deterministically ordered (see `sort_nodes`).
    pub nodes: Vec<ExecutionNode>,
    /// Deduplicated prerequisite -> consumer edges between display nodes.
    pub edges: Vec<ExecutionEdge>,
    /// Every authoritative production occurrence, one per
    /// [`VerificationOperationInput`], ordered ascending by the walk's own
    /// `op_index`. Grouping never replaces this -- it is the drill-down
    /// evidence behind every [`ExecutionNode::occurrence_ids`] entry.
    pub occurrences: Vec<ExecutionOccurrence>,
    /// External (`Buy`-resolution) shortage rollup -- see the module doc's
    /// acquisition scope decision. Only `shortage_quantity > 0` rows appear.
    pub acquisitions: Vec<AcquisitionLine>,
    /// `Unresolved` boundaries (Build/Reaction chosen, no linked Build yet)
    /// -- truthful placeholders, never fabricated descendants.
    pub unresolved: Vec<ExecutionUnresolvedPrerequisite>,
    /// Mirrors [`BuildCostProjection::complete`] verbatim -- never
    /// recomputed.
    pub complete: bool,
    /// Mirrors [`BuildCostProjection::warnings`] verbatim -- see the module
    /// doc's warning-strategy note.
    pub warnings: Vec<CostWarning>,
    pub generated_at: DateTime<Utc>,
    /// The facility-aware Logistics plan
    /// ([`crate::logistics::project_logistics`]) over the same walk's
    /// allocations. Empty from this pure projection; the application layer
    /// fills it (it needs the SDE's packaged volumes).
    pub logistics: crate::logistics::LogisticsPlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionStage {
    pub index: u32,
    /// [`ExecutionNode::id`]s at this stage, in the same deterministic order
    /// as [`ExecutionPlanProjection::nodes`]. No ordering within a stage
    /// implies execution priority -- same-stage nodes are topologically
    /// parallel by definition.
    pub node_ids: Vec<String>,
}

/// One display node over one or more production occurrences (the projection
/// emits one node per operation). Every quantity/cost field is a straight sum of
/// the authoritative values already computed for its `occurrence_ids` --
/// grouping never re-derives, re-rounds, or reroutes anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionNode {
    /// Deterministic within one render: the [`ExecutionOccurrence::id`]
    /// (`graph_node_id`) of the group's smallest-`op_index` member. Never
    /// persisted identity -- see the module doc.
    pub id: String,
    pub output_type_id: i64,
    pub output_type_name: String,
    pub activity: MaterialActivity,
    /// `max` over member occurrence stages -- display placement only, never
    /// fed back into any member's own stage.
    pub stage: u32,
    /// Every member's [`ExecutionOccurrence::id`], ascending by `op_index`.
    pub occurrence_ids: Vec<String>,
    pub facility_id: Option<Uuid>,
    pub facility_name: Option<String>,
    pub effective_me: Option<u8>,
    pub effective_te: Option<u8>,
    /// Σ member `required_quantity`.
    pub required_quantity: u64,
    /// Σ member `planned_inventory_quantity`.
    pub planned_inventory_quantity: u64,
    /// Σ member `production_demand`.
    pub production_demand: u64,
    /// Σ member `projected_output` -- display evidence only.
    pub projected_output: u64,
    /// Σ member `projected_runs` -- display evidence only, **never**
    /// `ceil(Σ demand / output_per_run)`. See the module doc.
    pub projected_runs: u64,
    /// Σ member `retained_surplus_quantity`.
    pub retained_surplus_quantity: u64,
    /// Σ member `retained_surplus_cost`, `None` whenever `cost_complete` is
    /// `false` (never a partially-summed figure with a silent gap).
    pub retained_surplus_cost: Option<Money>,
    pub material_component_cost: Option<Money>,
    pub own_installation_cost: Option<Money>,
    pub total_production_cost: Option<Money>,
    /// `true` iff every member occurrence's own cost evidence is complete.
    /// `false` here forces every `Option<Money>` field above to `None`.
    pub cost_complete: bool,
    /// Every occurrence-edge into this node from a consuming occurrence,
    /// **not** collapsed by quantity -- two distinct consumers (or the same
    /// consumer group reached via two distinct consuming occurrences) each
    /// keep their own entry so quantities stay individually reconcilable.
    pub consumers: Vec<ExecutionConsumerRef>,
    /// Every published production recipe for
    /// `output_type_id` (manufacturing and/or reaction) -- the methods a
    /// consumer may switch this component to. Empty from this pure
    /// projection; the application layer fills it from the SDE in one bulk
    /// read.
    pub production_methods: Vec<RecipeSelection>,
    /// The operation's unit production cost
    /// (single-occurrence rows only; `None` for a grouped row, whose
    /// members may differ) -- display evidence only.
    pub unit_production_cost: Option<Money>,
    /// The output type's whole-tree starting
    /// inventory (`AggregateMaterialLine::available_quantity`), `0` when the
    /// type is not a material of the plan (e.g. the root product).
    pub available_quantity: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionConsumerRef {
    /// The consuming occurrence's own [`ExecutionNode::id`].
    pub node_id: String,
    /// The specific consuming occurrence's id (never lost to the node-level
    /// grouping, even when several occurrences share `node_id`).
    pub occurrence_id: String,
    /// The quantity that specific consuming occurrence drew from this node's
    /// specific producing occurrence.
    pub quantity: u64,
    /// The demand edge itself -- the consuming
    /// Build (whose sourcing a Plan change targets) and that edge's own
    /// requirement evidence. `quantity` above is its production demand.
    pub build_id: BuildId,
    pub dependency_id: String,
    pub fulfillment_scope: crate::FulfillmentScope,
    pub required_quantity: u64,
    pub planned_inventory_quantity: u64,
}

/// A deduplicated prerequisite -> consumer edge between two display nodes.
/// Per-occurrence quantities are never lost -- see each node's own
/// `consumers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionEdge {
    pub from: String,
    pub to: String,
}

/// One authoritative production occurrence -- a 1:1 mirror of one
/// [`VerificationOperationInput`], enriched with its own stage and the
/// quantity/cost evidence from the single boundary (in its parent) that
/// spawned it. Never replaced or altered by grouping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionOccurrence {
    /// The durable per-projection identity -- `graph_node_id`
    /// (`root:<uuid>` / `build:<uuid>`). Never the ephemeral `op_index`.
    pub id: String,
    /// The [`ExecutionNode::id`] this occurrence currently belongs to.
    pub node_id: String,
    pub build_id: BuildId,
    /// `build_id`'s own **persisted** `Build.revision` at read time --
    /// mirrors [`crate::build_materials::VerificationOperationInput::revision`]
    /// verbatim. The Stages descendant-configuration mutation's own
    /// optimistic-concurrency token: a client edits one production
    /// operation by echoing back every member occurrence's own `buildId` +
    /// `revision` pair, never inventing or caching one separately.
    pub revision: u64,
    pub is_root: bool,
    pub stage: u32,
    pub activity: MaterialActivity,
    pub output_type_id: i64,
    pub output_type_name: String,
    pub blueprint_or_formula_type_id: i64,
    /// Mirrors `VerificationOperationInput::blueprint_or_formula_name`
    /// verbatim -- the Stages descendant-configuration editor's own
    /// blueprint/formula identity display.
    pub blueprint_or_formula_name: String,
    pub facility_id: Option<Uuid>,
    pub facility_name: Option<String>,
    pub effective_me: Option<u8>,
    pub effective_te: Option<u8>,
    /// Mirrors `VerificationOperationInput::blueprint_selection` verbatim --
    /// the raw, persisted selection (mode/kind/licensed runs/notes), not
    /// just the already-resolved `effective_me`/`effective_te` above. The
    /// Stages descendant-configuration editor's own edit-seed evidence:
    /// `None` for a reaction or an unresearched manufacturing Build.
    pub blueprint_selection: Option<crate::BlueprintSelection>,
    /// This occurrence's own projected runs (`VerificationOperationInput::node_runs`).
    pub projected_runs: u64,
    /// This occurrence's own projected output (`OperationCostProjection::produced_quantity`).
    pub projected_output: u64,
    /// The full requirement the parent boundary recorded for this occurrence
    /// (`0` for the root -- nothing external consumes it).
    pub required_quantity: u64,
    /// Inventory reused against that requirement (`0` for the root).
    pub planned_inventory_quantity: u64,
    /// Production demand handed to this occurrence by its parent boundary
    /// (`0` for the root).
    pub production_demand: u64,
    /// `projected_output - production_demand`, read verbatim from the parent
    /// boundary's own `child_surplus_quantity` (`0` for the root -- no
    /// consumer, no surplus concept).
    pub retained_surplus_quantity: u64,
    /// The parent boundary's own `child_surplus_retained_basis` (`None` for
    /// the root -- not unknown, simply not applicable; also `None` whenever
    /// `cost_complete` is `false`).
    pub retained_surplus_cost: Option<Money>,
    pub material_component_cost: Option<Money>,
    pub own_installation_cost: Option<Money>,
    pub total_production_cost: Option<Money>,
    /// `OperationCostProjection::unit_production_cost`
    /// (`total / produced`) -- display evidence only, never re-multiplied.
    pub unit_production_cost: Option<Money>,
    /// This occurrence's own `OperationCostProjection::complete` **and** (for
    /// a non-root occurrence) its parent boundary's own `complete`.
    pub cost_complete: bool,
    /// Direct material/component requirements owned by this occurrence,
    /// copied verbatim from the planning walk's node allocations. These are
    /// never replaced by a producer's aggregate demand.
    pub requirements: Vec<ExecutionRequirement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRequirement {
    pub type_id: i64,
    pub type_name: String,
    pub required_quantity: u64,
    pub planned_inventory_quantity: u64,
    pub shortage_quantity: u64,
    pub fulfillment_scope: crate::FulfillmentScope,
    pub resolution: MaterialBoundaryResolution,
    pub dependency_id: String,
    pub producer_build_id: Option<BuildId>,
    pub producer_node_id: Option<String>,
}

/// One external (`Buy`-resolution) shortage row, aggregated by `type_id`
/// from the canonical `NodeMaterialAllocation` rows whose `resolution ==
/// Buy` -- see the module doc's acquisition scope decision. This is a
/// filtered *view* of the same evidence `AggregateMaterialLine` rolls up,
/// never a value taken from a `Mixed`-strategy `AggregateMaterialLine` row
/// itself (which would conflate Buy shortage with Build/Reaction
/// production shortage).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcquisitionLine {
    pub type_id: i64,
    pub type_name: String,
    /// Σ `NodeMaterialAllocation::required_quantity` across every `Buy`
    /// contribution of this type.
    pub required_quantity: u64,
    /// Σ `NodeMaterialAllocation::allocated_quantity` across every `Buy`
    /// contribution of this type.
    pub planned_inventory_quantity: u64,
    /// Always `> 0` -- a fully inventory-covered `Buy` row never appears
    /// here.
    pub shortage_quantity: u64,
    /// The type's whole-tree starting inventory
    /// (`AggregateMaterialLine::available_quantity`, the original
    /// `PlanningInventory` seed) -- identical regardless of source
    /// strategy, so safe to read directly from the whole-tree rollup.
    /// `0` only if the type is absent from `material_lines` (should not
    /// happen for a `Buy` contribution the same walk already recorded).
    pub available_quantity: u64,
    /// Held by open Epics' reservations, excluded from
    /// `available_quantity` (`AggregateMaterialLine::reserved_quantity`).
    pub reserved_quantity: u64,
    /// The whole-tree source strategy for this type
    /// (`AggregateMaterialLine::strategy`) -- `Mixed` means this type is
    /// *also* produced (Build/Reaction) elsewhere in the tree. Purely
    /// informational: the quantities above are always Buy-only regardless
    /// of this value.
    pub source_strategy: MaterialRowStrategy,
    /// Every production occurrence that directly owns a `Buy` contribution
    /// to this type, with the specific EXTERNAL SHORTFALL that occurrence
    /// is responsible for (`NodeMaterialAllocation::shortage_quantity` for
    /// that occurrence's own `(build, type)` boundary) -- never the gross
    /// requirement, and never a proportional split invented to make a
    /// grouped total reconcile. `Σ consumers[].quantity ==
    /// shortage_quantity` exactly, because that is how `shortage_quantity`
    /// itself was summed. Only occurrences with a nonzero shortage
    /// contribution appear -- a consumer whose own need is already fully
    /// covered by inventory is not a reason to acquire more. Empty only if
    /// the owning occurrence could not be resolved (should not happen for
    /// evidence the same walk already recorded).
    pub consumers: Vec<AcquisitionConsumerRef>,
    /// See [`ExecutionNode::production_methods`]
    /// -- the methods this Buy requirement may switch to.
    pub production_methods: Vec<RecipeSelection>,
    /// Σ of every consumer's fresh (to-buy) cost
    /// (`BoundaryCostProjection::fresh_cost`). `None` when any contributing
    /// consumer's price is unknown -- never a partial sum.
    pub fresh_cost: Option<Money>,
    /// The fresh unit price when every priced consumer uses the same one,
    /// else `None`.
    pub fresh_unit_price: Option<Money>,
    /// Any contributing fresh price is stale.
    pub fresh_price_stale: bool,
}

/// One production occurrence's own contribution to an [`AcquisitionLine`]'s
/// external shortage. Mirrors [`ExecutionConsumerRef`]'s shape exactly --
/// the narrowest representation already established for "which occurrence,
/// how much."
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcquisitionConsumerRef {
    /// The consuming occurrence's own [`ExecutionNode::id`].
    pub node_id: String,
    /// The specific consuming occurrence's id -- never lost to node-level
    /// grouping even when several occurrences share `node_id`.
    pub occurrence_id: String,
    /// This occurrence's own external shortfall for this type (see the
    /// field doc on [`AcquisitionLine::consumers`]).
    pub quantity: u64,
    /// The demand edge itself -- the consuming
    /// Build whose sourcing a Plan inspector change targets, and that
    /// edge's own requirement evidence.
    pub build_id: BuildId,
    pub dependency_id: String,
    pub fulfillment_scope: crate::FulfillmentScope,
    pub required_quantity: u64,
    pub planned_inventory_quantity: u64,
    /// This edge's fresh (to-buy) cost and unit
    /// price, `None` when unpriced.
    pub fresh_cost: Option<Money>,
    pub fresh_unit_price: Option<Money>,
}

/// A `Build`/`Reaction`-intended component with no linked `Build` yet --
/// truthful placeholder evidence, never a fabricated descendant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionUnresolvedPrerequisite {
    /// The [`ExecutionNode::id`] of the occurrence that owns this
    /// unresolved requirement.
    pub owning_node_id: String,
    pub owning_occurrence_id: String,
    pub type_id: i64,
    pub type_name: String,
    /// The recipe the resolution intends to build with. `None` only if the
    /// upstream evidence itself omitted it (defensive; a genuinely
    /// `Unresolved` boundary always carries this).
    pub intended_recipe: Option<RecipeSelection>,
    pub required_quantity: u64,
    pub net_required_quantity: u64,
}

// ---------------------------------------------------------------------------
// project_execution_plan
// ---------------------------------------------------------------------------

/// Project the same allocation-aware operations/boundaries
/// (`IndustryService::project_build_materials`'s `verification_operations` /
/// `verification_inputs`, captured with `capture_verification: true`), the
/// same walk's canonical materials rollups (`node_allocations` /
/// `material_lines`, `BuildMaterialsSummary::node_allocations` /
/// `BuildMaterialsSummary::rows`), and cost
/// (`crate::build_cost::project_build_cost` over that exact same data) into
/// a staged, occurrence-preserving [`ExecutionPlanProjection`].
///
/// Pure, sync, `O(V + E)` for stage/interval computation plus small
/// deterministic sorting overhead within each compatibility bucket (see the
/// module's implementation report for the exact complexity breakdown) -- no
/// I/O, no recursion over recipes, no additional planning walk.
///
/// One operation is one canonical producer serving every consumer, so each
/// operation is its own display node. Consumers, stages and per-operation quantities read every
/// incoming demand edge ([`VerificationOperationInput::incoming`]).
#[must_use]
pub fn project_execution_plan(
    operations: &[VerificationOperationInput],
    boundaries: &[VerificationBoundaryInput],
    cost: &BuildCostProjection,
    node_allocations: &[NodeMaterialAllocation],
    material_lines: &[AggregateMaterialLine],
    generated_at: DateTime<Utc>,
) -> ExecutionPlanProjection {
    if operations.is_empty() {
        return ExecutionPlanProjection {
            root_node_id: String::new(),
            stages: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            occurrences: Vec::new(),
            acquisitions: Vec::new(),
            unresolved: Vec::new(),
            complete: cost.complete,
            warnings: cost.warnings.clone(),
            generated_at,
            logistics: crate::logistics::LogisticsPlan::default(),
        };
    }

    let mut ops_sorted: Vec<&VerificationOperationInput> = operations.iter().collect();
    ops_sorted.sort_by_key(|op| op.op_index);

    let op_cost_by_index: BTreeMap<u32, &OperationCostProjection> =
        cost.operations.iter().map(|op| (op.op_index, op)).collect();
    // Every consuming boundary of each operation (one for an occurrence
    // without `incoming`, one per demand edge for a canonical producer), ascending
    // traversal order.
    let mut boundary_costs_by_child_op: BTreeMap<u32, Vec<&BoundaryCostProjection>> =
        BTreeMap::new();
    for boundary in &cost.boundaries {
        if let Some(child) = boundary.child_op_index {
            boundary_costs_by_child_op
                .entry(child)
                .or_default()
                .push(boundary);
        }
    }
    let boundary_cost_by_traversal: BTreeMap<u32, &BoundaryCostProjection> = cost
        .boundaries
        .iter()
        .map(|boundary| (boundary.traversal_index, boundary))
        .collect();

    // Consumer operations of each operation: every incoming demand edge's
    // consumer (without `incoming`: the one parent).
    let consumer_ops_of = |op: &VerificationOperationInput| -> Vec<u32> {
        let mut consumers: Vec<u32> = if op.incoming.is_empty() {
            op.parent_op_index.into_iter().collect()
        } else {
            op.incoming
                .iter()
                .map(|demand| demand.consumer_op_index)
                .collect()
        };
        consumers.sort_unstable();
        consumers.dedup();
        consumers
    };
    let mut children_by_parent: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for op in &ops_sorted {
        for parent in consumer_ops_of(op) {
            children_by_parent
                .entry(parent)
                .or_default()
                .push(op.op_index);
        }
    }

    let (stage_by_op, _) = compute_stages_and_intervals(&ops_sorted, &children_by_parent);

    // One ProductionOperation == one Stages row.
    let groups: Vec<Vec<u32>> = ops_sorted.iter().map(|op| vec![op.op_index]).collect();

    let op_graph_node_id: BTreeMap<u32, &str> = ops_sorted
        .iter()
        .map(|op| (op.op_index, op.graph_node_id.as_str()))
        .collect();

    let mut group_id_by_op: BTreeMap<u32, usize> = BTreeMap::new();
    let mut group_anchor: Vec<String> = Vec::with_capacity(groups.len());
    for (group_index, members) in groups.iter().enumerate() {
        let anchor_op = *members.iter().min().expect("a group always has >=1 member");
        group_anchor.push(
            op_graph_node_id
                .get(&anchor_op)
                .expect("group member op_index is always a real operation")
                .to_string(),
        );
        for &member in members {
            group_id_by_op.insert(member, group_index);
        }
    }
    // The authoritative stage is computed over the
    // *display-node* (operation) DAG, never per occurrence -- see
    // `crate::production_stage`. A pooled sibling occurrence owns no walked
    // children, so an occurrence-level stage (0) would drag its consumer
    // down into the producer's own stage. Every occurrence reads
    // its node's stage, so `stage(P) < stage(C)` holds for every node edge.
    // A cycle between display nodes (only conceivable for corrupt data)
    // falls back to the occurrence-level stage.
    let group_stage_by_op: BTreeMap<u32, u32> = {
        let group_edges = ops_sorted.iter().flat_map(|op| {
            let producer_group = group_id_by_op[&op.op_index];
            consumer_ops_of(op)
                .into_iter()
                .filter_map(|consumer| group_id_by_op.get(&consumer).copied())
                .map(move |consumer_group| (producer_group, consumer_group))
                .collect::<Vec<_>>()
        });
        match crate::production_stage::production_stages(0..groups.len(), group_edges) {
            Ok(stage_by_group) => ops_sorted
                .iter()
                .map(|op| (op.op_index, stage_by_group[&group_id_by_op[&op.op_index]]))
                .collect(),
            Err(crate::production_stage::StageCycle) => stage_by_op.clone(),
        }
    };
    let node_id_of = |op_index: u32| -> String {
        group_anchor[*group_id_by_op
            .get(&op_index)
            .expect("every occurrence is placed into exactly one group")]
        .clone()
    };

    let producer_node_by_build: BTreeMap<Uuid, String> = ops_sorted
        .iter()
        .map(|op| (op.build_id.0, node_id_of(op.op_index)))
        .collect();
    let mut requirements_by_occurrence: BTreeMap<String, Vec<ExecutionRequirement>> =
        BTreeMap::new();
    for allocation in node_allocations {
        requirements_by_occurrence
            .entry(allocation.graph_node_id.clone())
            .or_default()
            .push(ExecutionRequirement {
                type_id: allocation.type_id,
                type_name: allocation.type_name.clone(),
                required_quantity: allocation.required_quantity,
                planned_inventory_quantity: allocation.allocated_quantity,
                shortage_quantity: allocation.shortage_quantity,
                fulfillment_scope: allocation.scope,
                resolution: allocation.resolution,
                dependency_id: allocation.dependency_id.clone(),
                producer_build_id: allocation.producer_build_id,
                producer_node_id: allocation
                    .producer_build_id
                    .and_then(|id| producer_node_by_build.get(&id.0).cloned()),
            });
    }
    for requirements in requirements_by_occurrence.values_mut() {
        requirements.sort_by(|a, b| {
            a.type_name
                .cmp(&b.type_name)
                .then(a.type_id.cmp(&b.type_id))
        });
    }

    // --- per-occurrence evidence ---------------------------------------
    let mut occ_by_op: BTreeMap<u32, ExecutionOccurrence> = BTreeMap::new();
    for op in &ops_sorted {
        let op_cost = *op_cost_by_index.get(&op.op_index).expect(
            "execution plan: operations and cost.operations must cover the same op_index set",
        );
        // Summed over every incoming demand edge (one for an occurrence
        // without `incoming`). Surplus is attributed to exactly one of them, so the
        // sum is the operation's one true surplus -- never double-counted.
        let incoming_boundaries = boundary_costs_by_child_op
            .get(&op.op_index)
            .cloned()
            .unwrap_or_default();
        let (
            required,
            planned_inventory,
            production_demand,
            surplus_qty,
            surplus_cost,
            boundary_complete,
        ) = if incoming_boundaries.is_empty() {
            // Root: no incoming boundary. Quantities are well-defined
            // zeros (there is no external requirement), never "unknown".
            (0, 0, 0, 0, None, true)
        } else {
            let sum = |f: fn(&BoundaryCostProjection) -> u64| {
                incoming_boundaries
                    .iter()
                    .map(|boundary| f(boundary))
                    .fold(0u64, u64::saturating_add)
            };
            let surplus_cost = incoming_boundaries
                .iter()
                .map(|boundary| boundary.child_surplus_retained_basis)
                .try_fold(Decimal::ZERO, |total, basis| {
                    basis.map(|basis| total.saturating_add(basis.0))
                })
                .map(Money);
            (
                sum(|boundary| boundary.required_quantity),
                sum(|boundary| boundary.inventory_quantity),
                sum(|boundary| boundary.child_consumed_quantity),
                sum(|boundary| boundary.child_surplus_quantity),
                surplus_cost,
                incoming_boundaries.iter().all(|boundary| boundary.complete),
            )
        };
        let cost_complete = op_cost.complete && boundary_complete;
        occ_by_op.insert(
            op.op_index,
            ExecutionOccurrence {
                id: op.graph_node_id.clone(),
                node_id: node_id_of(op.op_index),
                build_id: op.build_id,
                revision: op.revision,
                is_root: op.parent_op_index.is_none(),
                stage: group_stage_by_op[&op.op_index],
                activity: op.activity,
                output_type_id: op.product_type_id,
                output_type_name: op.product_name.clone(),
                blueprint_or_formula_type_id: op.blueprint_or_formula_type_id,
                blueprint_or_formula_name: op.blueprint_or_formula_name.clone(),
                facility_id: op.facility_id,
                facility_name: op.facility_name.clone(),
                effective_me: op.me,
                effective_te: op.te,
                blueprint_selection: op.blueprint_selection.clone(),
                projected_runs: op.node_runs,
                projected_output: op_cost.produced_quantity,
                required_quantity: required,
                planned_inventory_quantity: planned_inventory,
                production_demand,
                retained_surplus_quantity: surplus_qty,
                retained_surplus_cost: surplus_cost,
                material_component_cost: op_cost.material_component_cost,
                own_installation_cost: op_cost.own_installation.total,
                total_production_cost: op_cost.total_production_cost,
                unit_production_cost: op_cost.unit_production_cost,
                cost_complete,
                requirements: requirements_by_occurrence
                    .remove(&op.graph_node_id)
                    .unwrap_or_default(),
            },
        );
    }

    // --- consumer edges, per occurrence (needs parent_op_index, still in
    // scope here via `ops_sorted`; deliberately not stored on the public
    // `ExecutionOccurrence` type) ------------------------------------------
    let boundary_input_by_traversal: BTreeMap<u32, &VerificationBoundaryInput> = boundaries
        .iter()
        .map(|boundary| (boundary.traversal_index, boundary))
        .collect();
    let mut consumers_by_group: BTreeMap<usize, Vec<ExecutionConsumerRef>> = BTreeMap::new();
    for op in &ops_sorted {
        let this_group = group_id_by_op[&op.op_index];
        if op.incoming.is_empty() {
            let Some(parent_op_index) = op.parent_op_index else {
                continue;
            };
            let occurrence = &occ_by_op[&op.op_index];
            let parent = &occ_by_op[&parent_op_index];
            // An occurrence without `incoming` has exactly one consuming
            // boundary.
            let edge = boundary_costs_by_child_op
                .get(&op.op_index)
                .and_then(|edges| edges.first().copied());
            consumers_by_group
                .entry(this_group)
                .or_default()
                .push(ExecutionConsumerRef {
                    node_id: node_id_of(parent_op_index),
                    occurrence_id: parent.id.clone(),
                    quantity: occurrence.production_demand,
                    build_id: parent.build_id,
                    dependency_id: edge
                        .and_then(|edge| boundary_input_by_traversal.get(&edge.traversal_index))
                        .map_or_else(String::new, |input| input.dependency_id.clone()),
                    fulfillment_scope: edge
                        .map_or(crate::FulfillmentScope::Missing, |edge| edge.scope),
                    required_quantity: occurrence.required_quantity,
                    planned_inventory_quantity: occurrence.planned_inventory_quantity,
                });
            continue;
        }
        // One consumer entry per demand edge, with that edge's own demand.
        for demand in &op.incoming {
            let Some(consumer) = occ_by_op.get(&demand.consumer_op_index) else {
                continue;
            };
            let edge = boundary_cost_by_traversal.get(&demand.traversal_index);
            consumers_by_group
                .entry(this_group)
                .or_default()
                .push(ExecutionConsumerRef {
                    node_id: node_id_of(demand.consumer_op_index),
                    occurrence_id: consumer.id.clone(),
                    quantity: edge.map_or(0, |boundary| boundary.child_consumed_quantity),
                    build_id: consumer.build_id,
                    dependency_id: demand.dependency_id.clone(),
                    fulfillment_scope: edge
                        .map_or(crate::FulfillmentScope::Missing, |boundary| boundary.scope),
                    required_quantity: edge.map_or(0, |boundary| boundary.required_quantity),
                    planned_inventory_quantity: edge
                        .map_or(0, |boundary| boundary.inventory_quantity),
                });
        }
    }
    for consumers in consumers_by_group.values_mut() {
        consumers.sort_by(|a, b| {
            (a.node_id.as_str(), a.occurrence_id.as_str())
                .cmp(&(b.node_id.as_str(), b.occurrence_id.as_str()))
        });
    }

    // --- display nodes ---------------------------------------------------
    let mut nodes: Vec<ExecutionNode> = groups
        .iter()
        .enumerate()
        .map(|(group_index, members)| {
            build_node(
                &group_anchor[group_index],
                members,
                &occ_by_op,
                consumers_by_group
                    .get(&group_index)
                    .cloned()
                    .unwrap_or_default(),
            )
        })
        .collect();
    sort_nodes(&mut nodes);
    let available_by_type: BTreeMap<i64, u64> = material_lines
        .iter()
        .map(|line| (line.type_id, line.available_quantity))
        .collect();
    for node in &mut nodes {
        node.available_quantity = available_by_type
            .get(&node.output_type_id)
            .copied()
            .unwrap_or(0);
    }

    let mut edge_set: BTreeSet<(String, String)> = BTreeSet::new();
    for node in &nodes {
        for consumer in &node.consumers {
            edge_set.insert((node.id.clone(), consumer.node_id.clone()));
        }
    }
    let edges: Vec<ExecutionEdge> = edge_set
        .into_iter()
        .map(|(from, to)| ExecutionEdge { from, to })
        .collect();

    let max_stage = nodes.iter().map(|node| node.stage).max().unwrap_or(0);
    let mut stages = Vec::new();
    for stage_index in 0..=max_stage {
        let node_ids: Vec<String> = nodes
            .iter()
            .filter(|node| node.stage == stage_index)
            .map(|node| node.id.clone())
            .collect();
        if !node_ids.is_empty() {
            stages.push(ExecutionStage {
                index: stage_index,
                node_ids,
            });
        }
    }

    let occurrences: Vec<ExecutionOccurrence> = ops_sorted
        .iter()
        .map(|op| occ_by_op[&op.op_index].clone())
        .collect();

    let root_node_id = ops_sorted
        .iter()
        .find(|op| op.parent_op_index.is_none())
        .map(|op| node_id_of(op.op_index))
        .unwrap_or_default();

    let owner_by_op_index: BTreeMap<u32, (String, String)> = ops_sorted
        .iter()
        .map(|op| {
            (
                op.op_index,
                (node_id_of(op.op_index), occ_by_op[&op.op_index].id.clone()),
            )
        })
        .collect();

    // `NodeMaterialAllocation` identifies its owning node by `graph_node_id`
    // (not `op_index`), so acquisitions needs its own reverse lookup from
    // that durable identity to the node/group it belongs to.
    let node_id_by_graph_node_id: BTreeMap<&str, &str> = occ_by_op
        .values()
        .map(|occurrence| (occurrence.id.as_str(), occurrence.node_id.as_str()))
        .collect();
    let acquisitions = build_acquisitions(
        node_allocations,
        material_lines,
        &node_id_by_graph_node_id,
        &buy_cost_by_node_and_type(&ops_sorted, cost),
    );
    let unresolved = build_unresolved(boundaries, &owner_by_op_index);

    ExecutionPlanProjection {
        root_node_id,
        stages,
        nodes,
        edges,
        occurrences,
        acquisitions,
        unresolved,
        complete: cost.complete,
        warnings: cost.warnings.clone(),
        generated_at,
        logistics: crate::logistics::LogisticsPlan::default(),
    }
}

// ---------------------------------------------------------------------------
// Stage + ancestor/descendant interval computation
// ---------------------------------------------------------------------------

/// `stage(op) = 0` when `op` has no active production prerequisite (every
/// entry in `operations` is, by construction, an *active* production
/// occurrence -- a pruned/`Unresolved` boundary never gets one), else `1 +
/// max(stage(child))`. `out_time(op)` is the largest `op_index` anywhere in
/// `op`'s own subtree, used for O(1) ancestor/descendant tests: since
/// `op_index` is assigned in DFS pre-order (a parent's index is always less
/// than any descendant's), `op_index` doubles as each occurrence's DFS
/// "in-time" for free, and this is the one pass that computes the matching
/// "out-time". Both are computed in a single descending-`op_index` traversal
/// -- children are always visited before their parent, the same ordering
/// `project_build_cost` already uses for its own bottom-up fold
/// (`build_cost.rs`'s `Reverse(op_index)` sort). `O(V + E)`.
///
/// This does **not** rely on SDE recipes being acyclic -- it is computed
/// from the actual occurrence parent/child evidence, so it is correct even
/// for synthetic or corrupt data that revisits a type (see
/// `tests::ancestor_descendant_collision_never_groups`).
fn compute_stages_and_intervals(
    ops_sorted: &[&VerificationOperationInput],
    children_by_parent: &BTreeMap<u32, Vec<u32>>,
) -> (BTreeMap<u32, u32>, BTreeMap<u32, u32>) {
    let mut descending: Vec<u32> = ops_sorted.iter().map(|op| op.op_index).collect();
    descending.sort_unstable_by(|a, b| b.cmp(a));

    let mut stage_by_op: BTreeMap<u32, u32> = BTreeMap::new();
    let mut out_time_by_op: BTreeMap<u32, u32> = BTreeMap::new();
    for op_index in descending {
        match children_by_parent.get(&op_index) {
            None => {
                stage_by_op.insert(op_index, 0);
                out_time_by_op.insert(op_index, op_index);
            }
            Some(children) if children.is_empty() => {
                stage_by_op.insert(op_index, 0);
                out_time_by_op.insert(op_index, op_index);
            }
            Some(children) => {
                let max_child_stage = children
                    .iter()
                    .map(|child| stage_by_op[child])
                    .max()
                    .expect("non-empty children");
                let max_out_time = children
                    .iter()
                    .map(|child| out_time_by_op[child])
                    .max()
                    .expect("non-empty children")
                    .max(op_index);
                stage_by_op.insert(op_index, max_child_stage + 1);
                out_time_by_op.insert(op_index, max_out_time);
            }
        }
    }
    (stage_by_op, out_time_by_op)
}

// ---------------------------------------------------------------------------
// Compatibility key + grouping
// ---------------------------------------------------------------------------

fn activity_rank(activity: MaterialActivity) -> u8 {
    match activity {
        MaterialActivity::Manufacturing => 0,
        MaterialActivity::Reaction => 1,
    }
}

// ---------------------------------------------------------------------------
// Node assembly
// ---------------------------------------------------------------------------

fn build_node(
    anchor_id: &str,
    members: &[u32],
    occ_by_op: &BTreeMap<u32, ExecutionOccurrence>,
    consumers: Vec<ExecutionConsumerRef>,
) -> ExecutionNode {
    let member_occurrences: Vec<&ExecutionOccurrence> = members
        .iter()
        .map(|op_index| &occ_by_op[op_index])
        .collect();
    let sample = *member_occurrences
        .first()
        .expect("a group always has >=1 member");

    let stage = member_occurrences.iter().map(|m| m.stage).max().unwrap();
    let cost_complete = member_occurrences.iter().all(|m| m.cost_complete);

    let required_quantity = member_occurrences.iter().map(|m| m.required_quantity).sum();
    let planned_inventory_quantity = member_occurrences
        .iter()
        .map(|m| m.planned_inventory_quantity)
        .sum();
    let production_demand = member_occurrences.iter().map(|m| m.production_demand).sum();

    let projected_output = member_occurrences.iter().map(|m| m.projected_output).sum();
    let projected_runs = member_occurrences.iter().map(|m| m.projected_runs).sum();
    let retained_surplus_quantity = member_occurrences
        .iter()
        .map(|m| m.retained_surplus_quantity)
        .sum();

    let (
        material_component_cost,
        own_installation_cost,
        total_production_cost,
        retained_surplus_cost,
    ) = if cost_complete {
        (
            Some(sum_money(
                member_occurrences
                    .iter()
                    .map(|m| m.material_component_cost.unwrap()),
            )),
            Some(sum_money(
                member_occurrences
                    .iter()
                    .map(|m| m.own_installation_cost.unwrap()),
            )),
            Some(sum_money(
                member_occurrences
                    .iter()
                    .map(|m| m.total_production_cost.unwrap()),
            )),
            Some(sum_money(
                member_occurrences
                    .iter()
                    .map(|m| m.retained_surplus_cost.unwrap_or(Money::zero())),
            )),
        )
    } else {
        (None, None, None, None)
    };

    let mut occurrence_ids: Vec<(u32, String)> = members
        .iter()
        .map(|&op_index| (op_index, occ_by_op[&op_index].id.clone()))
        .collect();
    occurrence_ids.sort_by_key(|(op_index, _)| *op_index);
    let occurrence_ids: Vec<String> = occurrence_ids.into_iter().map(|(_, id)| id).collect();

    ExecutionNode {
        id: anchor_id.to_string(),
        output_type_id: sample.output_type_id,
        output_type_name: sample.output_type_name.clone(),
        activity: sample.activity,
        stage,
        occurrence_ids,
        facility_id: sample.facility_id,
        facility_name: sample.facility_name.clone(),
        effective_me: sample.effective_me,
        effective_te: sample.effective_te,
        required_quantity,
        planned_inventory_quantity,
        production_demand,
        projected_output,
        projected_runs,
        retained_surplus_quantity,
        retained_surplus_cost,
        material_component_cost,
        own_installation_cost,
        total_production_cost,
        cost_complete,
        consumers,
        production_methods: Vec::new(),
        unit_production_cost: match member_occurrences.as_slice() {
            [only] => only.unit_production_cost,
            _ => None,
        },
        available_quantity: 0,
    }
}

fn sort_nodes(nodes: &mut [ExecutionNode]) {
    nodes.sort_by(|a, b| {
        a.stage
            .cmp(&b.stage)
            .then_with(|| activity_rank(a.activity).cmp(&activity_rank(b.activity)))
            .then_with(|| a.output_type_name.cmp(&b.output_type_name))
            .then_with(|| a.output_type_id.cmp(&b.output_type_id))
            .then_with(|| a.facility_id.cmp(&b.facility_id))
            .then_with(|| a.id.cmp(&b.id))
    });
}

fn sum_money<I: IntoIterator<Item = Money>>(values: I) -> Money {
    let mut total = Decimal::ZERO;
    for value in values {
        total = total.saturating_add(value.0);
    }
    Money(total)
}

// ---------------------------------------------------------------------------
// Acquisitions / unresolved
// ---------------------------------------------------------------------------

/// Built from the canonical per-node `node_allocations`
/// (`BuildMaterialsSummary::node_allocations`), filtered to `resolution ==
/// Buy` -- **never** from `material_lines` directly, whose `Mixed`-strategy
/// rows sum Buy shortage together with Build/Reaction production shortage
/// (see the module doc's acquisition scope decision). `material_lines` is
/// consulted only for the two facts that are safe regardless of source
/// strategy: `available_quantity` (the type's whole-tree starting
/// inventory) and `source_strategy` (informational only).
/// Buy boundary cost evidence keyed by `(consuming graph_node_id, type_id)`.
fn buy_cost_by_node_and_type<'a>(
    ops: &[&'a VerificationOperationInput],
    cost: &'a BuildCostProjection,
) -> BTreeMap<(&'a str, i64), &'a BoundaryCostProjection> {
    let node_of: BTreeMap<u32, &str> = ops
        .iter()
        .map(|op| (op.op_index, op.graph_node_id.as_str()))
        .collect();
    cost.boundaries
        .iter()
        .filter(|boundary| boundary.resolution == MaterialBoundaryResolution::Buy)
        .filter_map(|boundary| {
            node_of
                .get(&boundary.op_index)
                .map(|node| ((*node, boundary.type_id), boundary))
        })
        .collect()
}

fn build_acquisitions(
    node_allocations: &[NodeMaterialAllocation],
    material_lines: &[AggregateMaterialLine],
    node_id_by_graph_node_id: &BTreeMap<&str, &str>,
    buy_cost: &BTreeMap<(&str, i64), &BoundaryCostProjection>,
) -> Vec<AcquisitionLine> {
    struct Totals {
        type_name: String,
        required: u64,
        planned_inventory: u64,
        shortage: u64,
        consumers: Vec<AcquisitionConsumerRef>,
        fresh_cost: Option<Decimal>,
        unit_prices: BTreeSet<Money>,
        stale: bool,
    }
    let mut by_type: BTreeMap<i64, Totals> = BTreeMap::new();
    for allocation in node_allocations {
        if allocation.resolution != MaterialBoundaryResolution::Buy {
            continue;
        }
        let entry = by_type.entry(allocation.type_id).or_insert_with(|| Totals {
            type_name: allocation.type_name.clone(),
            required: 0,
            planned_inventory: 0,
            shortage: 0,
            consumers: Vec::new(),
            fresh_cost: Some(Decimal::ZERO),
            unit_prices: BTreeSet::new(),
            stale: false,
        });
        entry.required = entry.required.saturating_add(allocation.required_quantity);
        entry.planned_inventory = entry
            .planned_inventory
            .saturating_add(allocation.allocated_quantity);
        entry.shortage = entry.shortage.saturating_add(allocation.shortage_quantity);
        // Only a nonzero shortage contribution is a reason to acquire more --
        // see the field doc on `AcquisitionLine::consumers`.
        if allocation.shortage_quantity > 0 {
            let evidence = buy_cost
                .get(&(allocation.graph_node_id.as_str(), allocation.type_id))
                .copied();
            let edge_cost = evidence.and_then(|boundary| boundary.fresh_cost);
            let edge_price = evidence.and_then(|boundary| boundary.fresh_unit_price);
            entry.fresh_cost = match (entry.fresh_cost, edge_cost) {
                (Some(sum), Some(cost)) => Some(sum.saturating_add(cost.0)),
                _ => None,
            };
            if let Some(price) = edge_price {
                entry.unit_prices.insert(price);
            }
            entry.stale |= evidence.is_some_and(|boundary| boundary.fresh_price_stale);
            if let Some(&node_id) = node_id_by_graph_node_id.get(allocation.graph_node_id.as_str())
            {
                entry.consumers.push(AcquisitionConsumerRef {
                    fresh_cost: edge_cost,
                    fresh_unit_price: edge_price,
                    node_id: node_id.to_string(),
                    occurrence_id: allocation.graph_node_id.clone(),
                    quantity: allocation.shortage_quantity,
                    build_id: allocation.build_id,
                    dependency_id: allocation.dependency_id.clone(),
                    fulfillment_scope: allocation.scope,
                    required_quantity: allocation.required_quantity,
                    planned_inventory_quantity: allocation.allocated_quantity,
                });
            }
        }
    }

    let line_by_type: BTreeMap<i64, &AggregateMaterialLine> = material_lines
        .iter()
        .map(|line| (line.type_id, line))
        .collect();

    by_type
        .into_iter()
        .filter(|(_, totals)| totals.shortage > 0)
        .map(|(type_id, mut totals)| {
            let line = line_by_type.get(&type_id).copied();
            totals.consumers.sort_by(|a, b| {
                (a.node_id.as_str(), a.occurrence_id.as_str())
                    .cmp(&(b.node_id.as_str(), b.occurrence_id.as_str()))
            });
            AcquisitionLine {
                type_id,
                type_name: totals.type_name,
                required_quantity: totals.required,
                planned_inventory_quantity: totals.planned_inventory,
                shortage_quantity: totals.shortage,
                available_quantity: line.map_or(0, |line| line.available_quantity),
                reserved_quantity: line.map_or(0, |line| line.reserved_quantity),
                source_strategy: line.map_or(MaterialRowStrategy::Buy, |line| line.strategy),
                consumers: totals.consumers,
                production_methods: Vec::new(),
                fresh_cost: totals.fresh_cost.map(Money),
                fresh_unit_price: match totals.unit_prices.len() {
                    1 => totals.unit_prices.into_iter().next(),
                    _ => None,
                },
                fresh_price_stale: totals.stale,
            }
        })
        .collect()
}

fn build_unresolved(
    boundaries: &[VerificationBoundaryInput],
    owner_by_op_index: &BTreeMap<u32, (String, String)>,
) -> Vec<ExecutionUnresolvedPrerequisite> {
    let mut out: Vec<ExecutionUnresolvedPrerequisite> = boundaries
        .iter()
        .filter(|boundary| boundary.resolution == MaterialBoundaryResolution::Unresolved)
        .filter_map(|boundary| {
            owner_by_op_index.get(&boundary.op_index).map(
                |(owning_node_id, owning_occurrence_id)| ExecutionUnresolvedPrerequisite {
                    owning_node_id: owning_node_id.clone(),
                    owning_occurrence_id: owning_occurrence_id.clone(),
                    type_id: boundary.type_id,
                    type_name: boundary.type_name.clone(),
                    intended_recipe: boundary.intended_recipe,
                    required_quantity: boundary.api_required,
                    net_required_quantity: boundary.api_shortage,
                },
            )
        })
        .collect();
    out.sort_by(|a, b| {
        (a.owning_occurrence_id.as_str(), a.type_id)
            .cmp(&(b.owning_occurrence_id.as_str(), b.type_id))
    });
    out
}

#[cfg(test)]
mod tests;
