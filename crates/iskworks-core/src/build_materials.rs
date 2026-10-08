//! Whole-tree **planning inventory allocator** primitives for the aggregate
//! Materials view.
//!
//! This module answers "what external inputs does the *entire* selected
//! production tree still need, and how much of the current inventory can one
//! coherent plan actually reuse -- counted once?".
//!
//! ## What lives here (pure, sync, no I/O)
//!
//! * [`PlanningInventory`] -- an **ephemeral, in-memory** pool. Seeded once
//!   from a snapshot of current inventory and drawn down exactly once as the
//!   traversal visits each component-requirement boundary. It is **not** a
//!   reservation: nothing is persisted, nothing is locked, no
//!   `inventory_balances` / `inventory_allocations` row is touched, and it
//!   never underflows.
//! * [`allocate`] -- the single boundary primitive: split one requirement
//!   into `(allocated_from_inventory, remaining)` given its
//!   [`FulfillmentScope`]. `Full` never draws the pool.
//! * [`MaterialsAccumulator`] -- collects boundary records into a
//!   [`BuildMaterialsAggregate`]: external-leaf totals by `type_id`, a
//!   per-`(build_id, type_id)` [`NodeMaterialAllocation`] (including
//!   intermediate Build/Reaction boundaries with their `child_runs` /
//!   `produced_quantity` / `surplus_quantity`), and leaf provenance
//!   ([`MaterialSource`]).
//!
//! ## What does NOT live here
//!
//! The **allocating traversal** itself. Sizing a Build/Reaction child to its
//! post-allocation production demand needs a fresh authoritative
//! `BuildPlanRevision` at a dynamically chosen run count, which is
//! async/DB-backed (`preview_plan_inner`). That traversal is
//! [`crate::industry::IndustryService::project_build_materials`]; it drives
//! these primitives. Keeping it out of here keeps this module a set of pure,
//! exhaustively-unit-testable pieces rather than a fake-pure callback shell.
//!
//! ## Identity
//!
//! `graph_node_id` strings (`root:<uuid>` / `build:<uuid>`) match
//! `crate::build_graph`'s scheme. Per-`(build_id, type_id)` there is exactly
//! one contribution (a node's component expansion is merged by `type_id`).
//!
//! ## Authoritative quantities
//!
//! Every quantity comes from a node's own [`BuildPlanRevision::material_lines`]
//! (via [`NodePlanMaterials`]) -- post ME / facility / rig, at the run count
//! that node was actually projected at. Raw single-level `expansion`
//! quantities (pre-modifier for descendants) are **never** substituted: a
//! demand-contributing component with no authoritative line fails the whole
//! projection ([`BuildMaterialsError::NodeRevisionUnavailable`]). Each line's
//! own `reused_quantity` / `missing_quantity` is ignored -- allocation is
//! recomputed from [`PlanningInventory`] alone.
//!
//! ## Result
//!
//! [`MaterialsAccumulator::finish`] returns `Result<BuildMaterialsAggregate,
//! BuildMaterialsError>`. An `Ok` value is **always complete and
//! authoritative**. **Every** requirement boundary the traversal records
//! produces an [`AggregateMaterialLine`] -- covered, partial, or short; Buy
//! leaf, Build/Reaction intermediate (whose subtree may still be pruned when
//! fully covered), or provisional. Materials is the whole active plan's
//! participating items, not a shortage-only list.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::industry::{
    BuildId, BuildPlanRevision, Money, PlannedMaterialLine, PricingSelectionKind, RecipeCurrency,
    RecipeSelection,
};
use crate::{FulfillmentScope, FulfillmentScopeOverride};

// ---------------------------------------------------------------------------
// PlanningInventory
// ---------------------------------------------------------------------------

/// An ephemeral, calculation-scoped inventory pool. Seeded once from a
/// snapshot of current stock, then drawn down exactly once across a single
/// Build-tree traversal so the same physical unit is never planned for reuse
/// twice.
///
/// **Not a reservation.** Purely in-memory: no persistence, no locking, no
/// `inventory_events` / `inventory_balances` / `inventory_allocations`
/// interaction, no `reserved_quantity`. Discard it when the calculation ends.
#[derive(Debug, Clone, Default)]
pub struct PlanningInventory {
    /// The immutable seed: `type_id -> quantity available at calculation start`.
    available_by_type: BTreeMap<i64, u64>,
    /// `type_id -> quantity still un-allocated`, decremented by [`Self::take`].
    remaining_by_type: BTreeMap<i64, u64>,
    /// `type_id -> quantity held by open Epics' reservations`, excluded from
    /// the seed. Informational only: never drawn from.
    reserved_by_type: BTreeMap<i64, u64>,
}

impl PlanningInventory {
    /// Seed the pool from `type_id -> available_quantity` pairs. Duplicate
    /// `type_id`s are summed (`saturating_add`). `remaining` starts equal to
    /// `available`.
    #[must_use]
    pub fn seed(available: impl IntoIterator<Item = (i64, u64)>) -> Self {
        let mut available_by_type: BTreeMap<i64, u64> = BTreeMap::new();
        for (type_id, quantity) in available {
            let entry = available_by_type.entry(type_id).or_insert(0);
            *entry = entry.saturating_add(quantity);
        }
        let remaining_by_type = available_by_type.clone();
        Self {
            available_by_type,
            remaining_by_type,
            reserved_by_type: BTreeMap::new(),
        }
    }

    /// Seed the pool with free stock from `(type_id, physical, reserved)`
    /// triples: each type's available quantity is `physical - reserved`
    /// (floored at `0`), and `reserved` is remembered for
    /// [`Self::reserved`]. Duplicate `type_id`s are summed.
    #[must_use]
    pub fn seed_free(stock: impl IntoIterator<Item = (i64, u64, u64)>) -> Self {
        let mut physical_by_type: BTreeMap<i64, u64> = BTreeMap::new();
        let mut reserved_by_type: BTreeMap<i64, u64> = BTreeMap::new();
        for (type_id, physical, reserved) in stock {
            let entry = physical_by_type.entry(type_id).or_insert(0);
            *entry = entry.saturating_add(physical);
            if reserved > 0 {
                let entry = reserved_by_type.entry(type_id).or_insert(0);
                *entry = entry.saturating_add(reserved);
            }
        }
        let mut pool = Self::seed(physical_by_type.iter().map(|(&type_id, &physical)| {
            let reserved = reserved_by_type.get(&type_id).copied().unwrap_or(0);
            (type_id, physical.saturating_sub(reserved))
        }));
        pool.reserved_by_type = reserved_by_type;
        pool
    }

    /// Draw up to `requested` units of `type_id` from what remains. Returns
    /// the amount actually taken (`min(requested, remaining)`), decrementing
    /// `remaining` by exactly that much. A `type_id` that was never seeded
    /// yields `0`. Never underflows, never panics.
    pub fn take(&mut self, type_id: i64, requested: u64) -> u64 {
        match self.remaining_by_type.get_mut(&type_id) {
            Some(remaining) => {
                let taken = (*remaining).min(requested);
                *remaining -= taken;
                taken
            }
            None => 0,
        }
    }

    /// Units of `type_id` still un-allocated (`0` if never seeded).
    #[must_use]
    pub fn remaining(&self, type_id: i64) -> u64 {
        self.remaining_by_type.get(&type_id).copied().unwrap_or(0)
    }

    /// The original seeded quantity for `type_id` (`0` if never seeded) --
    /// what the aggregate reports as `available_quantity`.
    #[must_use]
    pub fn available(&self, type_id: i64) -> u64 {
        self.available_by_type.get(&type_id).copied().unwrap_or(0)
    }

    /// Units of `type_id` held by open Epics and excluded from the seed
    /// (`0` when seeded with [`Self::seed`]) -- what the aggregate reports
    /// as `reserved_quantity`.
    #[must_use]
    pub fn reserved(&self, type_id: i64) -> u64 {
        self.reserved_by_type.get(&type_id).copied().unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Per-node input (the slice of a BuildPlanRevision the fold consumes)
// ---------------------------------------------------------------------------

/// The only part of one node's [`BuildPlanRevision`] the aggregator reads:
/// authoritative post-ME / facility / rig material quantities, plus each
/// line's fulfillment scope (which the revision itself does not carry -- the
/// caller resolves it from that node's
/// `draft_planning.input.fulfillment_scopes`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodePlanMaterials {
    pub lines: Vec<NodePlanMaterialLine>,
}

/// One authoritative material requirement of a single production node.
///
/// A Build/React component of the node has a line here too (with
/// `is_build_resolved == true`) -- its `total_quantity` and `scope` are read
/// only when that component has no linked child yet and its intended item is
/// surfaced as provisional demand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodePlanMaterialLine {
    pub type_id: i64,
    pub type_name: String,
    /// Post-modifier full requirement -- `PlannedMaterialLine::total_quantity`.
    pub total_quantity: u64,
    /// Mirrors `PlannedMaterialLine::is_build_resolved`. Only used to
    /// classify components when the tree's own `expansion` is empty
    /// (a descendant whose expansion could not be computed); otherwise the
    /// tree's `expansion` is authoritative for Buy-vs-Build.
    pub is_build_resolved: bool,
    /// `Missing` (draw from the pool) or `Full` (ignore inventory, source
    /// fresh -- contributes to `required` but never draws). Preserved through
    /// to the provisional demand of an unresolved Build/React slot: a `Full`
    /// unresolved slot never consumes the pool.
    pub scope: FulfillmentScope,
}

impl NodePlanMaterials {
    /// Build from a node's `BuildPlanRevision` and **that same node's**
    /// `fulfillment_scopes` (from `build.draft_planning.input`). A line with
    /// no override is `Missing` (the default). The caller owns pairing the
    /// right node's scopes with the right node's revision -- the fold trusts
    /// whatever scope arrives on the line, including for an unresolved Build
    /// slot.
    #[must_use]
    pub fn from_revision(
        revision: &BuildPlanRevision,
        fulfillment_scopes: &[FulfillmentScopeOverride],
    ) -> Self {
        let scope_by_type: HashMap<i64, FulfillmentScope> = fulfillment_scopes
            .iter()
            .map(|override_| (override_.type_id, override_.scope))
            .collect();
        Self {
            lines: revision
                .material_lines
                .iter()
                .map(|line| NodePlanMaterialLine::from_line(line, &scope_by_type))
                .collect(),
        }
    }
}

impl NodePlanMaterialLine {
    fn from_line(
        line: &PlannedMaterialLine,
        scope_by_type: &HashMap<i64, FulfillmentScope>,
    ) -> Self {
        Self {
            type_id: line.type_id,
            type_name: line.type_name.clone(),
            total_quantity: line.total_quantity,
            is_build_resolved: line.is_build_resolved,
            scope: scope_by_type
                .get(&line.type_id)
                .copied()
                .unwrap_or(FulfillmentScope::Missing),
        }
    }
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

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
    /// with no authoritative [`NodePlanMaterials`] line (the whole node's
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
    /// own [`BuildPlanRevision`] resolved, at that node's dynamic run count.
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
    /// [`BoundaryVerification::intended_recipe`]. `None` for every other
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
/// [`PlanningInventory`] -- retained for the verification workbook's
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

/// The full result of [`MaterialsAccumulator::finish`].
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

// ---------------------------------------------------------------------------
// Boundary allocation primitive
// ---------------------------------------------------------------------------

/// Split one component requirement into `(allocated_from_inventory,
/// remaining)`. A `Full`-scoped requirement contributes to `required` but
/// **never draws** the pool. `Missing` (the default) takes `min(required,
/// remaining_in_pool)`.
///
/// This is the one place inventory is consulted for a boundary -- the
/// traversal calls it at **every** component-requirement boundary (a node's
/// own Buy leaves, its unresolved slots, and every intermediate Build/Reaction
/// boundary), in deterministic DFS pre-order, so the same physical unit is
/// never planned for reuse twice.
pub fn allocate(
    inventory: &mut PlanningInventory,
    type_id: i64,
    required: u64,
    scope: FulfillmentScope,
) -> (u64, u64) {
    match scope {
        FulfillmentScope::Full => (0, required),
        FulfillmentScope::Missing => {
            let allocated = inventory.take(type_id, required);
            (allocated, required.saturating_sub(allocated))
        }
    }
}

// ---------------------------------------------------------------------------
// MaterialsAccumulator -- collects boundary records into the aggregate
// ---------------------------------------------------------------------------

/// The identifying context of one boundary the traversal is recording.
pub struct BoundaryRecord<'a> {
    pub build_id: BuildId,
    /// `root:<uuid>` for the tree root, `build:<uuid>` otherwise.
    pub graph_node_id: &'a str,
    /// `parent_component_type_id` chain from the root to this boundary's node
    /// (a provisional boundary's path already ends with its own slot type).
    pub tree_path: &'a [i64],
    pub type_id: i64,
    pub type_name: &'a str,
    pub scope: FulfillmentScope,
    pub required: u64,
    /// From [`allocate`].
    pub allocated: u64,
    /// From [`allocate`] (`required - allocated`).
    pub remaining: u64,
    /// The demand edge; `None` derives the natural key
    /// (`dep:<build_id>:<type_id>`).
    pub dependency_id: Option<&'a str>,
    /// The producer satisfying the edge, if any.
    pub producer_build_id: Option<BuildId>,
}

impl BoundaryRecord<'_> {
    fn dependency_id(&self) -> String {
        self.dependency_id.map_or_else(
            || crate::canonical_planner::natural_dependency_id(self.build_id, self.type_id),
            str::to_string,
        )
    }
}

/// The primitive-input evidence the traversal captures for one boundary so
/// [`MaterialsAccumulator`] can emit a [`VerificationBoundaryInput`]. Passed
/// as `Some(_)` only on the verification-export path; `None` (the default)
/// on the ordinary Materials projection, which never builds
/// `verification_inputs`.
#[derive(Debug, Clone)]
pub struct BoundaryVerification {
    pub activity: MaterialActivity,
    pub node_runs: u64,
    pub base_quantity_per_run: u64,
    pub blueprint_me: u8,
    pub facility_material_factor: Decimal,
    pub starting_inventory: u64,
    pub parent_traversal_index: Option<u32>,
    /// The `op_index` of the production node (operation) this boundary
    /// belongs to -- the join key between a ledger row and its operation
    /// sheet.
    pub op_index: u32,
    /// Resolved fresh unit price + provenance for this boundary's `type_id`,
    /// read verbatim from the owning node's [`BuildPlanRevision`]. See the
    /// matching fields on [`VerificationBoundaryInput`].
    pub fresh_unit_price: Option<Money>,
    pub fresh_price_selection: PricingSelectionKind,
    pub fresh_pricing_policy: Option<crate::MarketPricingPolicy>,
    pub fresh_price_note: String,
    pub fresh_price_stale: bool,
    pub market_region_id: Option<i64>,
    pub market_location_id: Option<i64>,
    /// The recipe an `Unresolved` slot intends to build with (its
    /// `ComponentResolution`'s own recipe, before any linked Build exists).
    /// `None` for every other boundary -- a resolved Build/Reaction node
    /// already carries its recipe identity on its own
    /// [`VerificationOperationInput`] (`blueprint_or_formula_type_id`), and
    /// a Buy leaf has no recipe at all.
    pub intended_recipe: Option<RecipeSelection>,
}

/// Discrete-run production evidence for a Build/Reaction boundary. All zero
/// when the boundary was pruned (`remaining == 0`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ProductionEvidence {
    pub child_runs: u64,
    /// The linked child's authoritative per-run yield (captured recipe
    /// primary-product `quantity_per_run`). Carried even for a pruned
    /// (`remaining == 0`) boundary so a covered Build row still reports its
    /// recipe yield; `0` only for a non-production boundary.
    pub output_per_run: u64,
    pub produced_quantity: u64,
    pub surplus_quantity: u64,
}

impl ProductionEvidence {
    /// `remaining > 0`: size the child to cover `remaining` at
    /// `output_per_run` per run. `produced` can exceed `remaining` (discrete
    /// runs); the difference is `surplus`, recorded but never reused.
    #[must_use]
    pub fn sized(child_runs: u64, output_per_run: u64, remaining: u64) -> Self {
        let produced = child_runs.saturating_mul(output_per_run);
        Self {
            child_runs,
            output_per_run,
            produced_quantity: produced,
            surplus_quantity: produced.saturating_sub(remaining),
        }
    }

    /// A pruned Build/Reaction boundary (`remaining == 0`): no runs, no
    /// production, but the recipe's per-run yield is still known and worth
    /// recording.
    #[must_use]
    pub fn pruned(output_per_run: u64) -> Self {
        Self {
            output_per_run,
            ..Self::default()
        }
    }
}

/// The run count that covers `total_production_demand` at `output_per_run`
/// per run: `ceil(demand / output_per_run)`, clamped to the planner's
/// `1..=1_000_000` run bound, and `0` for no demand. The single sizing rule
/// shared by the live allocating planner's pooled groups
/// (`IndustryService::finalize_production_group`) and the canonical-producer
/// read model (`crate::production_dependency::ProducerDemand::size`).
#[must_use]
pub fn pooled_production_runs(total_production_demand: u64, output_per_run: u64) -> u64 {
    if total_production_demand == 0 {
        0
    } else {
        total_production_demand
            .div_ceil(output_per_run.max(1))
            .clamp(1, 1_000_000)
    }
}

/// Collects the traversal's boundary records into a [`BuildMaterialsAggregate`].
/// Pure and sync -- the allocating traversal
/// ([`crate::industry::IndustryService::project_build_materials`]) owns the
/// async child projection and feeds this.
#[derive(Default)]
pub struct MaterialsAccumulator {
    by_type: BTreeMap<i64, TypeTotals>,
    by_node: BTreeMap<(Uuid, i64), NodeMaterialAllocation>,
    sources: Vec<MaterialSource>,
    warnings: Vec<MaterialsAggregateWarning>,
    /// Build nodes whose demand could not be authoritatively quantified.
    /// `BTreeSet` keeps them ascending + deduplicated for a stable error.
    missing_nodes: BTreeSet<Uuid>,
    /// Primitive calculation inputs, pushed in traversal order (only when a
    /// boundary carries [`BoundaryVerification`]).
    verification_inputs: Vec<VerificationBoundaryInput>,
    /// One entry per walked production node, in DFS-entry order (only on the
    /// verification-export path).
    verification_operations: Vec<VerificationOperationInput>,
    /// Monotonic counter assigning each recorded boundary its
    /// `traversal_index`, advanced on every `record_*` call.
    next_traversal_index: u32,
    /// Monotonic counter assigning each walked node its `op_index`.
    next_op_index: u32,
}

#[derive(Default)]
struct TypeTotals {
    type_name: String,
    required: u64,
    allocated: u64,
    shortage: u64,
    /// Folded across contributions: `None` until the first, then the common
    /// strategy, then `Mixed` once two disagree.
    strategy: Option<MaterialRowStrategy>,
    /// `Some(true)` while every contribution so far is provisional; `Some(false)`
    /// once any real one lands.
    all_provisional: Option<bool>,
}

impl TypeTotals {
    fn fold_strategy(&mut self, strategy: MaterialRowStrategy) {
        self.strategy = Some(match self.strategy {
            None => strategy,
            Some(current) if current == strategy => current,
            Some(_) => MaterialRowStrategy::Mixed,
        });
    }
    fn fold_provisional(&mut self, provisional: bool) {
        self.all_provisional = Some(self.all_provisional.unwrap_or(true) && provisional);
    }
}

impl MaterialsAccumulator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A **leaf** boundary -- a `Buy` leaf (`provisional == false`,
    /// `strategy == Buy`), or an unresolved Build/Reaction slot
    /// (`provisional == true`, `strategy == Build`/`Reaction` from the
    /// intended recipe). Contributes to the aggregate row, to
    /// `node_allocations`, and to `sources`. A provisional leaf also pushes an
    /// [`MaterialsAggregateWarningCode::UnresolvedBuild`] warning.
    pub fn record_leaf(
        &mut self,
        rec: BoundaryRecord<'_>,
        strategy: MaterialRowStrategy,
        provisional: bool,
        verification: Option<BoundaryVerification>,
    ) -> u32 {
        let resolution = if provisional {
            MaterialBoundaryResolution::Unresolved
        } else {
            MaterialBoundaryResolution::Buy
        };
        let production = ProductionEvidence::default();
        let traversal_index = self.record_verification(&rec, resolution, &production, verification);
        self.accumulate_type(&rec, strategy, provisional);
        self.insert_node_allocation(&rec, resolution, provisional, production);
        self.sources.push(MaterialSource {
            build_id: rec.build_id,
            graph_node_id: rec.graph_node_id.to_string(),
            type_id: rec.type_id,
            type_name: rec.type_name.to_string(),
            required_quantity: rec.required,
            allocated_quantity: rec.allocated,
            shortage_quantity: rec.remaining,
            scope: rec.scope,
            provisional,
            tree_path: rec.tree_path.to_vec(),
        });
        if provisional {
            self.warnings.push(MaterialsAggregateWarning {
                code: MaterialsAggregateWarningCode::UnresolvedBuild,
                build_id: rec.build_id,
                type_id: Some(rec.type_id),
                message: format!(
                    "component {} is set to Build/React but has no linked build yet; \
                     surfaced as provisional demand",
                    rec.type_id
                ),
            });
        }
        traversal_index
    }

    /// A **Build/Reaction intermediate** boundary with a resolved linked
    /// child. The produced item is itself a plan requirement -- it contributes
    /// an aggregate row **and** a `node_allocations` entry (with `production`
    /// evidence), but no `source` (it is not an external leaf). `rec.remaining`
    /// is the production demand handed to the child (`0` ⇒ the caller pruned
    /// the subtree and did not recurse). Returns this boundary's
    /// `traversal_index` -- the caller passes it as `parent_traversal_index`
    /// when recursing into the linked child.
    pub fn record_intermediate(
        &mut self,
        rec: BoundaryRecord<'_>,
        resolution: MaterialBoundaryResolution,
        production: ProductionEvidence,
        verification: Option<BoundaryVerification>,
    ) -> u32 {
        let strategy = match resolution {
            MaterialBoundaryResolution::Build => MaterialRowStrategy::Build,
            MaterialBoundaryResolution::Reaction => MaterialRowStrategy::Reaction,
            other => {
                debug_assert!(false, "record_intermediate got {other:?}");
                MaterialRowStrategy::Build
            }
        };
        let traversal_index = self.record_verification(&rec, resolution, &production, verification);
        self.accumulate_type(&rec, strategy, false);
        self.insert_node_allocation(&rec, resolution, false, production);
        traversal_index
    }

    /// Assign the next `traversal_index` and, when `verification` is present,
    /// push the boundary's primitive-input row. Called once per recorded
    /// boundary, in traversal order.
    fn record_verification(
        &mut self,
        rec: &BoundaryRecord<'_>,
        resolution: MaterialBoundaryResolution,
        production: &ProductionEvidence,
        verification: Option<BoundaryVerification>,
    ) -> u32 {
        let traversal_index = self.next_traversal_index;
        self.next_traversal_index += 1;
        if let Some(v) = verification {
            self.verification_inputs.push(VerificationBoundaryInput {
                traversal_index,
                parent_traversal_index: v.parent_traversal_index,
                build_id: rec.build_id,
                graph_node_id: rec.graph_node_id.to_string(),
                tree_path: rec.tree_path.to_vec(),
                type_id: rec.type_id,
                type_name: rec.type_name.to_string(),
                activity: v.activity,
                resolution,
                scope: rec.scope,
                node_runs: v.node_runs,
                base_quantity_per_run: v.base_quantity_per_run,
                blueprint_me: v.blueprint_me,
                facility_material_factor: v.facility_material_factor,
                output_per_run: production.output_per_run,
                starting_inventory: v.starting_inventory,
                op_index: v.op_index,
                api_required: rec.required,
                api_planned_use: rec.allocated,
                api_shortage: rec.remaining,
                api_child_runs: production.child_runs,
                api_produced: production.produced_quantity,
                api_surplus: production.surplus_quantity,
                fresh_unit_price: v.fresh_unit_price,
                fresh_price_selection: v.fresh_price_selection,
                fresh_pricing_policy: v.fresh_pricing_policy,
                fresh_price_note: v.fresh_price_note.clone(),
                fresh_price_stale: v.fresh_price_stale,
                market_region_id: v.market_region_id,
                market_location_id: v.market_location_id,
                intended_recipe: v.intended_recipe,
                dependency_id: rec.dependency_id(),
                producer_build_id: rec.producer_build_id,
            });
        }
        traversal_index
    }

    /// Register one walked production node (operation) and return its
    /// assigned `op_index` (`0` for the root, in DFS-entry order). The caller
    /// passes that index into every boundary's [`BoundaryVerification`] for
    /// this node, and as `parent_op_index` when recursing into a child. The
    /// `op_index` field of `operation` is overwritten here. A no-op-ish call
    /// on the ordinary Materials path is impossible -- the traversal only
    /// builds an `operation` when `capture_verification` is set.
    pub fn record_operation(&mut self, mut operation: VerificationOperationInput) -> u32 {
        let op_index = self.next_op_index;
        self.next_op_index += 1;
        operation.op_index = op_index;
        self.verification_operations.push(operation);
        op_index
    }

    /// A walked linked child had no authoritative revision (mid-edit draft,
    /// soft preview failure, or an empty material projection where the recipe
    /// guarantees materials). The whole projection then fails with
    /// [`BuildMaterialsError::NodeRevisionUnavailable`].
    pub fn note_missing_node(&mut self, build_id: BuildId) {
        self.missing_nodes.insert(build_id.0);
    }

    fn accumulate_type(
        &mut self,
        rec: &BoundaryRecord<'_>,
        strategy: MaterialRowStrategy,
        provisional: bool,
    ) {
        let totals = self.by_type.entry(rec.type_id).or_default();
        if totals.type_name.is_empty() {
            totals.type_name = rec.type_name.to_string();
        }
        totals.required = totals.required.saturating_add(rec.required);
        totals.allocated = totals.allocated.saturating_add(rec.allocated);
        totals.shortage = totals.shortage.saturating_add(rec.remaining);
        totals.fold_strategy(strategy);
        totals.fold_provisional(provisional);
    }

    fn insert_node_allocation(
        &mut self,
        rec: &BoundaryRecord<'_>,
        resolution: MaterialBoundaryResolution,
        provisional: bool,
        production: ProductionEvidence,
    ) {
        debug_assert!(
            !self.by_node.contains_key(&(rec.build_id.0, rec.type_id)),
            "two contributions for (build {}, type {}) -- expansion is merged by type_id",
            rec.build_id.0,
            rec.type_id
        );
        self.by_node.insert(
            (rec.build_id.0, rec.type_id),
            NodeMaterialAllocation {
                build_id: rec.build_id,
                graph_node_id: rec.graph_node_id.to_string(),
                tree_path: rec.tree_path.to_vec(),
                type_id: rec.type_id,
                type_name: rec.type_name.to_string(),
                required_quantity: rec.required,
                allocated_quantity: rec.allocated,
                shortage_quantity: rec.remaining,
                scope: rec.scope,
                resolution,
                provisional,
                child_runs: production.child_runs,
                output_per_run: production.output_per_run,
                produced_quantity: production.produced_quantity,
                surplus_quantity: production.surplus_quantity,
                dependency_id: rec.dependency_id(),
                producer_build_id: rec.producer_build_id,
            },
        );
    }

    /// Finalize. `inventory` is the pool **after** the whole traversal --
    /// `available()` feeds each row's `available_quantity`. **Every**
    /// requirement boundary the traversal recorded produces a row (covered,
    /// partial, or short).
    pub fn finish(
        self,
        inventory: &PlanningInventory,
    ) -> Result<BuildMaterialsAggregate, BuildMaterialsError> {
        if !self.missing_nodes.is_empty() {
            return Err(BuildMaterialsError::NodeRevisionUnavailable {
                missing_nodes: self.missing_nodes.into_iter().map(BuildId).collect(),
            });
        }

        let lines = self
            .by_type
            .into_iter()
            .map(|(type_id, totals)| AggregateMaterialLine {
                type_id,
                type_name: totals.type_name,
                required_quantity: totals.required,
                available_quantity: inventory.available(type_id),
                reserved_quantity: inventory.reserved(type_id),
                allocated_quantity: totals.allocated,
                shortage_quantity: totals.shortage,
                fully_covered: totals.shortage == 0,
                strategy: totals.strategy.unwrap_or(MaterialRowStrategy::Buy),
                provisional: totals.all_provisional.unwrap_or(false),
            })
            .collect();

        let mut node_allocations: Vec<NodeMaterialAllocation> =
            self.by_node.into_values().collect();
        node_allocations.sort_by_key(|allocation| (allocation.build_id.0, allocation.type_id));

        Ok(BuildMaterialsAggregate {
            lines,
            node_allocations,
            sources: self.sources,
            warnings: self.warnings,
            verification_inputs: self.verification_inputs,
            verification_operations: self.verification_operations,
        })
    }
}

// ---------------------------------------------------------------------------
// Slot normalization (shared with the traversal in `industry::service`)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
