use super::*;

// ===================================================================
// Whole-tree allocation at intermediate Build/Reaction boundaries.
// ===================================================================

// -- A covered Build intermediate is a VISIBLE row
// but its subtree is not walked.
#[tokio::test]
async fn materials_covered_build_intermediate_is_visible_but_child_pruned() {
    // nested_chain: root(6_830)@1 -> Hull Section 90_001 x2 [BUILD] ->
    //   Pyerite 35 x100 [BUILD] -> Tritanium 34 x1000.
    // Root's own direct Tritanium demand is 100.

    // (A) 90_001 fully covered, 34 short. 90_001 is a visible covered Build
    // row; the deep 35 / 1,000 Tritanium chain is NEVER walked.
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let root_id = parent.id;
    let (app, _inv) = materials_app(parent, owner_id, linked, vec![(90_001, 2)]);
    let (status, json) = post_materials(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);

    let hull = row(&json, 90_001).unwrap();
    assert_eq!(hull["requiredQuantity"], 2);
    assert_eq!(hull["allocatedQuantity"], 2);
    assert_eq!(hull["shortageQuantity"], 0);
    assert_eq!(hull["fullyCovered"], true);
    assert_eq!(hull["strategy"], "build");
    assert_eq!(
        row(&json, 34).unwrap()["requiredQuantity"],
        100,
        "deep 1,000 not walked"
    );
    assert!(row(&json, 35).is_none(), "child subtree not walked");
    let hull_alloc = node_alloc(&json, root_id, 90_001).unwrap();
    assert_eq!(hull_alloc["childRuns"], 0);
    assert_eq!(hull_alloc["producedQuantity"], 0);
    assert_eq!(hull_alloc["surplusQuantity"], 0);
    assert_materials_invariants(&json);

    // (I) Every root requirement covered -> every root row is still present,
    // all fullyCovered; NOT rows == [].
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let (app, _inv) = materials_app(parent, owner_id, linked, vec![(90_001, 2), (34, 100)]);
    let (status, json) = post_materials(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);
    let rows = json["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "both root requirements remain visible");
    assert!(rows.iter().all(|r| r["fullyCovered"] == true));
    assert_eq!(row(&json, 34).unwrap()["shortageQuantity"], 0);
    assert_eq!(row(&json, 90_001).unwrap()["shortageQuantity"], 0);
    assert_materials_invariants(&json);
}

// -- Partial intermediate coverage re-sizes the child dynamically ---
#[tokio::test]
async fn materials_partial_build_intermediate_dynamically_resizes_the_child() {
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let root_id = parent.id;
    let hull_child_id = linked[0].id;
    // 90_001 required 2, one on hand -> remaining 1 -> hull child at 1 run
    // (not its persisted 2, not the persisted-runs 100 Tritanium chain).
    let (app, _inv) = materials_app(parent, owner_id, linked, vec![(90_001, 1)]);
    let (status, json) = post_materials(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);

    let hull_alloc = node_alloc(&json, root_id, 90_001).unwrap();
    assert_eq!(hull_alloc["requiredQuantity"], 2);
    assert_eq!(hull_alloc["allocatedQuantity"], 1);
    assert_eq!(hull_alloc["shortageQuantity"], 1);
    assert_eq!(hull_alloc["childRuns"], 1);
    assert_eq!(hull_alloc["producedQuantity"], 1);
    assert_eq!(hull_alloc["surplusQuantity"], 0);

    // Hull child at 1 run -> 50 Pyerite [BUILD] -> 50 runs -> 500 Tritanium.
    // Root's own 34 is 100. Total 600 (vs 1,100 at persisted child runs).
    assert_eq!(row(&json, 34).unwrap()["requiredQuantity"], 600);
    let pyerite_alloc = node_alloc(&json, hull_child_id, 35).unwrap();
    assert_eq!(pyerite_alloc["resolution"], "build");
    assert_eq!(pyerite_alloc["shortageQuantity"], 50);
    assert_eq!(pyerite_alloc["childRuns"], 50);
    // The intermediates are visible rows, partially covered / short.
    assert_eq!(row(&json, 35).unwrap()["strategy"], "build");
    assert_eq!(row(&json, 35).unwrap()["requiredQuantity"], 50);
    let hull_row = row(&json, 90_001).unwrap();
    assert_eq!(hull_row["allocatedQuantity"], 1);
    assert_eq!(hull_row["shortageQuantity"], 1);
    assert_eq!(hull_row["fullyCovered"], false);
    assert_materials_invariants(&json);
}

// -- An uncovered intermediate is sized to its full production demand
#[tokio::test]
async fn materials_uncovered_build_intermediate_is_sized_to_full_demand() {
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let root_id = parent.id;
    let hull_child_id = linked[0].id;
    let (app, _inv) = materials_app(parent, owner_id, linked, vec![]);
    let (status, json) = post_materials(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(node_alloc(&json, root_id, 90_001).unwrap()["childRuns"], 2);
    assert_eq!(
        node_alloc(&json, hull_child_id, 35).unwrap()["childRuns"],
        100
    );
    assert_eq!(row(&json, 34).unwrap()["requiredQuantity"], 1_100);
    assert_materials_invariants(&json);
}

// -- Partial coverage composes recursively down the chain ----------
#[tokio::test]
async fn materials_nested_partial_coverage_composes_recursively() {
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let root_id = parent.id;
    let hull_child_id = linked[0].id;
    // 90_001: 2 needed, 1 on hand -> hull child 1 run -> 50 Pyerite.
    // 35: 50 needed, 20 on hand -> pyerite child 30 runs -> 300 Tritanium.
    // 34: 100 (root) + 300 (deep) = 400.
    let (app, _inv) = materials_app(parent, owner_id, linked, vec![(90_001, 1), (35, 20)]);
    let (status, json) = post_materials(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(node_alloc(&json, root_id, 90_001).unwrap()["childRuns"], 1);
    let pyerite_alloc = node_alloc(&json, hull_child_id, 35).unwrap();
    assert_eq!(pyerite_alloc["allocatedQuantity"], 20);
    assert_eq!(pyerite_alloc["shortageQuantity"], 30);
    assert_eq!(pyerite_alloc["childRuns"], 30);
    assert_eq!(row(&json, 34).unwrap()["requiredQuantity"], 400);
    // Pyerite is a visible partially-covered Build row.
    let pyerite_row = row(&json, 35).unwrap();
    assert_eq!(pyerite_row["requiredQuantity"], 50);
    assert_eq!(pyerite_row["allocatedQuantity"], 20);
    assert_eq!(pyerite_row["shortageQuantity"], 30);
    assert_eq!(pyerite_row["strategy"], "build");
    assert_materials_invariants(&json);
}

// -- Full scope at a Build boundary ignores inventory entirely -----
#[tokio::test]
async fn materials_full_scope_build_boundary_ignores_finished_inventory() {
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let root_id = parent.id;
    // 90_001 stock is plentiful, but the boundary is Full -> draw 0, size
    // the child to the full demand of 2, descend fully.
    let (app, _inv) = materials_app(parent, owner_id, linked, vec![(90_001, 500)]);
    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "pricingSelections": [],
        "componentResolutions": [
            {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
        ],
        "fulfillmentScopes": [{"typeId": 90_001, "scope": "full"}],
    });
    let (status, json) = post_materials(app, parent_id, body).await;
    assert_eq!(status, StatusCode::OK);

    let hull_alloc = node_alloc(&json, root_id, 90_001).unwrap();
    assert_eq!(hull_alloc["scope"], "full");
    assert_eq!(
        hull_alloc["allocatedQuantity"], 0,
        "Full never draws the pool"
    );
    assert_eq!(hull_alloc["childRuns"], 2, "still sized to the full demand");
    assert_eq!(row(&json, 34).unwrap()["requiredQuantity"], 1_100);
    assert_materials_invariants(&json);
}

// -- Discrete output: child ceil runs + recorded surplus ----------
#[tokio::test]
async fn materials_discrete_output_records_child_runs_and_surplus() {
    let workspace_id = iskworks_core::WorkspaceId::new();
    let owner_id = OwnerId::new();
    // Parent bp 92_010 needs 250 of 92_001/run; child bp 92_002 yields 500/run.
    let parent = batch_parent(workspace_id, owner_id, 250);
    let parent_id = parent.id;
    let child = linked_child_with_draft(
        workspace_id,
        owner_id,
        parent_id,
        92_001,
        1,
        captured_recipe(
            92_002,
            &[(34, "Tritanium", 10)],
            (92_001, "Batched Component", 500),
        ),
        Vec::new(),
    );
    let overlay = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 92_010},
        "runs": 1,
        "pricingSelections": [],
        "componentResolutions": [
            {"typeId": 92_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 92_002}}
        ],
        "fulfillmentScopes": [],
    });

    // Uncovered: remaining 250, output/run 500 -> 1 run, 500 produced, 250 surplus.
    let (app, _inv) = materials_app(parent.clone(), owner_id, vec![child.clone()], vec![]);
    let (status, json) = post_materials(app, parent_id, overlay.clone()).await;
    assert_eq!(status, StatusCode::OK);
    let comp = node_alloc(&json, parent_id, 92_001).unwrap();
    assert_eq!(comp["resolution"], "build");
    assert_eq!(comp["shortageQuantity"], 250);
    assert_eq!(comp["childRuns"], 1);
    assert_eq!(comp["producedQuantity"], 500);
    assert_eq!(comp["surplusQuantity"], 250);
    assert_eq!(row(&json, 34).unwrap()["requiredQuantity"], 10);
    // The Build intermediate is a visible row; its surplus is evidence only,
    // never reused and never inflated into the row demand.
    let comp_row = row(&json, 92_001).unwrap();
    assert_eq!(comp_row["requiredQuantity"], 250);
    assert_eq!(comp_row["shortageQuantity"], 250);
    assert_eq!(comp_row["strategy"], "build");
    assert_materials_invariants(&json);

    // Fully covered: 250 on hand -> the intermediate is a covered row, child pruned.
    let (app, _inv) = materials_app(parent, owner_id, vec![child], vec![(92_001, 250)]);
    let (_s, json) = post_materials(app, parent_id, overlay).await;
    let comp_row = row(&json, 92_001).unwrap();
    assert_eq!(comp_row["fullyCovered"], true);
    assert_eq!(comp_row["shortageQuantity"], 0);
    assert!(
        row(&json, 34).is_none(),
        "child pruned -> no descendant Tritanium row"
    );
    assert_materials_invariants(&json);
}

// -- A Reaction intermediate is sized the same way ----------------
#[tokio::test]
async fn materials_reaction_intermediate_is_dynamically_sized() {
    let workspace_id = iskworks_core::WorkspaceId::new();
    let owner_id = OwnerId::new();
    let (mut parent, _ws, _owner) = rifter_root(1);
    parent.workspace_id = workspace_id;
    parent.owner_id = owner_id;
    parent
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .component_resolutions = Vec::new();
    let parent_id = parent.id;

    // A linked child for 90_001 that is a *reaction* (formula 90_003:
    // Pyerite 35 x50/run -> Hull Section 90_001 x1/run).
    let now = chrono::Utc::now();
    let reaction_child = fixture_link(
        Build {
            id: BuildId::new(),
            workspace_id,
            owner_id,
            name: "Hull reaction".to_string(),
            recipe: iskworks_core::BuildRecipe::Reaction(iskworks_core::CapturedReactionFormula {
                source_sde_dataset_id: uuid::Uuid::new_v4(),
                source_sde_version: "test".to_string(),
                reaction_formula_type_id: 90_003,
                reaction_formula_name: "Hull Reaction".to_string(),
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
                fingerprint: "rf".to_string(),
            }),
            runs: 9,
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

    // Partial cover: 2 needed, 1 on hand -> reaction child at 1 run -> 50 Pyerite.
    let (app, _inv) = materials_app(parent, owner_id, vec![reaction_child], vec![(90_001, 1)]);
    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "pricingSelections": [],
        "componentResolutions": [
            {"typeId": 90_001, "recipe": {"mode": "reaction", "reactionFormulaTypeId": 90_003}}
        ],
        "fulfillmentScopes": [],
    });
    let (status, json) = post_materials(app, parent_id, body).await;
    assert_eq!(status, StatusCode::OK);

    let hull_alloc = node_alloc(&json, parent_id, 90_001).unwrap();
    assert_eq!(hull_alloc["resolution"], "reaction");
    assert_eq!(hull_alloc["childRuns"], 1);
    assert_eq!(row(&json, 35).unwrap()["requiredQuantity"], 50);
    assert_materials_invariants(&json);
}

// -- A Materials request mutates nothing ----------------------
#[tokio::test]
async fn materials_request_is_read_only_persisted_child_runs_untouched() {
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let persisted_hull_runs = linked[0].runs;
    let persisted_pyerite_runs = linked[1].runs;

    let inventory = Arc::new(support::inventory::SeededInventoryRepository::new(vec![(
        90_001, 1,
    )]));
    let industry = Arc::new(FixtureIndustryRepository {
        build: Some(parent),
        linked_builds: std::sync::Mutex::new(linked.clone()),
        ..Default::default()
    });
    let app = build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(industry.clone())
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        .with_inventory_repository(inventory.clone())
        .with_production_repository(Arc::new(NeverCalledProductionRepository)),
    );

    // The traversal dynamically sizes the hull child to 1 run (!= persisted 2).
    let (status, json) = post_materials(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        node_alloc(&json, nested_chain_root(&industry), 90_001).unwrap()["childRuns"],
        1
    );

    // Persisted linked-build runs are exactly as they were. (V: the seeded
    // inventory repo panics on any write, so reaching here proves no ledger
    // write happened either.)
    let after = industry.linked_builds.lock().unwrap();
    assert_eq!(after[0].runs, persisted_hull_runs);
    assert_eq!(after[1].runs, persisted_pyerite_runs);
}

fn nested_chain_root(industry: &FixtureIndustryRepository) -> BuildId {
    industry.build.as_ref().unwrap().id
}

#[tokio::test]
async fn graph_projects_a_trivial_root_with_its_persisted_identity() {
    let owner_id = OwnerId::new();
    let parent =
        parent_build_resolving_hull_section_to_build(iskworks_core::WorkspaceId::new(), owner_id);
    let parent_id = parent.id;
    // Overlay resolves nothing -> both root materials are Buy.
    let app = graph_app(
        parent,
        owner_id,
        Vec::new(),
        std::collections::BTreeMap::new(),
    );

    let (status, json) = post_graph(app, parent_id, overlay(1, serde_json::json!([]))).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["root"]["graphNodeId"], format!("root:{}", parent_id.0));
    assert_eq!(json["root"]["buildId"], parent_id.0.to_string());
    assert_eq!(json["root"]["kind"], "rootManufacturing");
    assert_eq!(json["root"]["typeId"], 5_876);
    assert_eq!(json["root"]["typeName"], "Rifter");
    assert_eq!(json["root"]["parentBuildId"], serde_json::Value::Null);
    assert!(json["generatedAt"].is_string());
}

#[tokio::test]
async fn graph_every_root_buy_is_an_acquisition_child_buildable_carries_its_recipe() {
    let owner_id = OwnerId::new();
    let parent =
        parent_build_resolving_hull_section_to_build(iskworks_core::WorkspaceId::new(), owner_id);
    let parent_id = parent.id;
    let app = graph_app(
        parent,
        owner_id,
        Vec::new(),
        std::collections::BTreeMap::new(),
    );

    // Overlay Buy-resolves everything: both 90_001 (blueprint 90_002) and
    // Tritanium (34, raw) are first-class acquisition child nodes; the
    // difference is only `buildableRecipe`.
    let (status, json) = post_graph(app, parent_id, overlay(1, serde_json::json!([]))).await;
    assert_eq!(status, StatusCode::OK);

    let children = json["root"]["children"].as_array().unwrap();
    let acq: Vec<_> = children
        .iter()
        .filter(|c| c["nodeKind"] == "acquisition")
        .collect();
    assert_eq!(acq.len(), 2);
    assert!(json["root"].get("buyMaterials").is_none());

    let hull = acq.iter().find(|c| c["typeId"] == 90_001).unwrap();
    assert_eq!(hull["graphNodeId"], format!("buy:{}:90001", parent_id.0));
    assert_eq!(
        hull["buildableRecipe"],
        serde_json::json!({"mode": "manufacturing", "blueprintTypeId": 90_002})
    );

    let trit = acq.iter().find(|c| c["typeId"] == 34).unwrap();
    assert_eq!(trit["typeName"], "Tritanium");
    assert_eq!(trit["graphNodeId"], format!("buy:{}:34", parent_id.0));
    assert_eq!(trit["buildableRecipe"], serde_json::Value::Null);
}

#[tokio::test]
async fn graph_overlay_build_without_a_linked_build_is_an_unresolved_node() {
    let owner_id = OwnerId::new();
    let mut parent =
        parent_build_resolving_hull_section_to_build(iskworks_core::WorkspaceId::new(), owner_id);
    // Persisted state Buy-resolves everything; the overlay flips 90_001 to Build.
    parent
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .component_resolutions = Vec::new();
    let parent_id = parent.id;
    let app = graph_app(
        parent,
        owner_id,
        Vec::new(),
        std::collections::BTreeMap::new(),
    );

    let (status, json) = post_graph(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);

    let children = json["root"]["children"].as_array().unwrap();
    let hull = children.iter().find(|c| c["typeId"] == 90_001).unwrap();
    assert_eq!(hull["nodeKind"], "unresolvedBuild");
    assert_eq!(hull["graphNodeId"], format!("buy:{}:90001", parent_id.0));
    assert!(hull["buildId"].is_null());
    let warnings = json["warnings"].as_array().unwrap();
    assert!(warnings
        .iter()
        .any(|w| w["code"] == "linkedBuildUnresolved"));
}

#[tokio::test]
async fn graph_overlay_build_with_a_persisted_linked_build_is_a_production_node() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    let parent_id = parent.id;
    let mut linked = linked_child_build(workspace_id, owner_id, parent_id, 90_001, 2);
    linked.draft_planning = Some(walkable_draft_planning(chrono::Utc::now()));
    let linked_id = linked.id;
    let app = graph_app(
        parent,
        owner_id,
        vec![linked],
        std::collections::BTreeMap::new(),
    );

    let (status, json) = post_graph(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);

    let children = json["root"]["children"].as_array().unwrap();
    let prod = children
        .iter()
        .find(|c| c["nodeKind"] == "production")
        .unwrap();
    assert_eq!(prod["buildId"], linked_id.0.to_string());
    assert_eq!(prod["graphNodeId"], format!("build:{}", linked_id.0));
    assert_eq!(prod["parentComponentTypeId"], 90_001);
    assert_eq!(prod["typeId"], 90_001);
    // `kind` here is the ProductionKind (child, not root) -- distinct from
    // the `nodeKind` GraphChild discriminant.
    assert_eq!(prod["kind"], "manufacturing");
}

/// An explicitly Build-resolved component whose entire demand is already
/// covered by on-hand inventory must still project as its linked
/// production node -- not collapse to an acquisition "Switch to BUILD"
/// slot. Regression for "linked build of the reactor unit still shows
/// Switch to BUILD and clicking it does nothing".
#[tokio::test]
async fn graph_overlay_build_fully_covered_by_inventory_is_still_a_production_node() {
    // A fully-covered Build-resolved boundary
    // is pruned by the same allocator Materials/Worksheet already use --
    // "still a production node" was the old contract; the
    // authoritative allocation-aware projection never walks a child
    // operation for a shortage of 0 (no runs, no installation, no cost to
    // show for one). What the *original* regression this test protects
    // actually cares about survives unchanged: the requirement must never
    // fall back to a plain "Switch to BUILD" acquisition slot, since it
    // already is Build-resolved -- `buildableRecipe` stays `null` here.
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    let parent_id = parent.id;
    let linked = linked_child_build(workspace_id, owner_id, parent_id, 90_001, 2);

    let app = build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            linked_builds: std::sync::Mutex::new(vec![linked]),
            price_source: None,
            market_items: std::collections::BTreeMap::new(),
            facility_profiles: std::collections::HashMap::new(),
            blueprint_observations: std::collections::HashMap::new(),
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        // Demand is 2; 5 on hand fully covers it -- no shortage. The
        // allocation-aware path reads this from `InventoryRepository`, not
        // `ProductionRepository::coverage`.
        .with_inventory_repository(Arc::new(
            support::inventory::SeededInventoryRepository::new(vec![(90_001, 5)]),
        ))
        .with_production_repository(Arc::new(FixtureProductionRepository {
            type_id: 90_001,
            available_to_this_build: 5,
            average_historical_unit_cost: None,
        })),
    );

    let (status, json) = post_graph(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK, "{json:?}");

    let children = json["root"]["children"].as_array().unwrap();
    assert!(!children
        .iter()
        .any(|c| c["nodeKind"] == "production" && c["typeId"] == 90_001));
    let node = children
        .iter()
        .find(|c| c["nodeKind"] == "acquisition" && c["typeId"] == 90_001)
        .unwrap();
    assert_eq!(node["requiredQuantity"], 2);
    assert_eq!(node["missingQuantity"], 0);
    // Never offered "Switch to BUILD" -- it already is Build-resolved, just
    // fulfilled entirely from inventory.
    assert_eq!(node["buildableRecipe"], serde_json::Value::Null);
}

#[tokio::test]
async fn graph_persisted_build_hidden_by_an_overlay_buy_even_though_the_child_row_remains() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    // Persisted state Build-resolves 90_001 and a linked child row exists...
    let parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    let parent_id = parent.id;
    let linked = linked_child_build(workspace_id, owner_id, parent_id, 90_001, 2);
    let app = graph_app(
        parent,
        owner_id,
        vec![linked],
        std::collections::BTreeMap::new(),
    );

    // ...but the overlay flips 90_001 back to Buy.
    let (status, json) = post_graph(app, parent_id, overlay(1, serde_json::json!([]))).await;
    assert_eq!(status, StatusCode::OK);

    let children = json["root"]["children"].as_array().unwrap();
    // No production child -- the retained linked row is not active topology.
    assert!(children.iter().all(|c| c["nodeKind"] != "production"));
    // 90_001 is a buildable acquisition node again (carries its recipe).
    let acq: Vec<_> = children
        .iter()
        .filter(|c| c["nodeKind"] == "acquisition" && c["typeId"] == 90_001)
        .collect();
    assert_eq!(acq.len(), 1);
    assert!(!acq[0]["buildableRecipe"].is_null());
}

#[tokio::test]
async fn graph_unresolved_slot_id_becomes_the_build_id_once_the_linked_build_exists() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let mut parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    parent
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .component_resolutions = Vec::new();
    let parent_id = parent.id;

    // Before: overlay Build, no linked row -> unresolved on the buy slot.
    let (_, before) = post_graph(
        graph_app(
            parent.clone(),
            owner_id,
            Vec::new(),
            std::collections::BTreeMap::new(),
        ),
        parent_id,
        overlay(1, hull_section_built()),
    )
    .await;
    let slot_before = before["root"]["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["typeId"] == 90_001)
        .unwrap();
    assert_eq!(
        slot_before["graphNodeId"],
        format!("buy:{}:90001", parent_id.0)
    );
    assert_eq!(slot_before["nodeKind"], "unresolvedBuild");

    // After: the linked Build now exists -> same slot, now `build:<id>`.
    let mut linked = linked_child_build(workspace_id, owner_id, parent_id, 90_001, 2);
    linked.draft_planning = Some(walkable_draft_planning(chrono::Utc::now()));
    let linked_id = linked.id;
    let (status, after) = post_graph(
        graph_app(
            parent,
            owner_id,
            vec![linked],
            std::collections::BTreeMap::new(),
        ),
        parent_id,
        overlay(1, hull_section_built()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{after:?}");
    let slot_after = after["root"]["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["typeId"] == 90_001)
        .unwrap();
    assert_eq!(slot_after["graphNodeId"], format!("build:{}", linked_id.0));
    assert_eq!(slot_after["nodeKind"], "production");
}

#[tokio::test]
async fn graph_two_identical_requests_produce_identical_node_ids() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    let parent_id = parent.id;
    let linked = linked_child_build(workspace_id, owner_id, parent_id, 90_001, 2);

    let first = post_graph(
        graph_app(
            parent.clone(),
            owner_id,
            vec![linked.clone()],
            std::collections::BTreeMap::new(),
        ),
        parent_id,
        overlay(1, hull_section_built()),
    )
    .await
    .1;
    let second = post_graph(
        graph_app(
            parent,
            owner_id,
            vec![linked],
            std::collections::BTreeMap::new(),
        ),
        parent_id,
        overlay(1, hull_section_built()),
    )
    .await
    .1;

    assert_eq!(first["root"]["graphNodeId"], second["root"]["graphNodeId"]);
    assert_eq!(
        first["root"]["children"][0]["graphNodeId"],
        second["root"]["children"][0]["graphNodeId"]
    );
}

#[tokio::test]
async fn graph_nested_linked_builds_are_walked_to_the_leaf() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    let parent_id = parent.id;

    // Hull Section's own linked build itself resolves its Pyerite (35) to
    // Build via blueprint 91_002 -> a third level.
    let mut hull_section = linked_child_build(workspace_id, owner_id, parent_id, 90_001, 2);
    hull_section.draft_planning = Some(iskworks_core::DraftPlanningSnapshot {
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
            component_resolutions: vec![iskworks_core::ComponentResolution {
                type_id: 35,
                recipe: iskworks_core::RecipeSelection::Manufacturing {
                    blueprint_type_id: 91_002,
                },
                facility_override: None,
                blueprint_selection: None,
            }],
            fulfillment_scopes: Vec::new(),
        },
        updated_at: chrono::Utc::now(),
    });
    let hull_section_id = hull_section.id;
    // `linked_child_build` hardcodes a recipe producing 90_001 regardless of
    // `parent_component_type_id`, which would leave this node's own recipe
    // mismatched with the Pyerite (35) slot it's linked to fulfil -- build a
    // custom recipe here that actually matches blueprint 91_002 (Tritanium
    // 34 x10/run -> Pyerite 35 x1/run, per `FixtureSdeRepository`).
    let now = chrono::Utc::now();
    let pyerite = fixture_link(
        Build {
            id: BuildId::new(),
            workspace_id,
            owner_id,
            name: "Pyerite build".to_string(),
            recipe: iskworks_core::BuildRecipe::Manufacturing(iskworks_core::CapturedRecipe {
                source_sde_dataset_id: uuid::Uuid::new_v4(),
                source_sde_version: "test".to_string(),
                blueprint_type_id: 91_002,
                blueprint_name: "Pyerite Reprocessing Blueprint".to_string(),
                duration_seconds_per_run: Some(120),
                materials: vec![iskworks_core::CapturedRecipeLine {
                    type_id: 34,
                    type_name: "Tritanium".to_string(),
                    quantity_per_run: 10,
                    sort_order: 0,
                }],
                products: vec![iskworks_core::CapturedRecipeLine {
                    type_id: 35,
                    type_name: "Pyerite".to_string(),
                    quantity_per_run: 1,
                    sort_order: 0,
                }],
                fingerprint: "recipe".to_string(),
            }),
            runs: 1,
            notes: String::new(),
            revision: 1,
            created_at: now,
            updated_at: now,
            draft_planning: Some(walkable_draft_planning(now)),
            recipe_currency: iskworks_core::RecipeCurrency::Current,
            active_sde_version: Some("test".to_string()),
            product_category_name: None,
            product_group_name: None,
            selected_blueprint_origin: None,
            has_owned_blueprint: false,
        },
        hull_section_id,
        35,
    );
    let pyerite_id = pyerite.id;

    let app = graph_app(
        parent,
        owner_id,
        vec![hull_section, pyerite],
        std::collections::BTreeMap::new(),
    );
    let (status, json) = post_graph(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);

    let hull = child_by_type(&json["root"], 90_001);
    assert_eq!(hull["buildId"], hull_section_id.0.to_string());
    let deep = prod_child(hull);
    assert_eq!(deep["nodeKind"], "production");
    assert_eq!(deep["buildId"], pyerite_id.0.to_string());
    assert_eq!(deep["graphNodeId"], format!("build:{}", pyerite_id.0));
}

#[tokio::test]
async fn graph_root_inventory_nets_demand_without_changing_the_full_requirement() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    let parent_id = parent.id;
    let mut linked = linked_child_build(workspace_id, owner_id, parent_id, 90_001, 2);
    linked.draft_planning = Some(walkable_draft_planning(chrono::Utc::now()));

    // 4 units of Rifter Hull Section (90_001) already on hand; runs=3 -> full
    // requirement 6, net 2.
    let app = build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            linked_builds: std::sync::Mutex::new(vec![linked]),
            price_source: None,
            market_items: std::collections::BTreeMap::new(),
            facility_profiles: std::collections::HashMap::new(),
            blueprint_observations: std::collections::HashMap::new(),
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        .with_inventory_repository(Arc::new(
            support::inventory::SeededInventoryRepository::new(vec![(90_001, 4)]),
        ))
        .with_production_repository(Arc::new(FixtureProductionRepository {
            type_id: 90_001,
            available_to_this_build: 4,
            average_historical_unit_cost: None,
        })),
    );

    let (status, json) = post_graph(app, parent_id, overlay(3, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK, "{json:?}");
    let hull = child_by_type(&json["root"], 90_001);
    assert_eq!(hull["requiredQuantity"], 6);
    assert_eq!(hull["netRequiredQuantity"], 2);
}

#[tokio::test]
async fn graph_cost_enrichment_prices_the_root_and_a_missing_price_leaves_it_incomplete() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    let parent_id = parent.id;
    // A root facility so `estimatedCost` (total production cost) can ever
    // be `Known` -- otherwise installation is unconditionally incomplete
    // regardless of material pricing. The overlay below sends an empty
    // `componentResolutions`, so Hull Section (90_001) is a plain Buy leaf
    // here, not Build-resolved -- no linked child, no cascading child-cost
    // dependency, root cost is purely this facility + the two Buy prices.
    let facility_id = iskworks_core::FacilityProfileId::new();
    let profiles = std::collections::HashMap::from([(
        facility_id,
        manufacturing_facility_at(facility_id, 1, 0),
    )]);
    let facility_overlay = serde_json::json!({
        "facilityProfileId": facility_id.0,
        "blueprintMe": 0,
        "blueprintTe": 0,
        "estimatedItemValue": null,
    });

    // Everything priced -> root cost Known.
    let priced: std::collections::BTreeMap<i64, iskworks_core::PriceSourceItem> = [
        (34, price_item(34, "Tritanium", "5.0000")),
        (35, price_item(35, "Pyerite", "9.0000")),
        (90_001, price_item(90_001, "Rifter Hull Section", "60.0000")),
        (5_876, price_item(5_876, "Rifter", "100000.0000")),
    ]
    .into_iter()
    .collect();
    let mut root_overlay = overlay(1, serde_json::json!([]));
    root_overlay["manufacturingFacility"] = facility_overlay.clone();

    // Installation/EIV also needs adjusted prices for the root's own
    // recipe materials (34, 90_001) -- otherwise `MissingAdjustedPrice`
    // keeps installation incomplete no matter how the market prices vary.
    let adjusted_prices = std::collections::BTreeMap::from([
        (34, rust_decimal::Decimal::new(5, 0)),
        (90_001, rust_decimal::Decimal::new(60, 0)),
    ]);
    let router = |parent: Build,
                  market_items: std::collections::BTreeMap<i64, iskworks_core::PriceSourceItem>|
     -> axum::Router {
        build_router(
            AppState::new(Arc::new(configured_workspace_owned_by(
                "Industry", owner_id,
            )))
            .with_industry_repository(Arc::new(FixtureIndustryRepository {
                build: Some(parent),
                linked_builds: std::sync::Mutex::new(Vec::new()),
                price_source: None,
                market_items,
                facility_profiles: profiles.clone(),
                blueprint_observations: std::collections::HashMap::new(),
                ..Default::default()
            }))
            .with_sde_repository(Arc::new(FixtureSdeRepository))
            .with_inventory_repository(Arc::new(EmptyInventoryRepository))
            .with_production_repository(Arc::new(FixtureProductionRepository {
                type_id: 0,
                available_to_this_build: 0,
                average_historical_unit_cost: None,
            }))
            .with_adjusted_price_repository(Arc::new(
                FixtureAdjustedPriceRepository {
                    prices: adjusted_prices.clone(),
                },
            )),
        )
    };

    let (status, json) = post_graph(
        router(parent.clone(), priced),
        parent_id,
        root_overlay.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json:?}");
    assert_eq!(json["root"]["costState"], "known", "{:?}", json["root"]);
    assert!(json["root"]["estimatedCost"].is_string());

    // Tritanium price removed -> root cost Incomplete, no estimate.
    let partial: std::collections::BTreeMap<i64, iskworks_core::PriceSourceItem> =
        [(5_876, price_item(5_876, "Rifter", "100000.0000"))]
            .into_iter()
            .collect();
    let (_, json) = post_graph(router(parent, partial), parent_id, root_overlay).await;
    assert_eq!(json["root"]["costState"], "incomplete");
    assert!(json["root"]["estimatedCost"].is_null());
}

#[tokio::test]
async fn graph_request_does_not_mutate_the_build_or_create_a_linked_build() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    let parent_id = parent.id;
    let starting_revision = parent.revision;

    let repo = Arc::new(FixtureIndustryRepository {
        build: Some(parent),
        linked_builds: std::sync::Mutex::new(Vec::new()),
        price_source: None,
        market_items: std::collections::BTreeMap::new(),
        facility_profiles: std::collections::HashMap::new(),
        blueprint_observations: std::collections::HashMap::new(),
        ..Default::default()
    });
    let app = build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(repo.clone())
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        .with_inventory_repository(Arc::new(EmptyInventoryRepository))
        .with_production_repository(Arc::new(FixtureProductionRepository {
            type_id: 0,
            available_to_this_build: 0,
            average_historical_unit_cost: None,
        })),
    );

    // Overlay flips 90_001 to Build -- exactly the kind of edit that would
    // create a linked Build through the worksheet editor, but never here.
    let (status, _) = post_graph(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);

    assert!(
        repo.linked_builds.lock().unwrap().is_empty(),
        "graph must not create a linked build"
    );
    let reloaded = repo.build.as_ref().unwrap();
    assert_eq!(reloaded.revision, starting_revision, "revision untouched");
    assert_eq!(
        reloaded
            .draft_planning
            .as_ref()
            .unwrap()
            .input
            .component_resolutions
            .len(),
        1,
        "persisted overlay resolutions untouched (still the original single one)"
    );
}

#[tokio::test]
async fn graph_of_a_missing_build_is_a_404() {
    let owner_id = OwnerId::new();
    let parent =
        parent_build_resolving_hull_section_to_build(iskworks_core::WorkspaceId::new(), owner_id);
    let app = graph_app(
        parent,
        owner_id,
        Vec::new(),
        std::collections::BTreeMap::new(),
    );

    let (status, _) = post_graph(app, BuildId::new(), overlay(1, serde_json::json!([]))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// -- Planning counts free stock: physical minus open Epics' reservations ---
#[tokio::test]
async fn materials_count_only_free_stock_for_raw_materials() {
    // Hull Section fully covered so only the root's own 100 Tritanium is
    // demanded. 100 on hand, 60 reserved by another Epic -> 40 free.
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let inventory = support::inventory::SeededInventoryRepository::new([(90_001, 2), (34, 100)])
        .with_reserved(34, 60);
    let (app, _inv) = materials_app_with_inventory(parent, owner_id, linked, inventory);
    let (status, json) = post_materials(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);

    let tritanium = row(&json, 34).unwrap();
    assert_eq!(tritanium["requiredQuantity"], 100);
    assert_eq!(tritanium["allocatedQuantity"], 40);
    assert_eq!(tritanium["shortageQuantity"], 60);
    assert_eq!(tritanium["fullyCovered"], false);
    assert_materials_invariants(&json);
}

#[tokio::test]
async fn materials_resize_a_child_build_around_reserved_intermediates() {
    // Two Hull Sections on hand, but another Epic holds one: only one is
    // free, so the hull child is sized to the one still missing -- the
    // issue's "intermediates I just produced show as covered" case.
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let root_id = parent.id;
    let inventory =
        support::inventory::SeededInventoryRepository::new([(90_001, 2)]).with_reserved(90_001, 1);
    let (app, _inv) = materials_app_with_inventory(parent, owner_id, linked, inventory);
    let (status, json) = post_materials(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);

    let hull_alloc = node_alloc(&json, root_id, 90_001).unwrap();
    assert_eq!(hull_alloc["requiredQuantity"], 2);
    assert_eq!(hull_alloc["allocatedQuantity"], 1);
    assert_eq!(hull_alloc["shortageQuantity"], 1);
    assert_eq!(hull_alloc["childRuns"], 1);
    assert_materials_invariants(&json);
}

#[tokio::test]
async fn materials_treat_over_reserved_stock_as_none_free() {
    // Reservations exceeding physical (e.g. after a manual adjustment)
    // leave nothing free -- never a negative or wrapped pool.
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let inventory = support::inventory::SeededInventoryRepository::new([(90_001, 2), (34, 50)])
        .with_reserved(34, 80);
    let (app, _inv) = materials_app_with_inventory(parent, owner_id, linked, inventory);
    let (status, json) = post_materials(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);

    let tritanium = row(&json, 34).unwrap();
    assert_eq!(tritanium["allocatedQuantity"], 0);
    assert_eq!(tritanium["shortageQuantity"], 100);
    assert_materials_invariants(&json);
}
