//! The aggregate Materials output: one [`BuildMaterialsAggregate`] with its
//! rows, per-node allocations, provenance, warnings and verification
//! inputs. Plain data; the allocator that fills it is the parent module.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::industry::{BuildId, Money, PricingSelectionKind, RecipeCurrency, RecipeSelection};
use crate::FulfillmentScope;

/// One aggregate row: the whole tree's **external** demand for one `type_id`
/// (Buy leaves + unresolved provisional slots), and how much of the seeded
/// inventory this plan allocates to it. A Build/Reaction intermediate never
/// appears here -- its own boundary allocation lives in
/// [`NodeMaterialAllocation`], and its recipe inputs surface as their own
/// leaf rows.
///
/// **Every** requirement boundary the traversal visits produces a row --
/// covered, partial, or short; Buy leaf, Build/Reaction intermediate, or
/// provisional. Materials is the whole active plan's participating items, not
/// a shortage-only shopping list.
///
/// Invariants (per row): `required_quantity == allocated_quantity +
/// shortage_quantity`, `allocated_quantity <= available_quantity`,
/// `fully_covered == (shortage_quantity == 0)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateMaterialLine {
    pub type_id: i64,
    pub type_name: String,
    /// Σ demand for this type across every boundary it appears at (external
    /// leaf demand **and** Build/Reaction intermediate demand), **before**
    /// that boundary's own inventory allocation.
    pub required_quantity: u64,
    /// The seeded inventory for this type (`PlanningInventory::available`)
    /// -- free stock, after open Epics' reservations.
    pub available_quantity: u64,
    /// Held by open Epics' reservations and so not in
    /// `available_quantity` (`PlanningInventory::reserved`).
    pub reserved_quantity: u64,
    /// Σ planned reuse from inventory across those boundaries (never exceeds
    /// `available_quantity`).
    pub allocated_quantity: u64,
    /// `required_quantity - allocated_quantity` -- still to acquire (`Buy`) or
    /// produce (`Build`/`Reaction`) after inventory.
    pub shortage_quantity: u64,
    /// `shortage_quantity == 0`.
    pub fully_covered: bool,
    /// The sourcing strategy across this row's contributing boundaries.
    /// `Mixed` when the same type is sourced differently at different nodes
    /// (the per-node truth stays on [`NodeMaterialAllocation`] -- the
    /// aggregate never invents a single resolution).
    pub strategy: MaterialRowStrategy,
    /// True only when **every** contributing boundary is a provisional
    /// (unresolved Build/Reaction) slot -- the item is intended-but-unsourced.
    pub provisional: bool,
}

/// The sourcing strategy summarised onto an [`AggregateMaterialLine`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MaterialRowStrategy {
    Buy,
    Build,
    Reaction,
    /// This type is sourced differently at different nodes -- read
    /// [`BuildMaterialsAggregate::node_allocations`] for the breakdown.
    Mixed,
}

/// How one component-requirement boundary is sourced -- the discriminant for
/// reading a [`NodeMaterialAllocation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MaterialBoundaryResolution {
    /// External acquisition. `remaining` is external Materials demand.
    Buy,
    /// A linked manufacturing child. `remaining` is production demand handed
    /// to that child (0 ⇒ fully covered by inventory, subtree pruned).
    Build,
    /// A linked reaction child. Same `remaining` semantics as `Build`.
    Reaction,
    /// Build/Reaction chosen, but no linked `Build` exists yet. `remaining`
    /// is **provisional** external demand; the recipe inputs are not expanded.
    Unresolved,
}

/// Per production node, per type: how one component-requirement boundary was
/// allocated against planning inventory. Every boundary the traversal visits
/// gets one -- Buy leaves, unresolved slots, **and** intermediate
/// Build/Reaction boundaries (so a drill-down can explain "Composite Armor
/// Plate: required 2,852, all from inventory, built 0").
///
/// `allocated_quantity` is **planned use of pre-existing inventory** -- never
/// a reservation (that is a separate, persistent concept). Create Epic
/// freezes `reused_quantity = allocated_quantity`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeMaterialAllocation {
    pub build_id: BuildId,
    /// `root:<uuid>` for the tree root, `build:<uuid>` otherwise -- matches
    /// `crate::build_graph`'s node identity scheme.
    pub graph_node_id: String,
    /// `parent_component_type_id` chain from the root to this boundary's node
    /// (a provisional boundary's path ends with its own slot type). Empty for
    /// a root-owned boundary. Enough, with `build_id`, to place a boundary in
    /// the tree without a second walk.
    pub tree_path: Vec<i64>,
    pub type_id: i64,
    pub type_name: String,
    pub required_quantity: u64,
    /// Pre-existing inventory allocated to this boundary (`0` for `Full`).
    pub allocated_quantity: u64,
    /// `required_quantity - allocated_quantity`. For `Buy` / `Unresolved`
    /// this is the external shortfall; for `Build` / `Reaction` it is the
    /// production demand handed to the linked child (`0` ⇒ pruned).
    pub shortage_quantity: u64,
    /// The one fulfillment scope for this `(build_id, type_id)`. A single
    /// value is never lossy: `ComponentExpansionService::expand` merges a
    /// node's components by `type_id` (one row per type), so
    /// `apply_component_expansion` emits one `PlannedMaterialLine` per type,
    /// and a Build's `fulfillment_scopes` holds one scope per type. One
    /// contribution per `(build node, type_id)` therefore, always -- enforced
    /// by a `debug_assert!` in the accumulator.
    pub scope: FulfillmentScope,
    pub resolution: MaterialBoundaryResolution,
    /// True iff `resolution == Unresolved` (kept for wire compatibility with
    /// the leaf [`MaterialSource::provisional`]).
    pub provisional: bool,
    /// `Build` / `Reaction` boundary only: runs the child was dynamically
    /// projected at (`ceil(shortage_quantity / output_per_run)`), `0` for a
    /// pruned or non-production boundary.
    pub child_runs: u64,
    /// `Build` / `Reaction` boundary only: the linked child's own captured
    /// recipe primary-product `quantity_per_run` -- the authoritative
    /// per-run yield used to size `child_runs`
    /// (`ceil(shortage_quantity / output_per_run)`). `0` for a `Buy` /
    /// `Unresolved` leaf boundary (no production recipe). Recorded here so a
    /// consumer (the verification export's Calculation Audit sheet, a future
    /// drill-down) can re-derive `child_runs` / `produced_quantity` /
    /// `surplus_quantity` from a non-circular input rather than from the
    /// numbers it is meant to check.
    pub output_per_run: u64,
    /// `child_runs * output_per_run` -- what building that many runs yields.
    pub produced_quantity: u64,
    /// `produced_quantity - shortage_quantity` -- discrete-run overproduction.
    /// Recorded as planning evidence only; **never** returned to the pool and
    /// **never** consumed by another branch. For a producer serving several
    /// demand edges (canonical producers), the operation's whole surplus is
    /// attributed to exactly one of its incoming edges and every other one
    /// reports `0`, so summing never double-counts it.
    pub surplus_quantity: u64,
    /// The demand edge this boundary is --
    /// `pd:<production_dependencies.id>` on a canonical root,
    /// `dep:<consumer build>:<component type>` (the edge's natural key) for
    /// an edge not yet persisted. `(build_id, type_id)` is that natural key: in the
    /// producer DAG a consumer is planned once, so one (consumer, component)
    /// is one requirement, never one tree position.
    pub dependency_id: String,
    /// The producer Build satisfying this edge (`Build` / `Reaction`
    /// boundaries with a producer), `None` otherwise.
    pub producer_build_id: Option<BuildId>,
}

/// One external-input demand contribution from a single leaf of the tree --
/// retained provenance for a future drill-down ("Tritanium 14,000 ← Thrasher
/// build 10,000 + …"). One per `(node, type_id)` leaf contribution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialSource {
    /// The node that owns this requirement.
    pub build_id: BuildId,
    /// `root:<uuid>` for the tree root, `build:<uuid>` for any other node --
    /// matches `crate::build_graph`'s node identity scheme.
    pub graph_node_id: String,
    pub type_id: i64,
    pub type_name: String,
    pub required_quantity: u64,
    pub allocated_quantity: u64,
    pub shortage_quantity: u64,
    pub scope: FulfillmentScope,
    /// True when this demand is for a Build/React component whose linked
    /// `Build` does not exist yet: the intended item is surfaced as demand,
    /// but its own recipe inputs are deliberately **not** expanded.
    pub provisional: bool,
    /// `parent_component_type_id` chain from the root down to the node that
    /// owns this contribution. Empty for a root-owned line. For a provisional
    /// contribution the final entry is the unresolved slot's own component
    /// `type_id`.
    pub tree_path: Vec<i64>,
}

/// Why a non-fatal issue was recorded while folding. Missing authoritative
/// quantities are **not** here -- they are a hard [`BuildMaterialsError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MaterialsAggregateWarningCode {
    /// A Build/React component of this node has no linked `Build` yet -- its
    /// intended item is surfaced as provisional demand (drawn from the pool
    /// only when its scope is `Missing`), but its recipe inputs are not
    /// expanded.
    UnresolvedBuild,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialsAggregateWarning {
    pub code: MaterialsAggregateWarningCode,
    pub build_id: BuildId,
    pub type_id: Option<i64>,
    pub message: String,
}

/// The fold could not produce a complete, authoritative aggregate.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BuildMaterialsError {
    /// One or more walked build nodes had a demand-contributing component
    /// with no authoritative [`NodePlanMaterials`](super::NodePlanMaterials) line (the whole node's
    /// materials were absent, or a specific component line was). Raw
    /// `expansion` quantities are post-ME/facility-*un*adjusted for
    /// descendants, so they are never substituted; the projection fails
    /// instead. `missing_nodes` is ascending by `BuildId` and deduplicated.
    #[error(
        "authoritative per-node plan materials are missing for {} build node(s); \
         the aggregate cannot be projected without them",
        missing_nodes.len()
    )]
    NodeRevisionUnavailable { missing_nodes: Vec<BuildId> },
}

/// Manufacturing vs reaction -- the discriminant an independent
/// reconstruction of `required_quantity` branches on (reactions have no
/// blueprint ME, and a Refinery gives no base material discount).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MaterialActivity {
    Manufacturing,
    Reaction,
}

/// The **primitive calculation inputs** for one requirement boundary, in
/// exact allocator traversal order -- enough for an external model (the
/// verification export's Quantity Audit sheet) to independently reconstruct
/// `required_quantity`, the planning-inventory allocation, shortage, child
/// runs, produced and surplus, and compare each against ISKWorks' own value.
///
/// Every quantity here is a *primitive* (SDE base quantity, run count,
/// blueprint ME, the combined facility material factor, the one-time
/// inventory seed) or an authoritative ISKWorks output copied verbatim for
/// side-by-side comparison -- never a value derived by rearranging other
/// outputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationBoundaryInput {
    /// 0-based position in the allocator's deterministic DFS pre-order
    /// (root Buy leaves ascending `type_id`, then root Build/Reaction
    /// boundaries ascending `type_id`, recursing into each surviving child).
    pub traversal_index: u32,
    /// The `traversal_index` of the parent Build/Reaction boundary that
    /// spawned this node, or `None` for a root-owned boundary. Lets an
    /// external model feed a child node's `runs` from the parent boundary's
    /// *reconstructed* child-run count rather than the API's.
    pub parent_traversal_index: Option<u32>,
    /// The [`VerificationOperationInput::op_index`] of the production node
    /// (operation) that owns this boundary -- joins a ledger row to its
    /// operation sheet.
    pub op_index: u32,
    pub build_id: BuildId,
    /// `root:<uuid>` / `build:<uuid>` -- matches `crate::build_graph`.
    pub graph_node_id: String,
    pub tree_path: Vec<i64>,
    pub type_id: i64,
    pub type_name: String,
    pub activity: MaterialActivity,
    pub resolution: MaterialBoundaryResolution,
    pub scope: FulfillmentScope,
    /// Runs the owning node was projected at (root runs, or a child's
    /// dynamically-sized `ceil(remaining / output_per_run)`).
    pub node_runs: u64,
    /// SDE recipe base quantity of this component per run (pre-ME,
    /// pre-facility).
    pub base_quantity_per_run: u64,
    /// Blueprint material efficiency 0..=10 (`0` for a reaction).
    pub blueprint_me: u8,
    /// `(1 - structure_material_reduction%/100) * Π applicable-rig
    /// (1 - rig_material_reduction%/100)`; exactly `1` when the plan used no
    /// facility. Blueprint ME is applied separately, on top of this.
    #[serde(with = "rust_decimal::serde::str")]
    pub facility_material_factor: Decimal,
    /// The linked child's captured-recipe primary-product `quantity_per_run`
    /// (`0` for a Buy / unresolved leaf).
    pub output_per_run: u64,
    /// The one-time `PlanningInventory` seed for this `type_id` -- the same
    /// snapshot value the traversal drew down; identical across every
    /// boundary of the same type.
    pub starting_inventory: u64,
    // --- authoritative ISKWorks outputs, copied for comparison ---------
    pub api_required: u64,
    pub api_planned_use: u64,
    pub api_shortage: u64,
    pub api_child_runs: u64,
    pub api_produced: u64,
    pub api_surplus: u64,
    // --- fresh-price evidence for the cost enrichment ------------------
    /// This boundary's resolved fresh unit price for its `type_id` -- the
    /// same market / manual-price-list / per-item-override price the node's
    /// own [`BuildPlanRevision`](crate::industry::BuildPlanRevision) resolved, at that node's dynamic run count.
    /// `None` for a boundary with no resolved price (a `Build` / `Reaction`
    /// boundary -- which is costed from its child instead -- or a genuinely
    /// unpriced `Buy`). Never a re-fetch: copied verbatim from the revision.
    pub fresh_unit_price: Option<Money>,
    /// How the fresh price was selected (`Default` market, an explicit market
    /// policy, or a manual override). `Default` when no snapshot line matched.
    pub fresh_price_selection: PricingSelectionKind,
    /// The resolved market policy when `fresh_price_selection` is an
    /// explicit policy. Kept separate from the diagnostic provenance note.
    pub fresh_pricing_policy: Option<crate::MarketPricingPolicy>,
    /// The provenance note the price resolution recorded (formula version,
    /// covered / marginal quantities, order count, observed timestamp, and a
    /// staleness marker). Empty when no snapshot line matched.
    pub fresh_price_note: String,
    /// `true` when `fresh_price_note` carries the market-staleness marker --
    /// the price is still usable (*complete but flagged*), matching the
    /// existing pricing-completeness convention.
    pub fresh_price_stale: bool,
    /// The market region the fresh price was evaluated against.
    pub market_region_id: Option<i64>,
    /// The market location (station / structure) the fresh price was
    /// evaluated against, or `None` for a region-wide scope.
    pub market_location_id: Option<i64>,
    /// The recipe an `Unresolved` boundary intends to build with -- see
    /// [`BoundaryVerification::intended_recipe`](super::BoundaryVerification::intended_recipe). `None` for every other
    /// boundary kind.
    pub intended_recipe: Option<RecipeSelection>,
    /// The demand edge this boundary is -- see
    /// [`NodeMaterialAllocation::dependency_id`].
    pub dependency_id: String,
    /// The producer Build satisfying this edge, if any.
    pub producer_build_id: Option<BuildId>,
}

/// One demand edge a production operation serves -- see
/// [`VerificationOperationInput::incoming`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationIncomingDemand {
    /// The consuming boundary's `traversal_index`.
    pub traversal_index: u32,
    /// The consuming operation's `op_index`.
    pub consumer_op_index: u32,
    /// The demand edge (see [`NodeMaterialAllocation::dependency_id`]).
    pub dependency_id: String,
}

/// One walked **production node** (operation) of the Build tree -- the
/// per-node state the verification workbook's `Operations` sheet and its
/// one-sheet-per-operation model need, read straight off that node's own
/// authoritative `BuildPlanRevision` during the same allocating traversal.
/// Emitted only on the verification-export path; never on the ordinary
/// Materials projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationOperationInput {
    /// 0-based, assigned in the order nodes are entered by the DFS
    /// (`0` = root). The stable join key from a boundary
    /// ([`VerificationBoundaryInput::op_index`]) to its operation.
    pub op_index: u32,
    /// `op_index` of the parent operation, or `None` for the root.
    pub parent_op_index: Option<u32>,
    /// The `traversal_index` of the parent Build/Reaction boundary that
    /// spawned this operation, or `None` for the root. Mirrors
    /// [`VerificationBoundaryInput::parent_traversal_index`] -- this is the
    /// **forward** link from operation to spawning boundary. It is stored
    /// directly because searching this operation's own boundaries for a
    /// matching `parent_traversal_index` silently finds nothing for a pooled
    /// sibling operation, which owns no boundaries of its own
    /// (`IndustryService::register_production_operation`'s own doc comment:
    /// only the pooled representative's own materials are walked).
    pub parent_traversal_index: Option<u32>,
    /// Every demand edge this operation
    /// serves, in the order the planner recorded them (ascending
    /// `traversal_index`). One entry for a single-consumer occurrence (its
    /// spawning boundary), none for the root, and one per consuming edge for
    /// a canonical producer -- one operation, many consumers. When non-empty,
    /// `parent_op_index` / `parent_traversal_index` are its first entry
    /// (kept for tree-shaped consumers). The authoritative consumer set:
    /// cost attribution, Stages consumers/stages, Graph and the workbook
    /// read this, never one parent per operation.
    pub incoming: Vec<OperationIncomingDemand>,
    /// `root:<uuid>` / `build:<uuid>` -- matches `crate::build_graph`. The
    /// stable per-operation identity (embeds the immutable Build uuid). On a
    /// canonical root this is the **one authoritative ProductionOperation
    /// identity** per active canonical producer (`build:<producer uuid>`):
    /// every projection (cost, Stages, Graph, workbook, Epic freeze) keys the
    /// operation by it, never by a consumer occurrence.
    pub graph_node_id: String,
    pub build_id: BuildId,
    /// This node's own **persisted** `Build.revision` at read time -- the
    /// optimistic-concurrency token a Stages descendant-configuration edit
    /// must echo back per member (see `crate::execution_plan::ExecutionOccurrence::revision`'s
    /// own doc comment). Never the plan's own quantity/cost authority.
    pub revision: u64,
    /// `parent_component_type_id` chain from the root to this node.
    pub tree_path: Vec<i64>,
    pub activity: MaterialActivity,
    pub product_type_id: i64,
    pub product_name: String,
    /// The recipe's primary-product `quantity_per_run`.
    pub output_per_run: u64,
    /// Number of direct recipe components (`recipe.materials().len()`).
    pub base_material_count: u32,
    /// Manufacturing blueprint `type_id` or reaction-formula `type_id`.
    pub blueprint_or_formula_type_id: i64,
    pub blueprint_or_formula_name: String,
    /// Runs this node was projected at (root: overlay runs; child:
    /// `ceil(remaining / output_per_run)`).
    pub node_runs: u64,
    /// This node's own **persisted** `Build.runs` -- the saved, standalone
    /// configuration, distinct from `node_runs` (the effective runs *this*
    /// parent overlay currently requires). Equal to `node_runs` for the
    /// root (there is no parent to size it against) and may legitimately
    /// diverge for a linked child whose saved runs are stale relative to
    /// the current plan. Informational only -- never the quantity/cost
    /// authority (see `crate::build_graph`'s live projection).
    pub persisted_runs: u64,
    /// This node's own `Build.recipe_currency` -- whether its captured
    /// recipe still matches the active SDE. Display-only evidence for the
    /// Graph presentation; never affects quantity/cost.
    pub recipe_currency: RecipeCurrency,
    /// Blueprint material efficiency; `None` for a reaction.
    pub me: Option<u8>,
    /// Blueprint time efficiency; `None` for a reaction.
    pub te: Option<u8>,
    /// This node's own **persisted** `Build.draft_planning.input.blueprint_selection`
    /// -- `None` for a reaction (no blueprint selection concept) or an
    /// unresearched manufacturing Build. Stages descendant-configuration
    /// editing needs the raw selection (mode/kind/licensed runs/notes),
    /// not just the already-resolved `me`/`te` above, so an ME/TE-only edit
    /// never silently drops an unrelated field (e.g. `licensedRuns` on a
    /// BPC) it was never shown. Display/edit-seed evidence only -- never
    /// the quantity/cost authority (`me`/`te` above already carry the
    /// resolved values every calculation uses).
    pub blueprint_selection: Option<crate::BlueprintSelection>,
    // --- facility (all `None` when the node used no facility) ----------
    pub facility_id: Option<Uuid>,
    pub facility_name: Option<String>,
    pub structure_type: Option<String>,
    pub solar_system: Option<String>,
    /// `IndustryFacilityProfile::material_reduction_percent` -- the
    /// *structure's* base material discount (always `0` for a reaction; a
    /// Refinery gives no base reaction discount).
    #[serde(with = "rust_decimal::serde::str")]
    pub structure_material_reduction_percent: Decimal,
    /// `IndustryFacilityProfile::time_reduction_percent`.
    #[serde(with = "rust_decimal::serde::str")]
    pub structure_time_reduction_percent: Decimal,
    /// The combined structure x applicable-rig material multiplier actually
    /// applied to this node's requirements (`1` with no facility) -- the same
    /// value carried on every boundary's `facility_material_factor`.
    #[serde(with = "rust_decimal::serde::str")]
    pub effective_material_factor: Decimal,
    // --- installation-cost primitives for the cost enrichment ----------
    /// `IndustryFacilityProfile::revision` of the facility this operation used
    /// (`None` when the operation selected no facility).
    pub facility_profile_revision: Option<u64>,
    /// `IndustryFacilityProfile::manual_system_cost_index` -- the manually
    /// configured system cost index. `None` blocks the installation total
    /// (never substituted with zero).
    #[serde(with = "rust_decimal::serde::str_option")]
    pub system_cost_index: Option<Decimal>,
    /// `IndustryFacilityProfile::job_cost_reduction_percent` (`0` when no
    /// facility).
    #[serde(with = "rust_decimal::serde::str")]
    pub job_cost_reduction_percent: Decimal,
    /// `IndustryFacilityProfile::facility_tax_percent` (`0` when no facility).
    #[serde(with = "rust_decimal::serde::str")]
    pub facility_tax_percent: Decimal,
    /// `IndustryFacilityProfile::scc_surcharge_percent` (`0` when no facility).
    #[serde(with = "rust_decimal::serde::str")]
    pub scc_surcharge_percent: Decimal,
    /// `IndustryFacilityProfile::alliance_surcharge_percent` (`0` when no
    /// facility).
    #[serde(with = "rust_decimal::serde::str")]
    pub alliance_surcharge_percent: Decimal,
    /// `IndustryFacilityProfile::fixed_supplemental_cost` (`0` when no
    /// facility).
    pub fixed_supplemental_cost: Money,
    /// How many industry jobs `node_runs` splits into
    /// (`crate::JobSplit` at the blueprint's per-job run limit): the fixed
    /// supplemental cost is charged once per job.
    pub job_count: u64,
    /// `InstallationCostBreakdown::formula_version` of the facility layer that
    /// resolved this operation (empty when no facility).
    pub installation_formula_version: String,
}

/// One type's slice of the **single** `list_balances` snapshot that seeded
/// [`PlanningInventory`](super::PlanningInventory) -- retained for the verification workbook's
/// `Materials` sheet so its inventory-basis columns cost no extra read.
/// `unit_basis` is the weighted-average historical unit cost
/// (`InventoryBalance::average_unit_cost`); `total_basis` is the persistent
/// `total_historical_cost` for the current balance. No FIFO / lots -- the
/// balance model is a single running total by design.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryBasisEntry {
    pub type_id: i64,
    /// Physical on-hand quantity.
    pub quantity: u64,
    /// Held by open Epics' active reservations; the planning pool was
    /// seeded with `quantity - reserved_quantity` (floored at 0).
    pub reserved_quantity: u64,
    pub unit_basis: Option<Decimal>,
    pub total_basis: Decimal,
}

/// The full result of [`MaterialsAccumulator::finish`](super::MaterialsAccumulator::finish).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildMaterialsAggregate {
    /// Aggregate totals, one row per `type_id`, ascending by `type_id`.
    pub lines: Vec<AggregateMaterialLine>,
    /// Per-node allocations, ascending by `(build_id, type_id)`.
    pub node_allocations: Vec<NodeMaterialAllocation>,
    /// Every leaf contribution, in deterministic DFS pre-order.
    pub sources: Vec<MaterialSource>,
    /// Non-fatal issues found while folding.
    pub warnings: Vec<MaterialsAggregateWarning>,
    /// Primitive calculation inputs per boundary, in traversal order --
    /// consumed only by the verification export.
    pub verification_inputs: Vec<VerificationBoundaryInput>,
    /// One entry per walked production node, in DFS-entry order --
    /// consumed only by the verification export.
    pub verification_operations: Vec<VerificationOperationInput>,
}

impl BuildMaterialsAggregate {
    /// The aggregate row for `type_id`, if any.
    #[must_use]
    pub fn line(&self, type_id: i64) -> Option<&AggregateMaterialLine> {
        self.lines.iter().find(|line| line.type_id == type_id)
    }

    /// One node's allocation for `type_id`, if any.
    #[must_use]
    pub fn node_allocation(
        &self,
        build_id: BuildId,
        type_id: i64,
    ) -> Option<&NodeMaterialAllocation> {
        self.node_allocations
            .iter()
            .find(|allocation| allocation.build_id == build_id && allocation.type_id == type_id)
    }

    /// `(build_id_uuid, type_id) -> allocation` -- the stable map Create Epic
    /// consumes to freeze `reused_quantity` /
    /// `fresh_quantity` from **one** whole-tree allocation. The key's first
    /// element is [`BuildId`]'s inner `Uuid`.
    #[must_use]
    pub fn node_allocation_map(&self) -> BTreeMap<(Uuid, i64), &NodeMaterialAllocation> {
        self.node_allocations
            .iter()
            .map(|allocation| ((allocation.build_id.0, allocation.type_id), allocation))
            .collect()
    }
}
