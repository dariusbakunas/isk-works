use super::*;

pub(super) fn snapshot_line(
    recipe_line: &CapturedRecipeLine,
    item_role: PlannerItemRole,
    item: Option<&PriceSourceItem>,
    pricing_policy: Option<crate::MarketPricingPolicy>,
    position: usize,
    scope: crate::MarketScope,
) -> Result<PriceSnapshotLine, IndustryError> {
    Ok(PriceSnapshotLine {
        type_id: recipe_line.type_id,
        type_name: recipe_line.type_name.clone(),
        item_role,
        selection_kind: PricingSelectionKind::Default,
        manual_unit_price: None,
        price: item.map(|value| value.price),
        pricing_policy,
        missing: item.is_none(),
        source_note: item.map(|value| value.note.clone()).unwrap_or_default(),
        sort_order: u32::try_from(position).map_err(|_| IndustryError::InvalidRecipe)?,
        market_region_id: Some(scope.region_id),
        market_location_id: scope.location_id,
    })
}

/// A reaction Build has no blueprint concept, so it has no ME/TE either --
/// `(0, 0)` is inert here, not a default, since both consumers
/// (`preview_facility`'s reaction branch and `preview_reaction_effects`)
/// ignore these parameters entirely for `BuildRecipe::Reaction`.
pub(super) fn blueprint_efficiencies(blueprint: Option<&crate::BlueprintSnapshot>) -> (u8, u8) {
    blueprint.map_or((0, 0), |snapshot| {
        (snapshot.material_efficiency, snapshot.time_efficiency)
    })
}

/// The no-facility fallback used by the plan/preview flows: branches on
/// recipe kind between `preview_blueprint_effects` (manufacturing, ME/TE
/// adjusted) and `preview_reaction_effects` (reaction, raw scaling -- see
/// its doc comment for why that's correct).
pub(super) fn preview_recipe_effects(
    recipe: &BuildRecipe,
    runs: u64,
    blueprint_me: u8,
    blueprint_te: u8,
    max_runs_per_job: Option<u64>,
) -> Result<(Vec<crate::EffectiveMaterialRequirement>, Option<u64>), IndustryError> {
    Ok(match recipe {
        BuildRecipe::Manufacturing(recipe) => crate::preview_blueprint_effects(
            recipe,
            runs,
            blueprint_me,
            blueprint_te,
            max_runs_per_job,
        )?,
        BuildRecipe::Reaction(formula) => {
            let (requirements, duration) = crate::preview_reaction_effects(formula, runs)?;
            let requirements: Vec<crate::EffectiveMaterialRequirement> =
                requirements.into_iter().map(Into::into).collect();
            (requirements, duration)
        }
    })
}

/// Materials and output, priced independently since they may use
/// different market scopes -- returned as two separate lists (rather than
/// one merged `Vec` tagged by role) so callers can send each to its own
/// `derive_market_price_items(scope, ...)` call. Preserves the single-scope
/// behavior for the rare case where the output type_id also appears as a
/// material: it's priced once, as a material, and never duplicated into the
/// output list -- same de-dup this function already did when both lived in
/// one combined `Vec`.
pub(super) fn market_price_requests(
    build: &Build,
    facility: Option<&FacilityPlanPreview>,
    material_policy: crate::MarketPricingPolicy,
    output_policy: crate::MarketPricingPolicy,
    item_policies: &[BuildItemPricingPolicy],
    expansion: Option<&crate::ComponentExpansion>,
) -> Result<
    (
        Vec<crate::MarketPriceRequest>,
        Vec<crate::MarketPriceRequest>,
    ),
    IndustryError,
> {
    let overrides = item_policies
        .iter()
        .map(|item| (item.type_id, item.pricing_policy))
        .collect::<BTreeMap<_, _>>();
    let requests = if let Some(expansion) = expansion {
        // Component expansion replaces the worksheet's entire material set
        // (see `apply_component_expansion`'s `material_lines.clear()`), so
        // the price request list must be drawn from the expansion's merged
        // components -- not the root recipe's own materials -- or any
        // material introduced only by an expanded sub-component (i.e. not
        // also a direct material of the root recipe) can never resolve a
        // price from an order-book-backed source.
        expansion
            .components
            .iter()
            .map(|component| crate::MarketPriceRequest {
                type_id: component.type_id,
                type_name: component.type_name.clone(),
                requested_quantity: component.total_quantity,
                pricing_policy: overrides
                    .get(&component.type_id)
                    .copied()
                    .unwrap_or(material_policy),
            })
            .collect()
    } else {
        let effective: BTreeMap<_, _> = facility
            .map(|preview| {
                preview
                    .requirements
                    .iter()
                    .map(|line| (line.type_id, line.final_required_quantity))
                    .collect()
            })
            .unwrap_or_default();
        let mut requests = Vec::with_capacity(build.recipe.materials().len() + 1);
        for material in build.recipe.materials() {
            let requested_quantity = effective.get(&material.type_id).copied().unwrap_or(
                material
                    .quantity_per_run
                    .checked_mul(build.runs)
                    .ok_or(IndustryError::MoneyOverflow)?,
            );
            requests.push(crate::MarketPriceRequest {
                type_id: material.type_id,
                type_name: material.type_name.clone(),
                requested_quantity,
                pricing_policy: overrides
                    .get(&material.type_id)
                    .copied()
                    .unwrap_or(material_policy),
            });
        }
        requests
    };
    let product = build.recipe.primary_product();
    let output_requests = if requests
        .iter()
        .any(|request| request.type_id == product.type_id)
    {
        Vec::new()
    } else {
        vec![crate::MarketPriceRequest {
            type_id: product.type_id,
            type_name: product.type_name.clone(),
            requested_quantity: product
                .quantity_per_run
                .checked_mul(build.runs)
                .ok_or(IndustryError::MoneyOverflow)?,
            pricing_policy: overrides
                .get(&product.type_id)
                .copied()
                .unwrap_or(output_policy),
        }]
    };
    Ok((requests, output_requests))
}

pub(super) fn resolve_planner_pricing(
    build: &Build,
    selections: Vec<ItemPricingSelectionInput>,
) -> Result<(Vec<PriceInput>, Vec<BuildItemPricingPolicy>), IndustryError> {
    let selections = crate::normalize_pricing_selections(selections)?;
    let material_type_ids = build
        .recipe
        .materials()
        .iter()
        .map(|item| item.type_id)
        .collect::<BTreeSet<_>>();
    let output_type_ids = build
        .recipe
        .products()
        .iter()
        .map(|item| item.type_id)
        .collect::<BTreeSet<_>>();
    let mut overrides = Vec::new();
    let mut policies = Vec::new();

    for item in selections {
        let valid_role = match item.role {
            PlannerItemRole::Material => material_type_ids.contains(&item.type_id),
            PlannerItemRole::Output => output_type_ids.contains(&item.type_id),
        };
        if !valid_role {
            return Err(IndustryError::Validation(
                "Pricing selection item and role must belong to this build.".into(),
            ));
        }
        match item.selection {
            ItemPricingSelection::Default => {}
            ItemPricingSelection::MarketPolicy { policy } => {
                policies.push(BuildItemPricingPolicy {
                    type_id: item.type_id,
                    pricing_policy: policy,
                });
            }
            ItemPricingSelection::Manual { unit_price } => {
                let type_name = build
                    .recipe
                    .materials()
                    .iter()
                    .chain(build.recipe.products().iter())
                    .find(|line| line.type_id == item.type_id)
                    .expect("validated planner item belongs to the captured recipe")
                    .type_name
                    .clone();
                overrides.push(PriceInput {
                    type_id: item.type_id,
                    type_name,
                    price: unit_price,
                    note: "Manual planner price".into(),
                });
            }
        }
    }
    Ok((overrides, policies))
}

pub(super) fn apply_pricing_provenance(
    lines: &mut [PriceSnapshotLine],
    selections: &[ItemPricingSelectionInput],
) -> Result<(), IndustryError> {
    for selection in selections {
        let line = lines
            .iter_mut()
            .find(|line| line.type_id == selection.type_id && line.item_role == selection.role)
            .ok_or_else(|| {
                IndustryError::Validation(
                    "Pricing selection item and role must belong to this build.".into(),
                )
            })?;
        match &selection.selection {
            ItemPricingSelection::Default => {
                line.selection_kind = PricingSelectionKind::Default;
                line.manual_unit_price = None;
            }
            ItemPricingSelection::MarketPolicy { .. } => {
                line.selection_kind = PricingSelectionKind::MarketPolicy;
                line.manual_unit_price = None;
            }
            ItemPricingSelection::Manual { unit_price } => {
                line.selection_kind = PricingSelectionKind::Manual;
                line.manual_unit_price = Some(Money::parse(unit_price)?);
            }
        }
    }
    Ok(())
}

pub(super) fn build_pricing_policies(
    build: &Build,
    material_policy: crate::MarketPricingPolicy,
    output_policy: crate::MarketPricingPolicy,
    item_policies: &[BuildItemPricingPolicy],
) -> BTreeMap<i64, crate::MarketPricingPolicy> {
    let mut policies = build
        .recipe
        .materials()
        .iter()
        .map(|line| (line.type_id, material_policy))
        .collect::<BTreeMap<_, _>>();
    for product in build.recipe.products() {
        policies.entry(product.type_id).or_insert(output_policy);
    }
    for item in item_policies {
        policies.insert(item.type_id, item.pricing_policy);
    }
    policies
}

pub fn calculate_plan(
    build: &Build,
    material_scope: crate::MarketScope,
    output_scope: crate::MarketScope,
    market_items: &[PriceSourceItem],
    manual_price_list: Option<&PriceSource>,
    overrides: Vec<PriceSourceItem>,
    pricing_policies: &BTreeMap<i64, crate::MarketPricingPolicy>,
) -> Result<BuildPlanRevision, IndustryError> {
    calculate_candidate_plan(
        build,
        material_scope,
        output_scope,
        market_items,
        manual_price_list,
        overrides,
        pricing_policies,
    )?
    .into_plan_revision(build, 1, Utc::now())
}

/// `market_items` is the merged result of pricing materials against
/// `material_scope` and output against `output_scope` --
/// including any manual-price-list fallback already folded in by the
/// caller for items with no market coverage. `manual_price_list` is only
/// consulted for the snapshot's own provenance fields (name/revision),
/// decoupled from the actual pricing data: `None` means the plan priced
/// entirely from market scope, with no fallback used or configured.
pub fn calculate_candidate_plan(
    build: &Build,
    material_scope: crate::MarketScope,
    output_scope: crate::MarketScope,
    market_items: &[PriceSourceItem],
    manual_price_list: Option<&PriceSource>,
    overrides: Vec<PriceSourceItem>,
    pricing_policies: &BTreeMap<i64, crate::MarketPricingPolicy>,
) -> Result<crate::CalculatedBuildPlan, IndustryError> {
    validate_runs(build.runs)?;
    let mut prices: BTreeMap<i64, PriceSourceItem> = market_items
        .iter()
        .cloned()
        .map(|item| (item.type_id, item))
        .collect();
    for item in overrides {
        prices.insert(item.type_id, item);
    }
    let mut snapshot_items = Vec::new();
    let mut material_lines = Vec::new();
    let mut estimated_material_cost = Money::zero();
    let mut missing_price_count = 0_u32;

    for material in build.recipe.materials() {
        let total_quantity = material
            .quantity_per_run
            .checked_mul(build.runs)
            .ok_or(IndustryError::MoneyOverflow)?;
        let price_item = prices.get(&material.type_id);
        let price = price_item.map(|item| item.price);
        let line_total = price
            .map(|value| value.checked_mul_quantity(total_quantity))
            .transpose()?;
        if let Some(total) = line_total {
            estimated_material_cost = estimated_material_cost.checked_add(total)?;
        } else {
            missing_price_count = missing_price_count
                .checked_add(1)
                .ok_or(IndustryError::MoneyOverflow)?;
        }
        snapshot_items.push(snapshot_line(
            material,
            PlannerItemRole::Material,
            price_item,
            pricing_policies.get(&material.type_id).copied(),
            snapshot_items.len(),
            material_scope,
        )?);
        material_lines.push(PlannedMaterialLine {
            type_id: material.type_id,
            type_name: material.type_name.clone(),
            quantity_per_run: material.quantity_per_run,
            total_quantity,
            unit_price: price,
            line_total,
            missing: price.is_none(),
            contributions: Vec::new(),
            is_build_resolved: false,
            installation_cost: None,
            reused_quantity: None,
            missing_quantity: None,
            reused_line_total: None,
            planning_evidence: None,
        });
    }

    let product = build.recipe.primary_product();
    let product_price_item = prices.get(&product.type_id);
    snapshot_items.push(snapshot_line(
        product,
        PlannerItemRole::Output,
        product_price_item,
        pricing_policies.get(&product.type_id).copied(),
        snapshot_items.len(),
        output_scope,
    )?);
    let expected_output = product
        .quantity_per_run
        .checked_mul(build.runs)
        .ok_or(IndustryError::MoneyOverflow)?;
    let expected_revenue = product_price_item
        .map(|item| item.price.checked_mul_quantity(expected_output))
        .transpose()?;
    let pricing_complete = missing_price_count == 0;
    let estimated_margin = if pricing_complete {
        expected_revenue
            .map(|revenue| revenue.checked_sub(estimated_material_cost))
            .transpose()?
    } else {
        None
    };
    Ok(crate::CalculatedBuildPlan {
        runs: build.runs,
        recipe_fingerprint: build.recipe.fingerprint().to_string(),
        price_source_id: manual_price_list.map(|source| source.id),
        price_source_name: manual_price_list
            .map(|source| source.name.clone())
            .unwrap_or_else(|| "Market".to_string()),
        price_source_revision: manual_price_list.map_or(0, |source| source.revision),
        price_lines: snapshot_items,
        pricing_complete,
        estimated_material_cost,
        expected_revenue,
        estimated_margin,
        missing_price_count,
        material_lines,
        manufacturing_facility: None,
        reaction_facility: None,
        blueprint: None,
        // Populated by `calculate_transient_plan` from the caller's
        // already-computed effective requirements; this base pass has none.
        effective_requirements: Vec::new(),
    })
}

pub(super) fn apply_effective_requirements(
    plan: &mut crate::CalculatedBuildPlan,
    effective_requirements: &[crate::EffectiveMaterialRequirement],
) -> Result<(), IndustryError> {
    let requirements: BTreeMap<_, _> = effective_requirements
        .iter()
        .map(|line| (line.type_id, line.final_required_quantity))
        .collect();
    let mut total = Money::zero();
    let mut missing = 0_u32;
    for line in &mut plan.material_lines {
        line.total_quantity = *requirements
            .get(&line.type_id)
            .ok_or(IndustryError::InvalidRecipe)?;
        line.line_total = line
            .unit_price
            .map(|price| price.checked_mul_quantity(line.total_quantity))
            .transpose()?;
        if let Some(line_total) = line.line_total {
            total = total.checked_add(line_total)?;
        } else {
            missing = missing.checked_add(1).ok_or(IndustryError::MoneyOverflow)?;
        }
    }
    plan.estimated_material_cost = total;
    plan.missing_price_count = missing;
    plan.pricing_complete = missing == 0;
    plan.estimated_margin = if plan.pricing_complete {
        plan.expected_revenue
            .map(|revenue| revenue.checked_sub(total))
            .transpose()?
    } else {
        None
    };
    Ok(())
}

/// Replaces the material portion of `plan` with the fully aggregated,
/// merged material list from a component expansion -- not an in-place
/// overwrite like `apply_effective_requirements`, since expansion produces
/// an entirely different `type_id` set (a build-resolved component's own
/// sub-materials were never among the root recipe's direct materials).
/// `ResolvedComponent::total_quantity` is used as-is: the expansion is
/// seeded with the root preview's effective requirements, so root blueprint
/// ME and facility material modifiers have already been applied with the
/// authoritative rounding rules. It is not a per-run rate to scale again.
/// Per-type_id inventory coverage, for `Missing`-scoped rows only -- a
/// type_id absent from the map (every row, on a build with no
/// `FulfillmentScopeOverride`s at all) is `Full` scope.
/// `average_historical_unit_cost` being `None` means the reused
/// portion's cost is genuinely unknown (it is always all-or-nothing per
/// type_id, never partial), not that it's zero.
#[derive(Debug, Clone, Copy)]
pub struct MaterialCoverageSummary {
    pub available_to_this_build: u64,
    pub average_historical_unit_cost: Option<Money>,
}

pub(super) struct ComponentExpansionQuantities<'a> {
    pub(super) available: &'a BTreeMap<i64, u64>,
    pub(super) effective_root_requirements: &'a BTreeMap<i64, u64>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_component_expansion(
    plan: &mut crate::CalculatedBuildPlan,
    expansion: &crate::ComponentExpansion,
    material_scope: crate::MarketScope,
    market_items: &[PriceSourceItem],
    overrides: &[PriceSourceItem],
    pricing_policies: &BTreeMap<i64, crate::MarketPricingPolicy>,
    manufacturing_profile: Option<&IndustryFacilityProfile>,
    reaction_profile: Option<&IndustryFacilityProfile>,
    component_facility_overrides: &BTreeMap<i64, IndustryFacilityProfile>,
    component_eivs: &BTreeMap<i64, Money>,
    // Per Resolved linked child (keyed by the component `type_id` it fulfils):
    // the child's **total** production cost -- its own materials *plus* its
    // own installation, its whole job -- rolled up. Used as-is as that
    // build-resolved row's line total, so the child's installation lands
    // inside that row's cost and is *not* added again to the parent's own
    // installation line. An entry's presence also marks the row as "linked
    // child, cost already whole": its own `installation_cost` breakdown is
    // left `None` and `sum_installation_costs` skips it.
    linked_build_material_costs: &BTreeMap<i64, Money>,
    material_coverage: &BTreeMap<i64, MaterialCoverageSummary>,
) -> Result<(), IndustryError> {
    let mut prices: BTreeMap<i64, PriceSourceItem> = market_items
        .iter()
        .cloned()
        .map(|item| (item.type_id, item))
        .collect();
    for item in overrides {
        prices.insert(item.type_id, item.clone());
    }

    let output_line = plan
        .price_lines
        .iter()
        .find(|line| line.item_role == PlannerItemRole::Output)
        .cloned();
    let root_type_name = output_line
        .as_ref()
        .map(|line| line.type_name.clone())
        .unwrap_or_else(|| "Build output".to_string());
    let component_names: BTreeMap<i64, String> = expansion
        .components
        .iter()
        .map(|component| (component.type_id, component.type_name.clone()))
        .collect();
    plan.material_lines.clear();

    let mut material_lines = Vec::with_capacity(expansion.components.len());
    let mut material_snapshot_lines = Vec::with_capacity(expansion.components.len());
    let mut estimated_material_cost = Money::zero();
    let mut missing_price_count = 0_u32;

    for (sort_order, component) in expansion.components.iter().enumerate() {
        let recipe_line = CapturedRecipeLine {
            type_id: component.type_id,
            type_name: component.type_name.clone(),
            quantity_per_run: component.total_quantity,
            sort_order: sort_order as u32,
        };
        let is_build_resolved = matches!(
            component.resolution,
            crate::ComponentResolutionOutcome::Build { .. }
        );
        // A build-resolved row's cost is what its own linked build actually
        // costs to build, never a market price -- a market price is what
        // it'd cost to buy instead, which is exactly the double-counting
        // this row's own materials already avoid by never being expanded
        // into new rows here (single-level worksheets). No linked-build
        // cost available yet reads as unknown, not a market-price fallback.
        let price_item = prices.get(&component.type_id);
        let coverage = material_coverage.get(&component.type_id);
        let reused_quantity = coverage
            .map(|summary| {
                component
                    .total_quantity
                    .min(summary.available_to_this_build)
            })
            .unwrap_or(0);
        let missing_quantity = component.total_quantity - reused_quantity;
        // Cost for whatever portion still needs buying or building --
        // exactly the pre-fulfillment-scope logic above, just scaled to
        // `missing_quantity` instead of the row's full requirement. For a
        // build-resolved row, `ComponentExpansionService` already sized
        // the linked build to cover exactly this shortage, so its own live total *is* the missing portion's cost already --
        // no further division/rescaling against the full requirement.
        let (missing_unit_price, missing_line_total) = if is_build_resolved {
            match linked_build_material_costs.get(&component.type_id) {
                Some(&total) => {
                    let unit = (missing_quantity > 0)
                        .then(|| total.checked_div_quantity(missing_quantity))
                        .transpose()?;
                    (unit, Some(total))
                }
                None => (None, None),
            }
        } else {
            let price = price_item.map(|item| item.price);
            let line_total = price
                .map(|value| value.checked_mul_quantity(missing_quantity))
                .transpose()?;
            (price, line_total)
        };
        // Blend in the reused (inventory) portion, if any. Degenerates to
        // exactly the pre-fulfillment-scope figures above when
        // `reused_quantity` is 0 (no `Missing` override, or one with
        // nothing yet available) -- `missing_quantity` already equals the
        // row's full requirement in that case.
        let (price, line_total, reused_line_total) = if reused_quantity == 0 {
            (missing_unit_price, missing_line_total, None)
        } else if missing_quantity > 0 && missing_unit_price.is_none() {
            // The portion still being bought/built is itself unpriced --
            // unknown, not zero, for the whole row, not just that portion.
            (None, None, None)
        } else {
            match coverage.and_then(|summary| summary.average_historical_unit_cost) {
                // The reused portion's own cost is unknown -- never
                // fabricated from the missing portion's market/build price,
                // even when that one is known. This is always
                // all-or-nothing per type_id, never a partially-known blend
                // within the reused quantity itself.
                None => (None, None, None),
                Some(inventory_unit_cost) => {
                    let reused_total = inventory_unit_cost.checked_mul_quantity(reused_quantity)?;
                    let missing_total = missing_line_total.unwrap_or(Money::zero());
                    let total = reused_total.checked_add(missing_total)?;
                    let unit = (component.total_quantity > 0)
                        .then(|| total.checked_div_quantity(component.total_quantity))
                        .transpose()?;
                    (unit, Some(total), Some(reused_total))
                }
            }
        };
        if let Some(total) = line_total {
            estimated_material_cost = estimated_material_cost.checked_add(total)?;
        } else {
            missing_price_count = missing_price_count
                .checked_add(1)
                .ok_or(IndustryError::MoneyOverflow)?;
        }
        material_snapshot_lines.push(snapshot_line(
            &recipe_line,
            PlannerItemRole::Material,
            price_item,
            pricing_policies.get(&component.type_id).copied(),
            sort_order,
            material_scope,
        )?);
        let contributions = component
            .contributions
            .iter()
            .map(|contribution| {
                let (parent_type_id, parent_type_name) = match contribution.source {
                    crate::ContributionSource::Root => (None, root_type_name.clone()),
                    crate::ContributionSource::Component { type_id } => (
                        Some(type_id),
                        component_names
                            .get(&type_id)
                            .cloned()
                            .unwrap_or_else(|| format!("Type {type_id}")),
                    ),
                };
                MaterialContribution {
                    parent_type_id,
                    parent_type_name,
                    quantity: contribution.quantity,
                }
            })
            .collect();
        let installation_cost = match &component.resolution {
            crate::ComponentResolutionOutcome::Buy => None,
            // A Resolved linked child's installation cost is already folded
            // into its line total (`linked_build_material_costs`), so this
            // row carries no separate installation figure and does not add to
            // the parent's own installation line. Only a Build slot with *no*
            // linked child cost yet (never created, or cost not fully known)
            // still gets a parent-slot / per-row-override estimate.
            crate::ComponentResolutionOutcome::Build { recipe, .. }
                if !linked_build_material_costs.contains_key(&component.type_id) =>
            {
                let profile =
                    component_facility_overrides
                        .get(&component.type_id)
                        .or(match recipe {
                            RecipeSelection::Manufacturing { .. } => manufacturing_profile,
                            RecipeSelection::Reaction { .. } => reaction_profile,
                        });
                let eiv = component_eivs.get(&component.type_id).copied();
                match (profile, eiv) {
                    (Some(profile), Some(eiv)) => Some(
                        crate::facility::installation_cost(profile, Some(eiv), 1)
                            .map_err(IndustryError::Facility)?,
                    ),
                    _ => None,
                }
            }
            crate::ComponentResolutionOutcome::Build { .. } => None,
        };
        material_lines.push(PlannedMaterialLine {
            type_id: component.type_id,
            type_name: component.type_name.clone(),
            quantity_per_run: component.total_quantity,
            total_quantity: component.total_quantity,
            unit_price: price,
            line_total,
            missing: price.is_none(),
            contributions,
            is_build_resolved,
            installation_cost,
            reused_quantity: coverage.map(|_| reused_quantity),
            missing_quantity: coverage.map(|_| missing_quantity),
            reused_line_total,
            planning_evidence: None,
        });
    }

    plan.material_lines = material_lines;
    plan.price_lines = material_snapshot_lines;
    if let Some(mut output_line) = output_line {
        output_line.sort_order = plan.price_lines.len() as u32;
        plan.price_lines.push(output_line);
    }

    plan.estimated_material_cost = estimated_material_cost;
    plan.missing_price_count = missing_price_count;
    plan.pricing_complete = missing_price_count == 0;
    plan.estimated_margin = if plan.pricing_complete {
        plan.expected_revenue
            .map(|revenue| revenue.checked_sub(estimated_material_cost))
            .transpose()?
    } else {
        None
    };
    Ok(())
}

/// The root job's own installation cost (if fully configured) plus any
/// build-resolved row that still carries its *own* installation breakdown --
/// i.e. a Build slot with no linked child cost folded in yet (an Unresolved
/// slot, or a child whose total cost isn't fully known). A Resolved linked
/// child's installation is already inside its line total
/// (`linked_build_material_costs`), so its row's `installation_cost` is
/// `None` and it is skipped here rather than treated as "unknown" -- adding
/// it would double-count. Returns `None` if a still-separate installation
/// figure is required but not known, matching the "unknown, not zero"
/// completeness contract used for pricing. Shared by the profit calculation
/// and the worksheet summary so the two can never silently disagree.
pub(crate) fn sum_installation_costs(
    root_facility: Option<&FacilityPlanPreview>,
    material_lines: &[PlannedMaterialLine],
) -> Result<Option<Money>, IndustryError> {
    let root_installation_cost = root_facility
        .filter(|facility| facility.installation_cost.complete)
        .and_then(|facility| facility.installation_cost.total);
    // A build-resolved row that still carries its own installation breakdown
    // is an as-yet-uncreated child sized from the parent's facility slot; its
    // cost must be known. A row with no breakdown (`None`) is either a
    // Resolved linked child (installation already in its line total) or a
    // plain Buy row -- neither contributes here and neither blocks
    // completeness.
    let component_installation_cost = material_lines
        .iter()
        .filter(|line| line.is_build_resolved && line.installation_cost.is_some())
        .try_fold(Some(Money::zero()), |total, line| {
            let cost = line
                .installation_cost
                .as_ref()
                .filter(|breakdown| breakdown.complete)
                .and_then(|breakdown| breakdown.total);
            Ok::<_, IndustryError>(match (total, cost) {
                (Some(sum), Some(cost)) => Some(sum.checked_add(cost)?),
                _ => None,
            })
        })?;
    Ok(
        match (root_installation_cost, component_installation_cost) {
            (Some(root), Some(components)) => Some(root.checked_add(components)?),
            _ => None,
        },
    )
}

pub(super) fn apply_profitability_costs(
    plan: &mut crate::CalculatedBuildPlan,
) -> Result<(), IndustryError> {
    let installation_cost = sum_installation_costs(plan.root_facility(), &plan.material_lines)?;
    plan.estimated_margin = estimated_profit(
        plan.pricing_complete,
        plan.expected_revenue,
        plan.estimated_material_cost,
        installation_cost,
    )?;
    Ok(())
}

pub(super) fn estimated_profit(
    pricing_complete: bool,
    expected_revenue: Option<Money>,
    material_cost: Money,
    installation_cost: Option<Money>,
) -> Result<Option<Money>, IndustryError> {
    if !pricing_complete {
        return Ok(None);
    }
    expected_revenue
        .zip(installation_cost)
        .map(|(revenue, installation)| {
            revenue
                .checked_sub(material_cost)?
                .checked_sub(installation)
        })
        .transpose()
}

pub(super) fn validate_runs(runs: u64) -> Result<(), IndustryError> {
    if !(1..=MAX_RUNS).contains(&runs) {
        return Err(IndustryError::Validation(
            "Runs must be between 1 and 1,000,000.".to_string(),
        ));
    }
    Ok(())
}
