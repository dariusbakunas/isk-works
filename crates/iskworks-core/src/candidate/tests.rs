use super::*;
use chrono::{TimeZone, Utc};
use uuid::Uuid;

use crate::{
    normalize_candidate, BlueprintKind, BuildPlanCandidateInput, CandidateBlueprintSelection,
    CandidateFacilitySelection, ComponentResolution, DraftPlanningInput, FacilityProfileId,
    ItemPricingSelection, MarketPricingPolicy, PlannerItemRole, PriceSourceId, RecipeSelection,
};

#[test]
fn draft_planning_normalizes_manual_prices_and_round_trips() {
    let input = DraftPlanningInput {
        material_scope: crate::DEFAULT_MARKET_SCOPE,
        output_scope: crate::DEFAULT_MARKET_SCOPE,
        manual_price_list_id: Some(PriceSourceId::new()),
        expected_manual_price_list_revision: Some(4),
        material_pricing_policy: MarketPricingPolicy::HighestBuy,
        output_pricing_policy: MarketPricingPolicy::LowestSell,
        pricing_selections: vec![ItemPricingSelectionInput {
            type_id: 34,
            role: PlannerItemRole::Material,
            selection: ItemPricingSelection::Manual {
                unit_price: "3.9".into(),
            },
        }],
        blueprint_selection: None,
        manufacturing_facility: None,
        reaction_facility: None,
        facility_eiv_manual: false,
        component_resolutions: vec![ComponentResolution {
            type_id: 57_479,
            recipe: RecipeSelection::Manufacturing {
                blueprint_type_id: 57_516,
            },
            facility_override: None,
            blueprint_selection: None,
        }],
        fulfillment_scopes: Vec::new(),
    };

    let normalized = normalize_draft_planning(input, 1).unwrap();

    assert_eq!(
        normalized.pricing_selections[0].selection,
        ItemPricingSelection::Manual {
            unit_price: "3.9000".into(),
        }
    );
    assert_eq!(normalized.component_resolutions.len(), 1);
    assert_eq!(normalized.component_resolutions[0].type_id, 57_479);
    assert_eq!(
        serde_json::from_value::<DraftPlanningInput>(serde_json::to_value(&normalized).unwrap())
            .unwrap(),
        normalized
    );
}

#[test]
fn draft_planning_allows_incomplete_assumptions_but_rejects_partial_manual_price_list_identity() {
    let incomplete = DraftPlanningInput {
        material_scope: crate::DEFAULT_MARKET_SCOPE,
        output_scope: crate::DEFAULT_MARKET_SCOPE,
        manual_price_list_id: None,
        expected_manual_price_list_revision: None,
        material_pricing_policy: MarketPricingPolicy::HighestBuy,
        output_pricing_policy: MarketPricingPolicy::LowestSell,
        pricing_selections: Vec::new(),
        blueprint_selection: None,
        manufacturing_facility: None,
        reaction_facility: None,
        facility_eiv_manual: false,
        component_resolutions: Vec::new(),
        fulfillment_scopes: Vec::new(),
    };
    assert_eq!(
        normalize_draft_planning(incomplete.clone(), 1).unwrap(),
        incomplete
    );

    let partial = DraftPlanningInput {
        expected_manual_price_list_revision: Some(2),
        ..incomplete
    };
    assert!(matches!(
        normalize_draft_planning(partial, 1),
        Err(IndustryError::Validation(_))
    ));
}

/// Multi-BPC job split: more runs than one copy licenses is planned as
/// several jobs, never rejected.
#[test]
fn manual_copy_may_request_more_runs_than_one_copy_licenses() {
    let mut input = candidate();
    input.runs = 4;
    input.blueprint = Some(CandidateBlueprintSelection::Manual {
        kind: BlueprintKind::Copy,
        material_efficiency: 2,
        time_efficiency: 4,
        licensed_runs: Some(1),
        notes: String::new(),
    });

    let normalized = normalize_candidate(input).unwrap();

    assert_eq!(normalized.runs, 4);
}

fn candidate() -> BuildPlanCandidateInput {
    BuildPlanCandidateInput {
        expected_build_revision: 7,
        expected_active_plan_revision: 3,
        runs: 1,
        blueprint: Some(CandidateBlueprintSelection::Manual {
            kind: BlueprintKind::Copy,
            material_efficiency: 10,
            time_efficiency: 20,
            licensed_runs: Some(30),
            notes: " current copy ".into(),
        }),
        price_source_id: PriceSourceId(
            Uuid::parse_str("10000000-0000-0000-0000-000000000001").unwrap(),
        ),
        expected_price_source_revision: 4,
        pricing_selections: vec![
            ItemPricingSelectionInput {
                type_id: 35,
                role: PlannerItemRole::Material,
                selection: ItemPricingSelection::MarketPolicy {
                    policy: MarketPricingPolicy::LowestSell,
                },
            },
            ItemPricingSelectionInput {
                type_id: 34,
                role: PlannerItemRole::Material,
                selection: ItemPricingSelection::MarketPolicy {
                    policy: MarketPricingPolicy::HighestBuy,
                },
            },
            ItemPricingSelectionInput {
                type_id: 35,
                role: PlannerItemRole::Output,
                selection: ItemPricingSelection::Manual {
                    unit_price: "12.5000".into(),
                },
            },
            ItemPricingSelectionInput {
                type_id: 34,
                role: PlannerItemRole::Output,
                selection: ItemPricingSelection::Manual {
                    unit_price: "3.2".into(),
                },
            },
        ],
        manufacturing_facility: CandidateFacilitySelection::Profile {
            facility_profile_id: FacilityProfileId(
                Uuid::parse_str("20000000-0000-0000-0000-000000000002").unwrap(),
            ),
            expected_facility_profile_revision: 2,
            adjusted_price_eiv: Some("1000000".into()),
        },
        reaction_facility: CandidateFacilitySelection::Profile {
            facility_profile_id: FacilityProfileId(
                Uuid::parse_str("20000000-0000-0000-0000-000000000003").unwrap(),
            ),
            expected_facility_profile_revision: 5,
            adjusted_price_eiv: Some("2000000".into()),
        },
    }
}

#[test]
fn pricing_selections_are_normalized_by_role_and_type() {
    let normalized = normalize_pricing_selections(vec![
        ItemPricingSelectionInput {
            type_id: 5_876,
            role: PlannerItemRole::Output,
            selection: ItemPricingSelection::Manual {
                unit_price: "3420000000".into(),
            },
        },
        ItemPricingSelectionInput {
            type_id: 34,
            role: PlannerItemRole::Material,
            selection: ItemPricingSelection::MarketPolicy {
                policy: MarketPricingPolicy::AcquireQuantityFromSellOrders,
            },
        },
    ])
    .unwrap();

    assert_eq!(normalized[0].type_id, 34);
    assert_eq!(normalized[0].role, PlannerItemRole::Material);
    assert_eq!(normalized[1].type_id, 5_876);
    assert_eq!(normalized[1].role, PlannerItemRole::Output);
    assert_eq!(
        normalized[1].selection,
        ItemPricingSelection::Manual {
            unit_price: "3420000000.0000".into(),
        }
    );
}

#[test]
fn duplicate_pricing_selection_for_same_role_and_type_is_rejected() {
    let duplicated = vec![
        ItemPricingSelectionInput {
            type_id: 34,
            role: PlannerItemRole::Material,
            selection: ItemPricingSelection::Manual {
                unit_price: "4.0000".into(),
            },
        },
        ItemPricingSelectionInput {
            type_id: 34,
            role: PlannerItemRole::Material,
            selection: ItemPricingSelection::Manual {
                unit_price: "4.0100".into(),
            },
        },
    ];

    assert!(matches!(
        normalize_pricing_selections(duplicated),
        Err(IndustryError::Validation(_))
    ));
}

#[test]
fn duplicate_component_resolution_for_same_type_is_rejected() {
    let duplicated = vec![
        ComponentResolution {
            type_id: 57_479,
            recipe: RecipeSelection::Manufacturing {
                blueprint_type_id: 57_516,
            },
            facility_override: None,
            blueprint_selection: None,
        },
        ComponentResolution {
            type_id: 57_479,
            recipe: RecipeSelection::Manufacturing {
                blueprint_type_id: 57_516,
            },
            facility_override: None,
            blueprint_selection: None,
        },
    ];

    assert!(matches!(
        normalize_component_resolutions(duplicated),
        Err(IndustryError::Validation(_))
    ));
}

#[test]
fn same_type_can_have_distinct_material_and_output_pricing_selections() {
    let normalized = normalize_pricing_selections(vec![
        ItemPricingSelectionInput {
            type_id: 34,
            role: PlannerItemRole::Material,
            selection: ItemPricingSelection::Default,
        },
        ItemPricingSelectionInput {
            type_id: 34,
            role: PlannerItemRole::Output,
            selection: ItemPricingSelection::Manual {
                unit_price: "5".into(),
            },
        },
    ])
    .unwrap();

    assert_eq!(normalized.len(), 2);
}

#[test]
fn manual_pricing_selection_requires_valid_money() {
    for invalid in ["", "-1", "1.00001"] {
        assert!(
            normalize_pricing_selections(vec![ItemPricingSelectionInput {
                type_id: 34,
                role: PlannerItemRole::Material,
                selection: ItemPricingSelection::Manual {
                    unit_price: invalid.into(),
                },
            }])
            .is_err()
        );
    }
}

#[test]
fn option_wrapped_blueprint_selection_serializes_identically_to_the_bare_value() {
    // Guards the fingerprint-stability property `BuildPlanCandidateInput.blueprint`
    // relies on when it went from `CandidateBlueprintSelection` to
    // `Option<CandidateBlueprintSelection>`: `serde_json` must serialize
    // `Some(x)` exactly like `x`, or every existing manufacturing candidate
    // fingerprint would have shifted and spuriously 409'd in-flight plans.
    let selection = CandidateBlueprintSelection::Manual {
        kind: BlueprintKind::Copy,
        material_efficiency: 10,
        time_efficiency: 20,
        licensed_runs: Some(30),
        notes: "copy".into(),
    };
    assert_eq!(
        serde_json::to_value(Some(&selection)).unwrap(),
        serde_json::to_value(&selection).unwrap()
    );
}

#[test]
fn fingerprint_is_stable_across_policy_and_override_input_order() {
    let left = candidate();
    let mut right = left.clone();
    right.pricing_selections.reverse();

    let left = normalize_candidate(left).unwrap();
    let right = normalize_candidate(right).unwrap();

    assert_eq!(left, right);
    assert_eq!(left.fingerprint(), right.fingerprint());
    assert_eq!(left.pricing_selections[0].type_id, 34);
    assert_eq!(left.pricing_selections[0].role, PlannerItemRole::Material);
    assert_eq!(left.pricing_selections[2].type_id, 34);
    assert_eq!(left.pricing_selections[2].role, PlannerItemRole::Output);
    assert_eq!(
        left.pricing_selections[2].selection,
        ItemPricingSelection::Manual {
            unit_price: "3.2000".into(),
        }
    );
}

#[test]
fn every_revision_and_calculation_input_changes_the_fingerprint() {
    let baseline = normalize_candidate(candidate()).unwrap().fingerprint();
    let observed_at = Utc.with_ymd_and_hms(2026, 7, 27, 20, 0, 0).unwrap();
    let observation_id = Uuid::parse_str("30000000-0000-0000-0000-000000000003").unwrap();
    let mut variants = Vec::new();

    let mut value = candidate();
    value.expected_build_revision += 1;
    variants.push(value);
    let mut value = candidate();
    value.expected_active_plan_revision += 1;
    variants.push(value);
    let mut value = candidate();
    value.runs = 2;
    variants.push(value);
    let mut value = candidate();
    value.blueprint = Some(CandidateBlueprintSelection::Manual {
        kind: BlueprintKind::Original,
        material_efficiency: 9,
        time_efficiency: 19,
        licensed_runs: None,
        notes: "different".into(),
    });
    variants.push(value);
    let mut value = candidate();
    value.blueprint = Some(CandidateBlueprintSelection::ObservedAsset {
        observation_id,
        observed_at,
    });
    variants.push(value);
    let mut value = candidate();
    value.blueprint = None;
    variants.push(value);
    let mut value = candidate();
    value.price_source_id =
        PriceSourceId(Uuid::parse_str("10000000-0000-0000-0000-000000000099").unwrap());
    variants.push(value);
    let mut value = candidate();
    value.expected_price_source_revision += 1;
    variants.push(value);
    let mut value = candidate();
    value.pricing_selections[0].selection = ItemPricingSelection::MarketPolicy {
        policy: MarketPricingPolicy::HighestBuy,
    };
    variants.push(value);
    let mut value = candidate();
    value.pricing_selections[2].selection = ItemPricingSelection::Manual {
        unit_price: "12.6000".into(),
    };
    variants.push(value);
    let mut value = candidate();
    value.manufacturing_facility = CandidateFacilitySelection::None;
    variants.push(value);
    let mut value = candidate();
    value.reaction_facility = CandidateFacilitySelection::None;
    variants.push(value);
    let mut value = candidate();
    let CandidateFacilitySelection::Profile {
        expected_facility_profile_revision,
        ..
    } = &mut value.manufacturing_facility
    else {
        unreachable!();
    };
    *expected_facility_profile_revision += 1;
    variants.push(value);
    let mut value = candidate();
    let CandidateFacilitySelection::Profile {
        expected_facility_profile_revision,
        ..
    } = &mut value.reaction_facility
    else {
        unreachable!();
    };
    *expected_facility_profile_revision += 1;
    variants.push(value);
    let mut value = candidate();
    let CandidateFacilitySelection::Profile {
        adjusted_price_eiv, ..
    } = &mut value.manufacturing_facility
    else {
        unreachable!();
    };
    *adjusted_price_eiv = Some("1000001".into());
    variants.push(value);
    let mut value = candidate();
    let CandidateFacilitySelection::Profile {
        adjusted_price_eiv, ..
    } = &mut value.reaction_facility
    else {
        unreachable!();
    };
    *adjusted_price_eiv = Some("2000001".into());
    variants.push(value);

    for variant in variants {
        assert_ne!(
            baseline,
            normalize_candidate(variant).unwrap().fingerprint()
        );
    }
}

#[test]
fn incomplete_installation_qualifies_profitability_without_zero_substitution() {
    let (completeness, basis) = candidate_completeness(true, false, true);

    assert_eq!(completeness.installation, CalculationState::Incomplete);
    assert_eq!(completeness.profitability, ProfitabilityState::Qualified);
    assert!(!basis
        .included_costs
        .contains(&ProfitabilityCost::InstallationCost));
    assert!(basis
        .excluded_costs
        .contains(&ProfitabilityCost::InstallationCost));
}

#[test]
fn stale_market_price_provenance_becomes_a_candidate_warning() {
    let warnings = market_price_warnings(&[PriceSnapshotLine {
        type_id: 34,
        type_name: "Tritanium".into(),
        item_role: PlannerItemRole::Material,
        selection_kind: crate::PricingSelectionKind::Default,
        manual_unit_price: None,
        price: Some(Money::parse("4.0000").unwrap()),
        pricing_policy: Some(MarketPricingPolicy::HighestBuy),
        missing: false,
        source_note: "market_freshness=stale; observed 2026-07-26T19:26:39Z".into(),
        sort_order: 0,
        market_region_id: None,
        market_location_id: None,
    }]);

    assert_eq!(
        warnings,
        vec![CandidateIssue {
            code: "staleMarketObservations".into(),
            message: "Imported market observations are stale; candidate prices may no longer reflect the market.".into(),
        }]
    );
}

#[test]
fn create_candidate_guidance_warns_about_missing_inventory_without_blocking_plan() {
    let mut coverage = crate::BuildCoverageReport {
        build_id: crate::BuildId::new(),
        owner_id: crate::OwnerId::new(),
        build_revision: 1,
        recipe_fingerprint: "recipe".into(),
        runs: 1,
        complete_quantity_coverage: false,
        complete_cost_coverage: true,
        material_lines: vec![crate::MaterialCoverage {
            type_id: 34,
            type_name: "Tritanium".into(),
            sort_order: 0,
            required_quantity: 1_000,
            accounted_owned_quantity: 900,
            reserved_for_this_build: 0,
            reserved_by_other_builds: 0,
            unreserved_available_quantity: 900,
            available_to_this_build: 900,
            reservable_additional_quantity: 900,
            covered_quantity: 900,
            missing_quantity: 100,
            average_historical_unit_cost: Some(Money::parse("5").unwrap()),
            projected_historical_cost: Some(Money::parse("5000").unwrap()),
            cost_quality: crate::MaterialCostQuality::Known,
            quantity_coverage_state: crate::QuantityCoverageState::PartiallyCovered,
            esi_observed_quantity: None,
            esi_reconciliation_difference: None,
            esi_observed_at: None,
            explanation: String::new(),
            warnings: Vec::new(),
            inventory_revision: 1,
        }],
        warnings: Vec::new(),
    };

    let decision = candidate_decision(true, &coverage).unwrap();

    assert_eq!(decision.tone, CandidateDecisionTone::Warning);
    assert_eq!(decision.headline, "1 input type has a shortage.");
    assert_eq!(
        decision.supporting_text,
        "100 total units must be acquired before production."
    );

    let mut second_shortage = coverage.material_lines[0].clone();
    second_shortage.type_id = 35;
    second_shortage.type_name = "Pyerite".into();
    second_shortage.missing_quantity = 126;
    coverage.material_lines.push(second_shortage);

    let decision = candidate_decision(true, &coverage).unwrap();
    assert_eq!(decision.headline, "2 input types have shortages.");
    assert_eq!(
        decision.supporting_text,
        "226 total units must be acquired before production."
    );
}

#[test]
fn pricing_blocker_names_every_missing_market_item() {
    let issue = pricing_incomplete_issue(&[
        PriceSnapshotLine {
            type_id: 626,
            type_name: "Vexor".into(),
            item_role: PlannerItemRole::Material,
            selection_kind: crate::PricingSelectionKind::Default,
            manual_unit_price: None,
            price: None,
            pricing_policy: Some(MarketPricingPolicy::HighestBuy),
            missing: true,
            source_note: String::new(),
            sort_order: 0,
            market_region_id: None,
            market_location_id: None,
        },
        PriceSnapshotLine {
            type_id: 34,
            type_name: "Tritanium".into(),
            item_role: PlannerItemRole::Material,
            selection_kind: crate::PricingSelectionKind::Default,
            manual_unit_price: None,
            price: Some(Money::parse("4").unwrap()),
            pricing_policy: Some(MarketPricingPolicy::HighestBuy),
            missing: false,
            source_note: String::new(),
            sort_order: 1,
            market_region_id: None,
            market_location_id: None,
        },
        PriceSnapshotLine {
            type_id: 12005,
            type_name: "Ishtar".into(),
            item_role: PlannerItemRole::Output,
            selection_kind: crate::PricingSelectionKind::Default,
            manual_unit_price: None,
            price: None,
            pricing_policy: Some(MarketPricingPolicy::LowestSell),
            missing: true,
            source_note: String::new(),
            sort_order: 2,
            market_region_id: None,
            market_location_id: None,
        },
    ]);

    assert_eq!(issue.code, "pricingIncomplete");
    assert_eq!(issue.message, "Missing market prices: Vexor and Ishtar.");
}

#[test]
fn create_candidate_fingerprint_ignores_transient_ids_and_timestamps() {
    let now = Utc::now();
    let source_id = PriceSourceId::new();
    let plan = BuildPlanRevision {
        id: BuildPlanId(Uuid::new_v4()),
        revision: 1,
        runs: 1,
        recipe_fingerprint: "recipe".into(),
        snapshot: PriceSnapshot {
            id: PriceSnapshotId(Uuid::new_v4()),
            price_source_id: Some(source_id),
            source_name: "Alliance Market".into(),
            source_revision: 4,
            created_at: now,
            items: vec![PriceSnapshotLine {
                type_id: 34,
                type_name: "Tritanium".into(),
                item_role: PlannerItemRole::Material,
                selection_kind: crate::PricingSelectionKind::Default,
                manual_unit_price: None,
                price: Some(Money::parse("4").unwrap()),
                pricing_policy: Some(MarketPricingPolicy::HighestBuy),
                missing: false,
                source_note: String::new(),
                sort_order: 0,
                market_region_id: None,
                market_location_id: None,
            }],
        },
        pricing_complete: true,
        estimated_material_cost: Money::parse("400").unwrap(),
        expected_revenue: Some(Money::parse("1000").unwrap()),
        estimated_margin: Some(Money::parse("600").unwrap()),
        missing_price_count: 0,
        active: true,
        planned_at: now,
        superseded_at: None,
        material_lines: vec![PlannedMaterialLine {
            type_id: 34,
            type_name: "Tritanium".into(),
            quantity_per_run: 100,
            total_quantity: 100,
            unit_price: Some(Money::parse("4").unwrap()),
            line_total: Some(Money::parse("400").unwrap()),
            missing: false,
            contributions: Vec::new(),
            is_build_resolved: false,
            installation_cost: None,
            reused_quantity: None,
            missing_quantity: None,
            reused_line_total: None,
            planning_evidence: None,
        }],
        manufacturing_facility: None,
        reaction_facility: None,
        blueprint: None,
        effective_requirements: Vec::new(),
    };
    let mut second = plan.clone();
    second.id = BuildPlanId(Uuid::new_v4());
    second.snapshot.id = PriceSnapshotId(Uuid::new_v4());
    second.snapshot.created_at = now + chrono::Duration::minutes(1);
    second.planned_at = now + chrono::Duration::minutes(1);

    assert_eq!(
        create_candidate_fingerprint(&plan),
        create_candidate_fingerprint(&second)
    );
}

fn fixture_root_facility_with_requirements(
    requirements: Vec<crate::EffectiveMaterialRequirement>,
) -> FacilityPlanPreview {
    FacilityPlanPreview {
        profile: crate::IndustryFacilityProfile {
            id: crate::FacilityProfileId::new(),
            workspace_id: crate::WorkspaceId::new(),
            name: "Test Assembly Array".into(),
            kind: crate::FacilityKind::Manual,
            role: crate::FacilityRole::Manufacturing,
            structure_id: None,
            structure_type_id: None,
            structure_type_name: String::new(),
            solar_system_id: None,
            solar_system_name: String::new(),
            security_class: crate::SecurityClass::Unknown,
            material_reduction_percent: Decimal::ZERO,
            time_reduction_percent: Decimal::ZERO,
            job_cost_reduction_percent: Decimal::ZERO,
            facility_tax_percent: Decimal::ZERO,
            scc_surcharge_percent: Decimal::ZERO,
            alliance_surcharge_percent: Decimal::ZERO,
            fixed_supplemental_cost: Money::zero(),
            manual_system_cost_index: Some(Decimal::new(5, 2)),
            notes: String::new(),
            rigs: Vec::new(),
            archived_at: None,
            revision: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        blueprint_me: Some(0),
        blueprint_te: Some(0),
        requirements,
        rig_material_factor: Decimal::ONE,
        planned_duration_seconds: None,
        duration_steps: Vec::new(),
        installation_cost: crate::InstallationCostBreakdown {
            complete: true,
            estimated_item_value: Some(Money::parse("1000000").unwrap()),
            system_cost_index: Some(Decimal::new(5, 2)),
            unmodified_system_index_cost: Some(Money::parse("10000").unwrap()),
            job_cost_reduction_percent: Decimal::ZERO,
            system_index_cost: Some(Money::parse("10000").unwrap()),
            facility_tax: None,
            scc_surcharge: None,
            alliance_surcharge: None,
            fixed_supplemental_cost: Money::zero(),
            total: Some(Money::parse("10000").unwrap()),
            warnings: Vec::new(),
            formula_version: "test".into(),
        },
        warnings: Vec::new(),
        formula_version: "test".into(),
    }
}

#[test]
fn calculation_evidence_succeeds_for_a_build_resolved_row_missing_from_the_root_facility_requirements(
) {
    // The root facility's requirements only ever cover the root recipe's
    // own direct materials (type 34 here). A build-resolved sub-component
    // row (type 99 - e.g. a component-expanded item, or its own
    // sub-materials) has no entry there, since no ME/TE is applied to
    // expansion rows. Regression test for a bug where looking up such a
    // row's base quantity from the facility's requirements alone caused
    // `calculation_evidence` to return `IndustryError::InvalidRecipe`.
    let root_facility =
        fixture_root_facility_with_requirements(vec![crate::EffectiveMaterialRequirement {
            type_id: 34,
            type_name: "Tritanium".into(),
            sort_order: 0,
            base_quantity_per_run: 100,
            runs: 1,
            base_extended_quantity: 100,
            blueprint_me: Some(0),
            facility_material_factor: rust_decimal::Decimal::ONE,
            final_required_quantity: 100,
            job_count: 1,
            calculation_trace: String::new(),
            formula_version: "test".into(),
            recipe_fingerprint: "recipe".into(),
        }]);
    let material_lines = vec![
        PlannedMaterialLine {
            type_id: 34,
            type_name: "Tritanium".into(),
            quantity_per_run: 100,
            total_quantity: 100,
            unit_price: Some(Money::parse("4").unwrap()),
            line_total: Some(Money::parse("400").unwrap()),
            missing: false,
            contributions: Vec::new(),
            is_build_resolved: false,
            installation_cost: None,
            reused_quantity: None,
            missing_quantity: None,
            reused_line_total: None,
            planning_evidence: None,
        },
        PlannedMaterialLine {
            type_id: 99,
            type_name: "Deflection Shield Emitter".into(),
            quantity_per_run: 5,
            total_quantity: 5,
            unit_price: Some(Money::parse("1000").unwrap()),
            line_total: Some(Money::parse("5000").unwrap()),
            missing: false,
            contributions: Vec::new(),
            is_build_resolved: true,
            installation_cost: Some(fixture_installation_cost("500")),
            reused_quantity: None,
            missing_quantity: None,
            reused_line_total: None,
            planning_evidence: None,
        },
    ];

    let evidence = calculation_evidence(
        1,
        &material_lines,
        true,
        Some(Money::parse("10000").unwrap()),
        Some(Money::parse("4600").unwrap()),
        Some(&root_facility),
        Some(0),
        None,
    );

    assert!(evidence.is_ok(), "expected Ok, got {evidence:?}");
}

/// The InvalidRecipe investigation's minimal reproducer: a Buy row
/// (`unit_price: Some`) alongside a Slice-B-shaped self-produced
/// Build-resolved row (`unit_price: None`, `line_total: Some`). Before
/// the fix, `calculation_evidence` returned `Err(InvalidRecipe)` here --
/// the recipe was never invalid, only this function's stale assumption
/// that `pricing_complete` implies every row has a per-unit price.
#[test]
fn calculation_evidence_slice_b_build_resolved_row_with_none_unit_price() {
    let root_facility =
        fixture_root_facility_with_requirements(vec![crate::EffectiveMaterialRequirement {
            type_id: 34,
            type_name: "Tritanium".into(),
            sort_order: 0,
            base_quantity_per_run: 100,
            runs: 1,
            base_extended_quantity: 100,
            blueprint_me: Some(0),
            facility_material_factor: rust_decimal::Decimal::ONE,
            final_required_quantity: 100,
            job_count: 1,
            calculation_trace: String::new(),
            formula_version: "test".into(),
            recipe_fingerprint: "recipe".into(),
        }]);
    let material_lines = vec![
        PlannedMaterialLine {
            type_id: 34,
            type_name: "Tritanium".into(),
            quantity_per_run: 100,
            total_quantity: 100,
            unit_price: Some(Money::parse("4").unwrap()),
            line_total: Some(Money::parse("400").unwrap()),
            missing: false,
            contributions: Vec::new(),
            is_build_resolved: false,
            installation_cost: None,
            reused_quantity: None,
            missing_quantity: None,
            reused_line_total: None,
            planning_evidence: None,
        },
        PlannedMaterialLine {
            type_id: 99,
            type_name: "Deflection Shield Emitter".into(),
            quantity_per_run: 5,
            total_quantity: 5,
            // `BuildCostProjection::apply_to_revision` sets
            // this to `None` unconditionally for a Build/Reaction row --
            // even though `line_total` (below) is fully known.
            unit_price: None,
            line_total: Some(Money::parse("5000").unwrap()),
            missing: false,
            contributions: Vec::new(),
            is_build_resolved: true,
            // `apply_to_revision` also always sets this `None` -- a
            // Build/Reaction row's installation lives inside its own
            // `line_total`, not a separate breakdown.
            installation_cost: None,
            reused_quantity: None,
            missing_quantity: None,
            reused_line_total: None,
            planning_evidence: None,
        },
    ];

    let evidence = calculation_evidence(
        1,
        &material_lines,
        true,
        Some(Money::parse("10000").unwrap()),
        Some(Money::parse("4600").unwrap()),
        Some(&root_facility),
        Some(0),
        None,
    )
    .expect("a fully-priced Build-resolved row must not be InvalidRecipe");

    // No ME/structure discount configured here (blueprint_me: 0, facility
    // material_reduction_percent: 0), so the Buy row's own contribution
    // (400) is identical at every stage too -- 400 + 5000 = 5400
    // everywhere. The Build row's 5000 is what this test exists to
    // prove: never dropped, never multiplied, never the cause of an
    // error.
    assert_eq!(
        evidence.material_cost.base_market_value,
        Some(Money::parse("5400").unwrap())
    );
    assert_eq!(
        evidence.material_cost.after_blueprint_me,
        Some(Money::parse("5400").unwrap())
    );
    assert_eq!(
        evidence.material_cost.after_structure,
        Some(Money::parse("5400").unwrap())
    );
    // `adjusted_material_cost` is unaffected by this fix -- straight sum
    // of `line_total`, unchanged code path.
    assert_eq!(
        evidence.material_cost.adjusted_material_cost,
        Money::parse("5400").unwrap()
    );
}

/// A fitted rig the facility preview skipped as non-applicable to the
/// product must not show up in the evidence's rig multiplier: the
/// evidence reports the preview's applied rig factor, not a fold over
/// every fitted rig.
#[test]
fn calculation_evidence_rig_multiplier_ignores_non_applicable_rigs() {
    let mut root_facility = fixture_root_facility_with_requirements(Vec::new());
    root_facility.profile.rigs.push(crate::FacilityRig {
        slot_number: 1,
        type_id: 46_496,
        type_name: "Standup L-Set Reactor Efficiency I".into(),
        material_reduction_percent: Decimal::new(22, 1),
        time_reduction_percent: Decimal::new(22, 0),
        applicability: Default::default(),
    });
    root_facility.rig_material_factor = Decimal::ONE;

    let evidence =
        calculation_evidence(1, &[], true, None, None, Some(&root_facility), None, None).unwrap();

    assert_eq!(evidence.material_cost.rig_multiplier, Decimal::ONE);

    root_facility.rig_material_factor = Decimal::new(978, 3);
    let evidence =
        calculation_evidence(1, &[], true, None, None, Some(&root_facility), None, None).unwrap();

    assert_eq!(evidence.material_cost.rig_multiplier, Decimal::new(978, 3));
}

/// Buy-only parity: the per-stage repricing for a plain Buy row must be
/// byte-identical to its pre-fix behavior. Non-trivial ME/structure
/// multipliers here so the three stages actually differ.
#[test]
fn calculation_evidence_buy_only_stage_pricing_is_unchanged() {
    let root_facility = fixture_root_facility_with_material_reduction(
        vec![crate::EffectiveMaterialRequirement {
            type_id: 34,
            type_name: "Tritanium".into(),
            sort_order: 0,
            base_quantity_per_run: 100,
            runs: 1,
            base_extended_quantity: 100,
            blueprint_me: Some(10),
            facility_material_factor: rust_decimal::Decimal::new(95, 2),
            final_required_quantity: 90,
            job_count: 1,
            calculation_trace: String::new(),
            formula_version: "test".into(),
            recipe_fingerprint: "recipe".into(),
        }],
        "5", // 5% structure material reduction.
    );
    let material_lines = vec![PlannedMaterialLine {
        type_id: 34,
        type_name: "Tritanium".into(),
        quantity_per_run: 100,
        total_quantity: 90,
        unit_price: Some(Money::parse("4").unwrap()),
        line_total: Some(Money::parse("360").unwrap()),
        missing: false,
        contributions: Vec::new(),
        is_build_resolved: false,
        installation_cost: None,
        reused_quantity: None,
        missing_quantity: None,
        reused_line_total: None,
        planning_evidence: None,
    }];

    let evidence = calculation_evidence(
        1,
        &material_lines,
        true,
        Some(Money::parse("10000").unwrap()),
        Some(Money::parse("9600").unwrap()),
        Some(&root_facility),
        Some(10),
        None,
    )
    .unwrap();

    // base: 100 * 1.00 = 100 units * 4 = 400.
    assert_eq!(
        evidence.material_cost.base_market_value,
        Some(Money::parse("400").unwrap())
    );
    // after ME (10%): ceil(100 * 0.90) = 90 units * 4 = 360.
    assert_eq!(
        evidence.material_cost.after_blueprint_me,
        Some(Money::parse("360").unwrap())
    );
    // after ME * structure (0.90 * 0.95 = 0.855): ceil(100 * 0.855) = 86
    // units * 4 = 344.
    assert_eq!(
        evidence.material_cost.after_structure,
        Some(Money::parse("344").unwrap())
    );
}

/// Multi-BPC job split: each stage reprices a Buy row at its per-job
/// rounded quantity, like the authoritative requirement. 2/run over five
/// 1-run jobs at ME10 is 5 x ceil(1.8) = 10, not ceil(9) = 9.
#[test]
fn calculation_evidence_rounds_stage_quantities_per_job() {
    let material_lines = vec![PlannedMaterialLine {
        type_id: 34,
        type_name: "Tritanium".into(),
        quantity_per_run: 2,
        total_quantity: 10,
        unit_price: Some(Money::parse("4").unwrap()),
        line_total: Some(Money::parse("40").unwrap()),
        missing: false,
        contributions: Vec::new(),
        is_build_resolved: false,
        installation_cost: None,
        reused_quantity: None,
        missing_quantity: None,
        reused_line_total: None,
        planning_evidence: None,
    }];
    let evidence = |max_runs_per_job| {
        calculation_evidence(
            5,
            &material_lines,
            true,
            None,
            None,
            None,
            Some(10),
            max_runs_per_job,
        )
        .unwrap()
        .material_cost
        .after_blueprint_me
    };

    assert_eq!(evidence(None), Some(Money::parse("36").unwrap()));
    assert_eq!(evidence(Some(1)), Some(Money::parse("40").unwrap()));
}

/// Mixed Buy + Build-resolved: the Buy row's contribution changes across
/// stages exactly as in the Buy-only case above; the Build-resolved
/// row's contribution is the same 5000 at every stage.
#[test]
fn calculation_evidence_mixed_buy_and_build_resolved_rows() {
    let root_facility = fixture_root_facility_with_material_reduction(
        vec![crate::EffectiveMaterialRequirement {
            type_id: 34,
            type_name: "Tritanium".into(),
            sort_order: 0,
            base_quantity_per_run: 100,
            runs: 1,
            base_extended_quantity: 100,
            blueprint_me: Some(10),
            facility_material_factor: rust_decimal::Decimal::new(95, 2),
            final_required_quantity: 90,
            job_count: 1,
            calculation_trace: String::new(),
            formula_version: "test".into(),
            recipe_fingerprint: "recipe".into(),
        }],
        "5",
    );
    let material_lines = vec![
        PlannedMaterialLine {
            type_id: 34,
            type_name: "Tritanium".into(),
            quantity_per_run: 100,
            total_quantity: 90,
            unit_price: Some(Money::parse("4").unwrap()),
            line_total: Some(Money::parse("360").unwrap()),
            missing: false,
            contributions: Vec::new(),
            is_build_resolved: false,
            installation_cost: None,
            reused_quantity: None,
            missing_quantity: None,
            reused_line_total: None,
            planning_evidence: None,
        },
        PlannedMaterialLine {
            type_id: 99,
            type_name: "Deflection Shield Emitter".into(),
            quantity_per_run: 5,
            total_quantity: 5,
            unit_price: None,
            line_total: Some(Money::parse("5000").unwrap()),
            missing: false,
            contributions: Vec::new(),
            is_build_resolved: true,
            installation_cost: None,
            reused_quantity: None,
            missing_quantity: None,
            reused_line_total: None,
            planning_evidence: None,
        },
    ];

    let evidence = calculation_evidence(
        1,
        &material_lines,
        true,
        Some(Money::parse("10000").unwrap()),
        Some(Money::parse("9240").unwrap()),
        Some(&root_facility),
        Some(10),
        None,
    )
    .unwrap();

    // base: 400 (Buy @ full quantity) + 5000 (Build, held constant).
    assert_eq!(
        evidence.material_cost.base_market_value,
        Some(Money::parse("5400").unwrap())
    );
    // after ME: 360 (Buy @ 90 units) + 5000 (Build, still constant).
    assert_eq!(
        evidence.material_cost.after_blueprint_me,
        Some(Money::parse("5360").unwrap())
    );
    // after structure: 344 (Buy @ 86 units) + 5000 (Build, still
    // constant) -- never repriced by the parent's own discount.
    assert_eq!(
        evidence.material_cost.after_structure,
        Some(Money::parse("5344").unwrap())
    );
}

/// A Build/Reaction-resolved row with neither a unit price nor a line
/// total means its child cost genuinely isn't known -- `pricing_complete`
/// promised otherwise, so this should be structurally unreachable via
/// the real `BuildCostProjection` path, but `calculation_evidence` must
/// still degrade to incomplete evidence rather than fabricate a price or
/// misreport a valid recipe as invalid.
#[test]
fn calculation_evidence_build_resolved_row_with_no_line_total_is_incomplete_not_invalid() {
    let root_facility = fixture_root_facility_with_requirements(Vec::new());
    let material_lines = vec![PlannedMaterialLine {
        type_id: 99,
        type_name: "Deflection Shield Emitter".into(),
        quantity_per_run: 5,
        total_quantity: 5,
        unit_price: None,
        line_total: None,
        missing: true,
        contributions: Vec::new(),
        is_build_resolved: true,
        installation_cost: None,
        reused_quantity: None,
        missing_quantity: None,
        reused_line_total: None,
        planning_evidence: None,
    }];

    let evidence = calculation_evidence(
        1,
        &material_lines,
        true,
        Some(Money::parse("10000").unwrap()),
        None,
        Some(&root_facility),
        Some(0),
        None,
    )
    .expect("incomplete evidence must not be InvalidRecipe");

    assert_eq!(evidence.material_cost.base_market_value, None);
    assert_eq!(evidence.material_cost.after_blueprint_me, None);
    assert_eq!(evidence.material_cost.after_structure, None);
}

/// A genuinely inconsistent Buy row -- `unit_price: None` on a row that is
/// *not* Build/Reaction-resolved while `pricing_complete: true` -- is still
/// `InvalidRecipe`: the `unit_price: None` exemption covers self-produced
/// rows only, not every row.
#[test]
fn calculation_evidence_buy_row_missing_price_despite_pricing_complete_stays_invalid_recipe() {
    let material_lines = vec![PlannedMaterialLine {
        type_id: 34,
        type_name: "Tritanium".into(),
        quantity_per_run: 100,
        total_quantity: 100,
        unit_price: None,
        line_total: Some(Money::parse("400").unwrap()),
        missing: false,
        contributions: Vec::new(),
        is_build_resolved: false,
        installation_cost: None,
        reused_quantity: None,
        missing_quantity: None,
        reused_line_total: None,
        planning_evidence: None,
    }];

    let evidence = calculation_evidence(
        1,
        &material_lines,
        true,
        Some(Money::parse("10000").unwrap()),
        Some(Money::parse("9600").unwrap()),
        None,
        Some(0),
        None,
    );

    assert!(matches!(evidence, Err(IndustryError::InvalidRecipe)));
}

fn fixture_root_facility_with_material_reduction(
    requirements: Vec<crate::EffectiveMaterialRequirement>,
    material_reduction_percent: &str,
) -> FacilityPlanPreview {
    let mut facility = fixture_root_facility_with_requirements(requirements);
    facility.profile.material_reduction_percent =
        rust_decimal::Decimal::from_str_exact(material_reduction_percent).unwrap();
    facility
}

fn fixture_installation_cost(total: &str) -> crate::InstallationCostBreakdown {
    crate::InstallationCostBreakdown {
        complete: true,
        estimated_item_value: Some(Money::parse("1000000").unwrap()),
        system_cost_index: Some(Decimal::new(5, 2)),
        unmodified_system_index_cost: Some(Money::parse(total).unwrap()),
        job_cost_reduction_percent: Decimal::ZERO,
        system_index_cost: Some(Money::parse(total).unwrap()),
        facility_tax: None,
        scc_surcharge: None,
        alliance_surcharge: None,
        fixed_supplemental_cost: Money::zero(),
        total: Some(Money::parse(total).unwrap()),
        warnings: Vec::new(),
        formula_version: "test".into(),
    }
}
