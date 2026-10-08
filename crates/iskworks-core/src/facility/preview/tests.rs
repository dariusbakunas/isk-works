use std::str::FromStr;

use super::*;
use crate::facility::tests_common::*;
use crate::facility::{FacilityRig, RigApplicability, RigTargetFilter};

#[test]
fn no_bonus_matches_direct_requirements() {
    let result = preview_facility(
        &fixture_recipe(),
        3,
        fixture_profile("0", "0"),
        ANY_PRODUCT,
        0,
        0,
        None,
        None,
    )
    .unwrap();
    assert_eq!(result.requirements[0].final_required_quantity, 300);
    assert_eq!(result.requirements[1].final_required_quantity, 3);
    assert_eq!(result.planned_duration_seconds, Some(3_000));
}

#[test]
fn adjusted_price_eiv_uses_base_materials_and_runs() {
    let prices = BTreeMap::from([
        (34, Decimal::from_str("3.5").unwrap()),
        (35, Decimal::from_str("10").unwrap()),
    ]);

    let result = calculate_adjusted_price_eiv(&fixture_recipe().materials, 3, &prices).unwrap();

    assert_eq!(result.value, Some(Money::parse("1080").unwrap()));
    assert!(result.missing_type_ids.is_empty());
}

#[test]
fn adjusted_price_eiv_is_incomplete_when_any_material_is_missing() {
    let prices = BTreeMap::from([(34, Decimal::from_str("3.5").unwrap())]);

    let result = calculate_adjusted_price_eiv(&fixture_recipe().materials, 3, &prices).unwrap();

    assert_eq!(result.value, None);
    assert_eq!(result.missing_type_ids, vec![35]);
}

#[test]
fn adjusted_price_eiv_is_rounded_to_money_precision() {
    let prices = BTreeMap::from([
        (34, Decimal::from_str("1.234567").unwrap()),
        (35, Decimal::from_str("2.345678").unwrap()),
    ]);

    let result = calculate_adjusted_price_eiv(&fixture_recipe().materials, 3, &prices).unwrap();

    assert_eq!(result.value, Some(Money::parse("377.4071").unwrap()));
}

#[test]
fn applies_me_and_facility_reductions_and_protects_whole_items() {
    let result = preview_facility(
        &fixture_recipe(),
        3,
        fixture_profile("2", "10"),
        ANY_PRODUCT,
        10,
        20,
        None,
        None,
    )
    .unwrap();
    assert_eq!(result.requirements[0].final_required_quantity, 265);
    assert_eq!(result.requirements[1].final_required_quantity, 3);
    assert_eq!(result.planned_duration_seconds, Some(2_160));
}

#[test]
fn role_rig_and_job_cost_modifiers_remain_separate() {
    let mut profile = fixture_profile("1", "30");
    profile.job_cost_reduction_percent = Decimal::from(5);
    profile.rigs.push(FacilityRig {
        slot_number: 1,
        type_id: 1,
        type_name: "Category manufacturing rig".into(),
        material_reduction_percent: Decimal::from(5),
        time_reduction_percent: Decimal::from_str("50.4").unwrap(),
        applicability: RigApplicability::default(),
    });
    let result = preview_facility(
        &fixture_recipe(),
        1,
        profile,
        ANY_PRODUCT,
        0,
        0,
        Some(Money::parse("1000").unwrap()),
        None,
    )
    .unwrap();

    assert_eq!(result.requirements[0].final_required_quantity, 95);
    assert_eq!(result.planned_duration_seconds, Some(348));
    assert_eq!(
        result.installation_cost.unmodified_system_index_cost,
        Some(Money::parse("50").unwrap())
    );
    assert_eq!(
        result.installation_cost.system_index_cost,
        Some(Money::parse("47.5").unwrap())
    );
    assert_eq!(
        result.installation_cost.total,
        Some(Money::parse("62.5").unwrap())
    );
}

#[test]
fn preview_facility_rejects_a_reaction_role_profile() {
    let mut profile = fixture_profile("0", "0");
    profile.role = FacilityRole::Reaction;

    let error =
        preview_facility(&fixture_recipe(), 1, profile, ANY_PRODUCT, 0, 0, None, None).unwrap_err();

    assert!(matches!(error, FacilityError::Validation(_)));
}

#[test]
fn preview_reaction_facility_rejects_a_manufacturing_role_profile() {
    let profile = fixture_profile("0", "0");

    let error =
        preview_reaction_facility(&fixture_reaction_formula(), 1, profile, ANY_PRODUCT, None)
            .unwrap_err();

    assert!(matches!(error, FacilityError::Validation(_)));
}

#[test]
fn reaction_preview_ignores_structure_bonus_fields_and_uses_rigs_only() {
    // The structure fields are deliberately nonzero here to prove the
    // reaction preview never reads them -- verified against real SDE
    // dogma that Refinery structures give zero base reaction discount,
    // unlike manufacturing Engineering Complexes.
    let mut profile = fixture_profile("50", "50");
    profile.role = FacilityRole::Reaction;

    let result =
        preview_reaction_facility(&fixture_reaction_formula(), 2, profile, ANY_PRODUCT, None)
            .unwrap();

    assert_eq!(result.requirements[0].final_required_quantity, 600);
    assert_eq!(result.requirements[1].final_required_quantity, 10);
    assert_eq!(result.planned_duration_seconds, Some(21_600));
    assert_eq!(result.formula_version, REACTION_FACILITY_FORMULA_VERSION);
}

#[test]
fn reaction_preview_applies_real_reaction_rig_bonuses() {
    // Real SDE data: type 46486 "Standup M-Set Composite Reactor
    // Material Efficiency I" gives a 2% reaction material discount and
    // is compatible with Refinery structures (group 1406). Reaction
    // rigs are split into separate ME-only and TE-only variants, unlike
    // manufacturing's combined M-Set rigs, so this rig alone gives no
    // time bonus.
    let mut profile = fixture_profile("0", "0");
    profile.role = FacilityRole::Reaction;
    profile.rigs.push(FacilityRig {
        slot_number: 1,
        type_id: 46_486,
        type_name: "Standup M-Set Composite Reactor Material Efficiency I".into(),
        material_reduction_percent: Decimal::from(2),
        time_reduction_percent: Decimal::ZERO,
        applicability: RigApplicability::default(),
    });

    let result =
        preview_reaction_facility(&fixture_reaction_formula(), 10, profile, ANY_PRODUCT, None)
            .unwrap();

    // 300 x 10 x 0.98 = 2940, exactly divisible, no rounding needed.
    assert_eq!(result.requirements[0].final_required_quantity, 2_940);
    // 5 x 10 x 0.98 = 49
    assert_eq!(result.requirements[1].final_required_quantity, 49);
    // Duration is untouched by the ME-only rig.
    assert_eq!(result.planned_duration_seconds, Some(108_000));
}

#[test]
fn manufacturing_preview_still_reports_its_blueprint_me() {
    let result = preview_facility(
        &fixture_recipe(),
        3,
        fixture_profile("2", "10"),
        ANY_PRODUCT,
        10,
        20,
        None,
        None,
    )
    .unwrap();

    assert_eq!(result.blueprint_me, Some(10));
    assert_eq!(result.blueprint_te, Some(20));
    assert_eq!(result.requirements[0].blueprint_me, Some(10));
}

#[test]
fn reaction_no_facility_effects_are_the_identity_case() {
    // No facility means no rigs, and a Refinery gives zero base bonus --
    // so this must be exactly the raw formula quantities x runs.
    let (requirements, duration) =
        preview_reaction_effects(&fixture_reaction_formula(), 10).unwrap();

    assert_eq!(requirements[0].final_required_quantity, 3_000);
    assert_eq!(requirements[1].final_required_quantity, 50);
    assert_eq!(duration, Some(108_000));
}

#[test]
fn reaction_facility_preview_converts_without_a_material_efficiency() {
    let mut profile = fixture_profile("0", "0");
    profile.role = FacilityRole::Reaction;
    profile.rigs.push(FacilityRig {
        slot_number: 1,
        type_id: 46_486,
        type_name: "Standup M-Set Composite Reactor Material Efficiency I".into(),
        material_reduction_percent: Decimal::from(2),
        time_reduction_percent: Decimal::ZERO,
        applicability: RigApplicability::default(),
    });

    let reaction_preview =
        preview_reaction_facility(&fixture_reaction_formula(), 10, profile, ANY_PRODUCT, None)
            .unwrap();
    let converted: FacilityPlanPreview = reaction_preview.clone().into();

    assert_eq!(converted.blueprint_me, None);
    assert_eq!(converted.blueprint_te, None);
    assert_eq!(converted.requirements[0].blueprint_me, None);
    assert_eq!(converted.requirements[0].final_required_quantity, 2_940);
    assert_eq!(converted.requirements[1].final_required_quantity, 49);
    assert_eq!(converted.planned_duration_seconds, Some(108_000));
    assert_eq!(converted.formula_version, REACTION_FACILITY_FORMULA_VERSION);
    assert_eq!(converted.formula_version, reaction_preview.formula_version);
}

#[test]
fn gez_rifter_materials_match_the_in_game_requirement() {
    let mut recipe = fixture_recipe();
    recipe.materials = vec![
        CapturedRecipeLine {
            type_id: 34,
            type_name: "Tritanium".into(),
            quantity_per_run: 32_000,
            sort_order: 0,
        },
        CapturedRecipeLine {
            type_id: 35,
            type_name: "Pyerite".into(),
            quantity_per_run: 6_000,
            sort_order: 1,
        },
        CapturedRecipeLine {
            type_id: 36,
            type_name: "Mexallon".into(),
            quantity_per_run: 2_500,
            sort_order: 2,
        },
        CapturedRecipeLine {
            type_id: 37,
            type_name: "Isogen".into(),
            quantity_per_run: 500,
            sort_order: 3,
        },
    ];
    let mut profile = fixture_profile("1", "30");
    profile.rigs.push(FacilityRig {
        slot_number: 1,
        type_id: 4_398,
        type_name: "Standup XL-Set Ship Manufacturing Efficiency II".into(),
        material_reduction_percent: Decimal::from_str("5.04").unwrap(),
        time_reduction_percent: Decimal::from_str("50.4").unwrap(),
        applicability: RigApplicability::default(),
    });

    let result = preview_facility(&recipe, 1, profile, ANY_PRODUCT, 10, 20, None, None).unwrap();
    let quantities = result
        .requirements
        .iter()
        .map(|line| line.final_required_quantity)
        .collect::<Vec<_>>();

    assert_eq!(quantities, vec![27_075, 5_077, 2_116, 424]);
    assert_eq!(quantities.into_iter().sum::<u64>(), 34_692);
}

#[test]
fn unrestricted_rig_still_applies_to_every_product() {
    // Guards against over-filtering: a rig with no applicability
    // restriction (structure base bonus, generic rig, or any profile
    // saved before the feature) must bonus every job, ship or not.
    let mut profile = fixture_profile("0", "0");
    profile
        .rigs
        .push(unrestricted_rig(1, 1, "Generic ME rig", "5", "10"));

    let ship_result = preview_facility(
        &fixture_recipe(),
        3,
        profile.clone(),
        product_in(Some(6), Some(25)),
        0,
        0,
        None,
        None,
    )
    .unwrap();
    let container_result = preview_facility(
        &fixture_recipe(),
        3,
        profile,
        product_in(Some(2), Some(448)),
        0,
        0,
        None,
        None,
    )
    .unwrap();

    // 300 x 0.95 = 285 in both cases.
    assert_eq!(ship_result.requirements[0].final_required_quantity, 285);
    assert_eq!(
        container_result.requirements[0].final_required_quantity,
        285
    );
    assert_eq!(ship_result.planned_duration_seconds, Some(2_700));
    assert_eq!(container_result.planned_duration_seconds, Some(2_700));
}

#[test]
fn nonapplicable_rig_contributes_no_bonus_and_equals_the_no_rig_result() {
    // The reported bug: a small-ship rig (SDE filter 5, groups 25/31/420)
    // on the facility must not touch a Station Warehouse Container
    // (category 2 Celestial, group 448 -- in no filter).
    let bare = preview_facility(
        &fixture_recipe(),
        3,
        fixture_profile("0", "0"),
        product_in(Some(2), Some(448)),
        0,
        0,
        None,
        None,
    )
    .unwrap();

    let mut with_ship_rig = fixture_profile("0", "0");
    with_ship_rig.rigs.push(group_scoped_rig(
        1,
        43_714,
        "Standup L-Set Basic Small Ship Manufacturing Efficiency I",
        "5",
        "10",
        [25, 31, 420],
    ));
    let rigged = preview_facility(
        &fixture_recipe(),
        3,
        with_ship_rig,
        product_in(Some(2), Some(448)),
        0,
        0,
        None,
        None,
    )
    .unwrap();

    assert_eq!(
        rigged
            .requirements
            .iter()
            .map(|line| line.final_required_quantity)
            .collect::<Vec<_>>(),
        bare.requirements
            .iter()
            .map(|line| line.final_required_quantity)
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        rigged.planned_duration_seconds,
        bare.planned_duration_seconds
    );
}

#[test]
fn applicable_rig_still_bonuses_a_product_in_its_target_group() {
    // Positive control: the same small-ship rig on a frigate Build
    // (group 25 in filter 5) does reduce material and time.
    let mut profile = fixture_profile("0", "0");
    profile.rigs.push(group_scoped_rig(
        1,
        43_714,
        "Standup L-Set Basic Small Ship Manufacturing Efficiency I",
        "5",
        "10",
        [25, 31, 420],
    ));

    let result = preview_facility(
        &fixture_recipe(),
        3,
        profile,
        product_in(Some(6), Some(25)),
        0,
        0,
        None,
        None,
    )
    .unwrap();

    // 300 x 0.95 = 285; duration 3000 x 0.9 = 2700.
    assert_eq!(result.requirements[0].final_required_quantity, 285);
    assert_eq!(result.planned_duration_seconds, Some(2_700));
}

#[test]
fn applicable_and_nonapplicable_rigs_together_only_fold_in_the_applicable_one() {
    let mut profile = fixture_profile("0", "0");
    profile.rigs.push(group_scoped_rig(
        1,
        43_714,
        "Small Ship ME rig",
        "5",
        "10",
        [25],
    ));
    profile.rigs.push(group_scoped_rig(
        2,
        43_920,
        "Structure/Component ME rig",
        "20",
        "20",
        [332, 334, 873],
    ));

    let result = preview_facility(
        &fixture_recipe(),
        3,
        profile,
        product_in(Some(6), Some(25)),
        0,
        0,
        None,
        None,
    )
    .unwrap();

    // Only the small-ship rig applies: 300 x 0.95 = 285, not 300 x 0.95 x 0.80.
    assert_eq!(result.requirements[0].final_required_quantity, 285);
    assert_eq!(result.rig_material_factor, Decimal::new(95, 2));
    assert_eq!(result.planned_duration_seconds, Some(2_700));
    let rig_step = result
        .duration_steps
        .iter()
        .find(|step| step.label == "Facility rig time bonuses")
        .unwrap();
    assert!(rig_step.detail.contains("Small Ship ME rig"));
    assert!(rig_step
        .detail
        .contains("not applicable to this product: Structure/Component ME rig"));
}

#[test]
fn preview_warns_about_a_fitted_rig_that_contributes_nothing() {
    let mut profile = fixture_profile("0", "0");
    profile.rigs.push(group_scoped_rig(
        1,
        43_714,
        "Standup L-Set Basic Small Ship Manufacturing Efficiency I",
        "5",
        "10",
        [25, 31, 420],
    ));

    let container = preview_facility(
        &fixture_recipe(),
        3,
        profile.clone(),
        product_in(Some(2), Some(448)),
        0,
        0,
        None,
        None,
    )
    .unwrap();
    assert!(container
        .warnings
        .iter()
        .any(|warning| warning.contains("don't apply to this product")
            && warning.contains("Standup L-Set Basic Small Ship Manufacturing Efficiency I")));

    // The same rig on an eligible frigate build produces no such warning.
    let frigate = preview_facility(
        &fixture_recipe(),
        3,
        profile,
        product_in(Some(6), Some(25)),
        0,
        0,
        None,
        None,
    )
    .unwrap();
    assert!(!frigate
        .warnings
        .iter()
        .any(|warning| warning.contains("don't apply to this product")));
}

#[test]
fn reaction_rig_applicability_is_filtered_the_same_way() {
    // Composite-reaction rig (SDE filter 18, group 428/429/4932) only
    // bonuses a formula whose product is in those groups.
    let make_profile = || {
        let mut profile = fixture_profile("0", "0");
        profile.role = FacilityRole::Reaction;
        profile.rigs.push(group_scoped_rig(
            1,
            46_486,
            "Standup M-Set Composite Reactor Material Efficiency I",
            "2",
            "0",
            [428, 429, 4932],
        ));
        profile
    };

    let applies = preview_reaction_facility(
        &fixture_reaction_formula(),
        10,
        make_profile(),
        product_in(None, Some(429)),
        None,
    )
    .unwrap();
    let does_not = preview_reaction_facility(
        &fixture_reaction_formula(),
        10,
        make_profile(),
        product_in(None, Some(974)),
        None,
    )
    .unwrap();

    // 300 x 10 x 0.98 = 2940 when it applies; raw 3000 when it doesn't.
    assert_eq!(applies.requirements[0].final_required_quantity, 2_940);
    assert_eq!(does_not.requirements[0].final_required_quantity, 3_000);
    assert_eq!(applies.rig_material_factor, Decimal::new(98, 2));
    assert_eq!(does_not.rig_material_factor, Decimal::ONE);
}

/// Post multi-filter fix: "Standup L-Set Reactor Efficiency I" resolves
/// to the union of reaction filters 16 + 17 + 18 (every reaction output
/// group). A Fullerides reaction (Composite, group 429) must get both
/// its 2% material and 20% time bonus and raise no skipped-rig warning.
#[test]
fn fullerides_reaction_accepts_the_l_set_reactor_efficiency_union() {
    let mut profile = fixture_profile("0", "0");
    profile.role = FacilityRole::Reaction;
    profile.rigs.push(group_scoped_rig(
        1,
        46_496,
        "Standup L-Set Reactor Efficiency I",
        "2",
        "20",
        [974, 712, 4096, 428, 429, 4932],
    ));

    let result = preview_reaction_facility(
        &fixture_reaction_formula(),
        10,
        profile,
        product_in(Some(4), Some(429)),
        None,
    )
    .unwrap();

    // 300 x 10 x 0.98 = 2940; 5 x 10 x 0.98 = 49; 10800 x 10 x 0.80 = 86400.
    assert_eq!(result.requirements[0].final_required_quantity, 2_940);
    assert_eq!(result.requirements[1].final_required_quantity, 49);
    assert_eq!(result.planned_duration_seconds, Some(86_400));
    assert!(!result
        .warnings
        .iter()
        .any(|warning| warning.contains("don't apply to this product")));
}

/// The fix must not make every reaction rig apply everywhere: a rig
/// genuinely scoped to Hybrid Reactions only (filter 16 -> group 974)
/// still contributes nothing to a Composite reaction (group 429).
#[test]
fn hybrid_only_reaction_rig_stays_non_applicable_for_a_composite_reaction() {
    let mut profile = fixture_profile("0", "0");
    profile.role = FacilityRole::Reaction;
    profile.rigs.push(group_scoped_rig(
        1,
        46_490,
        "Standup M-Set Hybrid Reactor Material Efficiency I",
        "2",
        "0",
        [974],
    ));

    let result = preview_reaction_facility(
        &fixture_reaction_formula(),
        10,
        profile,
        product_in(Some(4), Some(429)),
        None,
    )
    .unwrap();

    assert_eq!(result.requirements[0].final_required_quantity, 3_000);
    assert_eq!(result.planned_duration_seconds, Some(108_000));
    assert!(result.warnings.iter().any(|warning| {
        warning.contains("don't apply to this product")
            && warning.contains("Standup M-Set Hybrid Reactor Material Efficiency I")
    }));
}

/// Material and time filters stay independently evaluated after the
/// union change: a rig whose material bonus is scoped to Composite (429)
/// but whose time bonus is scoped to Hybrid (974) lands only its
/// material bonus on a Composite reaction -- and raises no warning,
/// since it does contribute something.
#[test]
fn reaction_rig_material_and_time_filters_stay_independent() {
    let mut profile = fixture_profile("0", "0");
    profile.role = FacilityRole::Reaction;
    let mut rig = group_scoped_rig(1, 46_496, "Split Reactor Rig", "2", "20", [429]);
    rig.applicability.time = RigTargetFilter::Restricted {
        category_ids: std::collections::BTreeSet::new(),
        group_ids: [974].into_iter().collect(),
    };
    profile.rigs.push(rig);

    let result = preview_reaction_facility(
        &fixture_reaction_formula(),
        10,
        profile,
        product_in(Some(4), Some(429)),
        None,
    )
    .unwrap();

    // Material bonus applies: 300 x 10 x 0.98 = 2940.
    assert_eq!(result.requirements[0].final_required_quantity, 2_940);
    // Time bonus does not: duration stays 10800 x 10.
    assert_eq!(result.planned_duration_seconds, Some(108_000));
    assert!(!result
        .warnings
        .iter()
        .any(|warning| warning.contains("don't apply to this product")));
}

/// The multi-filter manufacturing analogue: "Standup XL-Set Structure
/// and Component Manufacturing Efficiency" resolves to filters 12 + 13 +
/// 14 + 15. Building an actual Structure (filter 12 -> group 536) gets
/// the bonus that the pre-fix single-filter truncation would have
/// dropped.
#[test]
fn xl_set_structure_component_rig_union_bonuses_a_structure_build() {
    let mut profile = fixture_profile("0", "0");
    profile.rigs.push(group_scoped_rig(
        1,
        43_704,
        "Standup XL-Set Structure and Component Manufacturing Efficiency I",
        "5",
        "10",
        [536, 1136, 4736, 873, 332, 334, 716, 964, 913],
    ));

    let result = preview_facility(
        &fixture_recipe(),
        3,
        profile,
        product_in(Some(23), Some(536)),
        0,
        0,
        None,
        None,
    )
    .unwrap();

    // 100 x 3 x 0.95 = 285; duration 1000 x 3 x 0.90 = 2700.
    assert_eq!(result.requirements[0].final_required_quantity, 285);
    assert_eq!(result.planned_duration_seconds, Some(2_700));
    assert!(!result
        .warnings
        .iter()
        .any(|warning| warning.contains("don't apply to this product")));
}

fn single_line_recipe(quantity_per_run: u64) -> CapturedRecipe {
    let mut recipe = fixture_recipe();
    recipe.materials = vec![CapturedRecipeLine {
        type_id: 34,
        type_name: "Component".into(),
        quantity_per_run,
        sort_order: 0,
    }];
    recipe
}

/// Four 1-run BPCs are four jobs, each rounded on its own: a 4-run job
/// would need ceil(4 x 15 x 0.98 x 0.976) = 58, four 1-run jobs need
/// 4 x ceil(15 x 0.98 x 0.976) = 60.
#[test]
fn per_job_rounding_sums_each_bpc_job() {
    let preview = |max_runs_per_job| {
        preview_facility(
            &single_line_recipe(15),
            4,
            fixture_profile("2.4", "0"),
            ANY_PRODUCT,
            2,
            0,
            None,
            max_runs_per_job,
        )
        .unwrap()
    };

    let one_job = preview(None);
    let four_jobs = preview(Some(1));

    assert_eq!(one_job.requirements[0].final_required_quantity, 58);
    assert_eq!(one_job.requirements[0].job_count, 1);
    assert_eq!(four_jobs.requirements[0].final_required_quantity, 60);
    assert_eq!(four_jobs.requirements[0].job_count, 4);
    assert_eq!(four_jobs.requirements[0].runs, 4);
    assert_eq!(four_jobs.requirements[0].base_extended_quantity, 60);
    assert!(four_jobs.requirements[0]
        .calculation_trace
        .starts_with("4 x ceil(max(1, "));
}

#[test]
fn uneven_split_rounds_the_remainder_job_separately() {
    let result = preview_facility(
        &fixture_recipe(),
        5,
        fixture_profile("2", "10"),
        ANY_PRODUCT,
        10,
        20,
        None,
        Some(2),
    )
    .unwrap();

    // 2 x ceil(200 x 0.9 x 0.98 = 176.4) + ceil(100 x 0.9 x 0.98 = 88.2)
    assert_eq!(result.requirements[0].final_required_quantity, 2 * 177 + 89);
    // The whole-item floor applies per job: max(2, ..) x 2 + max(1, ..).
    assert_eq!(result.requirements[1].final_required_quantity, 5);
    assert_eq!(result.requirements[0].job_count, 3);
    // Parallel jobs: the longest (2-run) job, 2000 x 0.8 x 0.9.
    assert_eq!(result.planned_duration_seconds, Some(1_440));
}

#[test]
fn fixed_installation_fee_is_charged_once_per_job() {
    let mut profile = fixture_profile("0", "0");
    profile.fixed_supplemental_cost = Money::parse("100").unwrap();
    let preview = |max_runs_per_job| {
        preview_facility(
            &fixture_recipe(),
            4,
            profile.clone(),
            ANY_PRODUCT,
            0,
            0,
            Some(Money::parse("1000").unwrap()),
            max_runs_per_job,
        )
        .unwrap()
        .installation_cost
    };

    let one_job = preview(None);
    let four_jobs = preview(Some(1));

    assert_eq!(
        four_jobs.total.unwrap().0 - one_job.total.unwrap().0,
        Decimal::from(300)
    );
    assert_eq!(
        four_jobs.fixed_supplemental_cost,
        Money::parse("400").unwrap()
    );
}

#[test]
fn no_facility_blueprint_effects_split_per_job() {
    let (one_job, one_job_duration) =
        preview_blueprint_effects(&single_line_recipe(15), 4, 2, 20, None).unwrap();
    let (four_jobs, four_jobs_duration) =
        preview_blueprint_effects(&single_line_recipe(15), 4, 2, 20, Some(1)).unwrap();

    // ceil(60 x 0.98 = 58.8) vs 4 x ceil(14.7)
    assert_eq!(one_job[0].final_required_quantity, 59);
    assert_eq!(four_jobs[0].final_required_quantity, 60);
    assert_eq!(four_jobs[0].job_count, 4);
    assert_eq!(one_job_duration, Some(3_200));
    assert_eq!(four_jobs_duration, Some(800));
}
