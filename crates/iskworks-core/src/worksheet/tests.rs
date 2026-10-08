use super::*;
use crate::{BuildId, MaterialCostQuality, MaterialCoverage, OwnerId, QuantityCoverageState};

#[test]
fn groups_materials_and_projects_missing_output_price() {
    let material_price = line(34, "Tritanium", PlannerItemRole::Material, Some("4"));
    let output_price = line(587, "Rifter", PlannerItemRole::Output, None);
    let material = PlannedMaterialLine {
        type_id: 34,
        type_name: "Tritanium".into(),
        quantity_per_run: 100,
        total_quantity: 100,
        unit_price: Some(Money::parse("4").unwrap()),
        line_total: Some(Money::parse("400").unwrap()),
        missing: false,
        contributions: vec![MaterialContribution {
            parent_type_id: Some(90_001),
            parent_type_name: "Rifter Hull Section".into(),
            quantity: 100,
        }],
        is_build_resolved: false,
        installation_cost: None,
        reused_quantity: None,
        missing_quantity: None,
        reused_line_total: None,
        planning_evidence: None,
    };
    let coverage = coverage();
    let groups = BTreeMap::from([(34, "Minerals".into())]);

    let worksheet = project_production_worksheet(WorksheetProjectionInput {
        price_lines: &[material_price, output_price],
        material_lines: &[material],
        coverage: &coverage,
        facility: None,
        material_cost: Money::parse("400").unwrap(),
        expected_revenue: None,
        estimated_margin: None,
        pricing_complete: true,
        output_quantity: 1,
        group_labels: &groups,
        warnings: Vec::new(),
        planning: None,
    })
    .unwrap();

    assert_eq!(worksheet.groups[0].label, "Minerals");
    assert_eq!(worksheet.groups[0].items[0].covered_quantity, 80);
    assert_eq!(worksheet.groups[0].items[0].coverage_percentage, "80.00");
    assert_eq!(
        worksheet.groups[0].items[0].contributions,
        vec![MaterialContribution {
            parent_type_id: Some(90_001),
            parent_type_name: "Rifter Hull Section".into(),
            quantity: 100,
        }]
    );
    assert!(worksheet.output.items[0].contributions.is_empty());
    assert_eq!(worksheet.output.items[0].coverage_percentage, "0.00");
    assert!(worksheet.output.items[0].pricing.missing);
    assert_eq!(
        worksheet.summary.material_cost,
        Money::parse("400").unwrap()
    );
}

#[test]
fn a_build_resolved_rows_displayed_price_reflects_its_own_linked_build_cost_not_the_stale_market_snapshot(
) {
    // The price-source snapshot line (price_lines) still carries a
    // market price for this row (999) -- a build-resolved row never
    // actually uses it, but the snapshot exists regardless.
    // What the worksheet actually displays must come from the row's
    // own (correct, linked-build-sourced) unit_price, not that stale
    // market figure.
    let material_price = line(
        34,
        "Rifter Hull Section",
        PlannerItemRole::Material,
        Some("999"),
    );
    let output_price = line(587, "Rifter", PlannerItemRole::Output, Some("100"));
    let material = PlannedMaterialLine {
        type_id: 34,
        type_name: "Rifter Hull Section".into(),
        quantity_per_run: 2,
        total_quantity: 2,
        unit_price: Some(Money::parse("425").unwrap()),
        line_total: Some(Money::parse("850").unwrap()),
        missing: false,
        contributions: Vec::new(),
        is_build_resolved: true,
        installation_cost: None,
        reused_quantity: None,
        missing_quantity: None,
        reused_line_total: None,
        planning_evidence: None,
    };
    let coverage = coverage();
    let groups = BTreeMap::from([(34, "Minerals".into())]);

    let worksheet = project_production_worksheet(WorksheetProjectionInput {
        price_lines: &[material_price, output_price],
        material_lines: &[material],
        coverage: &coverage,
        facility: None,
        material_cost: Money::parse("850").unwrap(),
        expected_revenue: None,
        estimated_margin: None,
        pricing_complete: true,
        output_quantity: 1,
        group_labels: &groups,
        warnings: Vec::new(),
        planning: None,
    })
    .unwrap();

    let row = &worksheet.groups[0].items[0];
    assert_eq!(row.pricing.unit_price, Some(Money::parse("425").unwrap()));
    assert!(!row.pricing.missing);
}

#[test]
fn a_build_resolved_rows_displayed_price_is_missing_when_its_own_cost_is_unknown_even_if_the_market_snapshot_has_a_price(
) {
    let material_price = line(
        34,
        "Rifter Hull Section",
        PlannerItemRole::Material,
        Some("999"),
    );
    let output_price = line(587, "Rifter", PlannerItemRole::Output, Some("100"));
    let material = PlannedMaterialLine {
        type_id: 34,
        type_name: "Rifter Hull Section".into(),
        quantity_per_run: 2,
        total_quantity: 2,
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
    };
    let coverage = coverage();
    let groups = BTreeMap::from([(34, "Minerals".into())]);

    let worksheet = project_production_worksheet(WorksheetProjectionInput {
        price_lines: &[material_price, output_price],
        material_lines: &[material],
        coverage: &coverage,
        facility: None,
        material_cost: Money::zero(),
        expected_revenue: None,
        estimated_margin: None,
        pricing_complete: false,
        output_quantity: 1,
        group_labels: &groups,
        warnings: Vec::new(),
        planning: None,
    })
    .unwrap();

    let row = &worksheet.groups[0].items[0];
    assert_eq!(row.pricing.unit_price, None);
    assert!(row.pricing.missing);
}

#[test]
fn a_missing_scoped_buy_rows_displayed_price_is_the_blend_not_the_stale_market_snapshot() {
    // Same class of bug as the build-resolved tests above, but for a
    // plain Buy row with a Missing fulfillment scope: the price-source
    // snapshot still carries the pre-blend market price (999) -- the
    // worksheet must display the row's own blended unit_price instead,
    // exactly like line_total already does unconditionally.
    let material_price = line(34, "Tritanium", PlannerItemRole::Material, Some("999"));
    let output_price = line(587, "Rifter", PlannerItemRole::Output, Some("100"));
    let material = PlannedMaterialLine {
        type_id: 34,
        type_name: "Tritanium".into(),
        quantity_per_run: 100,
        total_quantity: 100,
        unit_price: Some(Money::parse("3.2750").unwrap()),
        line_total: Some(Money::parse("327.5000").unwrap()),
        missing: false,
        contributions: Vec::new(),
        is_build_resolved: false,
        installation_cost: None,
        reused_quantity: Some(40),
        missing_quantity: Some(60),
        reused_line_total: Some(Money::parse("80.0000").unwrap()),
        planning_evidence: None,
    };
    let coverage = coverage();
    let groups = BTreeMap::from([(34, "Minerals".into())]);

    let worksheet = project_production_worksheet(WorksheetProjectionInput {
        price_lines: &[material_price, output_price],
        material_lines: &[material],
        coverage: &coverage,
        facility: None,
        material_cost: Money::parse("327.5000").unwrap(),
        expected_revenue: None,
        estimated_margin: None,
        pricing_complete: true,
        output_quantity: 1,
        group_labels: &groups,
        warnings: Vec::new(),
        planning: None,
    })
    .unwrap();

    let row = &worksheet.groups[0].items[0];
    assert_eq!(
        row.pricing.unit_price,
        Some(Money::parse("3.2750").unwrap())
    );
    assert!(!row.pricing.missing);
    // Threaded through for the frontend's explanatory sentence --
    // missing_quantity isn't repeated: required_quantity (100) minus
    // reused_quantity (40) is 60.
    assert_eq!(row.reused_quantity, Some(40));
    assert_eq!(
        row.reused_line_total,
        Some(Money::parse("80.0000").unwrap())
    );
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

fn fixture_root_facility() -> FacilityPlanPreview {
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
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
        blueprint_me: Some(0),
        blueprint_te: Some(0),
        requirements: Vec::new(),
        rig_material_factor: rust_decimal::Decimal::ONE,
        planned_duration_seconds: None,
        duration_steps: Vec::new(),
        installation_cost: fixture_installation_cost("10000"),
        warnings: Vec::new(),
        formula_version: "test".into(),
    }
}

#[test]
fn summary_installation_cost_sums_the_root_and_every_build_resolved_rows_own_cost() {
    let material_price = line(34, "Tritanium", PlannerItemRole::Material, Some("4"));
    let output_price = line(587, "Rifter", PlannerItemRole::Output, Some("100"));
    let material = PlannedMaterialLine {
        type_id: 34,
        type_name: "Tritanium".into(),
        quantity_per_run: 100,
        total_quantity: 100,
        unit_price: Some(Money::parse("4").unwrap()),
        line_total: Some(Money::parse("400").unwrap()),
        missing: false,
        contributions: Vec::new(),
        is_build_resolved: true,
        installation_cost: Some(fixture_installation_cost("50000")),
        reused_quantity: None,
        missing_quantity: None,
        reused_line_total: None,
        planning_evidence: None,
    };
    let coverage = coverage();
    let groups = BTreeMap::from([(34, "Minerals".into())]);
    let root_facility = fixture_root_facility();

    let worksheet = project_production_worksheet(WorksheetProjectionInput {
        price_lines: &[material_price, output_price],
        material_lines: &[material],
        coverage: &coverage,
        facility: Some(&root_facility),
        material_cost: Money::parse("400").unwrap(),
        expected_revenue: Some(Money::parse("1000").unwrap()),
        estimated_margin: Some(Money::parse("540").unwrap()),
        pricing_complete: true,
        output_quantity: 1,
        group_labels: &groups,
        warnings: Vec::new(),
        planning: None,
    })
    .unwrap();

    assert!(worksheet.groups[0].items[0].is_build_resolved);
    assert_eq!(
        worksheet.groups[0].items[0].installation_cost,
        Some(Money::parse("50000").unwrap())
    );
    // Root's own (10000) plus the build-resolved row's own (50000).
    assert_eq!(
        worksheet.summary.installation_cost,
        Some(Money::parse("60000").unwrap())
    );
    assert_eq!(
        worksheet.summary.total_cost,
        Some(Money::parse("60400").unwrap())
    );
}

#[test]
fn summary_installation_cost_stays_unknown_without_the_roots_own_facility() {
    // A build-resolved sub-component's own cost being fully known
    // doesn't make the *root's* job cost known -- the root is still its
    // own job that needs its own facility selected, matching the
    // existing "unknown, not zero" contract already in place before
    // per-row costs existed.
    let material_price = line(34, "Tritanium", PlannerItemRole::Material, Some("4"));
    let output_price = line(587, "Rifter", PlannerItemRole::Output, Some("100"));
    let material = PlannedMaterialLine {
        type_id: 34,
        type_name: "Tritanium".into(),
        quantity_per_run: 100,
        total_quantity: 100,
        unit_price: Some(Money::parse("4").unwrap()),
        line_total: Some(Money::parse("400").unwrap()),
        missing: false,
        contributions: Vec::new(),
        is_build_resolved: true,
        installation_cost: Some(fixture_installation_cost("50000")),
        reused_quantity: None,
        missing_quantity: None,
        reused_line_total: None,
        planning_evidence: None,
    };
    let coverage = coverage();
    let groups = BTreeMap::from([(34, "Minerals".into())]);

    let worksheet = project_production_worksheet(WorksheetProjectionInput {
        price_lines: &[material_price, output_price],
        material_lines: &[material],
        coverage: &coverage,
        facility: None,
        material_cost: Money::parse("400").unwrap(),
        expected_revenue: Some(Money::parse("1000").unwrap()),
        estimated_margin: Some(Money::parse("550").unwrap()),
        pricing_complete: true,
        output_quantity: 1,
        group_labels: &groups,
        warnings: Vec::new(),
        planning: None,
    })
    .unwrap();

    assert_eq!(
        worksheet.groups[0].items[0].installation_cost,
        Some(Money::parse("50000").unwrap())
    );
    assert!(worksheet.summary.installation_cost.is_none());
    assert!(worksheet.summary.total_cost.is_none());
}

#[test]
fn summary_installation_excludes_a_resolved_linked_child_whose_install_is_folded_into_its_line_total(
) {
    // A Resolved linked child row carries its whole production cost
    // (materials + its own installation) in `line_total` and no separate
    // `installation_cost` breakdown. The parent's own installation line is
    // just the parent's own job -- the child's installation must not be
    // added again, and its absence as a per-row figure must not make the
    // summary "Incomplete".
    let material_price = line(34, "Tungsten Carbide", PlannerItemRole::Material, Some("5"));
    let output_price = line(
        11543,
        "Tungsten Carbide Armor Plate",
        PlannerItemRole::Output,
        Some("100"),
    );
    let child_row = PlannedMaterialLine {
        type_id: 34,
        type_name: "Tungsten Carbide".into(),
        quantity_per_run: 372_282,
        total_quantity: 372_282,
        // materials + the child's own installation, rolled up.
        unit_price: None,
        line_total: Some(Money::parse("2000000000").unwrap()),
        missing: false,
        contributions: Vec::new(),
        is_build_resolved: true,
        installation_cost: None,
        reused_quantity: None,
        missing_quantity: None,
        reused_line_total: None,
        planning_evidence: None,
    };
    let coverage = coverage();
    let groups = BTreeMap::from([(34, "Reactions".into())]);
    let root_facility = fixture_root_facility(); // own install 10000

    let worksheet = project_production_worksheet(WorksheetProjectionInput {
        price_lines: &[material_price, output_price],
        material_lines: &[child_row],
        coverage: &coverage,
        facility: Some(&root_facility),
        material_cost: Money::parse("2000000000").unwrap(),
        expected_revenue: Some(Money::parse("3000000000").unwrap()),
        estimated_margin: None,
        pricing_complete: true,
        output_quantity: 10_000,
        group_labels: &groups,
        warnings: Vec::new(),
        planning: None,
    })
    .unwrap();

    // The child row shows no separate installation figure.
    assert!(worksheet.groups[0].items[0].installation_cost.is_none());
    // Summary installation is the root's own job only -- complete, and
    // without the child's installation double-counted.
    assert_eq!(
        worksheet.summary.installation_cost,
        Some(Money::parse("10000").unwrap())
    );
    assert_eq!(
        worksheet.summary.total_cost,
        Some(Money::parse("2000010000").unwrap())
    );
}

fn line(
    type_id: i64,
    type_name: &str,
    role: PlannerItemRole,
    price: Option<&str>,
) -> PriceSnapshotLine {
    PriceSnapshotLine {
        type_id,
        type_name: type_name.into(),
        item_role: role,
        selection_kind: PricingSelectionKind::Default,
        manual_unit_price: None,
        price: price.map(|value| Money::parse(value).unwrap()),
        pricing_policy: None,
        missing: price.is_none(),
        source_note: String::new(),
        sort_order: 0,
        market_region_id: None,
        market_location_id: None,
    }
}

fn coverage() -> BuildCoverageReport {
    BuildCoverageReport {
        build_id: BuildId::new(),
        owner_id: OwnerId::new(),
        build_revision: 1,
        recipe_fingerprint: "recipe".into(),
        runs: 1,
        complete_quantity_coverage: false,
        complete_cost_coverage: true,
        material_lines: vec![MaterialCoverage {
            type_id: 34,
            type_name: "Tritanium".into(),
            sort_order: 0,
            required_quantity: 100,
            accounted_owned_quantity: 80,
            reserved_for_this_build: 0,
            reserved_by_other_builds: 0,
            unreserved_available_quantity: 80,
            available_to_this_build: 80,
            reservable_additional_quantity: 80,
            covered_quantity: 80,
            missing_quantity: 20,
            average_historical_unit_cost: Some(Money::parse("3").unwrap()),
            projected_historical_cost: Some(Money::parse("300").unwrap()),
            cost_quality: MaterialCostQuality::Known,
            quantity_coverage_state: QuantityCoverageState::PartiallyCovered,
            esi_observed_quantity: None,
            esi_reconciliation_difference: None,
            esi_observed_at: None,
            explanation: String::new(),
            warnings: Vec::new(),
            inventory_revision: 1,
        }],
        warnings: Vec::new(),
    }
}
