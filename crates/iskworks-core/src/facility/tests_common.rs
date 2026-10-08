//! Shared fixtures for the facility submodule tests.

use std::collections::BTreeSet;
use std::str::FromStr;

use rust_decimal::Decimal;
use uuid::Uuid;

use crate::{CapturedReactionFormula, CapturedRecipe, CapturedRecipeLine, WorkspaceId};

use super::{
    parse_profile, CreateFacilityProfileCommand, FacilityKind, FacilityRig, FacilityRole,
    IndustryFacilityProfile, ProductClassification, RigApplicability, RigTargetFilter,
    SecurityClass,
};

/// No product classification -- every `Unrestricted` rig still applies,
/// so preview results are unchanged from before rig applicability
/// existed. Classification-aware tests pass a real classification.
pub(crate) const ANY_PRODUCT: ProductClassification = ProductClassification {
    category_id: None,
    group_id: None,
};

pub(crate) fn product_in(category_id: Option<i64>, group_id: Option<i64>) -> ProductClassification {
    ProductClassification {
        category_id,
        group_id,
    }
}

/// A fitted rig with no applicability restriction -- the historical shape
/// before `RigApplicability`, kept as the default for tests that predate
/// the feature.
pub(crate) fn unrestricted_rig(
    slot: u8,
    type_id: i64,
    name: &str,
    material: &str,
    time: &str,
) -> FacilityRig {
    FacilityRig {
        slot_number: slot,
        type_id,
        type_name: name.into(),
        material_reduction_percent: Decimal::from_str(material).unwrap(),
        time_reduction_percent: Decimal::from_str(time).unwrap(),
        applicability: RigApplicability::default(),
    }
}

/// A rig whose material and time bonuses are both restricted to the given
/// product groups (the common single-`filterID` case).
pub(crate) fn group_scoped_rig(
    slot: u8,
    type_id: i64,
    name: &str,
    material: &str,
    time: &str,
    group_ids: impl IntoIterator<Item = i64>,
) -> FacilityRig {
    let filter = RigTargetFilter::Restricted {
        category_ids: BTreeSet::new(),
        group_ids: group_ids.into_iter().collect(),
    };
    FacilityRig {
        applicability: RigApplicability {
            material: filter.clone(),
            time: filter,
        },
        ..unrestricted_rig(slot, type_id, name, material, time)
    }
}

pub(crate) fn fixture_profile(material: &str, time: &str) -> IndustryFacilityProfile {
    parse_profile(
        WorkspaceId(Uuid::nil()),
        CreateFacilityProfileCommand {
            name: "Test".into(),
            kind: FacilityKind::Manual,
            role: FacilityRole::Manufacturing,
            structure_id: None,
            structure_type_id: None,
            structure_type_name: String::new(),
            solar_system_id: None,
            solar_system_name: "Jita".into(),
            security_class: SecurityClass::HighSec,
            material_reduction_percent: material.into(),
            time_reduction_percent: time.into(),
            job_cost_reduction_percent: "0".into(),
            facility_tax_percent: "1".into(),
            scc_surcharge_percent: "0.5".into(),
            alliance_surcharge_percent: String::new(),
            fixed_supplemental_cost: String::new(),
            manual_system_cost_index: Some("0.05".into()),
            notes: String::new(),
            rigs: vec![],
        },
    )
    .unwrap()
}

pub(crate) fn fixture_recipe() -> CapturedRecipe {
    CapturedRecipe {
        source_sde_dataset_id: Uuid::nil(),
        source_sde_version: "fixture".into(),
        blueprint_type_id: 1,
        blueprint_name: "Blueprint".into(),
        duration_seconds_per_run: Some(1_000),
        materials: vec![
            CapturedRecipeLine {
                type_id: 34,
                type_name: "Tritanium".into(),
                quantity_per_run: 100,
                sort_order: 0,
            },
            CapturedRecipeLine {
                type_id: 35,
                type_name: "Component".into(),
                quantity_per_run: 1,
                sort_order: 1,
            },
        ],
        products: vec![CapturedRecipeLine {
            type_id: 36,
            type_name: "Product".into(),
            quantity_per_run: 1,
            sort_order: 0,
        }],
        fingerprint: "recipe".into(),
    }
}

pub(crate) fn fixture_reaction_formula() -> CapturedReactionFormula {
    CapturedReactionFormula::capture(
        Uuid::nil(),
        "fixture".into(),
        iskworks_sde::ReactionFormulaRecipe {
            reaction_formula_type_id: 46_157,
            reaction_formula_name: "Methanofullerene Reaction Formula".into(),
            duration_seconds: Some(10_800),
            materials: vec![
                iskworks_sde::RecipeLine {
                    type_id: 37,
                    type_name: "Isogen".into(),
                    quantity: 300,
                },
                iskworks_sde::RecipeLine {
                    type_id: 4_246,
                    type_name: "Fullerite-C50".into(),
                    quantity: 5,
                },
            ],
            products: vec![iskworks_sde::RecipeLine {
                type_id: 30_306,
                type_name: "Methanofullerene".into(),
                quantity: 160,
            }],
        },
    )
    .unwrap()
}
