//! Single-level Worksheet used only by create/candidate preview
//! consumers. It is not the saved-Build Worksheet authority; saved root and
//! focused views use `crate::build_worksheet` over BuildCostProjection.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::Serialize;

use crate::{
    BuildCoverageReport, CandidateIssue, FacilityPlanPreview, IndustryError, MarketPricingPolicy,
    MaterialContribution, Money, PlannedMaterialLine, PlannerItemRole, PriceSnapshotLine,
    PricingSelectionKind,
};

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionWorksheetProjection {
    pub groups: Vec<WorksheetGroup>,
    pub output: WorksheetGroup,
    pub summary: WorksheetSummary,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorksheetGroup {
    pub key: String,
    pub label: String,
    pub items: Vec<WorksheetItemRow>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorksheetItemRow {
    pub type_id: i64,
    pub type_name: String,
    pub role: PlannerItemRole,
    pub required_quantity: u64,
    pub available_quantity: u64,
    pub covered_quantity: u64,
    pub missing_quantity: u64,
    pub coverage_percentage: String,
    pub projected_inventory_cost: Option<Money>,
    pub pricing: WorksheetPricingProjection,
    pub line_total: Option<Money>,
    pub contributions: Vec<MaterialContribution>,
    pub is_build_resolved: bool,
    pub installation_cost: Option<Money>,
    /// How much of this row's cost comes from existing inventory (a
    /// `Missing` fulfillment scope), and what that portion cost --
    /// `None` for a row with no `FulfillmentScopeOverride` at all. The
    /// missing/bought-or-built quantity isn't repeated here: it's
    /// `required_quantity - reused_quantity` whenever this is `Some`.
    /// Distinct from `available_quantity`/`missing_quantity` above, which
    /// are always-present, purely informational inventory coverage
    /// figures unrelated to what's actually being reused for pricing.
    #[serde(default)]
    pub reused_quantity: Option<u64>,
    #[serde(default)]
    pub reused_line_total: Option<Money>,
    /// Allocation-aware child-production evidence, carried
    /// through 1:1 from the matching [`crate::PlannedMaterialLine`] -- see
    /// [`crate::PlanningChildEvidence`] for why a Build/Reaction row's
    /// `line_total` can be less than the child's whole job cost.
    #[serde(default)]
    pub planning_evidence: Option<crate::PlanningChildEvidence>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorksheetPricingProjection {
    pub selection_kind: PricingSelectionKind,
    pub effective_policy: Option<MarketPricingPolicy>,
    pub unit_price: Option<Money>,
    pub manual_unit_price: Option<Money>,
    pub missing: bool,
    pub source_note: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorksheetSummary {
    pub material_cost: Money,
    pub installation_cost: Option<Money>,
    pub total_cost: Option<Money>,
    pub expected_revenue: Option<Money>,
    /// Compatibility alias. On a revision costed via `BuildCostProjection`,
    /// this is exactly `planning_margin` -- `expected_revenue - total_cost`
    /// on the allocation-aware planning-cost basis, never the
    /// economic/replacement-cost basis Opportunities uses. Kept under this
    /// name for existing API/frontend consumers; prefer `planning_margin` in
    /// new code.
    pub estimated_margin: Option<Money>,
    pub pricing_complete: bool,
    pub quantity_coverage_complete: bool,
    pub cost_coverage_complete: bool,
    pub warnings: Vec<CandidateIssue>,
    /// Explicit canonical name for `estimated_margin` on a
    /// planning-cost-sourced worksheet. `None` when this worksheet was not
    /// costed via `BuildCostProjection`.
    #[serde(default)]
    pub planning_margin: Option<Money>,
    /// Whether the root operation's `BuildCostProjection` is
    /// fully known (materials + installation, root only -- descendant
    /// completeness is folded in via `consumed_child_cost`). `None` when this
    /// worksheet was not costed via `BuildCostProjection`.
    #[serde(default)]
    pub planning_cost_complete: Option<bool>,
    /// Explanatory only -- fresh Buy spend + every operation's
    /// own installation across the *whole* tree. Distinct from `total_cost`
    /// because reused inventory carries historical basis, not fresh outlay.
    /// Never the headline cost. `None` outside the planning-cost path.
    #[serde(default)]
    pub total_fresh_outlay: Option<Money>,
    /// Explanatory only -- the basis of inventory this plan's
    /// discrete child production incidentally creates and retains. **Not**
    /// included in `total_cost` -- surplus is an asset produced, not consumed
    /// by the root Build. `None` outside the planning-cost path.
    #[serde(default)]
    pub total_surplus_retained_basis: Option<Money>,
}

pub struct WorksheetProjectionInput<'a> {
    pub price_lines: &'a [PriceSnapshotLine],
    pub material_lines: &'a [PlannedMaterialLine],
    pub coverage: &'a BuildCoverageReport,
    pub facility: Option<&'a FacilityPlanPreview>,
    pub material_cost: Money,
    pub expected_revenue: Option<Money>,
    pub estimated_margin: Option<Money>,
    pub pricing_complete: bool,
    pub output_quantity: u64,
    pub group_labels: &'a BTreeMap<i64, String>,
    pub warnings: Vec<CandidateIssue>,
    /// When this worksheet is built from a revision that went
    /// through `BuildCostProjection::apply_to_revision`, the caller passes
    /// the same projection's root explanatory totals + completeness here so
    /// `WorksheetSummary` can carry them without this function recomputing
    /// anything cost-related itself. `None` otherwise.
    pub planning: Option<WorksheetPlanningTotals>,
}

/// The subset of `RootCostSummary` the worksheet surfaces as explanatory
/// evidence, passed in by the caller (see `WorksheetProjectionInput::planning`).
#[derive(Debug, Clone, Copy)]
pub struct WorksheetPlanningTotals {
    pub complete: bool,
    pub total_fresh_outlay: Money,
    pub total_surplus_retained_basis: Money,
}

pub fn project_production_worksheet(
    input: WorksheetProjectionInput<'_>,
) -> Result<ProductionWorksheetProjection, IndustryError> {
    let coverage = input
        .coverage
        .material_lines
        .iter()
        .map(|line| (line.type_id, line))
        .collect::<BTreeMap<_, _>>();
    let prices = input
        .price_lines
        .iter()
        .map(|line| ((line.item_role, line.type_id), line))
        .collect::<BTreeMap<_, _>>();
    let mut grouped = BTreeMap::<String, Vec<WorksheetItemRow>>::new();

    for material in input.material_lines {
        let price = prices
            .get(&(PlannerItemRole::Material, material.type_id))
            .ok_or(IndustryError::InvalidRecipe)?;
        let inventory = coverage
            .get(&material.type_id)
            .ok_or(IndustryError::InvalidRecipe)?;
        let label = input
            .group_labels
            .get(&material.type_id)
            .cloned()
            .unwrap_or_else(|| "Other Materials".into());
        grouped.entry(label).or_default().push(WorksheetItemRow {
            type_id: material.type_id,
            type_name: material.type_name.clone(),
            role: PlannerItemRole::Material,
            required_quantity: material.total_quantity,
            available_quantity: inventory.available_to_this_build,
            covered_quantity: inventory.covered_quantity,
            missing_quantity: inventory.missing_quantity,
            coverage_percentage: coverage_percentage(
                inventory.covered_quantity,
                material.total_quantity,
            ),
            projected_inventory_cost: inventory.projected_historical_cost,
            pricing: pricing(price, material.unit_price, material.missing),
            line_total: material.line_total,
            contributions: material.contributions.clone(),
            is_build_resolved: material.is_build_resolved,
            installation_cost: material
                .installation_cost
                .as_ref()
                .filter(|breakdown| breakdown.complete)
                .and_then(|breakdown| breakdown.total),
            reused_quantity: material.reused_quantity,
            reused_line_total: material.reused_line_total,
            planning_evidence: material.planning_evidence.clone(),
        });
    }

    let groups = grouped
        .into_iter()
        .map(|(label, mut items)| {
            items.sort_by(|left, right| {
                left.type_name
                    .cmp(&right.type_name)
                    .then(left.type_id.cmp(&right.type_id))
            });
            WorksheetGroup {
                key: label.to_lowercase().replace(' ', "-"),
                label,
                items,
            }
        })
        .collect();

    let output_price = input
        .price_lines
        .iter()
        .find(|line| line.item_role == PlannerItemRole::Output)
        .ok_or(IndustryError::InvalidRecipe)?;
    let output_line_total = output_price
        .price
        .map(|price| price.checked_mul_quantity(input.output_quantity))
        .transpose()?;
    let output = WorksheetGroup {
        key: "output".into(),
        label: "Output".into(),
        items: vec![WorksheetItemRow {
            type_id: output_price.type_id,
            type_name: output_price.type_name.clone(),
            role: PlannerItemRole::Output,
            required_quantity: input.output_quantity,
            available_quantity: 0,
            covered_quantity: 0,
            missing_quantity: 0,
            coverage_percentage: "0.00".into(),
            projected_inventory_cost: None,
            pricing: pricing(output_price, output_price.price, output_price.missing),
            line_total: output_line_total,
            contributions: Vec::new(),
            is_build_resolved: false,
            installation_cost: None,
            reused_quantity: None,
            reused_line_total: None,
            planning_evidence: None,
        }],
    };
    let installation_cost =
        crate::industry::sum_installation_costs(input.facility, input.material_lines)?;
    let total_cost = installation_cost
        .map(|installation| input.material_cost.checked_add(installation))
        .transpose()?;

    Ok(ProductionWorksheetProjection {
        groups,
        output,
        summary: WorksheetSummary {
            material_cost: input.material_cost,
            installation_cost,
            total_cost,
            expected_revenue: input.expected_revenue,
            estimated_margin: input.estimated_margin,
            pricing_complete: input.pricing_complete,
            quantity_coverage_complete: input.coverage.complete_quantity_coverage,
            cost_coverage_complete: input.coverage.complete_cost_coverage,
            warnings: input.warnings,
            planning_margin: input.planning.and(input.estimated_margin),
            planning_cost_complete: input.planning.map(|planning| planning.complete),
            total_fresh_outlay: input.planning.map(|planning| planning.total_fresh_outlay),
            total_surplus_retained_basis: input
                .planning
                .map(|planning| planning.total_surplus_retained_basis),
        },
    })
}

/// `line` is the price-source snapshot for this row, used for
/// `selection_kind`/`manual_unit_price`/`source_note` -- there's no
/// per-row pricing-override concept for `material_unit_price`/
/// `material_missing` to reflect. `unit_price`/`missing` themselves
/// always come from `material_unit_price`/`material_missing`, matching
/// how `line_total` is already sourced unconditionally from the material
/// line elsewhere in this module: a build-resolved row's own cost is
/// never the market price it never actually uses, and a `Missing`-scoped
/// Buy row's own cost is the inventory/market blend, never the stale
/// pre-blend snapshot figure. For an unresolved, unscoped row these are
/// already identical to the snapshot's own values (same underlying price
/// lookup), so this is a no-op for every row that predates either
/// feature.
fn pricing(
    line: &PriceSnapshotLine,
    material_unit_price: Option<Money>,
    material_missing: bool,
) -> WorksheetPricingProjection {
    WorksheetPricingProjection {
        selection_kind: line.selection_kind,
        effective_policy: line.pricing_policy,
        unit_price: material_unit_price,
        manual_unit_price: line.manual_unit_price,
        missing: material_missing,
        source_note: line.source_note.clone(),
    }
}

fn coverage_percentage(covered: u64, required: u64) -> String {
    if required == 0 {
        return "0.00".into();
    }
    let percentage =
        (Decimal::from(covered) * Decimal::from(100u64) / Decimal::from(required)).round_dp(2);
    format!("{percentage:.2}")
}

#[cfg(test)]
mod tests;
