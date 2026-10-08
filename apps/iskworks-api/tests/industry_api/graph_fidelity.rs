use super::*;

// ===================================================================
// Build Graph <-> linked-Build worksheet fidelity
// ===================================================================
//
// Invariant: for every resolved graph `ProductionNode` backed by
// `build_id = X`, the recipe/Build-derived facts it displays (Buy material
// quantities, effective ME/TE, material cost) must equal what opening
// Build X's own worksheet computes -- persisted runs, X's own blueprint
// ME, X's own facility. `Need` / `Making` deliberately keep different
// bases and are NOT asserted equal.

fn fidelity_app(
    parent: Build,
    owner_id: OwnerId,
    linked_builds: Vec<Build>,
    market_items: std::collections::BTreeMap<i64, iskworks_core::PriceSourceItem>,
    blueprint_observations: std::collections::HashMap<
        uuid::Uuid,
        iskworks_core::BlueprintObservation,
    >,
) -> axum::Router {
    build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            linked_builds: std::sync::Mutex::new(linked_builds),
            price_source: None,
            market_items,
            facility_profiles: std::collections::HashMap::new(),
            blueprint_observations,
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        .with_inventory_repository(Arc::new(EmptyInventoryRepository))
        .with_production_repository(Arc::new(FixtureProductionRepository {
            type_id: 0,
            available_to_this_build: 0,
            average_historical_unit_cost: None,
        })),
    )
}

// 1 + 2 + 3 + 4 + 5 + 6 + 7: the motivating regression, all in one fixture.
// Root runs 201 -> Need 402 Hull Sections; the linked Hull Section build
// persists runs 450 with a Copy BPC at ME 10 / TE 20 and two BUY materials
// (Pyerite x50/run, Fidelity Isotope x1/run).
#[tokio::test]
async fn graph_linked_manufacturing_node_matches_its_own_worksheet_exactly() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = with_saved_resolution(
        rifter_parent(workspace_id, owner_id),
        90_001,
        iskworks_core::RecipeSelection::Manufacturing {
            blueprint_type_id: 90_010,
        },
    );
    let parent_id = parent.id;
    let hull_materials = vec![
        iskworks_core::CapturedRecipeLine {
            type_id: 35,
            type_name: "Pyerite".to_string(),
            quantity_per_run: 50,
            sort_order: 0,
        },
        iskworks_core::CapturedRecipeLine {
            type_id: 60_001,
            type_name: "Fidelity Isotope".to_string(),
            quantity_per_run: 1,
            sort_order: 1,
        },
    ];
    let linked = hull_section_linked_with_draft(
        workspace_id,
        owner_id,
        parent_id,
        450,
        90_010,
        hull_materials,
        Some(manual_bpc(10, 20)),
        Vec::new(),
    );

    let graph = post_graph(
        fidelity_app(
            parent,
            owner_id,
            vec![linked],
            chain_prices(),
            std::collections::HashMap::new(),
        ),
        parent_id,
        overlay(
            201,
            serde_json::json!([
                {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_010}}
            ]),
        ),
    )
    .await
    .1;

    let child = prod_child(&graph["root"]);
    assert_eq!(child["nodeKind"], "production");

    // `runs`/`producingQuantity` are the PROJECTED sizing (need
    // 402, qpr 1 -> exactly 402 runs, no surplus); the child's persisted
    // 450 runs surfaces separately as `persistedRuns`, purely informational
    // (see the `RunsDiverged` warning), never the quantity/cost authority.
    assert_eq!(child["requiredQuantity"], 402, "Need = parent demand");
    assert_eq!(
        child["producingQuantity"], 402,
        "Making = projected runs * qpr"
    );
    assert_eq!(child["surplus"], 0);
    assert_eq!(child["runs"], 402, "projected runs, not the persisted 450");
    assert_eq!(child["persistedRuns"], 450, "persisted linked-build runs");

    // 5. concrete effective ME/TE on the DTO.
    assert_eq!(child["effectiveMe"], 10);
    assert_eq!(child["effectiveTe"], 20);

    // The linked Hull Section build's own worksheet, same overlay a client
    // reconstructs when opening it.
    let worksheet = post_preview(
        fidelity_app(
            rifter_parent(workspace_id, owner_id),
            owner_id,
            Vec::new(),
            chain_prices(),
            std::collections::HashMap::new(),
        ),
        serde_json::json!({
            "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_010},
            "runs": 402,
            "pricingSelections": [],
            "blueprintSelection": {
                "mode": "manual", "kind": "copy",
                "materialEfficiency": 10, "timeEfficiency": 20,
                "licensedRuns": 100000, "notes": ""
            },
        }),
    )
    .await;

    // 1-4 + 7: every Buy quantity + the material cost agree with a worksheet
    // of the same child previewed at the graph's own PROJECTED runs (402,
    // not its persisted 450) -- the "child run parity" a caller relies on
    // when driving Worksheet from a Graph node's own `runs`.
    let ws_pyerite = line_qty(&worksheet, "materialLines", 35);
    let ws_isotope = line_qty(&worksheet, "materialLines", 60_001);
    assert_eq!(ws_pyerite, 18_090, "50/run * 402 runs * ME10");
    assert_eq!(ws_isotope, 402, "1/run floored at runs");

    let g_pyerite = line_qty(child, "buyMaterials", 35);
    let g_isotope = line_qty(child, "buyMaterials", 60_001);
    assert_eq!(g_pyerite, ws_pyerite, "graph Pyerite == worksheet Pyerite");
    assert_eq!(g_isotope, ws_isotope, "graph Isotope == worksheet Isotope");

    // A Build-resolved component would be a Production child, never an
    // acquisition node -- here the linked build Build-resolves nothing, so
    // both requirements are acquisition children.
    let mut buy_type_ids: Vec<i64> = child["children"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["nodeKind"] == "acquisition")
        .map(|m| m["typeId"].as_i64().unwrap())
        .collect();
    buy_type_ids.sort_unstable();
    assert_eq!(buy_type_ids, vec![35, 60_001]);

    // `estimatedCost` is total PRODUCTION cost (material +
    // installation, per `OperationCostProjection.total_production_cost`) --
    // this child has no facility, so installation is genuinely unknown and
    // the node is `costState: incomplete` with a null `estimatedCost`
    // (never zero-substituted). Its `materialComponentCost` -- exposed
    // separately, never bundled with a guessed installation figure -- still
    // matches the worksheet's material cost exactly.
    assert_eq!(child["costState"], "incomplete", "{child:?}");
    assert!(child["estimatedCost"].is_null());
    assert_eq!(
        child["materialComponentCost"], worksheet["estimatedMaterialCost"],
        "graph material component cost == worksheet MATERIAL COST"
    );
}

// 8. nested: root -> Hull Section (BPC ME 10) -> Pyerite (BPO ME 5). The
// deep node's Buy quantity + ME/TE must match ITS own worksheet too.
#[tokio::test]
async fn graph_nested_linked_node_matches_its_own_worksheet() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = rifter_parent(workspace_id, owner_id);
    let parent_id = parent.id;

    let hull = hull_section_linked_with_draft(
        workspace_id,
        owner_id,
        parent_id,
        20,
        90_002,
        pyerite_50(),
        Some(manual_bpc(10, 20)),
        vec![iskworks_core::ComponentResolution {
            type_id: 35,
            recipe: iskworks_core::RecipeSelection::Manufacturing {
                blueprint_type_id: 91_002,
            },
            facility_override: None,
            blueprint_selection: None,
        }],
    );
    let hull_id = hull.id;
    let mut pyerite = hull_section_linked_with_draft(
        workspace_id,
        owner_id,
        hull_id,
        900,
        91_002,
        vec![iskworks_core::CapturedRecipeLine {
            type_id: 34,
            type_name: "Tritanium".to_string(),
            quantity_per_run: 10,
            sort_order: 0,
        }],
        Some(iskworks_core::BlueprintSelection::Manual {
            kind: iskworks_core::BlueprintKind::Original,
            material_efficiency: 5,
            time_efficiency: 0,
            licensed_runs: None,
            notes: String::new(),
        }),
        Vec::new(),
    );
    // fix up the deep build's product / parent slot (helper hardcodes 90_001)
    if let iskworks_core::BuildRecipe::Manufacturing(recipe) = &mut pyerite.recipe {
        recipe.products[0].type_id = 35;
        recipe.products[0].type_name = "Pyerite".to_string();
        recipe.blueprint_name = "Pyerite Reprocessing Blueprint".to_string();
    }
    fixture_relink(&pyerite, 35);

    let graph = post_graph(
        fidelity_app(
            parent,
            owner_id,
            vec![hull, pyerite],
            chain_prices(),
            std::collections::HashMap::new(),
        ),
        parent_id,
        overlay(
            10,
            serde_json::json!([
                {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
            ]),
        ),
    )
    .await
    .1;

    let hull_node = prod_child(&graph["root"]);
    assert_eq!(hull_node["effectiveMe"], 10);
    let deep = prod_child(hull_node);
    assert_eq!(deep["nodeKind"], "production");
    assert_eq!(deep["typeId"], 35);
    assert_eq!(deep["effectiveMe"], 5);
    assert_eq!(deep["effectiveTe"], 0);
    assert_eq!(deep["runs"], 900);

    let worksheet = post_preview(
        fidelity_app(
            rifter_parent(workspace_id, owner_id),
            owner_id,
            Vec::new(),
            chain_prices(),
            std::collections::HashMap::new(),
        ),
        serde_json::json!({
            "recipe": {"mode": "manufacturing", "blueprintTypeId": 91_002},
            "runs": 900,
            "pricingSelections": [],
            "blueprintSelection": {
                "mode": "manual", "kind": "original",
                "materialEfficiency": 5, "timeEfficiency": 0,
                "licensedRuns": null, "notes": ""
            },
        }),
    )
    .await;

    let ws_trit = line_qty(&worksheet, "materialLines", 34);
    let g_trit = line_qty(deep, "buyMaterials", 34);
    assert_eq!(g_trit, ws_trit, "deep node Tritanium == its own worksheet");
    // No facility on this node -> installation unknown -> `estimatedCost`
    // (total production cost) is genuinely incomplete; the materials-only
    // figure still matches the worksheet exactly.
    assert_eq!(deep["costState"], "incomplete", "{deep:?}");
    assert!(deep["estimatedCost"].is_null());
    assert_eq!(
        deep["materialComponentCost"],
        worksheet["estimatedMaterialCost"]
    );
}

// 9. reaction linked node: no blueprint ME/TE, but recipe/quantity/cost
// facts still match its own worksheet.
#[tokio::test]
async fn graph_linked_reaction_node_matches_its_own_worksheet_and_has_no_me_te() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = with_saved_resolution(
        rifter_parent(workspace_id, owner_id),
        90_001,
        iskworks_core::RecipeSelection::Reaction {
            reaction_formula_type_id: 90_003,
        },
    );
    let parent_id = parent.id;

    let now = chrono::Utc::now();
    let reaction_hull = fixture_link(
        Build {
            id: BuildId::new(),
            workspace_id,
            owner_id,
            name: "Hull Section (reaction)".to_string(),
            recipe: iskworks_core::BuildRecipe::Reaction(iskworks_core::CapturedReactionFormula {
                source_sde_dataset_id: uuid::Uuid::new_v4(),
                source_sde_version: "test".to_string(),
                reaction_formula_type_id: 90_003,
                reaction_formula_name: "Rifter Hull Section Reaction Formula".to_string(),
                duration_seconds_per_run: Some(50),
                materials: vec![iskworks_core::CapturedRecipeLine {
                    type_id: 35,
                    type_name: "Pyerite".to_string(),
                    quantity_per_run: 50,
                    sort_order: 0,
                }],
                products: vec![iskworks_core::CapturedRecipeLine {
                    type_id: 90_001,
                    type_name: "Rifter Hull Section".to_string(),
                    quantity_per_run: 1,
                    sort_order: 0,
                }],
                fingerprint: "recipe".to_string(),
            }),
            runs: 30,
            notes: String::new(),
            revision: 1,
            created_at: now,
            updated_at: now,
            draft_planning: Some(iskworks_core::DraftPlanningSnapshot {
                input: iskworks_core::DraftPlanningInput {
                    material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
                    output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
                    manual_price_list_id: None,
                    expected_manual_price_list_revision: None,
                    material_pricing_policy: iskworks_core::MarketPricingPolicy::HighestBuy,
                    output_pricing_policy: iskworks_core::MarketPricingPolicy::LowestSell,
                    pricing_selections: Vec::new(),
                    blueprint_selection: None,
                    manufacturing_facility: None,
                    reaction_facility: None,
                    facility_eiv_manual: false,
                    component_resolutions: Vec::new(),
                    fulfillment_scopes: Vec::new(),
                },
                updated_at: now,
            }),
            recipe_currency: iskworks_core::RecipeCurrency::Current,
            active_sde_version: Some("test".to_string()),
            product_category_name: None,
            product_group_name: None,
            selected_blueprint_origin: None,
            has_owned_blueprint: false,
        },
        parent_id,
        90_001,
    );

    let graph = post_graph(
        fidelity_app(
            parent,
            owner_id,
            vec![reaction_hull],
            chain_prices(),
            std::collections::HashMap::new(),
        ),
        parent_id,
        overlay(
            10,
            serde_json::json!([
                {"typeId": 90_001, "recipe": {"mode": "reaction", "reactionFormulaTypeId": 90_003}}
            ]),
        ),
    )
    .await
    .1;

    let child = prod_child(&graph["root"]);
    assert_eq!(child["kind"], "reaction");
    assert_eq!(child["effectiveMe"], serde_json::Value::Null);
    assert_eq!(child["effectiveTe"], serde_json::Value::Null);
    // Root need is 2/run * 10 = 20 Hull Sections; `runs` is the
    // PROJECTED sizing to meet that need, not the persisted 30.
    assert_eq!(child["runs"], 20);
    assert_eq!(child["persistedRuns"], 30);

    let worksheet = post_preview(
        fidelity_app(
            rifter_parent(workspace_id, owner_id),
            owner_id,
            Vec::new(),
            chain_prices(),
            std::collections::HashMap::new(),
        ),
        serde_json::json!({
            "recipe": {"mode": "reaction", "reactionFormulaTypeId": 90_003},
            "runs": 20,
            "pricingSelections": [],
        }),
    )
    .await;

    let g_pyerite = line_qty(child, "buyMaterials", 35);
    let ws_pyerite = line_qty(&worksheet, "materialLines", 35);
    assert_eq!(g_pyerite, ws_pyerite);
    // No facility on this node -> installation unknown -> `estimatedCost`
    // is incomplete; the materials-only figure still matches the worksheet.
    assert!(child["estimatedCost"].is_null());
    assert_eq!(
        child["materialComponentCost"],
        worksheet["estimatedMaterialCost"]
    );
}

// 5 (ObservedAsset): effective ME/TE are the durable values captured onto
// the selection when the observed BPC was picked -- the graph reads them
// directly, with no live observation dependency at preview time. The
// fixture's observation map is still wired up (and matches the frozen
// values) to prove a *resolvable* observation doesn't change the outcome;
// see the sibling test below for the disappeared-blueprint case.
#[tokio::test]
async fn graph_linked_node_effective_me_te_resolve_an_observed_asset_selection() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = rifter_parent(workspace_id, owner_id);
    let parent_id = parent.id;

    let observation_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();
    let observation = iskworks_core::BlueprintObservation {
        id: observation_id,
        workspace_id,
        owner_id,
        owner_name: "Pilot".to_string(),
        eve_item_id: 42,
        blueprint_type_id: 90_002,
        blueprint_name: "Rifter Hull Section Blueprint".to_string(),
        kind: iskworks_core::BlueprintKind::Copy,
        material_efficiency: 10,
        time_efficiency: 20,
        licensed_runs: Some(300),
        location_id: 60_003_760,
        location_flag: "Hangar".to_string(),
        location_name: Some("Jita IV - Moon 4".to_string()),
        observed_at: now,
        imported_at: now,
    };

    let linked = hull_section_linked_with_draft(
        workspace_id,
        owner_id,
        parent_id,
        120,
        90_002,
        pyerite_50(),
        Some(iskworks_core::BlueprintSelection::ObservedAsset {
            observation_id,
            kind: iskworks_core::BlueprintKind::Copy,
            material_efficiency: 10,
            time_efficiency: 20,
            licensed_runs: None,
        }),
        Vec::new(),
    );

    let graph = post_graph(
        fidelity_app(
            parent,
            owner_id,
            vec![linked],
            chain_prices(),
            std::collections::HashMap::from([(observation_id, observation)]),
        ),
        parent_id,
        overlay(
            10,
            serde_json::json!([
                {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
            ]),
        ),
    )
    .await
    .1;

    let child = prod_child(&graph["root"]);
    assert_eq!(child["effectiveMe"], 10, "from the owned BPC observation");
    assert_eq!(child["effectiveTe"], 20);
    // Root need is 2/run * 10 = 20 Hull Sections; the plan sizes the child
    // to the PROJECTED 20 runs (not its persisted 120), so Pyerite reflects
    // that ME10 at 20 runs.
    assert_eq!(child["runs"], 20);
    assert_eq!(child["persistedRuns"], 120);
    assert_eq!(line_qty(child, "buyMaterials", 35), 50 * 20 * 9 / 10);
}

// 5b (ObservedAsset, blueprint since sold -- the central Muninn-x4
// regression): once an `ObservedAsset` selection has captured its effective
// kind/ME/TE, the physical blueprint asset disappearing entirely (sold,
// moved, consumed, or just temporarily desynced from ESI) must NOT change
// projected quantities/cost, and must NOT fail the graph. This is the exact
// failure class that broke a real Muninn build at runs=4: a linked child's
// re-preview called `get_blueprint_observation` on every projection, so a
// no-longer-resolvable observation (or a licensed-run count insufficient
// for the walker's dynamically-resized run count) hard-failed the whole
// tree. Effective config is now frozen on the selection at capture time;
// the observation is provenance only, never a planning dependency.
#[tokio::test]
async fn graph_tolerates_an_observed_asset_selection_whose_blueprint_is_gone() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = rifter_parent(workspace_id, owner_id);
    let parent_id = parent.id;

    // Captured selection: kind/ME/TE are frozen already. The observation id
    // it points at is one the fixture repo does not know (empty observation
    // map) -> `get_blueprint_observation` would yield `ObservationNotFound`
    // if anything still called it for quantities.
    let missing_observation_id = uuid::Uuid::new_v4();
    let linked = hull_section_linked_with_draft(
        workspace_id,
        owner_id,
        parent_id,
        120,
        90_002,
        pyerite_50(),
        Some(iskworks_core::BlueprintSelection::ObservedAsset {
            observation_id: missing_observation_id,
            kind: iskworks_core::BlueprintKind::Copy,
            material_efficiency: 10,
            time_efficiency: 20,
            licensed_runs: None,
        }),
        Vec::new(),
    );

    let (status, graph) = post_graph(
        fidelity_app(
            parent,
            owner_id,
            vec![linked],
            chain_prices(),
            std::collections::HashMap::new(),
        ),
        parent_id,
        overlay(
            10,
            serde_json::json!([
                {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
            ]),
        ),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a sold/desynced blueprint must not fail the graph"
    );
    let child = prod_child(&graph["root"]);
    assert_eq!(
        child["effectiveMe"], 10,
        "frozen ME survives the blueprint disappearing -- no live re-resolution"
    );
    assert_eq!(child["effectiveTe"], 20, "frozen TE survives too");
    // Root need is 2/run * 10 = 20 Hull Sections; the child's PROJECTED
    // runs (not its persisted 120) is what sizes Pyerite here.
    assert_eq!(child["runs"], 20);
    assert_eq!(child["persistedRuns"], 120);
    // ME10-derived quantity (50 * runs * 9/10), matching test 5 exactly --
    // proves the frozen ME still actually drives materials, not just the
    // reported `effectiveMe` field.
    assert_eq!(line_qty(child, "buyMaterials", 35), 50 * 20 * 9 / 10);
}

// Multi-BPC job split on a producer: the Hull Section producer's captured
// copy licenses 1 run, so its pooled 20 runs are 20 jobs, each rounded on
// its own -- 20 x ceil(50 x 0.91) = 920 Pyerite, not ceil(50 x 20 x 0.91) =
// 910. The licensed runs are frozen on the selection: the observation is
// gone and the split still applies.
#[tokio::test]
async fn graph_plans_a_producer_per_captured_bpc_job() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = rifter_parent(workspace_id, owner_id);
    let parent_id = parent.id;
    let linked = hull_section_linked_with_draft(
        workspace_id,
        owner_id,
        parent_id,
        120,
        90_002,
        pyerite_50(),
        Some(iskworks_core::BlueprintSelection::ObservedAsset {
            observation_id: uuid::Uuid::new_v4(),
            kind: iskworks_core::BlueprintKind::Copy,
            material_efficiency: 9,
            time_efficiency: 0,
            licensed_runs: Some(1),
        }),
        Vec::new(),
    );

    let (status, graph) = post_graph(
        fidelity_app(
            parent,
            owner_id,
            vec![linked],
            chain_prices(),
            std::collections::HashMap::new(),
        ),
        parent_id,
        overlay(
            10,
            serde_json::json!([
                {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
            ]),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let child = prod_child(&graph["root"]);
    assert_eq!(child["runs"], 20);
    assert_eq!(line_qty(child, "buyMaterials", 35), 20 * 46);
}

// 5c (ObservedAsset, never captured -- the legacy/unmigrated shape,
// exercised directly against the real endpoint the Muninn bug report
// failed on): a root selection that still carries the pre-migration
// sentinel (`kind: unknown`, i.e. only `observationId` was ever persisted)
// is the one case that's still allowed to fail when its observation can't
// be resolved -- there is no frozen config to fall back on, and fabricating
// ME0/TE0 as if it were real would be worse than a clear error. This is a
// known, temporary gap closed by backfilling existing rows (see
// `IndustryService::backfill_observed_blueprint_configurations`), not by
// the preview path itself.
#[tokio::test]
async fn candidate_preview_errors_on_an_unmigrated_observed_asset_selection_whose_blueprint_is_gone(
) {
    let price_source = PriceSource {
        id: PriceSourceId::new(),
        workspace_id: iskworks_core::WorkspaceId::new(),
        name: "Home Market".to_string(),
        description: String::new(),
        kind: PriceSourceKind::Manual,
        revision: 1,
        item_count: 0,
        recent_build_count: 0,
        items: Vec::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let missing_observation_id = uuid::Uuid::new_v4();

    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "pricingSelections": [],
        "blueprintSelection": {
            "mode": "observedAsset",
            "observationId": missing_observation_id,
            "kind": "unknown",
            "materialEfficiency": 0,
            "timeEfficiency": 0,
        },
    })
    .to_string();

    let response = app_with_price_source_and_sde(price_source)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/build-plans/candidate-preview")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "an unmigrated selection with no resolvable observation has no frozen \
         config to fall back on -- it must fail honestly, not fabricate ME0/TE0"
    );
    let json: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(json["error"]["code"], "blueprint_observation_not_found");
}

// 10. a settings change in the overlay (linked-build state edited after
// creation) is reflected on the next graph -- the enrichment reads the
// live per-node preview, never a stale snapshot. Here the linked build's
// persisted ME is 10; re-post the graph with the linked build carrying
// ME 0 and the Buy quantities + `effectiveMe` follow.
#[tokio::test]
async fn graph_refresh_reflects_a_linked_build_settings_change() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();

    let at_me = |me: u8| {
        let parent = rifter_parent(workspace_id, owner_id);
        let parent_id = parent.id;
        let linked = hull_section_linked_with_draft(
            workspace_id,
            owner_id,
            parent_id,
            120,
            90_002,
            pyerite_50(),
            Some(manual_bpc(me, 0)),
            Vec::new(),
        );
        (parent, parent_id, linked)
    };

    let (parent, parent_id, linked) = at_me(10);
    let g10 = post_graph(
        fidelity_app(
            parent,
            owner_id,
            vec![linked],
            chain_prices(),
            std::collections::HashMap::new(),
        ),
        parent_id,
        overlay(
            10,
            serde_json::json!([
                {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
            ]),
        ),
    )
    .await
    .1;
    let c10 = prod_child(&g10["root"]);
    assert_eq!(c10["effectiveMe"], 10);
    // Root need 2/run * 10 = 20 -> child's PROJECTED runs is 20, not its
    // persisted 120.
    assert_eq!(c10["runs"], 20);
    assert_eq!(line_qty(c10, "buyMaterials", 35), 50 * 20 * 9 / 10); // 900

    let (parent, parent_id, linked) = at_me(0);
    let g0 = post_graph(
        fidelity_app(
            parent,
            owner_id,
            vec![linked],
            chain_prices(),
            std::collections::HashMap::new(),
        ),
        parent_id,
        overlay(
            10,
            serde_json::json!([
                {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
            ]),
        ),
    )
    .await
    .1;
    let c0 = prod_child(&g0["root"]);
    assert_eq!(c0["effectiveMe"], 0);
    assert_eq!(c0["runs"], 20);
    assert_eq!(line_qty(c0, "buyMaterials", 35), 50 * 20); // 1000, no ME
}
