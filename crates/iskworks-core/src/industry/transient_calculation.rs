use super::*;

/// Fully resolved inputs for deterministic industry-plan arithmetic.
///
/// Repository and network work belongs outside this boundary so callers can
/// price many candidate recipes using shared, batch-loaded context.
pub struct TransientCalculationInput<'a> {
    pub build: &'a Build,
    pub material_scope: crate::MarketScope,
    pub output_scope: crate::MarketScope,
    /// Materials priced against `material_scope` + output priced against
    /// `output_scope`, merged -- including any manual-price-list
    /// fallback already folded in by the caller.
    pub market_items: &'a [PriceSourceItem],
    /// Only for the snapshot's own provenance fields -- `None` when no
    /// manual price list was configured/used, matching
    /// `calculate_candidate_plan`'s own doc comment.
    pub manual_price_list: Option<&'a PriceSource>,
    pub overrides: Vec<PriceSourceItem>,
    pub pricing_policies: &'a BTreeMap<i64, crate::MarketPricingPolicy>,
    pub effective_requirements: &'a [crate::EffectiveMaterialRequirement],
    pub root_facility: Option<FacilityPlanPreview>,
    pub blueprint: Option<crate::ResolvedBlueprintAssumptions>,
    pub expansion: Option<&'a crate::ComponentExpansion>,
    pub manufacturing_profile: Option<&'a IndustryFacilityProfile>,
    pub reaction_profile: Option<&'a IndustryFacilityProfile>,
    pub component_facility_overrides: &'a BTreeMap<i64, IndustryFacilityProfile>,
    pub component_eivs: &'a BTreeMap<i64, Money>,
    /// Per Resolved linked child, keyed by the component `type_id` it
    /// fulfils: the child's **total** production cost -- its own materials
    /// *plus* its own installation cost (its whole job) -- rolled up. A
    /// build-resolved row's line total is this figure; the child's
    /// installation is therefore folded into that row's cost, not added to
    /// the parent's own installation line. Absent = the child's total cost
    /// isn't fully known yet.
    pub linked_build_material_costs: &'a BTreeMap<i64, Money>,
    pub material_coverage: &'a BTreeMap<i64, MaterialCoverageSummary>,
}

pub fn calculate_transient_plan(
    input: TransientCalculationInput<'_>,
) -> Result<crate::CalculatedBuildPlan, IndustryError> {
    let mut plan = calculate_candidate_plan(
        input.build,
        input.material_scope,
        input.output_scope,
        input.market_items,
        input.manual_price_list,
        input.overrides.clone(),
        input.pricing_policies,
    )?;
    if let Some(expansion) = input.expansion {
        apply_component_expansion(
            &mut plan,
            expansion,
            input.material_scope,
            input.market_items,
            &input.overrides,
            input.pricing_policies,
            input.manufacturing_profile,
            input.reaction_profile,
            input.component_facility_overrides,
            input.component_eivs,
            input.linked_build_material_costs,
            input.material_coverage,
        )?;
    } else {
        apply_effective_requirements(&mut plan, input.effective_requirements)?;
    }
    if let Some(facility) = input.root_facility {
        match &input.build.recipe {
            BuildRecipe::Manufacturing(_) => plan.manufacturing_facility = Some(facility),
            BuildRecipe::Reaction(_) => plan.reaction_facility = Some(facility),
        }
    }
    apply_profitability_costs(&mut plan)?;
    plan.blueprint = input.blueprint;
    // Retain the per-material calculation evidence for this node's own recipe
    // (base qty/run, runs, ME, facility material factor, effective quantity)
    // so a downstream consumer can reconstruct `total_quantity` from the
    // primitive inputs -- see `BuildPlanRevision::effective_requirements`.
    plan.effective_requirements = input.effective_requirements.to_vec();
    Ok(plan)
}
