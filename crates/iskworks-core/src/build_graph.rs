//! Pure Build Graph projection: the same allocation-aware live planning data
//! Materials/Worksheet already produce -> `BuildGraphProjection`.
//!
//! Graph has no planning/cost pass of its own.
//! `project_live_build_graph` is the one live entry point: pure and
//! sync, it reads every quantity/cost fact straight off
//! `VerificationOperationInput` / `VerificationBoundaryInput` (the same
//! allocation-aware walk `IndustryService::project_build_materials` already
//! ran) and `BuildCostProjection` (the same cost enrichment Worksheet uses,
//! `crate::build_cost::project_build_cost`) -- no tree walk of its own, no
//! second cost fold, no re-resolved market/adjusted prices.
//!
//! Identity is entity-based (never a materialized `root/type/type` path):
//!
//! ```text
//! root:<rootBuildId>                       the root production node
//! build:<linkedBuildId>                    a linked production node, any depth
//! buy:<parentBuildId>:<componentTypeId>    a direct Buy/acquisition requirement
//!                                          of that Build -- buildable or raw --
//!                                          and the Unresolved slot it becomes
//!                                          when switched to Build before its
//!                                          linked Build exists
//! ```

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::build_cost::{
    BoundaryCostProjection, BuildCostProjection, CostWarning, OperationCostProjection,
};
use crate::build_materials::{
    MaterialActivity, MaterialBoundaryResolution, VerificationBoundaryInput,
    VerificationOperationInput,
};
use crate::industry::{BuildId, Money, RecipeCurrency, RecipeSelection};

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildGraphProjection {
    pub root: ProductionNode,
    /// Non-fatal issues surfaced while walking / projecting. The tree is
    /// still whole; these annotate it for Graph diagnostics.
    pub warnings: Vec<GraphWarning>,
    pub generated_at: DateTime<Utc>,
    /// The market-evidence identity every node was valued against, one entry
    /// per distinct scope the graph touched. Explains the valuation and lets
    /// a candidate-preview of a linked Build be pinned to the same evidence
    /// for exact reconciliation. Empty from `project_build_graph` alone
    /// (pure topology); filled by `IndustryService::build_graph`.
    #[serde(default)]
    pub market_evidence: Vec<crate::MarketScopeEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionNode {
    /// `root:<uuid>` for the root, `build:<uuid>` for a linked node.
    pub graph_node_id: String,
    /// Always a real, persisted `Build` id -- never synthetic.
    pub build_id: BuildId,
    pub parent_build_id: Option<BuildId>,
    pub parent_component_type_id: Option<i64>,
    pub type_id: i64,
    pub type_name: String,
    pub kind: ProductionKind,
    pub recipe: RecipeSelection,
    /// The *projected* operation runs this
    /// node is sized at under the current live plan -- root: the overlay's
    /// own `runs`; a linked child: `ceil(remaining / output_per_run)` after
    /// inventory allocation, exactly what `BuildCostProjection` and
    /// Materials/Worksheet already use. **Never** this Build's own saved
    /// configuration (see `persisted_runs` for that) -- the persisted value
    /// would silently diverge from the cost actually shown whenever a linked child's
    /// saved runs were stale.
    pub runs: u64,
    /// This node's own **persisted** `Build.runs` -- the saved, standalone
    /// configuration. Informational only: opening this Build on its own
    /// still shows this number, unaffected by whatever the current parent
    /// plan projects `runs` to. Equal to `runs` for the root.
    pub persisted_runs: u64,

    /// The parent's full recipe requirement for this node's output. `None`
    /// for the root (nothing external consumes it).
    pub required_quantity: Option<u64>,
    /// The inventory-net demand the parent sized this node's runs against.
    /// `None` for the root. For a canonical producer serving several
    /// consumers, `required_quantity` / `net_required_quantity` are the
    /// **aggregate** over every incoming demand edge (the operation's own
    /// sizing basis), so `surplus` is the operation's one surplus; each
    /// consumer's own share is listed in `incoming_demands`.
    pub net_required_quantity: Option<u64>,
    /// Every demand edge this operation
    /// serves when it serves more than one (empty otherwise).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub incoming_demands: Vec<GraphIncomingDemand>,
    /// `runs * output-per-run` for the parent-consumed product.
    pub producing_quantity: u64,
    /// Signed `producing_quantity - required_quantity` (full basis). `0` for
    /// the root. Health checks use `net_required_quantity`, not this.
    pub surplus: i64,

    /// `BuildCostProjection`'s
    /// `OperationCostProjection.total_production_cost` for this exact
    /// operation occurrence -- this node's own material/component cost plus
    /// its own installation, recursively including every descendant's
    /// *consumed* (never retained-surplus) production basis.
    pub estimated_cost: Option<Money>,
    /// This operation's own material/component cost alone (never
    /// includes `own_installation_cost`) -- `OperationCostProjection.material_component_cost`.
    pub material_component_cost: Option<Money>,
    /// This operation's own installation cost alone -- never a
    /// descendant's (which is already folded proportionally into
    /// `material_component_cost` via each Build/Reaction child's consumed
    /// share) -- `OperationCostProjection.own_installation.total`.
    pub own_installation_cost: Option<Money>,
    pub cost_state: CostState,
    pub recipe_currency: RecipeCurrency,

    /// The linked `Build`'s own effective blueprint material efficiency, as
    /// resolved by the same preview the worksheet uses (`Manual` selection
    /// value, or the owned-blueprint observation's ME for an `ObservedAsset`
    /// selection). `None` for a reaction node, the root before enrichment,
    /// or when the per-node snapshot could not be computed.
    ///
    /// Populated by `enrich_graph_materials`, never by `project_build_graph`
    /// (which is ME-free by construction).
    pub effective_me: Option<u8>,
    /// The linked `Build`'s own effective blueprint time efficiency, same
    /// provenance and nullability as `effective_me`. Always `None` for a
    /// reaction node.
    pub effective_te: Option<u8>,

    /// Every direct requirement of this production operation, in expansion
    /// order: a linked `Production` node, an `UnresolvedBuild` slot, or an
    /// `Acquisition` node (buildable or raw -- both are first-class graph
    /// nodes; the difference is only whether `buildable_recipe` is `Some`).
    /// A component is never both a `Production` child and an `Acquisition`
    /// child.
    pub children: Vec<GraphChild>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
// `nodeKind`, not `kind`: `ProductionNode` carries its own `kind`
// (`ProductionKind`), and an internally-tagged `kind` here would collide
// with it in JSON (the field would silently overwrite the tag).
#[serde(tag = "nodeKind", rename_all = "camelCase")]
pub enum GraphChild {
    Production(ProductionNode),
    /// A direct Buy requirement of the parent Build -- buildable or raw.
    Acquisition(AcquisitionNode),
    UnresolvedBuild(UnresolvedBuildNode),
    /// This consumer's demand edge is
    /// served by a canonical producer that is drawn (once, in full) under
    /// another consumer of the same plan. The tree-shaped Graph repeats the
    /// producer here only as an alias -- same `graph_node_id` / `build_id`
    /// as the one full `Production` node -- carrying this edge's own
    /// requirement, never the operation's runs / output / surplus / cost.
    ProducerReference(ProducerReferenceNode),
}

/// An alias of a canonical producer under one of its consumers -- see
/// [`GraphChild::ProducerReference`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProducerReferenceNode {
    /// The producer operation's own `graph_node_id` (`build:<uuid>`) --
    /// identical to its full `Production` node's.
    pub graph_node_id: String,
    /// The producer Build.
    pub build_id: BuildId,
    /// The consumer Build this edge belongs to.
    pub parent_build_id: BuildId,
    /// The demand edge (see `NodeMaterialAllocation::dependency_id`).
    pub dependency_id: String,
    pub type_id: i64,
    pub type_name: String,
    pub kind: ProductionKind,
    /// This edge's own full requirement.
    pub required_quantity: u64,
    /// This edge's own production demand after inventory.
    pub net_required_quantity: u64,
}

/// A direct **acquisition dependency** of a production node: a component the
/// owning Build currently sources as *Buy*. First-class regardless of
/// buildability -- a raw material still has quantity, cost, provenance and
/// its owning Build. `buildable_recipe` is the single capability flag:
///
/// * `Some(recipe)` -- can be switched to BUILD (that recipe), mutating the
///   **owning** `parent_build_id`, never the root.
/// * `None`         -- terminal: no BUILD action, still a selectable node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcquisitionNode {
    /// `buy:<parentBuildId>:<typeId>`. Two different parent Builds
    /// consuming the same `type_id` are two distinct nodes -- never
    /// deduplicated by `type_id`.
    pub graph_node_id: String,
    /// The Build that owns this requirement -- the target of any BUY->BUILD
    /// mutation on this node.
    pub parent_build_id: BuildId,
    pub type_id: i64,
    pub type_name: String,
    pub required_quantity: u64,
    /// The portion still to acquire after inventory / fulfillment scope --
    /// authoritative once `enrich_graph_materials` runs; equals
    /// `required_quantity` before enrichment or with no split.
    pub missing_quantity: u64,
    /// `Some` iff this component has a supported manufacturing/reaction
    /// recipe -- what "Switch to BUILD" would resolve it to. `None` = raw /
    /// terminal.
    pub buildable_recipe: Option<RecipeSelection>,
    pub estimated_cost: Option<Money>,
    pub cost_state: CostState,
    pub warning: Option<GraphWarning>,
}

/// One consumer's demand edge into a shared canonical producer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphIncomingDemand {
    pub dependency_id: String,
    pub consumer_build_id: BuildId,
    pub required_quantity: u64,
    pub net_required_quantity: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedBuildNode {
    /// The same `buy:<parentBuildId>:<typeId>` slot the actionable Buy
    /// occupied -- becomes `build:<linkedId>` once the linked Build exists.
    pub graph_node_id: String,
    pub parent_build_id: BuildId,
    pub type_id: i64,
    pub type_name: String,
    /// The intended recipe. No `build_id`, no `runs`, no `producing_quantity`
    /// -- there is no production entity yet.
    pub recipe: RecipeSelection,
    pub required_quantity: u64,
    pub net_required_quantity: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProductionKind {
    RootManufacturing,
    RootReaction,
    Manufacturing,
    Reaction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CostState {
    /// `project_build_graph` output -- cost has not been folded yet.
    NotComputed,
    Known,
    Incomplete,
    Stale,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphWarning {
    pub graph_node_id: String,
    pub code: GraphWarningCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GraphWarningCode {
    MarketPriceUnavailable,
    LinkedBuildUnresolved,
    StaleRecipe,
    /// Purely informational once `runs` is the projected
    /// value -- never implies the displayed cost is wrong (it's already
    /// costed at the projected runs). Emitted when a node's own persisted
    /// `Build.runs` differs from what the current plan projects it to.
    RunsDiverged,
    /// This operation's or boundary's own cost is incomplete
    /// -- a `crate::build_cost::CostWarning` translated as-is (missing
    /// fresh price, missing inventory basis, missing adjusted price,
    /// missing system cost index, no facility selected, an unresolved
    /// child, or an incomplete child's cost). `message` carries the
    /// specific reason; never implies the recipe itself is invalid.
    CostIncomplete,
    /// This boundary's fresh price is complete but backed by
    /// market evidence older than the staleness horizon -- still used, not
    /// a missing-cost condition.
    StaleMarketEvidence,
}

// ---------------------------------------------------------------------------
// project_live_build_graph -- the live, allocation-aware Graph
// projection. Pure and sync: every quantity/cost fact it emits is read
// straight off `VerificationOperationInput` / `VerificationBoundaryInput`
// (the same authoritative allocation-aware walk Materials/Worksheet use --
// `IndustryService::project_build_materials`) and `BuildCostProjection` (the
// same cost enrichment Worksheet uses -- `crate::build_cost::project_build_cost`).
// No second tree walk and no re-resolved market/adjusted prices.
// ---------------------------------------------------------------------------

/// Everything `project_live_build_graph` needs that isn't already on the
/// allocation-aware operations/boundaries themselves.
pub struct GraphDisplayContext<'a> {
    /// `Buy component type_id -> the recipe it could be built with`, for
    /// every Buy boundary in the tree. Absent = raw / terminal.
    pub buildable_recipes: &'a BTreeMap<i64, RecipeSelection>,
}

/// Project the same overlay's allocation-aware operations/boundaries
/// (`IndustryService::project_build_materials`'s `verification_operations` /
/// `verification_inputs`, captured with `capture_verification: true`) and
/// their cost (`crate::build_cost::project_build_cost` over that exact same
/// data) into a [`BuildGraphProjection`] -- Graph as a presentation of the
/// one authoritative planning projection, not a second planning engine.
///
/// `operations[0]` (`op_index == 0`) must be the root; every other operation
/// must be reachable from it via `parent_op_index` -- exactly what
/// `project_build_materials` always produces. `warnings` are built in one
/// flat pass afterward (never during node construction): a `RunsDiverged`
/// note per operation whose persisted `Build.runs` differs from its
/// projected `node_runs` (informational only -- cost is always at the
/// projected value), a `LinkedBuildUnresolved` note per `Unresolved`
/// boundary, and every `CostWarning` `BuildCostProjection` already computed,
/// translated as-is.
#[must_use]
pub fn project_live_build_graph(
    operations: &[VerificationOperationInput],
    boundaries: &[VerificationBoundaryInput],
    cost: &BuildCostProjection,
    ctx: &GraphDisplayContext<'_>,
    generated_at: DateTime<Utc>,
) -> BuildGraphProjection {
    let op_by_index: BTreeMap<u32, &VerificationOperationInput> =
        operations.iter().map(|op| (op.op_index, op)).collect();
    let op_cost_by_index: BTreeMap<u32, &OperationCostProjection> =
        cost.operations.iter().map(|op| (op.op_index, op)).collect();
    let boundary_cost_by_traversal: BTreeMap<u32, &BoundaryCostProjection> = cost
        .boundaries
        .iter()
        .map(|boundary| (boundary.traversal_index, boundary))
        .collect();
    let mut boundaries_by_op: BTreeMap<u32, Vec<&VerificationBoundaryInput>> = BTreeMap::new();
    for boundary in boundaries {
        boundaries_by_op
            .entry(boundary.op_index)
            .or_default()
            .push(boundary);
    }
    // A Build/Reaction boundary's spawned child operation, keyed by the
    // *owning* operation and the component `type_id` it fulfils -- a
    // boundary with no entry here is either `Unresolved` (no child at all)
    // or `FullyCovered` (shortage 0, subtree pruned; the allocator never
    // walked a child for it).
    let mut child_op_by_parent_and_type: BTreeMap<(u32, i64), u32> = BTreeMap::new();
    for op in operations {
        if op.incoming.is_empty() {
            if let Some(parent_op_index) = op.parent_op_index {
                child_op_by_parent_and_type
                    .insert((parent_op_index, op.product_type_id), op.op_index);
            }
            continue;
        }
        for demand in &op.incoming {
            child_op_by_parent_and_type
                .insert((demand.consumer_op_index, op.product_type_id), op.op_index);
        }
    }
    let boundary_by_traversal: BTreeMap<u32, &VerificationBoundaryInput> = boundaries
        .iter()
        .map(|boundary| (boundary.traversal_index, boundary))
        .collect();

    let mut rendered: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let root = build_live_node(
        0,
        None,
        None,
        None,
        true,
        &LiveGraphIndex {
            op_by_index: &op_by_index,
            op_cost_by_index: &op_cost_by_index,
            boundaries_by_op: &boundaries_by_op,
            boundary_cost_by_traversal: &boundary_cost_by_traversal,
            boundary_by_traversal: &boundary_by_traversal,
            child_op_by_parent_and_type: &child_op_by_parent_and_type,
            ctx,
        },
        &mut rendered,
    );

    let mut warnings = Vec::new();
    for op in operations {
        if op.persisted_runs != op.node_runs {
            warnings.push(GraphWarning {
                graph_node_id: op.graph_node_id.clone(),
                code: GraphWarningCode::RunsDiverged,
                message: format!(
                    "Saved Build has {} run{}; this plan currently requires {}.",
                    op.persisted_runs,
                    if op.persisted_runs == 1 { "" } else { "s" },
                    op.node_runs
                ),
            });
        }
    }
    for boundary in boundaries {
        if boundary.resolution == MaterialBoundaryResolution::Unresolved {
            warnings.push(GraphWarning {
                graph_node_id: buy_slot_id(boundary.build_id, boundary.type_id),
                code: GraphWarningCode::LinkedBuildUnresolved,
                message: format!(
                    "component {} is set to Build but has no linked build yet",
                    boundary.type_id
                ),
            });
        }
    }
    for warning in &cost.warnings {
        warnings.push(cost_warning_to_graph_warning(warning, &op_by_index));
    }

    BuildGraphProjection {
        root,
        warnings,
        generated_at,
        // Filled by the caller (`IndustryService::build_graph`), same as
        // `project_build_graph`.
        market_evidence: Vec::new(),
    }
}

/// Everything `build_live_node` looks things up in.
struct LiveGraphIndex<'a> {
    op_by_index: &'a BTreeMap<u32, &'a VerificationOperationInput>,
    op_cost_by_index: &'a BTreeMap<u32, &'a OperationCostProjection>,
    boundaries_by_op: &'a BTreeMap<u32, Vec<&'a VerificationBoundaryInput>>,
    boundary_cost_by_traversal: &'a BTreeMap<u32, &'a BoundaryCostProjection>,
    boundary_by_traversal: &'a BTreeMap<u32, &'a VerificationBoundaryInput>,
    child_op_by_parent_and_type: &'a BTreeMap<(u32, i64), u32>,
    ctx: &'a GraphDisplayContext<'a>,
}

/// `required` / `net_required` are this node's incoming requirement as seen
/// by the rendering consumer; for an operation serving several demand
/// edges they are replaced by the aggregate over all of them.
fn build_live_node(
    op_index: u32,
    parent_build_id: Option<BuildId>,
    parent_component_type_id: Option<i64>,
    requirement: Option<(u64, u64)>,
    is_root: bool,
    index: &LiveGraphIndex<'_>,
    rendered: &mut std::collections::BTreeSet<u32>,
) -> ProductionNode {
    rendered.insert(op_index);
    // Invariant: every `op_index` reachable from the root (directly the
    // root itself, or via `child_op_by_parent_and_type`, which is built
    // from -- and therefore a subset of -- `op_by_index`'s own keys) is
    // present. `project_build_materials` never emits a dangling reference.
    let op = *index
        .op_by_index
        .get(&op_index)
        .expect("op_index reachable from the root is always present");
    let opcost = index.op_cost_by_index.get(&op_index).copied();
    let incoming_demands: Vec<GraphIncomingDemand> = if op.incoming.len() > 1 {
        op.incoming
            .iter()
            .filter_map(|demand| index.boundary_by_traversal.get(&demand.traversal_index))
            .map(|boundary| GraphIncomingDemand {
                dependency_id: boundary.dependency_id.clone(),
                consumer_build_id: boundary.build_id,
                required_quantity: boundary.api_required,
                net_required_quantity: boundary.api_shortage,
            })
            .collect()
    } else {
        Vec::new()
    };
    let (required_quantity, net_required_quantity) = if incoming_demands.is_empty() {
        (
            requirement.map(|(required, _)| required),
            requirement.map(|(_, net)| net),
        )
    } else {
        (
            Some(
                incoming_demands
                    .iter()
                    .map(|demand| demand.required_quantity)
                    .fold(0u64, u64::saturating_add),
            ),
            Some(
                incoming_demands
                    .iter()
                    .map(|demand| demand.net_required_quantity)
                    .fold(0u64, u64::saturating_add),
            ),
        )
    };

    let kind = match (is_root, op.activity) {
        (true, MaterialActivity::Manufacturing) => ProductionKind::RootManufacturing,
        (true, MaterialActivity::Reaction) => ProductionKind::RootReaction,
        (false, MaterialActivity::Manufacturing) => ProductionKind::Manufacturing,
        (false, MaterialActivity::Reaction) => ProductionKind::Reaction,
    };
    let recipe = match op.activity {
        MaterialActivity::Manufacturing => RecipeSelection::Manufacturing {
            blueprint_type_id: op.blueprint_or_formula_type_id,
        },
        MaterialActivity::Reaction => RecipeSelection::Reaction {
            reaction_formula_type_id: op.blueprint_or_formula_type_id,
        },
    };
    let producing_quantity = opcost.map_or_else(
        || op.node_runs.saturating_mul(op.output_per_run),
        |c| c.produced_quantity,
    );
    // Surplus is against what this node was actually asked to cover -- the
    // NET (post-inventory) demand that sized it (`net_required_quantity`),
    // never the gross `required_quantity`. A parent's on-hand inventory
    // already accounts for part of the gross requirement; comparing
    // production against the gross figure would understate (or, as here,
    // even go negative on) a genuine discrete-output surplus.
    let surplus = match net_required_quantity.or(required_quantity) {
        Some(net_required) => (producing_quantity as i64) - (net_required as i64),
        None => 0,
    };
    let (cost_state, estimated_cost, material_component_cost, own_installation_cost) = match opcost
    {
        Some(c) => {
            let stale = c
                .warnings
                .iter()
                .any(|warning| matches!(warning, CostWarning::StaleFreshPrice { .. }));
            let state = if !c.complete {
                CostState::Incomplete
            } else if stale {
                CostState::Stale
            } else {
                CostState::Known
            };
            (
                state,
                c.total_production_cost,
                c.material_component_cost,
                c.own_installation.total,
            )
        }
        None => (CostState::Incomplete, None, None, None),
    };

    let ctx = index.ctx;
    let mut children = Vec::new();
    if let Some(op_boundaries) = index.boundaries_by_op.get(&op_index) {
        for boundary in op_boundaries {
            let bcost = index
                .boundary_cost_by_traversal
                .get(&boundary.traversal_index)
                .copied();
            match boundary.resolution {
                MaterialBoundaryResolution::Buy => {
                    children.push(GraphChild::Acquisition(live_acquisition(
                        boundary, bcost, ctx,
                    )));
                }
                MaterialBoundaryResolution::Unresolved => {
                    children.push(GraphChild::UnresolvedBuild(UnresolvedBuildNode {
                        graph_node_id: buy_slot_id(boundary.build_id, boundary.type_id),
                        parent_build_id: boundary.build_id,
                        type_id: boundary.type_id,
                        type_name: boundary.type_name.clone(),
                        // Always `Some` for a genuinely `Unresolved` boundary
                        // (see `BoundaryVerification::intended_recipe`); the
                        // fallback is defensive only.
                        recipe: boundary.intended_recipe.unwrap_or(
                            RecipeSelection::Manufacturing {
                                blueprint_type_id: 0,
                            },
                        ),
                        required_quantity: boundary.api_required,
                        net_required_quantity: boundary.api_shortage,
                    }));
                }
                MaterialBoundaryResolution::Build | MaterialBoundaryResolution::Reaction => {
                    let child = if boundary.api_shortage == 0 {
                        None
                    } else {
                        index
                            .child_op_by_parent_and_type
                            .get(&(op_index, boundary.type_id))
                    };
                    match child {
                        Some(&child_op_index) if rendered.contains(&child_op_index) => {
                            // Already drawn in full under another consumer:
                            // an alias with this edge's own requirement only.
                            let child_op = index.op_by_index[&child_op_index];
                            children.push(GraphChild::ProducerReference(ProducerReferenceNode {
                                graph_node_id: child_op.graph_node_id.clone(),
                                build_id: child_op.build_id,
                                parent_build_id: boundary.build_id,
                                dependency_id: boundary.dependency_id.clone(),
                                type_id: boundary.type_id,
                                type_name: boundary.type_name.clone(),
                                kind: match child_op.activity {
                                    MaterialActivity::Manufacturing => {
                                        ProductionKind::Manufacturing
                                    }
                                    MaterialActivity::Reaction => ProductionKind::Reaction,
                                },
                                required_quantity: boundary.api_required,
                                net_required_quantity: boundary.api_shortage,
                            }));
                        }
                        Some(&child_op_index) => {
                            children.push(GraphChild::Production(build_live_node(
                                child_op_index,
                                Some(boundary.build_id),
                                Some(boundary.type_id),
                                Some((boundary.api_required, boundary.api_shortage)),
                                false,
                                index,
                                rendered,
                            )));
                        }
                        None => {
                            // `FullyCovered`: shortage 0, the allocator
                            // pruned the subtree -- no active production
                            // operation. Sourcing intent (Build/Reaction) is
                            // preserved distinctly from fulfillment outcome
                            // elsewhere (Materials/Worksheet); here, absent a
                            // dedicated node kind for "Build-intended but
                            // fulfilled from inventory", it renders with the
                            // existing satisfied-from-inventory visual model
                            // an `Acquisition` node already has (required ==
                            // covered, missing == 0) rather than inventing
                            // one -- never offered a "switch to
                            // Build" action, since it already is one.
                            children.push(GraphChild::Acquisition(live_acquisition(
                                boundary, bcost, ctx,
                            )));
                        }
                    }
                }
            }
        }
    }

    ProductionNode {
        graph_node_id: op.graph_node_id.clone(),
        build_id: op.build_id,
        parent_build_id,
        parent_component_type_id,
        type_id: op.product_type_id,
        type_name: op.product_name.clone(),
        kind,
        recipe,
        runs: op.node_runs,
        persisted_runs: op.persisted_runs,
        required_quantity,
        net_required_quantity,
        incoming_demands,
        producing_quantity,
        surplus,
        estimated_cost,
        material_component_cost,
        own_installation_cost,
        cost_state,
        recipe_currency: op.recipe_currency,
        effective_me: op.me,
        effective_te: op.te,
        children,
    }
}

fn buy_slot_id(parent_build_id: BuildId, type_id: i64) -> String {
    format!("buy:{}:{}", parent_build_id.0, type_id)
}

fn live_acquisition(
    boundary: &VerificationBoundaryInput,
    bcost: Option<&BoundaryCostProjection>,
    ctx: &GraphDisplayContext<'_>,
) -> AcquisitionNode {
    AcquisitionNode {
        graph_node_id: buy_slot_id(boundary.build_id, boundary.type_id),
        parent_build_id: boundary.build_id,
        type_id: boundary.type_id,
        type_name: boundary.type_name.clone(),
        required_quantity: boundary.api_required,
        missing_quantity: boundary.api_shortage,
        // Never offered on an already Build/Reaction-resolved (fully
        // covered) boundary -- only a genuine Buy leaf can be switched.
        buildable_recipe: (boundary.resolution == MaterialBoundaryResolution::Buy)
            .then(|| ctx.buildable_recipes.get(&boundary.type_id).copied())
            .flatten(),
        estimated_cost: bcost.and_then(|cost| cost.requirement_cost),
        cost_state: match bcost {
            Some(cost) if cost.complete => CostState::Known,
            Some(_) => CostState::Incomplete,
            None => CostState::Incomplete,
        },
        warning: None,
    }
}

fn cost_warning_to_graph_warning(
    warning: &CostWarning,
    op_by_index: &BTreeMap<u32, &VerificationOperationInput>,
) -> GraphWarning {
    let op_index = match *warning {
        CostWarning::MissingFreshPrice { op_index, .. }
        | CostWarning::MissingInventoryBasis { op_index, .. }
        | CostWarning::MissingAdjustedPrice { op_index, .. }
        | CostWarning::MissingSystemCostIndex { op_index }
        | CostWarning::NoFacilitySelected { op_index }
        | CostWarning::UnresolvedBuild { op_index, .. }
        | CostWarning::ChildCostIncomplete { op_index, .. }
        | CostWarning::StaleFreshPrice { op_index, .. }
        | CostWarning::ArithmeticOverflow { op_index } => op_index,
    };
    let graph_node_id = op_by_index
        .get(&op_index)
        .map_or_else(|| format!("op:{op_index}"), |op| op.graph_node_id.clone());
    let code = match warning {
        CostWarning::StaleFreshPrice { .. } => GraphWarningCode::StaleMarketEvidence,
        CostWarning::UnresolvedBuild { .. } => GraphWarningCode::LinkedBuildUnresolved,
        _ => GraphWarningCode::CostIncomplete,
    };
    GraphWarning {
        graph_node_id,
        code,
        message: cost_warning_message(warning),
    }
}

fn cost_warning_message(warning: &CostWarning) -> String {
    match warning {
        CostWarning::MissingFreshPrice { type_id, .. } => {
            format!("no fresh market/manual price is available for type {type_id}")
        }
        CostWarning::MissingInventoryBasis { type_id, .. } => {
            format!("no historical inventory basis is available for type {type_id}")
        }
        CostWarning::MissingAdjustedPrice { type_ids, .. } => format!(
            "no adjusted price is available for {} of this operation's base materials",
            type_ids.len()
        ),
        CostWarning::MissingSystemCostIndex { .. } => {
            "this operation's facility has no configured system cost index".to_string()
        }
        CostWarning::NoFacilitySelected { .. } => {
            "this operation has no facility selected".to_string()
        }
        CostWarning::UnresolvedBuild { type_id, .. } => {
            format!("component {type_id} is set to Build but has no linked build yet")
        }
        CostWarning::ChildCostIncomplete { child_op_index, .. } => {
            format!("child operation {child_op_index}'s own cost is incomplete")
        }
        CostWarning::StaleFreshPrice { type_id, .. } => {
            format!("the fresh price for type {type_id} is backed by stale market evidence")
        }
        CostWarning::ArithmeticOverflow { .. } => {
            "a cost calculation exceeded the supported range".to_string()
        }
    }
}
