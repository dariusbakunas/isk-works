//! Allocation-aware production cost.
//!
//! [`project_build_cost`] is a **pure, sync, I/O-free** cost enrichment over
//! the authoritative allocation-aware Build Materials projection
//! ([`crate::industry::IndustryService::project_build_materials`]). It performs
//! **no** tree walk, **no** inventory allocation, **no** market / adjusted-price
//! I/O, **no** persisted-child-run costing, and **no** inventory mutation. It
//! reads the quantities, the global `PlanningInventory` allocation, the dynamic
//! child runs, the produced / surplus quantities, and the per-node price /
//! facility evidence the quantity projection already resolved, then computes
//! bottom-up: boundary costs -> operation costs -> root cost.
//!
//! ## Canonical planning-cost semantics
//!
//! * reused inventory  -> `planned_use * weighted-average historical unit basis`
//! * fresh Buy quantity -> `shortage * resolved market / manual / override price`
//! * Build/Reaction shortage -> the **proportional** basis of the
//!   dynamically-sized child production (see below) -- **never** the whole
//!   child job when the child overproduces
//! * installation -> included **once** per actual production operation, on that
//!   operation only (a child's installation is inside its own
//!   `total_production_cost`, therefore inside its `unit_production_cost`,
//!   therefore only its *consumed* share reaches the parent)
//! * unavoidable child surplus -> retains the portion of the child production
//!   basis the parent did not consume; **planning evidence only**, never added
//!   to the root cost
//!
//! ## Child consumed-cost arithmetic (surplus-conserving)
//!
//! ```text
//! consumed_child_cost    = round(child_total * consumed_quantity / child_produced_quantity, 4)
//! surplus_retained_basis = child_total - consumed_child_cost          // exact remainder
//! child_unit_production_cost = child_total / child_produced_quantity  // DISPLAY / EVIDENCE ONLY
//! ```
//!
//! Invariant, exact at [`Money`] scale: `consumed_child_cost +
//! surplus_retained_basis == child_total`.
//!
//! **Quantity/cost decoupling.** `BoundaryCostProjection::child_surplus_quantity`
//! (`produced - consumed`) is quantity truth and is populated whenever the
//! child operation is known, independent of whether the child's own cost is
//! complete. Only `child_surplus_retained_basis` (its cost basis) is gated
//! on cost completeness -- a missing price must never zero out a known
//! physical surplus.
//!
//! ## Consumers
//!
//! [`BuildCostProjection`] is the one production-cost source for the Build
//! preview/cost routes, the Worksheet (`crate::build_worksheet`), the Graph
//! (`crate::build_graph`), the Stages/execution plan
//! (`crate::execution_plan`), candidate previews, the verification workbook
//! export, and Epic freeze (`crate::order::freeze`).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;

use crate::build_materials::{
    InventoryBasisEntry, MaterialActivity, MaterialBoundaryResolution, VerificationBoundaryInput,
    VerificationOperationInput,
};
use crate::industry::{BuildId, Money};
use crate::{FulfillmentScope, PricingSelectionKind};

/// The scale every [`Money`] figure this module produces is rounded to -- the
/// project-wide `Money` convention (`rust_decimal` "banker's" / half-to-even,
/// matching [`Money::parse`] `rescale` and the facility EIV `round_dp`).
const MONEY_SCALE: u32 = 4;

fn m(value: Decimal) -> Money {
    Money(value.round_dp(MONEY_SCALE))
}

/// A typed, non-fatal reason a cost figure could not be completed. Never a hard
/// error: an incomplete cost is `None` + one of these, and the incompleteness
/// propagates upward through child cost. Stale evidence is *complete but
/// flagged*, matching the existing pricing-completeness convention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
// `rename_all_fields`: the variant tags *and* their fields are camelCase on
// the wire (`typeId`, `opIndex`), matching the web client's `CostWarning`.
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CostWarning {
    /// A `Buy` / unresolved boundary has a `shortage > 0` but no resolved fresh
    /// unit price.
    MissingFreshPrice {
        op_index: u32,
        traversal_index: u32,
        type_id: i64,
    },
    /// A boundary has `planned_use > 0` but no weighted-average inventory basis.
    MissingInventoryBasis {
        op_index: u32,
        traversal_index: u32,
        type_id: i64,
    },
    /// One or more of an operation's base recipe materials has no adjusted
    /// price, so its EIV -- and therefore its installation cost -- is unknown.
    MissingAdjustedPrice { op_index: u32, type_ids: Vec<i64> },
    /// The operation's facility profile carries no manual system cost index.
    MissingSystemCostIndex { op_index: u32 },
    /// The operation selected no facility, so it has no installation cost.
    NoFacilitySelected { op_index: u32 },
    /// A `Build` / `Reaction` boundary intends production but no child operation
    /// could be resolved for it (a genuinely `Unresolved` slot).
    UnresolvedBuild {
        op_index: u32,
        traversal_index: u32,
        type_id: i64,
    },
    /// The child operation this boundary consumes has an incomplete
    /// `total_production_cost`.
    ChildCostIncomplete {
        op_index: u32,
        traversal_index: u32,
        child_op_index: u32,
    },
    /// The fresh unit price is backed by market evidence older than the
    /// staleness horizon. **Complete but flagged** -- the price is still used.
    StaleFreshPrice {
        op_index: u32,
        traversal_index: u32,
        type_id: i64,
    },
    /// A `Decimal` multiplication / addition overflowed while costing this
    /// operation. Practically unreachable at ISK magnitudes; folded into
    /// completeness rather than panicking.
    ArithmeticOverflow { op_index: u32 },
}

/// How a requirement boundary is costed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BoundaryCostKind {
    /// External acquisition: `inventory_cost + fresh_cost`.
    Buy,
    /// Linked manufacturing child: `inventory_cost + consumed_child_cost`.
    Build,
    /// Linked reaction child: same shape as `Build`.
    Reaction,
    /// A `Build` / `Reaction` boundary whose shortage is `0` -- the child
    /// subtree was pruned; cost is `inventory_cost` alone.
    FullyCovered,
    /// A `Build` / `Reaction`-intended slot with no resolvable child: cost is
    /// unknown (`None`), never fabricated.
    Unresolved,
}

/// The cost of one requirement boundary, in exact allocator traversal order.
/// Preserves enough evidence for the verification workbook to independently
/// reconstruct every figure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryCostProjection {
    pub traversal_index: u32,
    pub op_index: u32,
    pub type_id: i64,
    pub type_name: String,
    pub resolution: MaterialBoundaryResolution,
    pub scope: FulfillmentScope,
    pub kind: BoundaryCostKind,
    pub required_quantity: u64,

    // --- reused inventory portion ---------------------------------------
    pub inventory_quantity: u64,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub inventory_unit_basis: Option<Decimal>,
    pub inventory_cost: Option<Money>,

    // --- fresh Buy portion --------------------------------------------------
    pub fresh_quantity: u64,
    pub fresh_unit_price: Option<Money>,
    pub fresh_price_selection: PricingSelectionKind,
    pub fresh_pricing_policy: Option<crate::MarketPricingPolicy>,
    pub fresh_price_note: String,
    pub fresh_price_stale: bool,
    pub market_region_id: Option<i64>,
    pub market_location_id: Option<i64>,
    pub fresh_cost: Option<Money>,

    // --- Build / Reaction child portion -----------------------------------
    pub child_op_index: Option<u32>,
    pub child_consumed_quantity: u64,
    pub child_produced_quantity: u64,
    pub child_total_production_cost: Option<Money>,
    /// `child_total / child_produced` -- **display / evidence only**, never
    /// multiplied back into `child_consumed_cost`.
    pub child_unit_production_cost: Option<Money>,
    pub child_consumed_cost: Option<Money>,
    /// `child_produced_quantity - child_consumed_quantity`. **Quantity
    /// truth, independent of cost completeness**: populated whenever the
    /// child operation itself was found (`child_op_index.is_some()`),
    /// because `child_produced_quantity` is always known unconditionally
    /// (`node_runs * output_per_run`, no price/cost gate) -- never `0`
    /// merely because `child_total_production_cost` is unknown. Contrast
    /// with `child_surplus_retained_basis` below, which genuinely *is*
    /// gated on cost completeness because it has no meaning without a
    /// known child total.
    pub child_surplus_quantity: u64,
    /// The cost basis of `child_surplus_quantity`, retained by this plan
    /// rather than consumed. `None` whenever the child's own
    /// `total_production_cost` is unknown -- never fabricated. Unlike
    /// `child_surplus_quantity`, this field's absence is a genuine "unknown
    /// cost", not "no surplus".
    pub child_surplus_retained_basis: Option<Money>,

    pub requirement_cost: Option<Money>,
    pub complete: bool,
    pub warnings: Vec<CostWarning>,
}

/// One operation's own industry-job installation cost, recomputed from
/// primitives at the operation's *projected* `node_runs` (never the persisted
/// child `Build.runs`). Mirrors the field set of the facility layer's
/// `InstallationCostBreakdown`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationInstallationCost {
    pub eiv: Option<Money>,
    pub eiv_missing_type_ids: Vec<i64>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub system_cost_index: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str")]
    pub job_cost_reduction_percent: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub facility_tax_percent: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub scc_surcharge_percent: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub alliance_surcharge_percent: Decimal,
    pub fixed_supplemental_cost: Money,
    pub unmodified_system_index_cost: Option<Money>,
    pub system_index_cost: Option<Money>,
    pub facility_tax: Option<Money>,
    pub scc_surcharge: Option<Money>,
    pub alliance_surcharge: Option<Money>,
    pub total: Option<Money>,
    pub complete: bool,
    pub formula_version: String,
    pub facility_profile_id: Option<uuid::Uuid>,
    pub facility_profile_revision: Option<u64>,
}

/// One production operation's rolled-up cost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationCostProjection {
    pub op_index: u32,
    pub parent_op_index: Option<u32>,
    pub graph_node_id: String,
    pub build_id: BuildId,
    pub activity: MaterialActivity,
    pub product_type_id: i64,
    pub node_runs: u64,
    pub output_per_run: u64,
    pub produced_quantity: u64,

    pub direct_inventory_cost: Money,
    pub direct_buy_cost: Money,
    pub consumed_child_cost: Money,
    pub material_component_cost: Option<Money>,

    pub own_installation: OperationInstallationCost,

    pub total_production_cost: Option<Money>,
    /// `total_production_cost / produced_quantity` -- **display / evidence
    /// only**, never part of additive arithmetic.
    pub unit_production_cost: Option<Money>,

    /// Σ of this operation's boundaries' `child_surplus_retained_basis` --
    /// value produced-and-retained by this operation's children, **not** part
    /// of this operation's `total_production_cost`.
    pub surplus_retained_basis_created: Money,

    pub complete: bool,
    pub warnings: Vec<CostWarning>,
}

/// Whole-build cost summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootCostSummary {
    pub root_op_index: u32,
    /// The final Build planning cost -- the root operation's
    /// `total_production_cost`. Recursively contains every descendant's
    /// *consumed* production basis and every descendant's installation
    /// (through the chain of child unit costs). **Does not** include
    /// `total_surplus_retained_basis`.
    pub planning_total_production_cost: Option<Money>,
    pub produced_quantity: u64,
    pub unit_production_cost: Option<Money>,

    // --- explanatory totals (proven non-overlapping) ---------------------
    /// Σ every boundary's `fresh_cost` + Σ every operation's own installation
    /// -- cash leaving the wallet.
    pub total_fresh_outlay: Money,
    /// Σ every boundary's `inventory_cost` -- historical basis of stock drawn
    /// down (each physical unit counted once by the global allocator).
    pub total_inventory_basis_consumed: Money,
    /// Σ every operation's own installation cost.
    pub total_own_installation_paid: Money,
    /// Σ every boundary's `child_surplus_retained_basis` -- basis of inventory
    /// this plan incidentally produces and keeps.
    pub total_surplus_retained_basis: Money,

    pub adjusted_price_observed_at: Option<DateTime<Utc>>,
    pub complete: bool,
}

/// The full result of [`project_build_cost`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildCostProjection {
    /// Ascending by `op_index` (`0` = root).
    pub operations: Vec<OperationCostProjection>,
    /// Ascending by `traversal_index`.
    pub boundaries: Vec<BoundaryCostProjection>,
    pub root: RootCostSummary,
    /// `true` iff every operation and every boundary is complete.
    pub complete: bool,
    /// Deduplicated union of every operation's and boundary's warnings.
    pub warnings: Vec<CostWarning>,
}

impl BuildCostProjection {
    /// Overwrite `revision`'s cost-bearing fields with this
    /// projection's numbers, so the existing (unmodified) worksheet /
    /// `sum_installation_costs` / candidate-preview derivation presents the
    /// allocation-aware planning-cost model without a parallel code path.
    ///
    /// `revision` must be the **same overlay** this projection was computed
    /// over, built via
    /// [`crate::industry::IndustryService::preview_plan`] -- its `material_lines` are matched to this
    /// projection's **root** (`op_index == 0`) boundaries by `type_id`.
    ///
    /// Never touches `revision.blueprint`, `.recipe_fingerprint`, `.snapshot`,
    /// a facility's `.profile` / `.requirements` / `.planned_duration_seconds`
    /// / `.duration_steps`, or any `material_lines` field other than the cost
    /// ones listed below -- only cost *values*:
    /// * per line: `unit_price` (Buy only; `None` for a Build/Reaction row --
    ///   there is no single market price for a self-produced item),
    ///   `line_total`, `missing`, `reused_quantity`, `reused_line_total`,
    ///   `installation_cost` (always `None` -- installation lives only at
    ///   the operation level, never per-row), `planning_evidence`.
    /// * `estimated_material_cost`, `missing_price_count`, `pricing_complete`,
    ///   `estimated_margin` (becomes the **planning margin**: `expected_revenue
    ///   - total_production_cost`; `expected_revenue` itself is untouched).
    /// * `manufacturing_facility` / `reaction_facility`'s `installation_cost`
    ///   breakdown, replaced with the root operation's own installation --
    ///   never the root's + descendants'.
    ///
    /// A no-op if this projection has no root operation (`operations` empty).
    pub fn apply_to_revision(&self, revision: &mut crate::industry::BuildPlanRevision) {
        let Some(root) = self
            .operations
            .iter()
            .find(|operation| operation.op_index == 0)
        else {
            return;
        };
        let boundaries_by_type: BTreeMap<i64, &BoundaryCostProjection> = self
            .boundaries
            .iter()
            .filter(|boundary| boundary.op_index == 0)
            .map(|boundary| (boundary.type_id, boundary))
            .collect();

        let mut missing_price_count = 0_u32;
        for line in &mut revision.material_lines {
            match boundaries_by_type.get(&line.type_id) {
                Some(boundary) => {
                    // A Buy row's `unit_price` is the *blended* average
                    // across the row's full requirement (inventory portion
                    // at its basis + fresh portion at market,
                    // `total / component.total_quantity`) -- never the raw
                    // fresh/market price alone, which would misrepresent a
                    // row that is partly filled from inventory. A
                    // Build/Reaction row has no single per-unit market
                    // price, so it stays `None` (evidence is carried via
                    // `planning_evidence` instead).
                    line.unit_price = if boundary.child_op_index.is_some() {
                        None
                    } else {
                        boundary.requirement_cost.and_then(|total| {
                            (boundary.required_quantity > 0)
                                .then(|| total.checked_div_quantity(boundary.required_quantity))
                                .transpose()
                                .ok()
                                .flatten()
                        })
                    };
                    line.line_total = boundary.requirement_cost;
                    line.missing = !boundary.complete;
                    line.reused_quantity = Some(boundary.inventory_quantity);
                    line.reused_line_total = boundary.inventory_cost;
                    line.installation_cost = None;
                    line.planning_evidence = boundary.child_op_index.map(|child_op_index| {
                        crate::industry::PlanningChildEvidence {
                            child_op_index,
                            child_produced_quantity: boundary.child_produced_quantity,
                            child_consumed_quantity: boundary.child_consumed_quantity,
                            child_unit_production_cost: boundary.child_unit_production_cost,
                            child_consumed_cost: boundary.child_consumed_cost,
                            child_surplus_quantity: boundary.child_surplus_quantity,
                            child_surplus_retained_basis: boundary.child_surplus_retained_basis,
                        }
                    });
                    if !boundary.complete {
                        missing_price_count += 1;
                    }
                }
                None => {
                    // No matching root boundary (should not happen for a
                    // revision built over the same overlay) -- never
                    // fabricate a value; leave the row visibly incomplete.
                    line.line_total = None;
                    line.missing = true;
                    line.installation_cost = None;
                    line.planning_evidence = None;
                    missing_price_count += 1;
                }
            }
        }

        revision.estimated_material_cost = root.material_component_cost.unwrap_or_else(Money::zero);
        revision.missing_price_count = missing_price_count;
        revision.pricing_complete = root.complete;
        revision.estimated_margin = if revision.pricing_complete {
            revision.expected_revenue.and_then(|revenue| {
                root.total_production_cost
                    .and_then(|total| revenue.checked_sub(total).ok())
            })
        } else {
            None
        };

        // Root's own installation only -- descendant installation is already
        // embedded proportionally through each Build/Reaction row's
        // `child_consumed_cost` (line.installation_cost is always `None`
        // above), never added again here.
        let breakdown = crate::InstallationCostBreakdown {
            complete: root.own_installation.complete,
            estimated_item_value: root.own_installation.eiv,
            system_cost_index: root.own_installation.system_cost_index,
            unmodified_system_index_cost: root.own_installation.unmodified_system_index_cost,
            job_cost_reduction_percent: root.own_installation.job_cost_reduction_percent,
            system_index_cost: root.own_installation.system_index_cost,
            facility_tax: root.own_installation.facility_tax,
            scc_surcharge: root.own_installation.scc_surcharge,
            alliance_surcharge: root.own_installation.alliance_surcharge,
            fixed_supplemental_cost: root.own_installation.fixed_supplemental_cost,
            total: root.own_installation.total,
            warnings: Vec::new(),
            formula_version: root.own_installation.formula_version.clone(),
        };
        if let Some(facility) = revision.manufacturing_facility.as_mut() {
            facility.installation_cost = breakdown.clone();
        }
        if let Some(facility) = revision.reaction_facility.as_mut() {
            facility.installation_cost = breakdown;
        }
    }
}

/// Cost-enrich an already-resolved allocation-aware quantity projection.
///
/// * `operations` / `boundaries` -- `BuildMaterialsAggregate.verification_operations`
///   / `.verification_inputs` from a **`Complete`** projection, in DFS-entry /
///   traversal order.
/// * `inventory_basis` -- the per-type slice of the **single** `list_balances`
///   snapshot that seeded `PlanningInventory` (no second read).
/// * `adjusted_prices` -- resolved **once in bulk** for the union of every
///   operation's base recipe material `type_id`s (no per-operation lookup).
/// * `adjusted_price_observed_at` -- provenance for the EIV inputs, surfaced on
///   [`RootCostSummary`].
///
/// Returns a projection with `Option<Money>` figures + typed warnings; never an
/// `Err`. An empty `operations` slice yields a trivially-complete empty
/// projection.
#[must_use]
pub fn project_build_cost(
    operations: &[VerificationOperationInput],
    boundaries: &[VerificationBoundaryInput],
    inventory_basis: &[InventoryBasisEntry],
    adjusted_prices: &BTreeMap<i64, Decimal>,
    adjusted_price_observed_at: Option<DateTime<Utc>>,
) -> BuildCostProjection {
    let basis_by_type: BTreeMap<i64, Option<Decimal>> = inventory_basis
        .iter()
        .map(|entry| (entry.type_id, entry.unit_basis))
        .collect();

    // Boundaries grouped by owning operation, each bucket in traversal order.
    let mut boundaries_by_op: BTreeMap<u32, Vec<&VerificationBoundaryInput>> = BTreeMap::new();
    for boundary in boundaries {
        boundaries_by_op
            .entry(boundary.op_index)
            .or_default()
            .push(boundary);
    }
    for bucket in boundaries_by_op.values_mut() {
        bucket.sort_by_key(|boundary| boundary.traversal_index);
    }

    // Child linkage by *occurrence*, not build_id: a non-root operation's
    // boundaries all carry the same `parent_traversal_index` -- the traversal
    // index of the parent Build/Reaction boundary that spawned this node.
    // Prefer the operation's own `parent_traversal_index` (the direct,
    // forward link) when present; fall back to searching this operation's
    // own boundaries for a matching one otherwise (evidence without the
    // forward link, for an operation that owns at least one boundary of its
    // own). A pooled *sibling* operation
    // owns no boundaries at all (only the representative's own materials
    // are walked -- `IndustryService::register_production_operation`'s own
    // doc comment), so it always sets the forward field directly; the
    // fallback search would silently find nothing for it.
    let mut child_op_by_parent_boundary: BTreeMap<u32, u32> = BTreeMap::new();
    for operation in operations {
        // An operation lists every demand edge
        // it serves -- one producer, many consuming boundaries.
        if !operation.incoming.is_empty() {
            for demand in &operation.incoming {
                child_op_by_parent_boundary.insert(demand.traversal_index, operation.op_index);
            }
            continue;
        }
        if operation.parent_op_index.is_none() {
            continue;
        }
        let spawn = operation.parent_traversal_index.or_else(|| {
            boundaries_by_op
                .get(&operation.op_index)
                .and_then(|bucket| bucket.iter().find_map(|b| b.parent_traversal_index))
        });
        if let Some(spawn) = spawn {
            child_op_by_parent_boundary.insert(spawn, operation.op_index);
        }
    }

    // Two-or-more parent boundaries (in
    // different, structurally unrelated consumer operations) can name
    // the *same* child op_index here -- one pooled operation with multiple
    // real consumers, never assumed away by a "one child, one consumer"
    // shape. Computing each boundary's own
    // `child_surplus_quantity`/`child_surplus_retained_basis` independently
    // as "the child's *entire* leftover past *my own* consumed share" would
    // double- (triple-, ...) count that one true leftover once per
    // consuming boundary. The true aggregate is computed once per child op
    // (`produced - Σ every consuming boundary's own short_qty`) and
    // attributed to exactly one boundary -- deterministically, the smallest
    // `traversal_index` among that child's own consumers -- with every
    // other boundary reporting `0` ("surplus belongs to the operation, not
    // each consumer"). A child with only one consuming boundary (the
    // overwhelmingly common, non-pooled case) is unaffected: that one
    // boundary is trivially both "every consumer" and "the smallest",
    // reproducing this function's own original per-boundary formula
    // exactly.

    // A shared producer is one operation consumed at several boundaries:
    // group those boundaries by the producing operation.
    let mut consuming_boundaries_by_pool: BTreeMap<u32, Vec<(u32, u32)>> = BTreeMap::new();
    for (&parent_traversal_index, &child_op_index) in &child_op_by_parent_boundary {
        consuming_boundaries_by_pool
            .entry(child_op_index)
            .or_default()
            .push((parent_traversal_index, child_op_index));
    }
    let boundary_by_traversal_index: BTreeMap<u32, &VerificationBoundaryInput> = boundaries
        .iter()
        .map(|boundary| (boundary.traversal_index, boundary))
        .collect();
    let mut total_consumed_by_child_op: BTreeMap<u32, u64> = BTreeMap::new();
    let mut surplus_owner_by_child_op: BTreeMap<u32, u32> = BTreeMap::new();
    // Every consuming boundary's own consumed quantity per child op, in
    // traversal order -- the surplus owner's retained basis is the exact
    // remainder of the one producer total after all of their consumed
    // shares (cost conservation at Money scale).
    let mut consumer_shortages_by_child_op: BTreeMap<u32, Vec<u64>> = BTreeMap::new();
    for (_pool_key, mut consumers) in consuming_boundaries_by_pool {
        consumers.sort_unstable();
        let shortages: Vec<u64> = consumers
            .iter()
            .filter_map(|(traversal_index, _)| boundary_by_traversal_index.get(traversal_index))
            .map(|boundary| boundary.api_shortage)
            .collect();
        let total_consumed = shortages.iter().copied().fold(0u64, u64::saturating_add);
        for &(_, child_op_index) in &consumers {
            consumer_shortages_by_child_op.insert(child_op_index, shortages.clone());
        }
        let owner_traversal_index = consumers
            .first()
            .map(|&(traversal_index, _)| traversal_index);
        for &(_, child_op_index) in &consumers {
            total_consumed_by_child_op.insert(child_op_index, total_consumed);
            if let Some(owner) = owner_traversal_index {
                surplus_owner_by_child_op.insert(child_op_index, owner);
            }
        }
    }

    // Bottom-up: DFS-entry order guarantees a parent's `op_index` is strictly
    // less than any descendant's, so descending `op_index` visits every child
    // before its parent. One operation, costed once.
    let mut ordered: Vec<&VerificationOperationInput> = operations.iter().collect();
    ordered.sort_by_key(|operation| std::cmp::Reverse(operation.op_index));

    let mut op_by_index: BTreeMap<u32, OperationCostProjection> = BTreeMap::new();
    let mut boundary_out: Vec<BoundaryCostProjection> = Vec::with_capacity(boundaries.len());

    for operation in ordered {
        let op_index = operation.op_index;
        let node_runs = operation.node_runs;
        let produced_quantity = node_runs.saturating_mul(operation.output_per_run);
        let op_boundaries = boundaries_by_op.get(&op_index).cloned().unwrap_or_default();

        let mut direct_inventory = Decimal::ZERO;
        let mut direct_buy = Decimal::ZERO;
        let mut consumed_child = Decimal::ZERO;
        let mut surplus_created = Decimal::ZERO;
        let mut boundaries_complete = true;
        let mut op_warnings: Vec<CostWarning> = Vec::new();
        let mut this_op_boundaries: Vec<BoundaryCostProjection> = Vec::new();

        for boundary in &op_boundaries {
            let type_id = boundary.type_id;
            let inv_qty = boundary.api_planned_use;
            let short_qty = boundary.api_shortage;
            let basis = basis_by_type.get(&type_id).copied().flatten();
            let mut warnings: Vec<CostWarning> = Vec::new();
            let mut complete = true;

            // Reused inventory portion (Buy and Build/Reaction alike).
            let inventory_cost: Option<Money> = if inv_qty == 0 {
                Some(Money::zero())
            } else {
                match basis {
                    Some(unit) => match unit.checked_mul(Decimal::from(inv_qty)) {
                        Some(value) => Some(m(value)),
                        None => {
                            complete = false;
                            warnings.push(CostWarning::ArithmeticOverflow { op_index });
                            None
                        }
                    },
                    None => {
                        complete = false;
                        warnings.push(CostWarning::MissingInventoryBasis {
                            op_index,
                            traversal_index: boundary.traversal_index,
                            type_id,
                        });
                        None
                    }
                }
            };

            // Per-resolution: fresh Buy portion, or child production portion.
            let kind;
            let mut fresh_quantity = 0_u64;
            let mut fresh_cost: Option<Money> = Some(Money::zero());
            let mut child_op_index: Option<u32> = None;
            let mut child_consumed_quantity = 0_u64;
            let mut child_produced_quantity = 0_u64;
            let mut child_total_production_cost: Option<Money> = None;
            let mut child_unit_production_cost: Option<Money> = None;
            let mut child_consumed_cost: Option<Money> = None;
            let mut child_surplus_quantity = 0_u64;
            let mut child_surplus_retained_basis: Option<Money> = None;

            match boundary.resolution {
                MaterialBoundaryResolution::Buy => {
                    kind = BoundaryCostKind::Buy;
                    fresh_quantity = short_qty;
                    if short_qty == 0 {
                        fresh_cost = Some(Money::zero());
                    } else if let Some(price) = boundary.fresh_unit_price {
                        match price.0.checked_mul(Decimal::from(short_qty)) {
                            Some(value) => fresh_cost = Some(m(value)),
                            None => {
                                complete = false;
                                fresh_cost = None;
                                warnings.push(CostWarning::ArithmeticOverflow { op_index });
                            }
                        }
                    } else {
                        complete = false;
                        fresh_cost = None;
                        warnings.push(CostWarning::MissingFreshPrice {
                            op_index,
                            traversal_index: boundary.traversal_index,
                            type_id,
                        });
                    }
                    if boundary.fresh_price_stale && short_qty > 0 {
                        warnings.push(CostWarning::StaleFreshPrice {
                            op_index,
                            traversal_index: boundary.traversal_index,
                            type_id,
                        });
                    }
                }
                MaterialBoundaryResolution::Unresolved => {
                    kind = BoundaryCostKind::Unresolved;
                    complete = false;
                    warnings.push(CostWarning::UnresolvedBuild {
                        op_index,
                        traversal_index: boundary.traversal_index,
                        type_id,
                    });
                }
                resolution @ (MaterialBoundaryResolution::Build
                | MaterialBoundaryResolution::Reaction) => {
                    let is_reaction = resolution == MaterialBoundaryResolution::Reaction;
                    if short_qty == 0 {
                        // Fully covered by inventory: the child subtree was
                        // pruned. Cost is the reused inventory portion alone.
                        kind = BoundaryCostKind::FullyCovered;
                    } else {
                        kind = if is_reaction {
                            BoundaryCostKind::Reaction
                        } else {
                            BoundaryCostKind::Build
                        };
                        child_op_index = child_op_by_parent_boundary
                            .get(&boundary.traversal_index)
                            .copied();
                        match child_op_index.and_then(|ci| op_by_index.get(&ci)) {
                            Some(child) => {
                                child_consumed_quantity = short_qty;
                                child_produced_quantity = child.produced_quantity;
                                child_total_production_cost = child.total_production_cost;
                                // The child's own true aggregate consumption/surplus,
                                // across every consuming boundary (one, in the
                                // overwhelmingly common non-pooled case) -- see this
                                // function's own doc comment on
                                // `surplus_owner_by_child_op` for why this must never
                                // be computed from `short_qty` (this boundary's own
                                // share) alone once a child can have more than one
                                // real consumer.
                                let total_consumed = child_op_index
                                    .and_then(|ci| total_consumed_by_child_op.get(&ci).copied())
                                    .unwrap_or(short_qty);
                                let is_surplus_owner = child_op_index
                                    .and_then(|ci| surplus_owner_by_child_op.get(&ci).copied())
                                    == Some(boundary.traversal_index);
                                // Physical surplus is quantity truth: `produced_quantity`
                                // is always known unconditionally (`node_runs *
                                // output_per_run`, no price/cost gate), so this must
                                // never wait on `total_production_cost`. Previously this
                                // was computed only inside the cost-complete arm below,
                                // so an unpriced/incomplete child silently reported zero
                                // surplus quantity even though the physical
                                // overproduction was fully known -- discovered while
                                // building the Execution Plan projector. Attributed to
                                // exactly one boundary when a child has more than one
                                // consumer (see above) -- every other boundary reports
                                // `0`, never the child's full leftover again.
                                child_surplus_quantity = if is_surplus_owner {
                                    child.produced_quantity.saturating_sub(total_consumed)
                                } else {
                                    0
                                };
                                match (child.total_production_cost, child.produced_quantity) {
                                    (Some(total), produced) if produced > 0 => {
                                        child_unit_production_cost =
                                            total.checked_div_quantity(produced).ok();
                                        let consumed = total
                                            .0
                                            .checked_mul(Decimal::from(short_qty))
                                            .and_then(|value| {
                                                value.checked_div(Decimal::from(produced))
                                            })
                                            .map(m);
                                        // The sole-consumer case (`total_consumed ==
                                        // short_qty`, the overwhelmingly common,
                                        // non-pooled shape) uses the plain
                                        // "exact remainder" formula
                                        // (`retained = total - consumed`, using
                                        // this same `consumed` value) --
                                        // `consumed_child_cost +
                                        // surplus_retained_basis == child_total`
                                        // exactly, by construction, never a
                                        // second independent rounding that could
                                        // drift from it at a half-to-even
                                        // midpoint. Only a genuinely pooled child
                                        // (`total_consumed != short_qty`) falls
                                        // back to the ratio-of-the-true-aggregate
                                        // computation, since no single boundary's
                                        // own `consumed` is the whole story there.
                                        //
                                        // With several
                                        // consumers the owner retains the *exact
                                        // remainder* of the one producer total after
                                        // every consumer's own rounded consumed share
                                        // (the same `round(total * short / produced)`
                                        // each consumer computes), so
                                        // `Σ consumed + retained == total` holds
                                        // exactly at Money scale for any number of
                                        // consumers -- never a separately rounded
                                        // ratio that could leak or double-count a
                                        // 0.0001.
                                        let retained_basis = if !is_surplus_owner {
                                            Some(Money::zero())
                                        } else if total_consumed == short_qty {
                                            consumed.map(|consumed| m(total.0 - consumed.0))
                                        } else {
                                            let shortages = child_op_index
                                                .and_then(|ci| {
                                                    consumer_shortages_by_child_op.get(&ci)
                                                })
                                                .cloned()
                                                .unwrap_or_else(|| vec![short_qty]);
                                            shortages
                                                .iter()
                                                .try_fold(Decimal::ZERO, |sum, share| {
                                                    total
                                                        .0
                                                        .checked_mul(Decimal::from(*share))
                                                        .and_then(|value| {
                                                            value.checked_div(Decimal::from(
                                                                produced,
                                                            ))
                                                        })
                                                        .map(|value| value.round_dp(MONEY_SCALE))
                                                        .and_then(|value| sum.checked_add(value))
                                                })
                                                .and_then(|consumed_sum| {
                                                    total.0.checked_sub(consumed_sum)
                                                })
                                                .map(m)
                                        };
                                        match (consumed, retained_basis) {
                                            (Some(consumed), Some(retained_basis)) => {
                                                child_consumed_cost = Some(consumed);
                                                child_surplus_retained_basis = Some(retained_basis);
                                            }
                                            _ => {
                                                complete = false;
                                                warnings.push(CostWarning::ArithmeticOverflow {
                                                    op_index,
                                                });
                                            }
                                        }
                                    }
                                    _ => {
                                        complete = false;
                                        warnings.push(CostWarning::ChildCostIncomplete {
                                            op_index,
                                            traversal_index: boundary.traversal_index,
                                            child_op_index: child_op_index.unwrap_or_default(),
                                        });
                                    }
                                }
                            }
                            None => {
                                // A `Complete` quantity projection always has a
                                // sized child here; a missing one is corrupt
                                // input -- treat as incomplete, never guess.
                                complete = false;
                                warnings.push(CostWarning::UnresolvedBuild {
                                    op_index,
                                    traversal_index: boundary.traversal_index,
                                    type_id,
                                });
                            }
                        }
                    }
                }
            }

            // requirement_cost = inventory_cost + (fresh_cost | consumed_child_cost)
            let requirement_cost: Option<Money> = match kind {
                BoundaryCostKind::Buy => add_opt(inventory_cost, fresh_cost),
                BoundaryCostKind::Build | BoundaryCostKind::Reaction => {
                    add_opt(inventory_cost, child_consumed_cost)
                }
                BoundaryCostKind::FullyCovered => inventory_cost,
                BoundaryCostKind::Unresolved => None,
            };
            if requirement_cost.is_none() {
                complete = false;
            }
            if !complete {
                boundaries_complete = false;
            }

            if let Some(cost) = inventory_cost {
                direct_inventory = direct_inventory.saturating_add(cost.0);
            }
            if matches!(kind, BoundaryCostKind::Buy) {
                if let Some(cost) = fresh_cost {
                    direct_buy = direct_buy.saturating_add(cost.0);
                }
            }
            if let Some(cost) = child_consumed_cost {
                consumed_child = consumed_child.saturating_add(cost.0);
            }
            if let Some(basis) = child_surplus_retained_basis {
                surplus_created = surplus_created.saturating_add(basis.0);
            }
            op_warnings.extend(warnings.iter().cloned());

            let projection = BoundaryCostProjection {
                traversal_index: boundary.traversal_index,
                op_index,
                type_id,
                type_name: boundary.type_name.clone(),
                resolution: boundary.resolution,
                scope: boundary.scope,
                kind,
                required_quantity: boundary.api_required,
                inventory_quantity: inv_qty,
                inventory_unit_basis: basis,
                inventory_cost,
                fresh_quantity,
                fresh_unit_price: boundary.fresh_unit_price,
                fresh_price_selection: boundary.fresh_price_selection,
                fresh_pricing_policy: boundary.fresh_pricing_policy,
                fresh_price_note: boundary.fresh_price_note.clone(),
                fresh_price_stale: boundary.fresh_price_stale,
                market_region_id: boundary.market_region_id,
                market_location_id: boundary.market_location_id,
                fresh_cost,
                child_op_index,
                child_consumed_quantity,
                child_produced_quantity,
                child_total_production_cost,
                child_unit_production_cost,
                child_consumed_cost,
                child_surplus_quantity,
                child_surplus_retained_basis,
                requirement_cost,
                complete,
                warnings,
            };
            this_op_boundaries.push(projection);
        }

        // --- own installation cost (from primitives, at node_runs) --------
        let installation = operation_installation_cost(operation, &op_boundaries, adjusted_prices);
        if !installation.eiv_missing_type_ids.is_empty() {
            op_warnings.push(CostWarning::MissingAdjustedPrice {
                op_index,
                type_ids: installation.eiv_missing_type_ids.clone(),
            });
        } else if operation.facility_id.is_none() {
            op_warnings.push(CostWarning::NoFacilitySelected { op_index });
        } else if operation.system_cost_index.is_none() {
            op_warnings.push(CostWarning::MissingSystemCostIndex { op_index });
        }

        let material_component_cost: Option<Money> = if boundaries_complete {
            Some(m(direct_inventory
                .saturating_add(direct_buy)
                .saturating_add(consumed_child)))
        } else {
            None
        };
        let total_production_cost = match (material_component_cost, installation.total) {
            (Some(material), Some(install)) => material.0.checked_add(install.0).map(m),
            _ => None,
        };
        let unit_production_cost = total_production_cost.and_then(|total| {
            (produced_quantity > 0)
                .then(|| total.checked_div_quantity(produced_quantity).ok())
                .flatten()
        });
        let complete = material_component_cost.is_some()
            && installation.complete
            && total_production_cost.is_some();

        op_warnings.sort_by_key(warning_key);
        op_warnings.dedup();

        let projection = OperationCostProjection {
            op_index,
            parent_op_index: operation.parent_op_index,
            graph_node_id: operation.graph_node_id.clone(),
            build_id: operation.build_id,
            activity: operation.activity,
            product_type_id: operation.product_type_id,
            node_runs,
            output_per_run: operation.output_per_run,
            produced_quantity,
            direct_inventory_cost: m(direct_inventory),
            direct_buy_cost: m(direct_buy),
            consumed_child_cost: m(consumed_child),
            material_component_cost,
            own_installation: installation,
            total_production_cost,
            unit_production_cost,
            surplus_retained_basis_created: m(surplus_created),
            complete,
            warnings: op_warnings,
        };
        op_by_index.insert(op_index, projection);
        boundary_out.extend(this_op_boundaries);
    }

    boundary_out.sort_by_key(|boundary| boundary.traversal_index);
    let mut operations_out: Vec<OperationCostProjection> = op_by_index.into_values().collect();
    operations_out.sort_by_key(|operation| operation.op_index);

    // --- explanatory totals (non-overlapping -- see the module doc) -----
    let mut total_fresh = Decimal::ZERO;
    let mut total_inventory = Decimal::ZERO;
    let mut total_installation = Decimal::ZERO;
    let mut total_surplus = Decimal::ZERO;
    for boundary in &boundary_out {
        if let Some(cost) = boundary.fresh_cost {
            total_fresh = total_fresh.saturating_add(cost.0);
        }
        if let Some(cost) = boundary.inventory_cost {
            total_inventory = total_inventory.saturating_add(cost.0);
        }
        if let Some(basis) = boundary.child_surplus_retained_basis {
            total_surplus = total_surplus.saturating_add(basis.0);
        }
    }
    for operation in &operations_out {
        if let Some(total) = operation.own_installation.total {
            total_installation = total_installation.saturating_add(total.0);
        }
    }
    total_fresh = total_fresh.saturating_add(total_installation);

    let root = operations_out
        .iter()
        .find(|operation| operation.op_index == 0);
    let complete = operations_out.iter().all(|operation| operation.complete)
        && boundary_out.iter().all(|boundary| boundary.complete);

    let mut warnings: Vec<CostWarning> = operations_out
        .iter()
        .flat_map(|operation| operation.warnings.iter().cloned())
        .chain(
            boundary_out
                .iter()
                .flat_map(|boundary| boundary.warnings.iter().cloned()),
        )
        .collect();
    warnings.sort_by_key(warning_key);
    warnings.dedup();

    BuildCostProjection {
        root: RootCostSummary {
            root_op_index: 0,
            planning_total_production_cost: root
                .and_then(|operation| operation.total_production_cost),
            produced_quantity: root.map_or(0, |operation| operation.produced_quantity),
            unit_production_cost: root.and_then(|operation| operation.unit_production_cost),
            total_fresh_outlay: m(total_fresh),
            total_inventory_basis_consumed: m(total_inventory),
            total_own_installation_paid: m(total_installation),
            total_surplus_retained_basis: m(total_surplus),
            adjusted_price_observed_at,
            complete,
        },
        operations: operations_out,
        boundaries: boundary_out,
        complete,
        warnings,
    }
}

/// `a + b`, propagating `None`.
fn add_opt(a: Option<Money>, b: Option<Money>) -> Option<Money> {
    match (a, b) {
        (Some(a), Some(b)) => a.0.checked_add(b.0).map(m),
        _ => None,
    }
}

/// EIV = Σ (over this operation's own recipe boundaries) `adjusted_price[type]
/// * base_quantity_per_run * node_runs` -- base recipe quantity, **no** ME,
/// **no** facility / rig material reduction, **no** ceil -- then the facility
/// job-cost formula. Reuses the exact arithmetic of
/// `crate::facility::installation_cost`.
fn operation_installation_cost(
    operation: &VerificationOperationInput,
    boundaries: &[&VerificationBoundaryInput],
    adjusted_prices: &BTreeMap<i64, Decimal>,
) -> OperationInstallationCost {
    let hundred = Decimal::from(100);
    let node_runs = Decimal::from(operation.node_runs);

    let mut eiv_total = Some(Decimal::ZERO);
    let mut missing: Vec<i64> = Vec::new();
    for boundary in boundaries {
        if boundary.base_quantity_per_run == 0 {
            continue;
        }
        match adjusted_prices.get(&boundary.type_id) {
            Some(price) => {
                let term = price
                    .checked_mul(Decimal::from(boundary.base_quantity_per_run))
                    .and_then(|value| value.checked_mul(node_runs));
                eiv_total = match (eiv_total, term) {
                    (Some(acc), Some(term)) => acc.checked_add(term),
                    _ => None,
                };
            }
            None => missing.push(boundary.type_id),
        }
    }
    missing.sort_unstable();
    missing.dedup();

    let eiv: Option<Money> = if missing.is_empty() {
        eiv_total.map(m)
    } else {
        None
    };
    let system_cost_index = operation.system_cost_index;
    let has_facility = operation.facility_id.is_some();

    let component = |percent: Decimal| -> Option<Money> {
        eiv.and_then(|value| {
            percent
                .checked_div(hundred)
                .and_then(|fraction| value.0.checked_mul(fraction))
                .map(m)
        })
    };
    let unmodified_system_index_cost = match (eiv, system_cost_index) {
        (Some(value), Some(index)) => value.0.checked_mul(index).map(m),
        _ => None,
    };
    let job_cost_factor = Decimal::ONE
        - operation
            .job_cost_reduction_percent
            .checked_div(hundred)
            .unwrap_or(Decimal::ZERO);
    let system_index_cost =
        unmodified_system_index_cost.and_then(|value| value.0.checked_mul(job_cost_factor).map(m));
    let facility_tax = component(operation.facility_tax_percent);
    let scc_surcharge = component(operation.scc_surcharge_percent);
    let alliance_surcharge = component(operation.alliance_surcharge_percent);

    // Charged once per industry job; every other component is a share of
    // the EIV, which is already linear in runs.
    let fixed_supplemental_cost = m(operation
        .fixed_supplemental_cost
        .0
        .saturating_mul(Decimal::from(operation.job_count.max(1))));
    let complete = has_facility && eiv.is_some() && system_cost_index.is_some();
    let total = if complete {
        let mut running = fixed_supplemental_cost.0;
        let mut ok = true;
        for value in [
            system_index_cost,
            facility_tax,
            scc_surcharge,
            alliance_surcharge,
        ]
        .into_iter()
        .flatten()
        {
            match running.checked_add(value.0) {
                Some(next) => running = next,
                None => ok = false,
            }
        }
        ok.then(|| m(running))
    } else {
        None
    };

    OperationInstallationCost {
        eiv,
        eiv_missing_type_ids: missing,
        system_cost_index,
        job_cost_reduction_percent: operation.job_cost_reduction_percent,
        facility_tax_percent: operation.facility_tax_percent,
        scc_surcharge_percent: operation.scc_surcharge_percent,
        alliance_surcharge_percent: operation.alliance_surcharge_percent,
        fixed_supplemental_cost,
        unmodified_system_index_cost,
        system_index_cost,
        facility_tax,
        scc_surcharge,
        alliance_surcharge,
        total,
        complete,
        formula_version: operation.installation_formula_version.clone(),
        facility_profile_id: operation.facility_id,
        facility_profile_revision: operation.facility_profile_revision,
    }
}

/// A total order over `CostWarning` for stable dedup (`derive`d `PartialEq`
/// handles equality; this only needs to be *consistent*).
fn warning_key(warning: &CostWarning) -> (u8, u32, u32, i64) {
    match warning {
        CostWarning::MissingFreshPrice {
            op_index,
            traversal_index,
            type_id,
        } => (0, *op_index, *traversal_index, *type_id),
        CostWarning::MissingInventoryBasis {
            op_index,
            traversal_index,
            type_id,
        } => (1, *op_index, *traversal_index, *type_id),
        CostWarning::MissingAdjustedPrice { op_index, type_ids } => {
            (2, *op_index, 0, type_ids.first().copied().unwrap_or(0))
        }
        CostWarning::MissingSystemCostIndex { op_index } => (3, *op_index, 0, 0),
        CostWarning::NoFacilitySelected { op_index } => (4, *op_index, 0, 0),
        CostWarning::UnresolvedBuild {
            op_index,
            traversal_index,
            type_id,
        } => (5, *op_index, *traversal_index, *type_id),
        CostWarning::ChildCostIncomplete {
            op_index,
            traversal_index,
            child_op_index,
        } => (6, *op_index, *traversal_index, i64::from(*child_op_index)),
        CostWarning::StaleFreshPrice {
            op_index,
            traversal_index,
            type_id,
        } => (7, *op_index, *traversal_index, *type_id),
        CostWarning::ArithmeticOverflow { op_index } => (8, *op_index, 0, 0),
    }
}

#[cfg(test)]
mod tests;
