use super::*;

const TEST_SCOPE: crate::MarketScope = crate::DEFAULT_MARKET_SCOPE;

#[test]
fn estimated_profit_subtracts_installation_and_requires_complete_cost() {
    let revenue = Some(Money::parse("1000").unwrap());
    let materials = Money::parse("600").unwrap();
    let installation = Some(Money::parse("125").unwrap());

    assert_eq!(
        estimated_profit(true, revenue, materials, installation)
            .unwrap()
            .unwrap()
            .0
            .to_string(),
        "275.0000"
    );
    assert_eq!(
        estimated_profit(true, revenue, materials, None).unwrap(),
        None
    );
    assert_eq!(
        estimated_profit(false, revenue, materials, installation).unwrap(),
        None
    );
}

#[test]
fn build_has_no_status_field() {
    // Compile-only assertion: constructing a `Build` literal must not
    // require (or accept) a `status` field. If this compiles, the
    // field is gone.
    let build = build();
    assert_eq!(build.runs, 3);
}

#[test]
fn canonical_descendant_runs_are_not_user_editable() {
    let root = BuildId::new();
    let producer = BuildId::new();

    assert!(ensure_canonical_run_update(root, root, 1, 2).is_ok());
    assert!(ensure_canonical_run_update(root, producer, 1, 1).is_ok());
    assert!(matches!(
        ensure_canonical_run_update(root, producer, 1, 12),
        Err(IndustryError::Validation(message))
            if message == "Runs are derived from the top-level Build plan for this production step."
    ));
}

fn captured_recipe() -> CapturedRecipe {
    CapturedRecipe::capture(
        Uuid::new_v4(),
        "3389399".to_string(),
        ManufacturingRecipe {
            blueprint_type_id: 6_830,
            blueprint_name: "Rifter Blueprint".to_string(),
            duration_seconds: Some(600),
            materials: vec![
                iskworks_sde::RecipeLine {
                    type_id: 34,
                    type_name: "Tritanium".to_string(),
                    quantity: 1_000,
                },
                iskworks_sde::RecipeLine {
                    type_id: 35,
                    type_name: "Pyerite".to_string(),
                    quantity: 200,
                },
            ],
            products: vec![iskworks_sde::RecipeLine {
                type_id: 5_876,
                type_name: "Rifter".to_string(),
                quantity: 1,
            }],
        },
    )
    .unwrap()
}

#[test]
fn captured_reaction_formula_has_no_material_efficiency_concept() {
    let dataset_id = Uuid::new_v4();
    let formula = CapturedReactionFormula::capture(
        dataset_id,
        "3453885".to_string(),
        iskworks_sde::ReactionFormulaRecipe {
            reaction_formula_type_id: 46_157,
            reaction_formula_name: "Methanofullerene Reaction Formula".to_string(),
            duration_seconds: Some(10_800),
            materials: vec![
                iskworks_sde::RecipeLine {
                    type_id: 37,
                    type_name: "Isogen".to_string(),
                    quantity: 300,
                },
                iskworks_sde::RecipeLine {
                    type_id: 4_246,
                    type_name: "Fullerite-C50".to_string(),
                    quantity: 5,
                },
            ],
            products: vec![iskworks_sde::RecipeLine {
                type_id: 30_306,
                type_name: "Methanofullerene".to_string(),
                quantity: 160,
            }],
        },
    )
    .unwrap();

    assert_eq!(formula.reaction_formula_type_id, 46_157);
    assert_eq!(formula.source_sde_dataset_id, dataset_id);
    assert_eq!(formula.duration_seconds_per_run, Some(10_800));
    assert_eq!(formula.materials.len(), 2);
    assert_eq!(formula.products[0].type_id, 30_306);
    assert_eq!(formula.primary_product().type_id, 30_306);
    assert!(!formula.fingerprint.is_empty());
}

#[test]
fn captured_reaction_formula_rejects_recipes_without_products_or_materials() {
    let no_products = CapturedReactionFormula::capture(
        Uuid::new_v4(),
        "3453885".to_string(),
        iskworks_sde::ReactionFormulaRecipe {
            reaction_formula_type_id: 46_157,
            reaction_formula_name: "Methanofullerene Reaction Formula".to_string(),
            duration_seconds: Some(10_800),
            materials: vec![iskworks_sde::RecipeLine {
                type_id: 37,
                type_name: "Isogen".to_string(),
                quantity: 300,
            }],
            products: vec![],
        },
    );
    assert!(matches!(no_products, Err(IndustryError::InvalidRecipe)));

    let no_materials = CapturedReactionFormula::capture(
        Uuid::new_v4(),
        "3453885".to_string(),
        iskworks_sde::ReactionFormulaRecipe {
            reaction_formula_type_id: 46_157,
            reaction_formula_name: "Methanofullerene Reaction Formula".to_string(),
            duration_seconds: Some(10_800),
            materials: vec![],
            products: vec![iskworks_sde::RecipeLine {
                type_id: 30_306,
                type_name: "Methanofullerene".to_string(),
                quantity: 160,
            }],
        },
    );
    assert!(matches!(no_materials, Err(IndustryError::InvalidRecipe)));
}

#[test]
fn build_recipe_serializes_manufacturing_with_the_legacy_flat_shape() {
    // BuildRecipe is internally tagged specifically so a manufacturing
    // recipe keeps the exact JSON shape it had before this enum existed
    // -- the SPA reads build.recipe.blueprintName/.blueprintTypeId at
    // the top level. This test is the regression guard for that.
    let recipe = BuildRecipe::Manufacturing(captured_recipe());

    let value = serde_json::to_value(&recipe).unwrap();

    assert_eq!(value["kind"], "manufacturing");
    assert_eq!(
        value["blueprintTypeId"],
        captured_recipe().blueprint_type_id
    );
    assert_eq!(value["blueprintName"], captured_recipe().blueprint_name);
    assert!(value["materials"].is_array());
    assert!(value["products"].is_array());

    let round_tripped: BuildRecipe = serde_json::from_value(value).unwrap();
    assert_eq!(round_tripped, recipe);
}

#[test]
fn build_recipe_serializes_reaction_with_an_additive_kind_tag() {
    let recipe = BuildRecipe::Reaction(fixture_reaction_formula());

    let value = serde_json::to_value(&recipe).unwrap();

    assert_eq!(value["kind"], "reaction");
    assert_eq!(
        value["reactionFormulaTypeId"],
        fixture_reaction_formula().reaction_formula_type_id
    );
    assert_eq!(
        value["reactionFormulaName"],
        fixture_reaction_formula().reaction_formula_name
    );

    let round_tripped: BuildRecipe = serde_json::from_value(value).unwrap();
    assert_eq!(round_tripped, recipe);
}

#[test]
fn recipe_selection_serializes_with_a_mode_tag() {
    let manufacturing = RecipeSelection::Manufacturing {
        blueprint_type_id: 6_830,
    };
    let value = serde_json::to_value(manufacturing).unwrap();
    assert_eq!(value["mode"], "manufacturing");
    assert_eq!(value["blueprintTypeId"], 6_830);
    assert_eq!(
        serde_json::from_value::<RecipeSelection>(value).unwrap(),
        manufacturing
    );

    let reaction = RecipeSelection::Reaction {
        reaction_formula_type_id: 46_157,
    };
    let value = serde_json::to_value(reaction).unwrap();
    assert_eq!(value["mode"], "reaction");
    assert_eq!(value["reactionFormulaTypeId"], 46_157);
    assert_eq!(
        serde_json::from_value::<RecipeSelection>(value).unwrap(),
        reaction
    );
}

fn fixture_reaction_formula() -> CapturedReactionFormula {
    CapturedReactionFormula::capture(
        Uuid::new_v4(),
        "3453885".to_string(),
        iskworks_sde::ReactionFormulaRecipe {
            reaction_formula_type_id: 46_157,
            reaction_formula_name: "Methanofullerene Reaction Formula".to_string(),
            duration_seconds: Some(10_800),
            materials: vec![iskworks_sde::RecipeLine {
                type_id: 37,
                type_name: "Isogen".to_string(),
                quantity: 300,
            }],
            products: vec![iskworks_sde::RecipeLine {
                type_id: 30_306,
                type_name: "Methanofullerene".to_string(),
                quantity: 160,
            }],
        },
    )
    .unwrap()
}

fn build() -> Build {
    let now = Utc::now();
    Build {
        id: BuildId::new(),
        workspace_id: WorkspaceId::new(),
        owner_id: OwnerId::new(),
        name: "Doctrine Rifters".to_string(),
        recipe: BuildRecipe::Manufacturing(captured_recipe()),
        runs: 3,
        notes: String::new(),
        revision: 1,
        created_at: now,
        updated_at: now,
        draft_planning: None,
        recipe_currency: RecipeCurrency::Current,
        active_sde_version: Some("3389399".to_string()),
        product_category_name: None,
        product_group_name: None,
        selected_blueprint_origin: None,
        has_owned_blueprint: false,
    }
}

fn source(items: Vec<PriceSourceItem>) -> PriceSource {
    let now = Utc::now();
    PriceSource {
        id: PriceSourceId::new(),
        workspace_id: WorkspaceId::new(),
        name: "Home Market".to_string(),
        description: String::new(),
        kind: PriceSourceKind::Manual,
        revision: 4,
        item_count: items.len() as u64,
        recent_build_count: 0,
        items,
        created_at: now,
        updated_at: now,
    }
}

fn item(type_id: i64, name: &str, price: &str) -> PriceSourceItem {
    PriceSourceItem {
        type_id,
        type_name: name.to_string(),
        price: Money::parse(price).unwrap(),
        note: String::new(),
        updated_at: Utc::now(),
    }
}

#[test]
fn money_is_exact_and_rejects_invalid_precision() {
    assert_eq!(
        Money::parse("4.1250")
            .unwrap()
            .checked_mul_quantity(3)
            .unwrap()
            .0
            .to_string(),
        "12.3750"
    );
    assert!(matches!(
        Money::parse("1.00001"),
        Err(IndustryError::InvalidMoney)
    ));
    assert!(matches!(
        Money::parse("-1"),
        Err(IndustryError::InvalidMoney)
    ));
}

#[test]
fn market_source_kinds_use_order_books() {
    assert!(!PriceSourceKind::Manual.uses_order_book());
    assert!(PriceSourceKind::EveClientMarketExport.uses_order_book());
    assert!(PriceSourceKind::EsiMarketOrders.uses_order_book());
    assert_eq!(
        serde_json::to_string(&PriceSourceKind::EsiMarketOrders).unwrap(),
        "\"esiMarketOrders\""
    );
}

#[test]
fn build_market_coverage_contains_deduplicated_materials_and_products() {
    let mut recipe = captured_recipe();
    recipe.products.push(CapturedRecipeLine {
        type_id: 34,
        type_name: "Tritanium".to_string(),
        quantity_per_run: 1,
        sort_order: 1,
    });

    assert_eq!(
        build_market_coverage(&BuildRecipe::Manufacturing(recipe)),
        vec![
            MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            },
            MarketCoverageRegistration {
                type_id: 35,
                type_name: "Pyerite".to_string(),
            },
            MarketCoverageRegistration {
                type_id: 5_876,
                type_name: "Rifter".to_string(),
            },
        ]
    );
}

#[test]
fn planner_pricing_resolves_explicit_policy_and_manual_price() {
    let (overrides, policies) = resolve_planner_pricing(
        &build(),
        vec![
            ItemPricingSelectionInput {
                type_id: 34,
                role: PlannerItemRole::Material,
                selection: ItemPricingSelection::MarketPolicy {
                    policy: crate::MarketPricingPolicy::LowestSell,
                },
            },
            ItemPricingSelectionInput {
                type_id: 5_876,
                role: PlannerItemRole::Output,
                selection: ItemPricingSelection::Manual {
                    unit_price: "3420000000".into(),
                },
            },
        ],
    )
    .unwrap();

    assert_eq!(policies[0].type_id, 34);
    assert_eq!(
        policies[0].pricing_policy,
        crate::MarketPricingPolicy::LowestSell
    );
    assert_eq!(overrides[0].type_name, "Rifter");
    assert_eq!(overrides[0].price, "3420000000.0000");
}

#[test]
fn planner_pricing_rejects_item_role_not_present_in_recipe() {
    let result = resolve_planner_pricing(
        &build(),
        vec![ItemPricingSelectionInput {
            type_id: 34,
            role: PlannerItemRole::Output,
            selection: ItemPricingSelection::Default,
        }],
    );

    assert!(matches!(result, Err(IndustryError::Validation(_))));
}

#[test]
fn calculates_complete_plan_with_exact_totals() {
    let source = source(vec![
        item(34, "Tritanium", "4.1250"),
        item(35, "Pyerite", "8.5000"),
        item(5_876, "Rifter", "100000.0000"),
    ]);
    let plan = calculate_plan(
        &build(),
        TEST_SCOPE,
        TEST_SCOPE,
        &source.items,
        None,
        Vec::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    assert!(plan.pricing_complete);
    assert_eq!(plan.estimated_material_cost.0.to_string(), "17475.0000");
    assert_eq!(plan.expected_revenue.unwrap().0.to_string(), "300000.0000");
    assert_eq!(plan.estimated_margin.unwrap().0.to_string(), "282525.0000");
}

#[test]
fn transient_candidate_converts_to_persisted_plan_without_calculation_drift() {
    let build = build();
    let source = source(vec![
        item(34, "Tritanium", "4.1250"),
        item(35, "Pyerite", "8.5000"),
        item(5_876, "Rifter", "100000.0000"),
    ]);
    let calculated = calculate_candidate_plan(
        &build,
        TEST_SCOPE,
        TEST_SCOPE,
        &source.items,
        None,
        Vec::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    assert_eq!(calculated.runs, 3);
    assert_eq!(calculated.material_lines[0].total_quantity, 3_000);
    assert_eq!(
        calculated.estimated_material_cost.0.to_string(),
        "17475.0000"
    );

    let persisted = calculated
        .into_plan_revision(&build, 1, Utc::now())
        .unwrap();
    assert_eq!(persisted.runs, 3);
    assert_eq!(persisted.material_lines[0].total_quantity, 3_000);
    assert_eq!(
        persisted.estimated_material_cost.0.to_string(),
        "17475.0000"
    );
    assert_eq!(
        persisted.expected_revenue.unwrap().0.to_string(),
        "300000.0000"
    );
}

#[test]
fn transient_calculator_prices_a_direct_manufacturing_recipe_without_build_persistence() {
    let build = build();
    let source = source(vec![
        item(34, "Tritanium", "4.1250"),
        item(35, "Pyerite", "8.5000"),
        item(5_876, "Rifter", "100000.0000"),
    ]);
    let (requirements, _) = preview_recipe_effects(&build.recipe, build.runs, 0, 0, None).unwrap();

    let calculated = calculate_transient_plan(TransientCalculationInput {
        build: &build,
        material_scope: TEST_SCOPE,
        output_scope: TEST_SCOPE,
        market_items: &source.items,
        manual_price_list: None,
        overrides: Vec::new(),
        pricing_policies: &BTreeMap::new(),
        effective_requirements: &requirements,
        root_facility: None,
        blueprint: None,
        expansion: None,
        manufacturing_profile: None,
        reaction_profile: None,
        component_facility_overrides: &BTreeMap::new(),
        component_eivs: &BTreeMap::new(),
        linked_build_material_costs: &BTreeMap::new(),
        material_coverage: &BTreeMap::new(),
    })
    .unwrap();

    assert!(calculated.pricing_complete);
    assert_eq!(calculated.material_lines[0].total_quantity, 3_000);
    assert_eq!(
        calculated.estimated_material_cost.0.to_string(),
        "17475.0000"
    );
    assert_eq!(
        calculated.expected_revenue.unwrap().0.to_string(),
        "300000.0000"
    );
    assert_eq!(calculated.estimated_margin, None);
}

#[test]
fn candidate_comparison_reports_exact_material_and_duration_deltas() {
    let source = source(vec![
        item(34, "Tritanium", "4.1250"),
        item(35, "Pyerite", "8.5000"),
        item(5_876, "Rifter", "100000.0000"),
    ]);
    let mut current = calculate_candidate_plan(
        &build(),
        TEST_SCOPE,
        TEST_SCOPE,
        &source.items,
        None,
        Vec::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    current.material_lines[0].total_quantity = 28_800;
    current.blueprint = Some(test_blueprint_snapshot(1_860).into());
    let current = current.into_plan_revision(&build(), 1, Utc::now()).unwrap();
    let mut candidate = calculate_candidate_plan(
        &build(),
        TEST_SCOPE,
        TEST_SCOPE,
        &source.items,
        None,
        Vec::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    candidate.material_lines[0].total_quantity = 27_075;
    candidate.blueprint = Some(test_blueprint_snapshot(1_620).into());

    let comparison = crate::compare_build_plan_candidate(&current, &candidate).unwrap();
    let tritanium = comparison
        .materials
        .iter()
        .find(|line| line.type_id == 34)
        .unwrap();

    assert_eq!(tritanium.current_quantity, 28_800);
    assert_eq!(tritanium.candidate_quantity, 27_075);
    assert_eq!(tritanium.delta, -1_725);
    assert_eq!(comparison.duration_seconds_delta, Some(-240));
}

#[test]
fn applies_component_expansion_replacing_material_lines_and_recomputing_totals() {
    let source = source(vec![
        item(34, "Tritanium", "4.1250"),
        item(35, "Pyerite", "8.5000"),
        item(5_876, "Rifter", "100000.0000"),
        item(9_301, "Material M", "10.0000"),
        // deliberately no price for 9_401, to prove missing_price_count reacts
    ]);
    let mut calculated = calculate_candidate_plan(
        &build(),
        TEST_SCOPE,
        TEST_SCOPE,
        &source.items,
        None,
        Vec::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(calculated.material_lines.len(), 2);

    let expansion = crate::ComponentExpansion {
        components: vec![
            crate::ResolvedComponent {
                type_id: 9_301,
                type_name: "Material M".to_string(),
                total_quantity: 20,
                // Consumed by both the root recipe directly and by
                // Component C -- exercises both `ContributionSource`
                // variants and multi-parent aggregation on one row.
                contributions: vec![
                    crate::Contribution {
                        source: crate::ContributionSource::Root,
                        quantity: 15,
                    },
                    crate::Contribution {
                        source: crate::ContributionSource::Component { type_id: 9_401 },
                        quantity: 5,
                    },
                ],
                resolution: crate::ComponentResolutionOutcome::Buy,
            },
            crate::ResolvedComponent {
                type_id: 9_401,
                type_name: "Component C".to_string(),
                total_quantity: 5,
                contributions: vec![crate::Contribution {
                    source: crate::ContributionSource::Root,
                    quantity: 5,
                }],
                resolution: crate::ComponentResolutionOutcome::Buy,
            },
        ],
    };

    apply_component_expansion(
        &mut calculated,
        &expansion,
        TEST_SCOPE,
        &source.items,
        &[],
        &BTreeMap::new(),
        None,
        None,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    // Old root materials (Tritanium/Pyerite) are gone entirely, replaced
    // by the expansion's own type_id set.
    assert_eq!(calculated.material_lines.len(), 2);
    assert!(calculated
        .material_lines
        .iter()
        .all(|line| line.type_id != 34 && line.type_id != 35));

    // build().runs is 3 -- quantities must NOT be re-multiplied, they're
    // already final per ComponentExpansion's contract.
    let material_m = calculated
        .material_lines
        .iter()
        .find(|line| line.type_id == 9_301)
        .unwrap();
    assert_eq!(material_m.total_quantity, 20);
    assert_eq!(material_m.line_total.unwrap().0.to_string(), "200.0000");
    assert_eq!(
        material_m.contributions,
        vec![
            MaterialContribution {
                parent_type_id: None,
                parent_type_name: "Rifter".to_string(),
                quantity: 15,
            },
            MaterialContribution {
                parent_type_id: Some(9_401),
                parent_type_name: "Component C".to_string(),
                quantity: 5,
            },
        ]
    );

    let component_c = calculated
        .material_lines
        .iter()
        .find(|line| line.type_id == 9_401)
        .unwrap();
    assert_eq!(component_c.total_quantity, 5);
    assert!(component_c.missing);

    assert_eq!(calculated.missing_price_count, 1);
    assert!(!calculated.pricing_complete);
    assert!(calculated.estimated_margin.is_none());
    assert_eq!(calculated.estimated_material_cost.0.to_string(), "200.0000");

    let output_lines: Vec<_> = calculated
        .price_lines
        .iter()
        .filter(|line| line.item_role == PlannerItemRole::Output)
        .collect();
    assert_eq!(output_lines.len(), 1);
    assert_eq!(output_lines[0].type_id, 5_876);
    let material_price_lines: Vec<_> = calculated
        .price_lines
        .iter()
        .filter(|line| line.item_role == PlannerItemRole::Material)
        .collect();
    assert_eq!(material_price_lines.len(), 2);
}

fn fixture_installation_profile(role: FacilityRole) -> IndustryFacilityProfile {
    let now = Utc::now();
    IndustryFacilityProfile {
        id: FacilityProfileId::new(),
        workspace_id: WorkspaceId::new(),
        name: match role {
            FacilityRole::Manufacturing => "Test Assembly Array".to_string(),
            FacilityRole::Reaction => "Test Athanor".to_string(),
        },
        kind: crate::FacilityKind::Manual,
        role,
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
        created_at: now,
        updated_at: now,
    }
}

fn build_resolved_component(
    type_id: i64,
    name: &str,
    total_quantity: u64,
    recipe: RecipeSelection,
) -> crate::ResolvedComponent {
    crate::ResolvedComponent {
        type_id,
        type_name: name.to_string(),
        total_quantity,
        contributions: vec![crate::Contribution {
            source: crate::ContributionSource::Root,
            quantity: total_quantity,
        }],
        resolution: crate::ComponentResolutionOutcome::Build {
            recipe,
            runs: 1,
            produced_quantity: total_quantity,
            surplus: 0,
            duration_seconds: None,
        },
    }
}

fn buy_resolved_component(
    type_id: i64,
    name: &str,
    total_quantity: u64,
) -> crate::ResolvedComponent {
    crate::ResolvedComponent {
        type_id,
        type_name: name.to_string(),
        total_quantity,
        contributions: vec![crate::Contribution {
            source: crate::ContributionSource::Root,
            quantity: total_quantity,
        }],
        resolution: crate::ComponentResolutionOutcome::Buy,
    }
}

#[test]
fn apply_component_expansion_blends_buy_missing_between_inventory_cost_and_market_price() {
    let source = source(vec![item(9_301, "Material M", "10.0000")]);
    let mut calculated = calculate_candidate_plan(
        &build(),
        TEST_SCOPE,
        TEST_SCOPE,
        &source.items,
        None,
        Vec::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    let expansion = crate::ComponentExpansion {
        components: vec![buy_resolved_component(9_301, "Material M", 100)],
    };
    let material_coverage = BTreeMap::from([(
        9_301,
        MaterialCoverageSummary {
            available_to_this_build: 40,
            average_historical_unit_cost: Some(Money::parse("4.0000").unwrap()),
        },
    )]);

    apply_component_expansion(
        &mut calculated,
        &expansion,
        TEST_SCOPE,
        &source.items,
        &[],
        &BTreeMap::new(),
        None,
        None,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &material_coverage,
    )
    .unwrap();

    let row = calculated
        .material_lines
        .iter()
        .find(|line| line.type_id == 9_301)
        .unwrap();
    assert!(!row.missing);
    // 60 missing @ 10.0000 market + 40 reused @ 4.0000 inventory --
    // not 100 @ 10.0000.
    assert_eq!(row.line_total.unwrap().0.to_string(), "760.0000");
    assert_eq!(row.unit_price.unwrap().0.to_string(), "7.6000");
}

#[test]
fn apply_component_expansion_blends_build_missing_between_inventory_cost_and_linked_build_cost() {
    let source = source(vec![]);
    let mut calculated = calculate_candidate_plan(
        &build(),
        TEST_SCOPE,
        TEST_SCOPE,
        &source.items,
        None,
        Vec::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    let expansion = crate::ComponentExpansion {
        components: vec![build_resolved_component(
            9_301,
            "Sub-Component M",
            100,
            RecipeSelection::Manufacturing {
                blueprint_type_id: 12_345,
            },
        )],
    };
    // The linked build was already sized to cover just the
    // shortage -- this total is its own live cost for producing that
    // shortage, not for the row's full 100-unit requirement.
    let linked_build_material_costs = BTreeMap::from([(9_301, Money::parse("300.0000").unwrap())]);
    let material_coverage = BTreeMap::from([(
        9_301,
        MaterialCoverageSummary {
            available_to_this_build: 40,
            average_historical_unit_cost: Some(Money::parse("4.0000").unwrap()),
        },
    )]);

    apply_component_expansion(
        &mut calculated,
        &expansion,
        TEST_SCOPE,
        &source.items,
        &[],
        &BTreeMap::new(),
        None,
        None,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &linked_build_material_costs,
        &material_coverage,
    )
    .unwrap();

    let row = calculated
        .material_lines
        .iter()
        .find(|line| line.type_id == 9_301)
        .unwrap();
    assert!(!row.missing);
    // 300 (the linked build's own cost for the shortage) + 40 reused
    // @ 4.0000 inventory = 460, not divided/rescaled against the full
    // 100-unit requirement a second time.
    assert_eq!(row.line_total.unwrap().0.to_string(), "460.0000");
    assert_eq!(row.unit_price.unwrap().0.to_string(), "4.6000");
}

#[test]
fn apply_component_expansion_leaves_a_missing_scoped_row_unknown_when_inventory_cost_is_unknown() {
    let source = source(vec![item(9_301, "Material M", "10.0000")]);
    let mut calculated = calculate_candidate_plan(
        &build(),
        TEST_SCOPE,
        TEST_SCOPE,
        &source.items,
        None,
        Vec::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    let expansion = crate::ComponentExpansion {
        components: vec![buy_resolved_component(9_301, "Material M", 100)],
    };
    let material_coverage = BTreeMap::from([(
        9_301,
        MaterialCoverageSummary {
            available_to_this_build: 40,
            average_historical_unit_cost: None, // unknown
        },
    )]);

    apply_component_expansion(
        &mut calculated,
        &expansion,
        TEST_SCOPE,
        &source.items,
        &[],
        &BTreeMap::new(),
        None,
        None,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &material_coverage,
    )
    .unwrap();

    let row = calculated
        .material_lines
        .iter()
        .find(|line| line.type_id == 9_301)
        .unwrap();
    // Unknown, not zero and not the market-price-only figure -- a
    // known market price for the *missing* portion doesn't rescue a
    // row whose *reused* portion has no known cost.
    assert!(row.missing);
    assert!(row.unit_price.is_none());
    assert!(row.line_total.is_none());
    assert!(!calculated.pricing_complete);
}

#[test]
fn apply_component_expansion_computes_and_sums_each_build_resolved_rows_own_installation_cost() {
    let source = source(vec![item(9_301, "Sub-Component M", "10.0000")]);
    let mut calculated = calculate_candidate_plan(
        &build(),
        TEST_SCOPE,
        TEST_SCOPE,
        &source.items,
        None,
        Vec::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    let expansion = crate::ComponentExpansion {
        components: vec![
            build_resolved_component(
                9_301,
                "Sub-Component M",
                20,
                RecipeSelection::Manufacturing {
                    blueprint_type_id: 12_345,
                },
            ),
            build_resolved_component(
                9_401,
                "Sub-Component R",
                5,
                RecipeSelection::Reaction {
                    reaction_formula_type_id: 67_890,
                },
            ),
        ],
    };
    let manufacturing_profile = fixture_installation_profile(FacilityRole::Manufacturing);
    let reaction_profile = fixture_installation_profile(FacilityRole::Reaction);
    let component_eivs = BTreeMap::from([
        (9_301, Money::parse("1000000").unwrap()),
        (9_401, Money::parse("2000000").unwrap()),
    ]);

    apply_component_expansion(
        &mut calculated,
        &expansion,
        TEST_SCOPE,
        &source.items,
        &[],
        &BTreeMap::new(),
        Some(&manufacturing_profile),
        Some(&reaction_profile),
        &BTreeMap::new(),
        &component_eivs,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    let manufacturing_row = calculated
        .material_lines
        .iter()
        .find(|line| line.type_id == 9_301)
        .unwrap();
    assert!(manufacturing_row.is_build_resolved);
    let manufacturing_cost = manufacturing_row.installation_cost.as_ref().unwrap();
    assert!(manufacturing_cost.complete);
    assert_eq!(
        manufacturing_cost.total.unwrap().0.to_string(),
        "50000.000000"
    );

    let reaction_row = calculated
        .material_lines
        .iter()
        .find(|line| line.type_id == 9_401)
        .unwrap();
    assert!(reaction_row.is_build_resolved);
    let reaction_cost = reaction_row.installation_cost.as_ref().unwrap();
    assert!(reaction_cost.complete);
    assert_eq!(reaction_cost.total.unwrap().0.to_string(), "100000.000000");

    apply_profitability_costs(&mut calculated).unwrap();
    // No root facility is selected here, so the root's own installation
    // cost is unknown -- the overall total (and therefore margin) stays
    // None even though every component row's own cost is fully known,
    // matching the existing "unknown, not zero" completeness contract.
    assert!(calculated.estimated_margin.is_none());
}

#[test]
fn apply_component_expansion_folds_a_resolved_linked_childs_install_into_its_line_total() {
    // When a build-resolved row has a rolled-up linked-child cost, that
    // figure IS the row's whole cost (materials + the child's own
    // installation). The row must then carry no separate `installation_cost`
    // breakdown -- even though a matching facility slot / EIV is available --
    // so the child's installation is never also added to the parent's own
    // installation line.
    let source = source(vec![item(9_401, "Sub-Component R", "10.0000")]);
    let mut calculated = calculate_candidate_plan(
        &build(),
        TEST_SCOPE,
        TEST_SCOPE,
        &source.items,
        None,
        Vec::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    let expansion = crate::ComponentExpansion {
        components: vec![build_resolved_component(
            9_401,
            "Sub-Component R",
            5,
            RecipeSelection::Reaction {
                reaction_formula_type_id: 67_890,
            },
        )],
    };
    let reaction_profile = fixture_installation_profile(FacilityRole::Reaction);
    let component_eivs = BTreeMap::from([(9_401, Money::parse("2000000").unwrap())]);
    // The linked child's whole production cost: its materials plus its own
    // installation, already rolled up by the hierarchy walk.
    let linked_build_material_costs =
        BTreeMap::from([(9_401, Money::parse("15000000000").unwrap())]);

    apply_component_expansion(
        &mut calculated,
        &expansion,
        TEST_SCOPE,
        &source.items,
        &[],
        &BTreeMap::new(),
        None,
        Some(&reaction_profile),
        &BTreeMap::new(),
        &component_eivs,
        &linked_build_material_costs,
        &BTreeMap::new(),
    )
    .unwrap();

    let row = calculated
        .material_lines
        .iter()
        .find(|line| line.type_id == 9_401)
        .unwrap();
    assert!(row.is_build_resolved);
    assert_eq!(row.line_total.unwrap().0.to_string(), "15000000000.0000");
    // No separate installation figure on the row, despite a reaction profile
    // + EIV being available -- it's inside `line_total`, so it can't also be
    // added to the parent's own installation line.
    assert!(row.installation_cost.is_none());
}

#[test]
fn apply_component_expansion_leaves_a_rows_cost_unknown_without_its_matching_facility() {
    let source = source(vec![item(9_301, "Sub-Component M", "10.0000")]);
    let mut calculated = calculate_candidate_plan(
        &build(),
        TEST_SCOPE,
        TEST_SCOPE,
        &source.items,
        None,
        Vec::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    calculated.expected_revenue = Some(Money::parse("1000000").unwrap());

    let expansion = crate::ComponentExpansion {
        components: vec![build_resolved_component(
            9_301,
            "Sub-Component M",
            20,
            RecipeSelection::Manufacturing {
                blueprint_type_id: 12_345,
            },
        )],
    };
    // No manufacturing facility supplied, even though the one
    // build-resolved row is manufacturing-kind.
    let component_eivs = BTreeMap::from([(9_301, Money::parse("1000000").unwrap())]);

    apply_component_expansion(
        &mut calculated,
        &expansion,
        TEST_SCOPE,
        &source.items,
        &[],
        &BTreeMap::new(),
        None,
        None,
        &BTreeMap::new(),
        &component_eivs,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    let row = calculated
        .material_lines
        .iter()
        .find(|line| line.type_id == 9_301)
        .unwrap();
    assert!(row.is_build_resolved);
    assert!(row.installation_cost.is_none());

    apply_profitability_costs(&mut calculated).unwrap();
    assert!(calculated.estimated_margin.is_none());
}

#[test]
fn apply_component_expansion_prefers_a_rows_own_facility_override_over_the_shared_slot() {
    let source = source(vec![item(9_301, "Sub-Component M", "10.0000")]);
    let mut calculated = calculate_candidate_plan(
        &build(),
        TEST_SCOPE,
        TEST_SCOPE,
        &source.items,
        None,
        Vec::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    let expansion = crate::ComponentExpansion {
        components: vec![build_resolved_component(
            9_301,
            "Sub-Component M",
            20,
            RecipeSelection::Manufacturing {
                blueprint_type_id: 12_345,
            },
        )],
    };
    let shared_manufacturing_profile = fixture_installation_profile(FacilityRole::Manufacturing);
    let mut override_profile = shared_manufacturing_profile.clone();
    // A distinct system cost index (0.10 vs the shared slot's fixture
    // default of 0.05) so the two profiles' totals are provably
    // different, not just structurally different objects.
    override_profile.manual_system_cost_index = Some(Decimal::new(10, 2));
    let component_eivs = BTreeMap::from([(9_301, Money::parse("1000000").unwrap())]);
    let component_facility_overrides = BTreeMap::from([(9_301, override_profile)]);

    apply_component_expansion(
        &mut calculated,
        &expansion,
        TEST_SCOPE,
        &source.items,
        &[],
        &BTreeMap::new(),
        Some(&shared_manufacturing_profile),
        None,
        &component_facility_overrides,
        &component_eivs,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    let row = calculated
        .material_lines
        .iter()
        .find(|line| line.type_id == 9_301)
        .unwrap();
    let cost = row.installation_cost.as_ref().unwrap();
    // 1,000,000 EIV * the override's 0.10 index = 100,000 -- not the
    // shared slot's 0.05 index, which would give 50,000.
    assert_eq!(cost.total.unwrap().0.to_string(), "100000.000000");
}

#[test]
fn apply_component_expansion_falls_back_to_shared_slot_without_an_override() {
    let source = source(vec![item(9_301, "Sub-Component M", "10.0000")]);
    let mut calculated = calculate_candidate_plan(
        &build(),
        TEST_SCOPE,
        TEST_SCOPE,
        &source.items,
        None,
        Vec::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    let expansion = crate::ComponentExpansion {
        components: vec![build_resolved_component(
            9_301,
            "Sub-Component M",
            20,
            RecipeSelection::Manufacturing {
                blueprint_type_id: 12_345,
            },
        )],
    };
    let manufacturing_profile = fixture_installation_profile(FacilityRole::Manufacturing);
    let component_eivs = BTreeMap::from([(9_301, Money::parse("1000000").unwrap())]);

    apply_component_expansion(
        &mut calculated,
        &expansion,
        TEST_SCOPE,
        &source.items,
        &[],
        &BTreeMap::new(),
        Some(&manufacturing_profile),
        None,
        &BTreeMap::new(),
        &component_eivs,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    let row = calculated
        .material_lines
        .iter()
        .find(|line| line.type_id == 9_301)
        .unwrap();
    let cost = row.installation_cost.as_ref().unwrap();
    assert_eq!(cost.total.unwrap().0.to_string(), "50000.000000");
}

#[test]
fn apply_component_expansion_costs_a_demand_merged_row_via_its_override_when_two_parents_need_it() {
    let source = source(vec![item(9_301, "Material M", "10.0000")]);
    let mut calculated = calculate_candidate_plan(
        &build(),
        TEST_SCOPE,
        TEST_SCOPE,
        &source.items,
        None,
        Vec::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    let expansion = crate::ComponentExpansion {
        components: vec![
            crate::ResolvedComponent {
                type_id: 9_301,
                type_name: "Material M".to_string(),
                total_quantity: 20,
                // Needed both directly by the root and by Component C --
                // demand for this type_id has already been merged into
                // one row before facility resolution ever runs.
                contributions: vec![
                    crate::Contribution {
                        source: crate::ContributionSource::Root,
                        quantity: 15,
                    },
                    crate::Contribution {
                        source: crate::ContributionSource::Component { type_id: 9_401 },
                        quantity: 5,
                    },
                ],
                resolution: crate::ComponentResolutionOutcome::Build {
                    recipe: RecipeSelection::Manufacturing {
                        blueprint_type_id: 12_345,
                    },
                    runs: 1,
                    produced_quantity: 20,
                    surplus: 0,
                    duration_seconds: None,
                },
            },
            crate::ResolvedComponent {
                type_id: 9_401,
                type_name: "Component C".to_string(),
                total_quantity: 5,
                contributions: vec![crate::Contribution {
                    source: crate::ContributionSource::Root,
                    quantity: 5,
                }],
                resolution: crate::ComponentResolutionOutcome::Buy,
            },
        ],
    };
    let override_profile = fixture_installation_profile(FacilityRole::Manufacturing);
    let component_eivs = BTreeMap::from([(9_301, Money::parse("1000000").unwrap())]);
    let component_facility_overrides = BTreeMap::from([(9_301, override_profile)]);

    apply_component_expansion(
        &mut calculated,
        &expansion,
        TEST_SCOPE,
        &source.items,
        &[],
        &BTreeMap::new(),
        None,
        None,
        &component_facility_overrides,
        &component_eivs,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    // Still exactly one row for the merged type_id, costed via its
    // override even though no shared slot is selected at all.
    assert_eq!(
        calculated
            .material_lines
            .iter()
            .filter(|line| line.type_id == 9_301)
            .count(),
        1
    );
    let row = calculated
        .material_lines
        .iter()
        .find(|line| line.type_id == 9_301)
        .unwrap();
    let cost = row.installation_cost.as_ref().unwrap();
    assert_eq!(cost.total.unwrap().0.to_string(), "50000.000000");
}

fn test_blueprint_snapshot(duration: u64) -> crate::BlueprintSnapshot {
    crate::BlueprintSnapshot {
        id: Uuid::new_v4(),
        build_id: BuildId::new(),
        source_mode: crate::BlueprintSourceMode::Manual,
        blueprint_type_id: 6_830,
        blueprint_name: "Rifter Blueprint".into(),
        kind: crate::BlueprintKind::Copy,
        material_efficiency: 10,
        time_efficiency: 20,
        licensed_runs: Some(30),
        requested_runs: 1,
        source_observation_id: None,
        source_eve_item_id: None,
        source_owner_id: None,
        source_owner_name: None,
        source_location_id: None,
        source_location_name: None,
        observed_at: None,
        imported_at: None,
        manual_notes: None,
        planned_duration_seconds: Some(duration),
        formula_version: "test".into(),
        captured_at: Utc::now(),
    }
}

#[test]
fn missing_prices_are_not_treated_as_zero_or_complete_margin() {
    let source = source(vec![item(34, "Tritanium", "4.0000")]);
    let plan = calculate_plan(
        &build(),
        TEST_SCOPE,
        TEST_SCOPE,
        &source.items,
        None,
        Vec::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    assert!(!plan.pricing_complete);
    assert_eq!(plan.missing_price_count, 1);
    assert_eq!(plan.estimated_material_cost.0.to_string(), "12000.0000");
    assert!(plan.expected_revenue.is_none());
    assert!(plan.estimated_margin.is_none());
}

#[test]
fn captured_recipe_and_snapshot_are_owned_values() {
    let mut source = source(vec![item(34, "Tritanium", "4.0000")]);
    let plan = calculate_plan(
        &build(),
        TEST_SCOPE,
        TEST_SCOPE,
        &source.items,
        None,
        Vec::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    source.items[0].price = Money::parse("99.0000").unwrap();

    assert_eq!(
        plan.snapshot.items[0].price.unwrap().0.to_string(),
        "4.0000"
    );
}

#[test]
fn build_pricing_policies_apply_defaults_and_item_overrides() {
    let build = build();
    let (materials, output) = market_price_requests(
        &build,
        None,
        crate::MarketPricingPolicy::HighestBuy,
        crate::MarketPricingPolicy::LowestSell,
        &[BuildItemPricingPolicy {
            type_id: 35,
            pricing_policy: crate::MarketPricingPolicy::AcquireQuantityFromSellOrders,
        }],
        None,
    )
    .unwrap();
    let requests: Vec<_> = materials.into_iter().chain(output).collect();

    assert_eq!(
        requests
            .iter()
            .find(|request| request.type_id == 34)
            .unwrap()
            .pricing_policy,
        crate::MarketPricingPolicy::HighestBuy
    );
    assert_eq!(
        requests
            .iter()
            .find(|request| request.type_id == 35)
            .unwrap()
            .pricing_policy,
        crate::MarketPricingPolicy::AcquireQuantityFromSellOrders
    );
    assert_eq!(
        requests
            .iter()
            .find(|request| request.type_id == 5_876)
            .unwrap()
            .pricing_policy,
        crate::MarketPricingPolicy::LowestSell
    );
}

#[test]
fn market_price_requests_are_drawn_from_the_expansion_when_component_resolutions_are_present() {
    let build = build();
    // 90_001 is a sub-material only introduced by the expansion (not
    // one of the root recipe's own materials, 34/35) -- exactly the
    // "sub-component's own materials" case that regressed pricing for
    // order-book-backed sources when `market_price_requests` still
    // read from `build.recipe.materials()` instead of the expansion.
    let expansion = crate::ComponentExpansion {
        components: vec![
            crate::ResolvedComponent {
                type_id: 34,
                type_name: "Tritanium".to_string(),
                total_quantity: 3_000,
                contributions: vec![crate::Contribution {
                    source: crate::ContributionSource::Root,
                    quantity: 3_000,
                }],
                resolution: crate::ComponentResolutionOutcome::Buy,
            },
            crate::ResolvedComponent {
                type_id: 90_001,
                type_name: "Rifter Hull Section".to_string(),
                total_quantity: 6,
                contributions: vec![crate::Contribution {
                    source: crate::ContributionSource::Root,
                    quantity: 6,
                }],
                resolution: crate::ComponentResolutionOutcome::Build {
                    recipe: crate::RecipeSelection::Manufacturing {
                        blueprint_type_id: 90_002,
                    },
                    runs: 3,
                    produced_quantity: 6,
                    surplus: 0,
                    duration_seconds: None,
                },
            },
        ],
    };

    let (materials, output) = market_price_requests(
        &build,
        None,
        crate::MarketPricingPolicy::HighestBuy,
        crate::MarketPricingPolicy::LowestSell,
        &[],
        Some(&expansion),
    )
    .unwrap();
    let requests: Vec<_> = materials.into_iter().chain(output).collect();

    assert_eq!(
        requests
            .iter()
            .map(|request| request.type_id)
            .collect::<Vec<_>>(),
        vec![34, 90_001, 5_876],
    );
    assert_eq!(
        requests
            .iter()
            .find(|r| r.type_id == 90_001)
            .unwrap()
            .requested_quantity,
        6
    );
    // Pyerite (35) is the root recipe's own material but is absent
    // from the expansion's merged component set -- it must not be
    // requested, since the expanded worksheet no longer contains it.
    assert!(!requests.iter().any(|request| request.type_id == 35));
}
