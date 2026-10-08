use super::*;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildPlanCandidateInput {
    pub expected_build_revision: u64,
    pub expected_active_plan_revision: u64,
    pub runs: u64,
    #[serde(default)]
    pub blueprint: Option<CandidateBlueprintSelection>,
    pub price_source_id: PriceSourceId,
    pub expected_price_source_revision: u64,
    #[serde(default)]
    pub pricing_selections: Vec<ItemPricingSelectionInput>,
    #[serde(default)]
    pub manufacturing_facility: CandidateFacilitySelection,
    #[serde(default)]
    pub reaction_facility: CandidateFacilitySelection,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CandidateBlueprintSelection {
    Manual {
        kind: BlueprintKind,
        material_efficiency: u8,
        time_efficiency: u8,
        licensed_runs: Option<u64>,
        #[serde(default)]
        notes: String,
    },
    ObservedAsset {
        observation_id: Uuid,
        observed_at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CandidateFacilitySelection {
    #[default]
    None,
    Profile {
        facility_profile_id: FacilityProfileId,
        expected_facility_profile_revision: u64,
        adjusted_price_eiv: Option<String>,
    },
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedBuildPlanCandidate {
    pub expected_build_revision: u64,
    pub expected_active_plan_revision: u64,
    pub runs: u64,
    pub blueprint: Option<CandidateBlueprintSelection>,
    pub price_source_id: PriceSourceId,
    pub expected_price_source_revision: u64,
    pub pricing_selections: Vec<ItemPricingSelectionInput>,
    pub manufacturing_facility: CandidateFacilitySelection,
    pub reaction_facility: CandidateFacilitySelection,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CandidateFingerprint(pub String);

impl NormalizedBuildPlanCandidate {
    #[must_use]
    pub fn fingerprint(&self) -> CandidateFingerprint {
        let canonical = serde_json::to_vec(self)
            .expect("normalized candidate serialization only contains supported domain values");
        CandidateFingerprint(format!("{:x}", Sha256::digest(canonical)))
    }
}

pub fn normalize_candidate(
    input: BuildPlanCandidateInput,
) -> Result<NormalizedBuildPlanCandidate, IndustryError> {
    if input.runs == 0 || input.runs > 1_000_000 {
        return Err(IndustryError::Validation(
            "Build runs must be between 1 and 1,000,000.".into(),
        ));
    }

    let blueprint = input.blueprint.map(normalize_blueprint).transpose()?;
    let pricing_selections = normalize_pricing_selections(input.pricing_selections)?;

    let manufacturing_facility = normalize_candidate_facility(input.manufacturing_facility)?;
    let reaction_facility = normalize_candidate_facility(input.reaction_facility)?;

    Ok(NormalizedBuildPlanCandidate {
        expected_build_revision: input.expected_build_revision,
        expected_active_plan_revision: input.expected_active_plan_revision,
        runs: input.runs,
        blueprint,
        price_source_id: input.price_source_id,
        expected_price_source_revision: input.expected_price_source_revision,
        pricing_selections,
        manufacturing_facility,
        reaction_facility,
    })
}

pub(super) fn normalize_candidate_facility(
    facility: CandidateFacilitySelection,
) -> Result<CandidateFacilitySelection, IndustryError> {
    Ok(match facility {
        CandidateFacilitySelection::None => CandidateFacilitySelection::None,
        CandidateFacilitySelection::Profile {
            facility_profile_id,
            expected_facility_profile_revision,
            adjusted_price_eiv,
        } => CandidateFacilitySelection::Profile {
            facility_profile_id,
            expected_facility_profile_revision,
            adjusted_price_eiv: adjusted_price_eiv
                .map(|value| Money::parse(&value).map(|money| money.0.to_string()))
                .transpose()?,
        },
    })
}

pub fn normalize_pricing_selections(
    mut selections: Vec<ItemPricingSelectionInput>,
) -> Result<Vec<ItemPricingSelectionInput>, IndustryError> {
    selections.sort_by_key(|item| (item.role, item.type_id));
    let mut seen = BTreeSet::new();

    for item in &mut selections {
        if item.type_id <= 0 || !seen.insert((item.role, item.type_id)) {
            return Err(IndustryError::Validation(
                "Each pricing selection must identify one unique planner item.".into(),
            ));
        }
        if let ItemPricingSelection::Manual { unit_price } = &mut item.selection {
            *unit_price = Money::parse(unit_price)?.0.to_string();
        }
    }

    Ok(selections)
}

pub fn normalize_component_resolutions(
    resolutions: Vec<ComponentResolution>,
) -> Result<Vec<ComponentResolution>, IndustryError> {
    let mut seen = BTreeSet::new();
    for resolution in &resolutions {
        if resolution.type_id <= 0 || !seen.insert(resolution.type_id) {
            return Err(IndustryError::Validation(
                "Each component resolution must identify one unique material.".into(),
            ));
        }
    }
    Ok(resolutions)
}

pub fn normalize_draft_planning(
    mut input: DraftPlanningInput,
    requested_runs: u64,
) -> Result<DraftPlanningInput, IndustryError> {
    if requested_runs == 0 || requested_runs > 1_000_000 {
        return Err(IndustryError::Validation(
            "Build runs must be between 1 and 1,000,000.".into(),
        ));
    }
    if input.manual_price_list_id.is_some() != input.expected_manual_price_list_revision.is_some() {
        return Err(IndustryError::Validation(
            "Manual price list and its expected revision must either both be present or both be absent."
                .into(),
        ));
    }
    input.pricing_selections = normalize_pricing_selections(input.pricing_selections)?;
    input.component_resolutions = normalize_component_resolutions(input.component_resolutions)?;
    input.blueprint_selection = input
        .blueprint_selection
        .map(normalize_draft_blueprint)
        .transpose()?;
    if let Some(facility) = &mut input.manufacturing_facility {
        facility.estimated_item_value = facility
            .estimated_item_value
            .take()
            .map(|value| Money::parse(&value).map(|money| money.0.to_string()))
            .transpose()?;
    }
    if let Some(facility) = &mut input.reaction_facility {
        facility.estimated_item_value = facility
            .estimated_item_value
            .take()
            .map(|value| Money::parse(&value).map(|money| money.0.to_string()))
            .transpose()?;
    }
    Ok(input)
}

pub(super) fn normalize_draft_blueprint(
    selection: BlueprintSelection,
) -> Result<BlueprintSelection, IndustryError> {
    match selection {
        BlueprintSelection::Manual {
            kind,
            material_efficiency,
            time_efficiency,
            licensed_runs,
            notes,
        } => match normalize_blueprint(CandidateBlueprintSelection::Manual {
            kind,
            material_efficiency,
            time_efficiency,
            licensed_runs,
            notes,
        })? {
            CandidateBlueprintSelection::Manual {
                kind,
                material_efficiency,
                time_efficiency,
                licensed_runs,
                notes,
            } => Ok(BlueprintSelection::Manual {
                kind,
                material_efficiency,
                time_efficiency,
                licensed_runs,
                notes,
            }),
            CandidateBlueprintSelection::ObservedAsset { .. } => unreachable!(),
        },
        observed @ BlueprintSelection::ObservedAsset { .. } => Ok(observed),
    }
}

pub(super) fn normalize_blueprint(
    blueprint: CandidateBlueprintSelection,
) -> Result<CandidateBlueprintSelection, IndustryError> {
    match blueprint {
        CandidateBlueprintSelection::Manual {
            kind,
            material_efficiency,
            time_efficiency,
            licensed_runs,
            notes,
        } => {
            if kind == BlueprintKind::Unknown {
                return Err(crate::BlueprintError::ManualKindUnknown.into());
            }
            if material_efficiency > 10 {
                return Err(crate::BlueprintError::InvalidMaterialEfficiency.into());
            }
            if time_efficiency > 20 {
                return Err(crate::BlueprintError::InvalidTimeEfficiency.into());
            }
            // Requesting more runs than one copy licenses is never an
            // error: the planner splits it into one job per copy
            // (`BlueprintSnapshot::max_runs_per_job`).
            if kind == BlueprintKind::Copy && licensed_runs.map_or(true, |runs| runs == 0) {
                return Err(crate::BlueprintError::CopyRunsRequired.into());
            }
            Ok(CandidateBlueprintSelection::Manual {
                kind,
                material_efficiency,
                time_efficiency,
                licensed_runs,
                notes: notes.trim().to_string(),
            })
        }
        observed @ CandidateBlueprintSelection::ObservedAsset { .. } => Ok(observed),
    }
}
