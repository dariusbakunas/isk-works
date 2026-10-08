use super::*;

pub fn build_create_candidate_preview(
    candidate: BuildPlanRevision,
    coverage: crate::BuildCoverageReport,
    worksheet: crate::ProductionWorksheetProjection,
) -> Result<CreateBuildPlanPreview, IndustryError> {
    let calculation_evidence = calculation_evidence(
        candidate.runs,
        &candidate.material_lines,
        candidate.pricing_complete,
        candidate.expected_revenue,
        candidate.estimated_margin,
        candidate.root_facility(),
        candidate
            .blueprint
            .as_ref()
            .map(|blueprint| blueprint.material_efficiency),
        candidate
            .blueprint
            .as_ref()
            .and_then(crate::BlueprintSnapshot::max_runs_per_job),
    )?;
    let mut validation = CandidateValidation::default();
    if !candidate.pricing_complete {
        validation
            .blockers
            .push(pricing_incomplete_issue(&candidate.snapshot.items));
    }
    let installation_complete = candidate
        .root_facility()
        .is_some_and(|facility| facility.installation_cost.complete);
    let (mut completeness, profitability_basis) = candidate_completeness(
        candidate.pricing_complete,
        installation_complete,
        coverage.complete_cost_coverage,
    );
    if candidate.root_facility().is_none() {
        completeness.installation = CalculationState::NotConfigured;
    }
    let mut warnings = market_price_warnings(&candidate.snapshot.items);
    warnings.extend(
        candidate
            .root_facility()
            .iter()
            .flat_map(|facility| facility.warnings.iter())
            .enumerate()
            .map(|(index, message)| CandidateIssue {
                code: format!("facilityWarning{index}"),
                message: message.clone(),
            }),
    );
    let projected_inventory_cost = if coverage.complete_cost_coverage {
        coverage
            .material_lines
            .iter()
            .try_fold(Money::zero(), |total, line| {
                line.projected_historical_cost
                    .ok_or(IndustryError::MoneyOverflow)
                    .and_then(|cost| total.checked_add(cost))
            })
            .map(Some)?
    } else {
        None
    };
    let decision = candidate_decision(candidate.pricing_complete, &coverage)?;
    Ok(CreateBuildPlanPreview {
        candidate_fingerprint: create_candidate_fingerprint(&candidate),
        can_plan: validation.blockers.is_empty(),
        candidate,
        coverage,
        projected_inventory_cost,
        decision,
        validation,
        warnings,
        completeness,
        profitability_basis,
        calculation_evidence,
        worksheet,
    })
}

pub(super) fn create_candidate_fingerprint(candidate: &BuildPlanRevision) -> CandidateFingerprint {
    let blueprint = candidate.blueprint.as_ref().map(|item| {
        serde_json::json!({
            "sourceMode": item.source_mode,
            "blueprintTypeId": item.blueprint_type_id,
            "kind": item.kind,
            "materialEfficiency": item.material_efficiency,
            "timeEfficiency": item.time_efficiency,
            "licensedRuns": item.licensed_runs,
            "requestedRuns": item.requested_runs,
            "sourceObservationId": item.source_observation_id,
            "observedAt": item.observed_at,
            "plannedDurationSeconds": item.planned_duration_seconds,
            "formulaVersion": item.formula_version,
        })
    });
    let facility_fingerprint = |item: &FacilityPlanPreview| {
        serde_json::json!({
            "profileId": item.profile.id,
            "profileRevision": item.profile.revision,
            "blueprintMe": item.blueprint_me,
            "blueprintTe": item.blueprint_te,
            "requirements": item.requirements,
            "plannedDurationSeconds": item.planned_duration_seconds,
            "installationCost": item.installation_cost,
            "formulaVersion": item.formula_version,
        })
    };
    let manufacturing_facility = candidate
        .manufacturing_facility
        .as_ref()
        .map(facility_fingerprint);
    let reaction_facility = candidate
        .reaction_facility
        .as_ref()
        .map(facility_fingerprint);
    let semantic = serde_json::json!({
        "runs": candidate.runs,
        "recipeFingerprint": candidate.recipe_fingerprint,
        "priceSourceId": candidate.snapshot.price_source_id,
        "priceSourceRevision": candidate.snapshot.source_revision,
        "priceItems": candidate.snapshot.items,
        "pricingComplete": candidate.pricing_complete,
        "estimatedMaterialCost": candidate.estimated_material_cost,
        "expectedRevenue": candidate.expected_revenue,
        "estimatedMargin": candidate.estimated_margin,
        "materialLines": candidate.material_lines,
        "manufacturingFacility": manufacturing_facility,
        "reactionFacility": reaction_facility,
        "blueprint": blueprint,
    });
    CandidateFingerprint(format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&semantic)
                .expect("Create candidate fingerprint values are serializable")
        )
    ))
}

pub(super) fn pricing_incomplete_issue(price_lines: &[PriceSnapshotLine]) -> CandidateIssue {
    let names = price_lines
        .iter()
        .filter(|line| line.missing)
        .map(|line| line.type_name.as_str())
        .collect::<Vec<_>>();
    let missing = match names.as_slice() {
        [] => "one or more required items".to_owned(),
        [name] => (*name).to_owned(),
        [first, second] => format!("{first} and {second}"),
        _ => {
            let (last, rest) = names.split_last().expect("non-empty missing price names");
            format!("{}, and {last}", rest.join(", "))
        }
    };
    CandidateIssue {
        code: "pricingIncomplete".into(),
        message: format!("Missing market prices: {missing}."),
    }
}
