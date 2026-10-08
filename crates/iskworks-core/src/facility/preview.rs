//! The facility calculation surface: adjusted-price EIV, manufacturing and
//! reaction previews (with and without a facility), and the installation-cost
//! breakdown.

use std::collections::BTreeMap;

use rust_decimal::{Decimal, RoundingStrategy};

use crate::{CapturedReactionFormula, CapturedRecipe, CapturedRecipeLine, JobSplit, Money};

use super::rig_applicability::{
    applicable_rigs, rig_bonus_detail, skipped_rig_warning, ProductClassification,
};
use super::types::{
    AdjustedPriceEiv, DurationCalculationStep, EffectiveMaterialRequirement,
    EffectiveReactionMaterialRequirement, FacilityError, FacilityPlanPreview, FacilityRole,
    IndustryFacilityProfile, InstallationCostBreakdown, ReactionFacilityPlanPreview,
    FACILITY_FORMULA_VERSION, REACTION_FACILITY_FORMULA_VERSION,
};

pub fn calculate_adjusted_price_eiv(
    materials: &[CapturedRecipeLine],
    runs: u64,
    adjusted_prices: &BTreeMap<i64, Decimal>,
) -> Result<AdjustedPriceEiv, FacilityError> {
    if runs == 0 {
        return Err(FacilityError::Validation(
            "Runs must be greater than zero.".to_string(),
        ));
    }
    let mut total = Decimal::ZERO;
    let mut missing_type_ids = Vec::new();
    for material in materials {
        let Some(price) = adjusted_prices.get(&material.type_id) else {
            missing_type_ids.push(material.type_id);
            continue;
        };
        let quantity = material
            .quantity_per_run
            .checked_mul(runs)
            .ok_or(FacilityError::ArithmeticOverflow)?;
        total = total
            .checked_add(
                price
                    .checked_mul(Decimal::from(quantity))
                    .ok_or(FacilityError::ArithmeticOverflow)?,
            )
            .ok_or(FacilityError::ArithmeticOverflow)?;
    }
    Ok(AdjustedPriceEiv {
        value: missing_type_ids
            .is_empty()
            .then_some(Money(total.round_dp(4))),
        missing_type_ids,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn preview_facility(
    recipe: &CapturedRecipe,
    runs: u64,
    profile: IndustryFacilityProfile,
    product: ProductClassification,
    blueprint_me: u8,
    blueprint_te: u8,
    estimated_item_value: Option<Money>,
    max_runs_per_job: Option<u64>,
) -> Result<FacilityPlanPreview, FacilityError> {
    if blueprint_me > 10 || blueprint_te > 20 {
        return Err(FacilityError::Validation(
            "Blueprint ME must be 0-10 and TE must be 0-20.".to_string(),
        ));
    }
    if profile.archived_at.is_some() {
        return Err(FacilityError::Archived);
    }
    if profile.role != FacilityRole::Manufacturing {
        return Err(FacilityError::Validation(
            "This facility profile is not a manufacturing facility.".to_string(),
        ));
    }
    // Only rigs whose applicability filter admits this job's product
    // contribute a bonus; an installed-but-nonapplicable rig is a zero
    // modifier. Material and time are filtered independently.
    let material_rigs = applicable_rigs(&profile.rigs, product, |rig| &rig.applicability.material);
    let time_rigs = applicable_rigs(&profile.rigs, product, |rig| &rig.applicability.time);
    let material_factor = reduction_factor(
        profile.material_reduction_percent,
        material_rigs
            .iter()
            .map(|rig| rig.material_reduction_percent),
    )?;
    let rig_material_factor = reduction_factor(
        Decimal::ZERO,
        material_rigs
            .iter()
            .map(|rig| rig.material_reduction_percent),
    )?;
    let me_factor = Decimal::ONE - Decimal::from(blueprint_me) / Decimal::ONE_HUNDRED;
    let split = JobSplit::for_runs(runs, max_runs_per_job);
    let requirements = recipe
        .materials
        .iter()
        .map(|line| {
            let extended = line
                .quantity_per_run
                .checked_mul(runs)
                .ok_or(FacilityError::ArithmeticOverflow)?;
            let (final_required_quantity, calculation_trace) =
                per_job_quantity(split, line.quantity_per_run, |job_extended| {
                    let exact = Decimal::from(job_extended)
                        .checked_mul(me_factor)
                        .and_then(|value| value.checked_mul(material_factor))
                        .ok_or(FacilityError::ArithmeticOverflow)?;
                    Ok((
                        decimal_to_ceiling_u64(exact)?,
                        format!("{job_extended} x {me_factor} x {material_factor}"),
                    ))
                })?;
            Ok(EffectiveMaterialRequirement {
                type_id: line.type_id,
                type_name: line.type_name.clone(),
                sort_order: line.sort_order,
                base_quantity_per_run: line.quantity_per_run,
                runs,
                base_extended_quantity: extended,
                blueprint_me: Some(blueprint_me),
                facility_material_factor: material_factor,
                final_required_quantity,
                job_count: split.job_count(),
                calculation_trace,
                formula_version: FACILITY_FORMULA_VERSION.to_string(),
                recipe_fingerprint: recipe.fingerprint.clone(),
            })
        })
        .collect::<Result<Vec<_>, FacilityError>>()?;
    let mut duration_steps = Vec::new();
    let planned_duration_seconds = if let Some(base) = recipe.duration_seconds_per_run {
        let job_runs = split.longest_job_runs();
        let extended = base
            .checked_mul(job_runs)
            .ok_or(FacilityError::ArithmeticOverflow)?;
        duration_steps.push(DurationCalculationStep {
            label: "Blueprint base duration".into(),
            detail: duration_basis_detail(split, "from the active SDE recipe"),
            multiplier: Decimal::ONE,
            running_duration_seconds: extended,
        });
        let te_factor = Decimal::ONE - Decimal::from(blueprint_te) / Decimal::ONE_HUNDRED;
        let after_te = Decimal::from(extended)
            .checked_mul(te_factor)
            .ok_or(FacilityError::ArithmeticOverflow)?;
        duration_steps.push(DurationCalculationStep {
            label: format!("Blueprint TE {blueprint_te}"),
            detail: format!("{blueprint_te}% time reduction"),
            multiplier: te_factor,
            running_duration_seconds: decimal_to_ceiling_u64(after_te)?.max(1),
        });
        let structure_factor = Decimal::ONE - profile.time_reduction_percent / Decimal::ONE_HUNDRED;
        let after_structure = after_te
            .checked_mul(structure_factor)
            .ok_or(FacilityError::ArithmeticOverflow)?;
        duration_steps.push(DurationCalculationStep {
            label: format!("{} structure time bonus", profile.structure_type_name),
            detail: format!("{}% time reduction", profile.time_reduction_percent),
            multiplier: structure_factor,
            running_duration_seconds: decimal_to_ceiling_u64(after_structure)?.max(1),
        });
        let rig_factor = reduction_factor(
            Decimal::ZERO,
            time_rigs.iter().map(|rig| rig.time_reduction_percent),
        )?;
        let final_duration = after_structure
            .checked_mul(rig_factor)
            .ok_or(FacilityError::ArithmeticOverflow)?;
        duration_steps.push(DurationCalculationStep {
            label: "Facility rig time bonuses".into(),
            detail: rig_bonus_detail(&time_rigs, &profile.rigs),
            multiplier: rig_factor,
            running_duration_seconds: decimal_to_ceiling_u64(final_duration)?.max(1),
        });
        Some(decimal_to_ceiling_u64(final_duration)?.max(1))
    } else {
        None
    };
    let installation_cost = installation_cost(&profile, estimated_item_value, split.job_count())?;
    let mut warnings = installation_cost.warnings.clone();
    warnings.extend(skipped_rig_warning(&profile.rigs, product));
    warnings.push(
        "Duration includes blueprint and facility assumptions but no character skills.".to_string(),
    );
    Ok(FacilityPlanPreview {
        profile,
        blueprint_me: Some(blueprint_me),
        blueprint_te: Some(blueprint_te),
        requirements,
        rig_material_factor,
        planned_duration_seconds,
        duration_steps,
        installation_cost,
        warnings,
        formula_version: FACILITY_FORMULA_VERSION.to_string(),
    })
}

/// The reaction counterpart of `preview_facility`. Unlike manufacturing,
/// reaction formulas have no material/time efficiency of their own, and
/// Refinery structures give no base reaction discount in EVE (verified
/// against real SDE dogma: Athanor/Tatara carry none of the manufacturing
/// structure-bonus attributes) -- so the entire material and time reduction
/// comes from the facility's rigs.
pub fn preview_reaction_facility(
    formula: &CapturedReactionFormula,
    runs: u64,
    profile: IndustryFacilityProfile,
    product: ProductClassification,
    estimated_item_value: Option<Money>,
) -> Result<ReactionFacilityPlanPreview, FacilityError> {
    if profile.archived_at.is_some() {
        return Err(FacilityError::Archived);
    }
    if profile.role != FacilityRole::Reaction {
        return Err(FacilityError::Validation(
            "This facility profile is not a reaction facility.".to_string(),
        ));
    }
    // Same applicability filtering as manufacturing -- a reaction rig fitted
    // for a product class other than this formula's contributes nothing.
    let material_rigs = applicable_rigs(&profile.rigs, product, |rig| &rig.applicability.material);
    let time_rigs = applicable_rigs(&profile.rigs, product, |rig| &rig.applicability.time);
    let material_factor = reduction_factor(
        Decimal::ZERO,
        material_rigs
            .iter()
            .map(|rig| rig.material_reduction_percent),
    )?;
    let requirements = formula
        .materials
        .iter()
        .map(|line| {
            let extended = line
                .quantity_per_run
                .checked_mul(runs)
                .ok_or(FacilityError::ArithmeticOverflow)?;
            let exact = Decimal::from(extended)
                .checked_mul(material_factor)
                .ok_or(FacilityError::ArithmeticOverflow)?;
            let final_required_quantity = decimal_to_ceiling_u64(exact)?.max(runs);
            Ok(EffectiveReactionMaterialRequirement {
                type_id: line.type_id,
                type_name: line.type_name.clone(),
                sort_order: line.sort_order,
                base_quantity_per_run: line.quantity_per_run,
                runs,
                base_extended_quantity: extended,
                facility_material_factor: material_factor,
                final_required_quantity,
                calculation_trace: format!("ceil(max(runs, {extended} x {material_factor}))"),
                formula_version: REACTION_FACILITY_FORMULA_VERSION.to_string(),
                recipe_fingerprint: formula.fingerprint.clone(),
            })
        })
        .collect::<Result<Vec<_>, FacilityError>>()?;
    let mut duration_steps = Vec::new();
    let planned_duration_seconds = if let Some(base) = formula.duration_seconds_per_run {
        let extended = base
            .checked_mul(runs)
            .ok_or(FacilityError::ArithmeticOverflow)?;
        duration_steps.push(DurationCalculationStep {
            label: "Reaction formula base duration".into(),
            detail: format!("{} runs from the active SDE reaction formula", runs),
            multiplier: Decimal::ONE,
            running_duration_seconds: extended,
        });
        let rig_factor = reduction_factor(
            Decimal::ZERO,
            time_rigs.iter().map(|rig| rig.time_reduction_percent),
        )?;
        let final_duration = Decimal::from(extended)
            .checked_mul(rig_factor)
            .ok_or(FacilityError::ArithmeticOverflow)?;
        duration_steps.push(DurationCalculationStep {
            label: "Facility rig time bonuses".into(),
            detail: rig_bonus_detail(&time_rigs, &profile.rigs),
            multiplier: rig_factor,
            running_duration_seconds: decimal_to_ceiling_u64(final_duration)?.max(1),
        });
        Some(decimal_to_ceiling_u64(final_duration)?.max(1))
    } else {
        None
    };
    let installation_cost = installation_cost(&profile, estimated_item_value, 1)?;
    let mut warnings = installation_cost.warnings.clone();
    warnings.extend(skipped_rig_warning(&profile.rigs, product));
    warnings
        .push("Duration includes facility rig assumptions but no character skills.".to_string());
    Ok(ReactionFacilityPlanPreview {
        profile,
        requirements,
        rig_material_factor: material_factor,
        planned_duration_seconds,
        duration_steps,
        installation_cost,
        warnings,
        formula_version: REACTION_FACILITY_FORMULA_VERSION.to_string(),
    })
}

/// The reaction counterpart of `preview_blueprint_effects`: the no-facility
/// case for a reaction Build. Reaction formulas have no ME/TE, and a
/// Refinery gives zero base reaction discount with no rigs fitted (verified
/// in `preview_reaction_facility`'s doc comment), so this is the identity --
/// raw formula quantities and duration scaled by runs.
pub fn preview_reaction_effects(
    formula: &CapturedReactionFormula,
    runs: u64,
) -> Result<(Vec<EffectiveReactionMaterialRequirement>, Option<u64>), FacilityError> {
    let requirements = formula
        .materials
        .iter()
        .map(|line| {
            let extended = line
                .quantity_per_run
                .checked_mul(runs)
                .ok_or(FacilityError::ArithmeticOverflow)?;
            Ok(EffectiveReactionMaterialRequirement {
                type_id: line.type_id,
                type_name: line.type_name.clone(),
                sort_order: line.sort_order,
                base_quantity_per_run: line.quantity_per_run,
                runs,
                base_extended_quantity: extended,
                facility_material_factor: Decimal::ONE,
                final_required_quantity: extended.max(runs),
                calculation_trace: format!("max(runs, {extended})"),
                formula_version: REACTION_FACILITY_FORMULA_VERSION.to_string(),
                recipe_fingerprint: formula.fingerprint.clone(),
            })
        })
        .collect::<Result<Vec<_>, FacilityError>>()?;
    let duration = formula
        .duration_seconds_per_run
        .map(|base| {
            base.checked_mul(runs)
                .ok_or(FacilityError::ArithmeticOverflow)
                .map(|value| value.max(1))
        })
        .transpose()?;
    Ok((requirements, duration))
}

pub fn preview_blueprint_effects(
    recipe: &CapturedRecipe,
    runs: u64,
    blueprint_me: u8,
    blueprint_te: u8,
    max_runs_per_job: Option<u64>,
) -> Result<(Vec<EffectiveMaterialRequirement>, Option<u64>), FacilityError> {
    if blueprint_me > 10 || blueprint_te > 20 {
        return Err(FacilityError::Validation(
            "Blueprint ME must be 0-10 and TE must be 0-20.".to_string(),
        ));
    }
    let me_factor = Decimal::ONE - Decimal::from(blueprint_me) / Decimal::ONE_HUNDRED;
    let split = JobSplit::for_runs(runs, max_runs_per_job);
    let requirements = recipe
        .materials
        .iter()
        .map(|line| {
            let extended = line
                .quantity_per_run
                .checked_mul(runs)
                .ok_or(FacilityError::ArithmeticOverflow)?;
            let (final_required_quantity, calculation_trace) =
                per_job_quantity(split, line.quantity_per_run, |job_extended| {
                    let exact = Decimal::from(job_extended)
                        .checked_mul(me_factor)
                        .ok_or(FacilityError::ArithmeticOverflow)?;
                    Ok((
                        decimal_to_ceiling_u64(exact)?,
                        format!("{job_extended} x {me_factor}"),
                    ))
                })?;
            Ok(EffectiveMaterialRequirement {
                type_id: line.type_id,
                type_name: line.type_name.clone(),
                sort_order: line.sort_order,
                base_quantity_per_run: line.quantity_per_run,
                runs,
                base_extended_quantity: extended,
                blueprint_me: Some(blueprint_me),
                facility_material_factor: Decimal::ONE,
                final_required_quantity,
                job_count: split.job_count(),
                calculation_trace,
                formula_version: FACILITY_FORMULA_VERSION.to_string(),
                recipe_fingerprint: recipe.fingerprint.clone(),
            })
        })
        .collect::<Result<Vec<_>, FacilityError>>()?;
    let te_factor = Decimal::ONE - Decimal::from(blueprint_te) / Decimal::ONE_HUNDRED;
    let duration = recipe
        .duration_seconds_per_run
        .map(|base| {
            base.checked_mul(split.longest_job_runs())
                .ok_or(FacilityError::ArithmeticOverflow)
                .and_then(|extended| {
                    Decimal::from(extended)
                        .checked_mul(te_factor)
                        .ok_or(FacilityError::ArithmeticOverflow)
                })
                .and_then(decimal_to_ceiling_u64)
                .map(|value| value.max(1))
        })
        .transpose()?;
    Ok((requirements, duration))
}

/// `job_count` scales only the fixed per-job fee: every other component is
/// a percentage of `eiv`, which is already linear in total runs.
pub(crate) fn installation_cost(
    profile: &IndustryFacilityProfile,
    eiv: Option<Money>,
    job_count: u64,
) -> Result<InstallationCostBreakdown, FacilityError> {
    let fixed_supplemental_cost = profile
        .fixed_supplemental_cost
        .checked_mul_quantity(job_count.max(1))
        .map_err(|_| FacilityError::ArithmeticOverflow)?;
    let mut warnings = Vec::new();
    if eiv.is_none() {
        warnings
            .push("Adjusted-price basis is unavailable; installation cost is incomplete.".into());
    }
    if profile.manual_system_cost_index.is_none() {
        warnings.push("System cost index is unavailable; zero was not substituted.".into());
    }
    let component = |percent: Decimal| -> Result<Option<Money>, FacilityError> {
        eiv.map(|value| {
            value
                .0
                .checked_mul(percent / Decimal::ONE_HUNDRED)
                .map(Money)
                .ok_or(FacilityError::ArithmeticOverflow)
        })
        .transpose()
    };
    let unmodified_index_cost = match (eiv, profile.manual_system_cost_index) {
        (Some(value), Some(index)) => Some(Money(
            value
                .0
                .checked_mul(index)
                .ok_or(FacilityError::ArithmeticOverflow)?,
        )),
        _ => None,
    };
    let job_cost_factor = Decimal::ONE - profile.job_cost_reduction_percent / Decimal::ONE_HUNDRED;
    let index_cost = unmodified_index_cost
        .map(|value| {
            value
                .0
                .checked_mul(job_cost_factor)
                .map(Money)
                .ok_or(FacilityError::ArithmeticOverflow)
        })
        .transpose()?;
    let facility_tax = component(profile.facility_tax_percent)?;
    let scc = component(profile.scc_surcharge_percent)?;
    let alliance = component(profile.alliance_surcharge_percent)?;
    let complete = eiv.is_some() && profile.manual_system_cost_index.is_some();
    let total = if complete {
        let mut total = fixed_supplemental_cost;
        for value in [index_cost, facility_tax, scc, alliance]
            .into_iter()
            .flatten()
        {
            total = total
                .checked_add(value)
                .map_err(|_| FacilityError::ArithmeticOverflow)?;
        }
        Some(total)
    } else {
        None
    };
    Ok(InstallationCostBreakdown {
        complete,
        estimated_item_value: eiv,
        system_cost_index: profile.manual_system_cost_index,
        unmodified_system_index_cost: unmodified_index_cost,
        job_cost_reduction_percent: profile.job_cost_reduction_percent,
        system_index_cost: index_cost,
        facility_tax,
        scc_surcharge: scc,
        alliance_surcharge: alliance,
        fixed_supplemental_cost,
        total,
        warnings,
        formula_version: FACILITY_FORMULA_VERSION.into(),
    })
}

/// Sums one material line over every job of `split`, each job rounded on its
/// own with EVE's whole-item floor: `max(job_runs, ceil(exact(job_runs)))`.
/// `exact` maps a job's extended base quantity to its rounded-up quantity
/// and the trace of its factors. A single job keeps the historical
/// `ceil(max(runs, ...))` trace.
fn per_job_quantity(
    split: JobSplit,
    quantity_per_run: u64,
    exact: impl Fn(u64) -> Result<(u64, String), FacilityError>,
) -> Result<(u64, String), FacilityError> {
    let mut total = 0_u64;
    let mut traces = Vec::new();
    for (job_runs, job_count) in split.jobs() {
        let job_extended = quantity_per_run
            .checked_mul(job_runs)
            .ok_or(FacilityError::ArithmeticOverflow)?;
        let (rounded, factors) = exact(job_extended)?;
        let job_quantity = rounded.max(job_runs);
        total = job_quantity
            .checked_mul(job_count)
            .and_then(|value| total.checked_add(value))
            .ok_or(FacilityError::ArithmeticOverflow)?;
        traces.push(if split.job_count() == 1 {
            format!("ceil(max(runs, {factors}))")
        } else {
            format!("{job_count} x ceil(max({job_runs}, {factors}))")
        });
    }
    Ok((total, traces.join(" + ")))
}

fn duration_basis_detail(split: JobSplit, source: &str) -> String {
    if split.job_count() == 1 {
        format!("{} runs {source}", split.longest_job_runs())
    } else {
        format!(
            "{} runs per job ({} parallel jobs) {source}",
            split.longest_job_runs(),
            split.job_count()
        )
    }
}

fn reduction_factor(
    base_percent: Decimal,
    mut extras: impl Iterator<Item = Decimal>,
) -> Result<Decimal, FacilityError> {
    extras.try_fold(
        Decimal::ONE - base_percent / Decimal::ONE_HUNDRED,
        |factor, percent| {
            factor
                .checked_mul(Decimal::ONE - percent / Decimal::ONE_HUNDRED)
                .ok_or(FacilityError::ArithmeticOverflow)
        },
    )
}

pub(crate) fn decimal_to_ceiling_u64(value: Decimal) -> Result<u64, FacilityError> {
    value
        .round_dp_with_strategy(0, RoundingStrategy::ToPositiveInfinity)
        .to_string()
        .parse()
        .map_err(|_| FacilityError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests;
