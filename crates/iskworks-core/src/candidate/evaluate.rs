use super::*;

pub fn compare_build_plan_candidate(
    current: &BuildPlanRevision,
    candidate: &CalculatedBuildPlan,
) -> Result<BuildPlanCandidateComparison, IndustryError> {
    let current_by_type = current
        .material_lines
        .iter()
        .map(|line| (line.type_id, line))
        .collect::<std::collections::BTreeMap<_, _>>();
    let candidate_by_type = candidate
        .material_lines
        .iter()
        .map(|line| (line.type_id, line))
        .collect::<std::collections::BTreeMap<_, _>>();
    let type_ids = current_by_type
        .keys()
        .chain(candidate_by_type.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let materials = type_ids
        .into_iter()
        .map(|type_id| {
            let current = current_by_type.get(&type_id);
            let candidate = candidate_by_type.get(&type_id);
            let current_quantity = current.map_or(0, |line| line.total_quantity);
            let candidate_quantity = candidate.map_or(0, |line| line.total_quantity);
            Ok(CandidateMaterialComparison {
                type_id,
                type_name: candidate
                    .or(current)
                    .map(|line| line.type_name.clone())
                    .unwrap_or_default(),
                current_quantity,
                candidate_quantity,
                delta: signed_quantity_delta(candidate_quantity, current_quantity)?,
                available_inventory: 0,
                candidate_covered_quantity: 0,
                candidate_missing_quantity: candidate_quantity,
            })
        })
        .collect::<Result<Vec<_>, IndustryError>>()?;
    let current_total_material_units =
        current.material_lines.iter().try_fold(0_u64, |sum, line| {
            sum.checked_add(line.total_quantity)
                .ok_or(IndustryError::MoneyOverflow)
        })?;
    let candidate_total_material_units =
        candidate
            .material_lines
            .iter()
            .try_fold(0_u64, |sum, line| {
                sum.checked_add(line.total_quantity)
                    .ok_or(IndustryError::MoneyOverflow)
            })?;
    let current_duration_seconds = current
        .blueprint
        .as_ref()
        .and_then(|blueprint| blueprint.planned_duration_seconds);
    let candidate_duration_seconds = candidate
        .blueprint
        .as_ref()
        .and_then(|blueprint| blueprint.planned_duration_seconds);

    Ok(BuildPlanCandidateComparison {
        materials,
        current_total_material_units,
        candidate_total_material_units,
        total_material_units_delta: signed_quantity_delta(
            candidate_total_material_units,
            current_total_material_units,
        )?,
        current_duration_seconds,
        candidate_duration_seconds,
        duration_seconds_delta: match (candidate_duration_seconds, current_duration_seconds) {
            (Some(candidate), Some(current)) => Some(signed_quantity_delta(candidate, current)?),
            _ => None,
        },
    })
}

pub(super) fn signed_quantity_delta(candidate: u64, current: u64) -> Result<i64, IndustryError> {
    let delta = i128::from(candidate) - i128::from(current);
    i64::try_from(delta).map_err(|_| IndustryError::MoneyOverflow)
}

#[must_use]
pub fn candidate_completeness(
    pricing_complete: bool,
    installation_complete: bool,
    inventory_cost_complete: bool,
) -> (CandidateCompleteness, ProfitabilityBasis) {
    let profitability = if !pricing_complete {
        ProfitabilityState::Unavailable
    } else if installation_complete && inventory_cost_complete {
        ProfitabilityState::Complete
    } else {
        ProfitabilityState::Qualified
    };
    let mut included_costs = Vec::new();
    if inventory_cost_complete {
        included_costs.push(ProfitabilityCost::ProjectedInventoryCost);
    } else if pricing_complete {
        included_costs.push(ProfitabilityCost::MarketMaterialEstimate);
    }
    if installation_complete {
        included_costs.push(ProfitabilityCost::InstallationCost);
    }
    let mut excluded_costs = vec![
        ProfitabilityCost::MarketFees,
        ProfitabilityCost::SalesTax,
        ProfitabilityCost::Hauling,
    ];
    if !installation_complete {
        excluded_costs.insert(0, ProfitabilityCost::InstallationCost);
    }

    (
        CandidateCompleteness {
            materials: CalculationState::Complete,
            duration: CalculationState::Complete,
            pricing: if pricing_complete {
                CalculationState::Complete
            } else {
                CalculationState::Incomplete
            },
            installation: if installation_complete {
                CalculationState::Complete
            } else {
                CalculationState::Incomplete
            },
            inventory_cost: if inventory_cost_complete {
                CalculationState::Complete
            } else {
                CalculationState::Incomplete
            },
            profitability,
        },
        ProfitabilityBasis {
            included_costs,
            excluded_costs,
        },
    )
}

#[must_use]
pub fn market_price_warnings(price_lines: &[PriceSnapshotLine]) -> Vec<CandidateIssue> {
    if price_lines
        .iter()
        .any(|line| line.source_note.contains(STALE_MARKET_PROVENANCE))
    {
        vec![CandidateIssue {
            code: "staleMarketObservations".into(),
            message: "Imported market observations are stale; candidate prices may no longer reflect the market.".into(),
        }]
    } else {
        Vec::new()
    }
}

pub fn candidate_decision(
    pricing_complete: bool,
    coverage: &crate::BuildCoverageReport,
) -> Result<CandidateDecisionGuidance, IndustryError> {
    let shortage_type_count = coverage
        .material_lines
        .iter()
        .filter(|line| line.missing_quantity > 0)
        .count();
    let missing_units = coverage
        .material_lines
        .iter()
        .try_fold(0_u64, |total, line| {
            total
                .checked_add(line.missing_quantity)
                .ok_or(IndustryError::MoneyOverflow)
        })?;
    Ok(if !pricing_complete {
        CandidateDecisionGuidance {
            headline: "Candidate pricing is incomplete.".into(),
            supporting_text:
                "Resolve missing material or output prices before applying this plan revision."
                    .into(),
            tone: CandidateDecisionTone::Blocking,
        }
    } else if missing_units > 0 {
        let shortage_label = if shortage_type_count == 1 {
            "1 input type has a shortage.".to_owned()
        } else {
            format!("{shortage_type_count} input types have shortages.")
        };
        CandidateDecisionGuidance {
            headline: shortage_label,
            supporting_text: format!(
                "{} total units must be acquired before production.",
                missing_units
            ),
            tone: CandidateDecisionTone::Warning,
        }
    } else {
        CandidateDecisionGuidance {
            headline: "Candidate materials are fully covered.".into(),
            supporting_text:
                "Current accounted inventory can cover every candidate material requirement.".into(),
            tone: CandidateDecisionTone::Positive,
        }
    })
}

/// See [`MaterialCostEvidence`]'s doc comment for what the staged totals
/// this builds actually represent, and why a self-produced Build/Reaction
/// row is deliberately excluded from the per-stage repricing.
#[allow(clippy::too_many_arguments)]
pub(super) fn calculation_evidence(
    runs: u64,
    material_lines: &[PlannedMaterialLine],
    pricing_complete: bool,
    expected_revenue: Option<Money>,
    estimated_profit: Option<Money>,
    facility: Option<&FacilityPlanPreview>,
    blueprint_me: Option<u8>,
    max_runs_per_job: Option<u64>,
) -> Result<CalculationEvidenceProjection, IndustryError> {
    let split = crate::JobSplit::for_runs(runs, max_runs_per_job);
    let blueprint_me = blueprint_me.unwrap_or(0);
    let blueprint_multiplier = Decimal::ONE - Decimal::from(blueprint_me) / Decimal::ONE_HUNDRED;
    let structure_multiplier = facility
        .map(|preview| {
            Decimal::ONE - preview.profile.material_reduction_percent / Decimal::ONE_HUNDRED
        })
        .unwrap_or(Decimal::ONE);
    // Only the rigs the preview actually applied to this product -- a
    // fitted-but-non-applicable rig contributes no bonus.
    let rig_multiplier = facility
        .map(|preview| preview.rig_material_factor)
        .unwrap_or(Decimal::ONE);
    // The root facility's own requirements only cover the root recipe's
    // direct materials. A component-expanded row (a build-resolved
    // sub-component's own materials, or the sub-component itself) has no
    // entry there -- no ME/TE is applied to those rows in the first place
    // (apply_component_expansion applies none), so their base quantity is
    // just their own quantity_per_run x runs, same as the no-facility case.
    let facility_base_quantities: BTreeMap<i64, u64> = facility
        .map(|preview| {
            preview
                .requirements
                .iter()
                .map(|line| (line.type_id, line.base_extended_quantity))
                .collect()
        })
        .unwrap_or_default();
    let base_quantities: BTreeMap<i64, u64> = material_lines
        .iter()
        .map(|line| {
            let base = facility_base_quantities
                .get(&line.type_id)
                .copied()
                .unwrap_or_else(|| line.quantity_per_run.saturating_mul(runs));
            (line.type_id, base)
        })
        .collect();
    // A Buy row is repriced at each stage's ME/structure/rig-adjusted
    // quantity, since that's exactly what buying more or less would cost.
    // A self-produced Build/Reaction row has no such per-unit market price
    // (`BuildCostProjection::apply_to_revision` leaves `unit_price: None` for one on
    // purpose) -- its planning cost was already computed independently, from
    // its own recipe, own ME/facility effects, own dynamic runs, own
    // installation, and proportional consumed production basis. The parent's
    // buy-side ME/structure multiplier must not be applied to that cost a
    // second time, so such a row's `line_total` is carried unmultiplied into
    // every stage instead -- see `MaterialCostEvidence`'s doc comment.
    let stage_cost = |multiplier: Decimal| -> Result<Option<Money>, IndustryError> {
        if !pricing_complete {
            return Ok(None);
        }
        let mut total = Money::zero();
        for line in material_lines {
            let contribution = match (line.unit_price, line.is_build_resolved) {
                (Some(price), _) => {
                    let base = *base_quantities
                        .get(&line.type_id)
                        .ok_or(IndustryError::InvalidRecipe)?;
                    // Rounded per job, like the authoritative quantity
                    // (`facility::preview`'s per-job sum).
                    let per_run = base / runs.max(1);
                    let mut quantity = 0_u64;
                    for (job_runs, job_count) in split.jobs() {
                        let job_base = if split.job_count() == 1 {
                            base
                        } else {
                            per_run.saturating_mul(job_runs)
                        };
                        let job_quantity = (Decimal::from(job_base) * multiplier)
                            .ceil()
                            .to_u64()
                            .ok_or(IndustryError::MoneyOverflow)?
                            .max(job_runs);
                        quantity = job_quantity
                            .checked_mul(job_count)
                            .and_then(|value| quantity.checked_add(value))
                            .ok_or(IndustryError::MoneyOverflow)?;
                    }
                    price.checked_mul_quantity(quantity)?
                }
                (None, true) => match line.line_total {
                    Some(total) => total,
                    // `pricing_complete` promises every row's cost is fully
                    // known, so a resolved row with neither a unit price nor
                    // a line total should be unreachable in practice -- but
                    // if that promise doesn't hold for this row, report the
                    // evidence as incomplete rather than fabricate a price
                    // or misreport a perfectly valid recipe as invalid.
                    None => return Ok(None),
                },
                // A Buy row with no price despite `pricing_complete: true`
                // is an internally inconsistent caller state, distinct from
                // a self-produced row (which legitimately has none).
                (None, false) => return Err(IndustryError::InvalidRecipe),
            };
            total = total.checked_add(contribution)?;
        }
        Ok(Some(total))
    };
    let profit_margin_percent = match (estimated_profit, expected_revenue) {
        (Some(profit), Some(revenue)) if !revenue.0.is_zero() => {
            let mut percent = profit
                .0
                .checked_div(revenue.0)
                .and_then(|value| value.checked_mul(Decimal::ONE_HUNDRED))
                .ok_or(IndustryError::MoneyOverflow)?;
            percent.rescale(1);
            Some(percent.to_string())
        }
        _ => None,
    };
    Ok(CalculationEvidenceProjection {
        profit_margin_percent,
        system_cost_index_percent: facility
            .and_then(|preview| preview.installation_cost.system_cost_index)
            .map(|index| {
                let mut percent = index * Decimal::ONE_HUNDRED;
                percent.rescale(2);
                percent.to_string()
            }),
        material_cost: MaterialCostEvidence {
            complete: pricing_complete,
            base_market_value: stage_cost(Decimal::ONE)?,
            after_blueprint_me: stage_cost(blueprint_multiplier)?,
            after_structure: stage_cost(blueprint_multiplier * structure_multiplier)?,
            adjusted_material_cost: material_lines.iter().try_fold(
                Money::zero(),
                |total, line| {
                    line.line_total
                        .map_or(Ok(total), |value| total.checked_add(value))
                },
            )?,
            blueprint_multiplier,
            structure_multiplier,
            rig_multiplier,
            requirement_traces: facility
                .map(|preview| {
                    preview
                        .requirements
                        .iter()
                        .map(|line| line.calculation_trace.clone())
                        .collect()
                })
                .unwrap_or_default(),
        },
        duration_steps: facility
            .map(|preview| preview.duration_steps.clone())
            .unwrap_or_default(),
    })
}
