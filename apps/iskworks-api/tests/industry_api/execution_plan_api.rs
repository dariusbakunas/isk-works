use super::*;

// -------------------------------------------------------------------
// app / post helpers (mirror `materials_app` / `post_materials`)
// -------------------------------------------------------------------

fn execution_plan_app(
    parent: Build,
    owner_id: OwnerId,
    linked_builds: Vec<Build>,
    balances: Vec<(i64, u64)>,
) -> (
    axum::Router,
    Arc<support::inventory::SeededInventoryRepository>,
) {
    execution_plan_app_with(
        parent,
        owner_id,
        linked_builds,
        balances,
        Arc::new(FixtureSdeRepository),
        std::collections::HashMap::new(),
    )
}

#[allow(clippy::type_complexity)]
fn execution_plan_app_with(
    parent: Build,
    owner_id: OwnerId,
    linked_builds: Vec<Build>,
    balances: Vec<(i64, u64)>,
    sde: Arc<dyn SdeReadRepository>,
    facility_profiles: std::collections::HashMap<
        iskworks_core::FacilityProfileId,
        iskworks_core::IndustryFacilityProfile,
    >,
) -> (
    axum::Router,
    Arc<support::inventory::SeededInventoryRepository>,
) {
    execution_plan_app_with_inventory(
        parent,
        owner_id,
        linked_builds,
        support::inventory::SeededInventoryRepository::new(balances),
        sde,
        facility_profiles,
    )
}

#[allow(clippy::type_complexity)]
fn execution_plan_app_with_inventory(
    parent: Build,
    owner_id: OwnerId,
    linked_builds: Vec<Build>,
    inventory: support::inventory::SeededInventoryRepository,
    sde: Arc<dyn SdeReadRepository>,
    facility_profiles: std::collections::HashMap<
        iskworks_core::FacilityProfileId,
        iskworks_core::IndustryFacilityProfile,
    >,
) -> (
    axum::Router,
    Arc<support::inventory::SeededInventoryRepository>,
) {
    let inventory = Arc::new(inventory);
    let router = build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            linked_builds: std::sync::Mutex::new(linked_builds),
            facility_profiles,
            ..Default::default()
        }))
        .with_sde_repository(sde)
        .with_inventory_repository(inventory.clone())
        // Same invariant `materials_app` enforces: the execution plan
        // reuses the Materials/Graph prelude as-is and must never
        // touch `ProductionRepository::coverage`.
        .with_production_repository(Arc::new(NeverCalledProductionRepository)),
    );
    (router, inventory)
}

async fn post_execution_plan(
    app: axum::Router,
    build_id: BuildId,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/builds/{}/execution-plan", build_id.0))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, json)
}

fn node_for_type(json: &serde_json::Value, type_id: i64) -> Option<&serde_json::Value> {
    json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["outputTypeId"] == type_id)
}

fn occurrence_for_build(json: &serde_json::Value, build_id: BuildId) -> Option<&serde_json::Value> {
    json["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|occurrence| occurrence["buildId"] == build_id.0.to_string())
}

fn acquisition_for_type(json: &serde_json::Value, type_id: i64) -> Option<&serde_json::Value> {
    json["acquisitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|line| line["typeId"] == type_id)
}

fn overlay_with_facility(
    runs: u64,
    component_resolutions: serde_json::Value,
    facility_profile_id: iskworks_core::FacilityProfileId,
) -> serde_json::Value {
    let mut body = overlay(runs, component_resolutions);
    body["manufacturingFacility"] = serde_json::json!({
        "facilityProfileId": facility_profile_id,
        "blueprintMe": 0,
        "blueprintTe": 0,
        "estimatedItemValue": null,
    });
    body
}

fn overlay_with_scope(
    runs: u64,
    component_resolutions: serde_json::Value,
    type_id: i64,
    scope: &str,
) -> serde_json::Value {
    let mut body = overlay(runs, component_resolutions);
    body["fulfillmentScopes"] = serde_json::json!([{"typeId": type_id, "scope": scope}]);
    body
}

// ---------------------------------------------------------------------
// Unsaved overlay parity against the same fixtures `/materials`
// already exercises. The one-inventory-read I/O assertion rides along on every one of
// these via `inventory.list_balances_call_count()`.
// ---------------------------------------------------------------------

#[tokio::test]
async fn execution_plan_matches_materials_acquisitions_for_the_same_overlay() {
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let (materials_router, _inv) =
        materials_app(parent.clone(), owner_id, linked.clone(), vec![(34, 60)]);
    let (exec_router, inv) = execution_plan_app(parent, owner_id, linked, vec![(34, 60)]);

    let (materials_status, materials_json) = post_materials(
        materials_router,
        parent_id,
        overlay(1, hull_section_built()),
    )
    .await;
    let (exec_status, exec_json) =
        post_execution_plan(exec_router, parent_id, overlay(1, hull_section_built())).await;

    assert_eq!(materials_status, StatusCode::OK);
    assert_eq!(exec_status, StatusCode::OK);

    // Tritanium (34) is a shared Buy leaf across the root and the deep
    // Pyerite descendant -- the exact double-count regression Materials
    // itself guards. The Execution Plan's Buy-only acquisition rollup
    // must show the identical, already-deduplicated shortage.
    let materials_trit = row(&materials_json, 34).unwrap();
    let exec_trit = acquisition_for_type(&exec_json, 34).unwrap();
    assert_eq!(
        exec_trit["requiredQuantity"], materials_trit["requiredQuantity"],
        "execution-plan acquisitions must match materials' own required total"
    );
    assert_eq!(
        exec_trit["shortageQuantity"],
        materials_trit["shortageQuantity"]
    );
    assert_eq!(
        exec_trit["plannedInventoryQuantity"],
        materials_trit["allocatedQuantity"]
    );
    assert_eq!(
        inv.list_balances_call_count(),
        1,
        "exactly one inventory read"
    );
}

// Stages polish pass: acquisition consumer provenance ("Used By" on the
// acquisition inspector). Tritanium (34) is needed both directly by the
// root AND by the deep Pyerite descendant (`nested_chain`'s own shared-
// leaf shape) -- an authoritative real-world case where one purchased
// type has more than one distinct owning production occurrence. The API
// must expose both, at their own true shortage contributions, never
// collapsed into a single entry or a proportional split.
#[tokio::test]
async fn execution_plan_acquisition_exposes_every_consumer_occurrence_with_reconciling_quantities()
{
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let (exec_router, _inv) = execution_plan_app(parent, owner_id, linked, Vec::new());

    let (status, json) =
        post_execution_plan(exec_router, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);

    let trit = acquisition_for_type(&json, 34).unwrap();
    let shortage = trit["shortageQuantity"].as_u64().unwrap();
    let consumers = trit["consumers"].as_array().unwrap();

    assert!(
        consumers.len() >= 2,
        "Tritanium is needed by both the root and the deep Pyerite descendant -- \
             collapsing them into one entry would lose real evidence"
    );
    assert_eq!(
        consumers
            .iter()
            .map(|consumer| consumer["quantity"].as_u64().unwrap())
            .sum::<u64>(),
        shortage,
        "consumer quantities must reconcile exactly with the line's own shortage -- \
             no proportional allocation invented anywhere"
    );

    let node_ids: std::collections::HashSet<&str> = json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["id"].as_str().unwrap())
        .collect();
    for consumer in consumers {
        let node_id = consumer["nodeId"].as_str().unwrap();
        assert!(
            node_ids.contains(node_id),
            "every consumer must resolve to a real display node, node {node_id} did not"
        );
    }
}

#[tokio::test]
async fn execution_plan_reflects_unsaved_runs_change() {
    let (parent, _ws, owner_id) = rifter_root(1);
    let parent_id = parent.id;
    let (app, inv) = execution_plan_app(parent, owner_id, Vec::new(), vec![]);

    let (status, json) =
        post_execution_plan(app, parent_id, overlay(3, serde_json::json!([]))).await;
    assert_eq!(status, StatusCode::OK);

    // Root's own persisted runs is 1; the overlay's unsaved runs (3)
    // must be what the execution plan actually projects.
    let root = occurrence_for_build(&json, parent_id).unwrap();
    assert_eq!(root["projectedRuns"], 3);
    // Root recipe needs 100 Tritanium/run -> 300 at the unsaved runs.
    assert_eq!(
        acquisition_for_type(&json, 34).unwrap()["requiredQuantity"],
        300
    );
    assert_eq!(inv.list_balances_call_count(), 1);
}

#[tokio::test]
async fn execution_plan_reflects_unsaved_facility_change() {
    let (parent, _ws, owner_id) = rifter_root(1);
    let parent_id = parent.id;
    let facility_id = iskworks_core::FacilityProfileId::new();
    let profiles = std::collections::HashMap::from([(
        facility_id,
        manufacturing_facility_at(facility_id, 1, 10),
    )]);

    // No facility in the overlay -> root has none.
    let (app_none, _inv) = execution_plan_app_with(
        parent.clone(),
        owner_id,
        Vec::new(),
        vec![],
        Arc::new(FixtureSdeRepository),
        profiles.clone(),
    );
    let (status_none, json_none) =
        post_execution_plan(app_none, parent_id, overlay(1, serde_json::json!([]))).await;
    assert_eq!(status_none, StatusCode::OK);
    assert!(occurrence_for_build(&json_none, parent_id).unwrap()["facilityId"].is_null());

    // Same persisted Build, unsaved overlay now names the facility --
    // the response must reflect it without saving anything.
    let (app_some, _inv) = execution_plan_app_with(
        parent,
        owner_id,
        Vec::new(),
        vec![],
        Arc::new(FixtureSdeRepository),
        profiles,
    );
    let (status_some, json_some) = post_execution_plan(
        app_some,
        parent_id,
        overlay_with_facility(1, serde_json::json!([]), facility_id),
    )
    .await;
    assert_eq!(status_some, StatusCode::OK);
    assert_eq!(
        occurrence_for_build(&json_some, parent_id).unwrap()["facilityId"],
        facility_id.0.to_string()
    );
}

#[tokio::test]
async fn execution_plan_reflects_buy_to_build_and_build_to_buy() {
    let (parent, ws, owner_id) = rifter_root(1);
    let parent_id = parent.id;
    let child = linked_child_with_draft(
        ws,
        owner_id,
        parent_id,
        90_001,
        2,
        captured_recipe(
            90_002,
            &[(35, "Pyerite", 50)],
            (90_001, "Rifter Hull Section", 1),
        ),
        Vec::new(),
    );

    // Buy: Hull Section (90_001) is a raw acquisition, no production node.
    let (app_buy, _inv) = execution_plan_app(parent.clone(), owner_id, vec![child.clone()], vec![]);
    let (status_buy, json_buy) =
        post_execution_plan(app_buy, parent_id, overlay(1, serde_json::json!([]))).await;
    assert_eq!(status_buy, StatusCode::OK);
    assert!(
        node_for_type(&json_buy, 90_001).is_none(),
        "Buy -> no production node"
    );
    assert_eq!(
        acquisition_for_type(&json_buy, 90_001).unwrap()["shortageQuantity"],
        2
    );

    // Build: Hull Section becomes a production node; its own Buy input
    // (Pyerite) appears as a new acquisition instead.
    let (app_build, _inv) = execution_plan_app(parent, owner_id, vec![child], vec![]);
    let (status_build, json_build) =
        post_execution_plan(app_build, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status_build, StatusCode::OK);
    assert!(
        node_for_type(&json_build, 90_001).is_some(),
        "Build -> a production node now exists"
    );
    assert!(acquisition_for_type(&json_build, 90_001).is_none());
    assert_eq!(
        acquisition_for_type(&json_build, 35).unwrap()["shortageQuantity"],
        100
    );
}

#[tokio::test]
async fn execution_plan_reflects_missing_to_full_fulfillment_scope() {
    // 100 Tritanium on hand -- `Missing` (default) nets it; `Full` must
    // ignore it and demand the whole 100 fresh, matching Materials'
    // own `materials_full_scope_contribution_never_consumes_the_pool`.
    let (parent, _ws, owner_id) = rifter_root(1);
    let parent_id = parent.id;

    let (app_missing, _inv) =
        execution_plan_app(parent.clone(), owner_id, Vec::new(), vec![(34, 100)]);
    let (status_missing, json_missing) =
        post_execution_plan(app_missing, parent_id, overlay(1, serde_json::json!([]))).await;
    assert_eq!(status_missing, StatusCode::OK);
    assert!(
        acquisition_for_type(&json_missing, 34).is_none(),
        "Missing scope nets fully against 100 on hand -> no shortage"
    );

    let (app_full, _inv) = execution_plan_app(parent, owner_id, Vec::new(), vec![(34, 100)]);
    let (status_full, json_full) = post_execution_plan(
        app_full,
        parent_id,
        overlay_with_scope(1, serde_json::json!([]), 34, "full"),
    )
    .await;
    assert_eq!(status_full, StatusCode::OK);
    let full_line = acquisition_for_type(&json_full, 34)
        .expect("Full scope ignores the 100 on hand -> a real shortage");
    assert_eq!(full_line["shortageQuantity"], 100);
    assert_eq!(full_line["plannedInventoryQuantity"], 0);
}

// ---------------------------------------------------------------------
// Inventory-threshold N -> N+1, reusing the exact fixture
// that reproduces the real Muninn regression (durable observed-blueprint
// configuration) for `/materials`.
// ---------------------------------------------------------------------

#[tokio::test]
async fn execution_plan_inventory_threshold_activates_production_with_durable_observed_blueprint() {
    let missing_observation_id = uuid::Uuid::new_v4();
    let captured_selection = iskworks_core::BlueprintSelection::ObservedAsset {
        observation_id: missing_observation_id,
        kind: iskworks_core::BlueprintKind::Copy,
        material_efficiency: 10,
        time_efficiency: 20,
        licensed_runs: None,
    };

    // N: 2 Hull Sections on hand fully covers the root's demand (1 run)
    // -> pruned, no production node, regardless of the vanished
    // observation (the boundary is never walked).
    let (parent_n, mut linked_n, parent_id, owner_id) = nested_chain(1);
    linked_n[0]
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .blueprint_selection = Some(captured_selection.clone());
    let (app_n, _inv_n) = execution_plan_app(parent_n, owner_id, linked_n, vec![(90_001, 2)]);
    let (status_n, json_n) =
        post_execution_plan(app_n, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status_n, StatusCode::OK);
    assert!(
        node_for_type(&json_n, 90_001).is_none(),
        "N: fully covered -> no production node"
    );
    assert!(
        node_for_type(&json_n, 35).is_none(),
        "N: deep chain never walked, no Pyerite node either"
    );

    // N+1 (root runs 3): 6 Hull Sections needed, only 2 on hand -> the
    // intermediate activates. Must succeed using the frozen ME/TE
    // without touching the vanished
    // observation -- this is the exact class of bug that broke Muninn.
    let (parent_n1, mut linked_n1, parent_id_n1, owner_id_n1) = nested_chain(1);
    linked_n1[0]
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .blueprint_selection = Some(captured_selection);
    let hull_build_id = linked_n1[0].id;
    let pyerite_build_id = linked_n1[1].id;
    let (app_n1, _inv_n1) =
        execution_plan_app(parent_n1, owner_id_n1, linked_n1, vec![(90_001, 2)]);
    let (status_n1, json_n1) =
        post_execution_plan(app_n1, parent_id_n1, overlay(3, hull_section_built())).await;
    assert_eq!(
        status_n1,
        StatusCode::OK,
        "activating a Build-resolved intermediate whose observed blueprint has \
             vanished must not fail the projection"
    );

    let hull_node = node_for_type(&json_n1, 90_001)
        .expect("N+1: shortage 4 -> a real production node now exists");
    assert_eq!(hull_node["productionDemand"], 4);
    let hull_occurrence = occurrence_for_build(&json_n1, hull_build_id).unwrap();
    assert_eq!(
        hull_occurrence["projectedRuns"], 4,
        "dynamically resized, not the persisted 2"
    );
    assert_eq!(
        hull_occurrence["stage"], 1,
        "Hull Section's own persisted resolution still Build-resolves Pyerite \
             (its own occurrence stage 0), so Hull Section itself is stage 1"
    );
    assert_eq!(
        occurrence_for_build(&json_n1, pyerite_build_id).unwrap()["stage"],
        0,
        "the deep Pyerite descendant, activated alongside Hull Section"
    );
    assert_eq!(
        occurrence_for_build(&json_n1, parent_id_n1).unwrap()["stage"],
        2
    );
    // Consumer edge: the root's own occurrence consumes exactly 4.
    let root_id = occurrence_for_build(&json_n1, parent_id_n1).unwrap()["nodeId"].clone();
    let consumer = hull_node["consumers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["nodeId"] == root_id)
        .expect("root must be a consumer of the activated Hull Section node");
    assert_eq!(consumer["quantity"], 4);
    // ME10-derived Pyerite demand proves the frozen ME actually drove
    // the child's own recursive re-preview. Pyerite is itself
    // Build-resolved (never a Buy leaf), so it is never in
    // `acquisitions` -- its own occurrence carries the requirement.
    assert_eq!(
        occurrence_for_build(&json_n1, pyerite_build_id).unwrap()["requiredQuantity"],
        180
    );
}

// ---------------------------------------------------------------------
// Grouping, incompatible config, and Reaction-chain
// stage parity need a shared-intermediate / deep-Reaction topology the
// Rifter fixture doesn't have. Self-contained SDE + hand-built linked
// builds; never touches the shared `FixtureSdeRepository`.
// ---------------------------------------------------------------------

struct MuninnLikeSde;

#[async_trait]
impl SdeReadRepository for MuninnLikeSde {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        Ok(Some(ActiveSde {
            import_id: uuid::Uuid::nil(),
            source_version: "test".to_string(),
            source_label: "fixture.zip".to_string(),
            source_checksum: "abc123".to_string(),
            completed_at: chrono::Utc::now(),
            counts: iskworks_sde::ImportCounts {
                types: 8,
                blueprints: 6,
                material_lines: 8,
                product_lines: 6,
                skipped_blueprints: 0,
                ..Default::default()
            },
        }))
    }
    async fn search_manufacturing_blueprints(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<BlueprintSearchResult>, SdeError> {
        Ok(Vec::new())
    }
    async fn search_types(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        Ok(Vec::new())
    }
    async fn manufacturing_recipe(
        &self,
        blueprint_type_id: i64,
    ) -> Result<Option<ManufacturingRecipe>, SdeError> {
        fn ln(type_id: i64, name: &str, quantity: i64) -> RecipeLine {
            RecipeLine {
                type_id,
                type_name: name.to_string(),
                quantity,
            }
        }
        let recipe = match blueprint_type_id {
            // Neo Mercurite two-consumer branching.
            80_000 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Muninn Blueprint".to_string(),
                duration_seconds: Some(1_200),
                materials: vec![
                    ln(80_810, "Nanotransistor Component", 1),
                    ln(80_820, "Plasmonic Component", 1),
                ],
                products: vec![ln(80_500, "Muninn", 1)],
            },
            80_811 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Nanotransistor Component Blueprint".to_string(),
                duration_seconds: Some(600),
                materials: vec![ln(80_700, "Neo Mercurite", 250)],
                products: vec![ln(80_810, "Nanotransistor Component", 1)],
            },
            80_821 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Plasmonic Component Blueprint".to_string(),
                duration_seconds: Some(600),
                materials: vec![ln(80_700, "Neo Mercurite", 620)],
                products: vec![ln(80_820, "Plasmonic Component", 1)],
            },
            // Reaction-chain stage parity: independent topology.
            80_900 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Ship Blueprint".to_string(),
                duration_seconds: Some(3_600),
                materials: vec![ln(80_910, "Component", 1)],
                products: vec![ln(80_950, "Ship", 1)],
            },
            80_911 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Component Blueprint".to_string(),
                duration_seconds: Some(600),
                materials: vec![ln(80_920, "Nanotransistors", 5)],
                products: vec![ln(80_910, "Component", 1)],
            },
            _ => return Ok(None),
        };
        Ok(Some(recipe))
    }
    async fn reaction_formula(
        &self,
        reaction_formula_type_id: i64,
    ) -> Result<Option<ReactionFormulaRecipe>, SdeError> {
        fn ln(type_id: i64, name: &str, quantity: i64) -> RecipeLine {
            RecipeLine {
                type_id,
                type_name: name.to_string(),
                quantity,
            }
        }
        let recipe = match reaction_formula_type_id {
            80_701 => ReactionFormulaRecipe {
                reaction_formula_type_id,
                reaction_formula_name: "Neo Mercurite Reaction".to_string(),
                duration_seconds: Some(3_600),
                materials: vec![ln(34, "Tritanium", 10)],
                products: vec![ln(80_700, "Neo Mercurite", 100)],
            },
            80_921 => ReactionFormulaRecipe {
                reaction_formula_type_id,
                reaction_formula_name: "Nanotransistors Reaction".to_string(),
                duration_seconds: Some(3_600),
                materials: vec![ln(80_930, "Neo Mercurite", 10)],
                products: vec![ln(80_920, "Nanotransistors", 1)],
            },
            80_931 => ReactionFormulaRecipe {
                reaction_formula_type_id,
                reaction_formula_name: "Neo Mercurite Reaction (deep)".to_string(),
                duration_seconds: Some(3_600),
                materials: vec![ln(34, "Tritanium", 2)],
                products: vec![ln(80_930, "Neo Mercurite", 1)],
            },
            _ => return Ok(None),
        };
        Ok(Some(recipe))
    }
}

fn ship_overlay(runs: u64, component_resolutions: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 80_900},
        "runs": runs,
        "pricingSelections": [],
        "componentResolutions": component_resolutions,
        "fulfillmentScopes": [],
    })
}

fn reaction_resolution(
    type_id: i64,
    reaction_formula_type_id: i64,
) -> iskworks_core::ComponentResolution {
    iskworks_core::ComponentResolution {
        type_id,
        recipe: iskworks_core::RecipeSelection::Reaction {
            reaction_formula_type_id,
        },
        facility_override: None,
        blueprint_selection: None,
    }
}

fn captured_reaction_formula(
    formula_type_id: i64,
    materials: &[(i64, &str, u64)],
    product: (i64, &str, u64),
) -> iskworks_core::CapturedReactionFormula {
    iskworks_core::CapturedReactionFormula::capture(
        uuid::Uuid::new_v4(),
        "test".to_string(),
        ReactionFormulaRecipe {
            reaction_formula_type_id: formula_type_id,
            reaction_formula_name: format!("formula {formula_type_id}"),
            duration_seconds: Some(3_600),
            materials: materials
                .iter()
                .map(|(type_id, name, quantity)| RecipeLine {
                    type_id: *type_id,
                    type_name: (*name).to_string(),
                    quantity: *quantity as i64,
                })
                .collect(),
            products: vec![RecipeLine {
                type_id: product.0,
                type_name: product.1.to_string(),
                quantity: product.2 as i64,
            }],
        },
    )
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
fn linked_reaction_child(
    workspace_id: iskworks_core::WorkspaceId,
    owner_id: OwnerId,
    parent_build_id: BuildId,
    parent_component_type_id: i64,
    runs: u64,
    formula: iskworks_core::CapturedReactionFormula,
    component_resolutions: Vec<iskworks_core::ComponentResolution>,
    reaction_facility: Option<iskworks_core::ReactionFacilityPreviewCommand>,
) -> Build {
    let now = chrono::Utc::now();
    fixture_link(
        Build {
            id: BuildId::new(),
            workspace_id,
            owner_id,
            name: "reaction child".to_string(),
            recipe: iskworks_core::BuildRecipe::Reaction(formula),
            runs,
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
                    reaction_facility,
                    facility_eiv_manual: false,
                    component_resolutions,
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
        parent_build_id,
        parent_component_type_id,
    )
}

fn muninn_root(workspace_id: iskworks_core::WorkspaceId, owner_id: OwnerId) -> Build {
    let now = chrono::Utc::now();
    Build {
        id: BuildId::new(),
        workspace_id,
        owner_id,
        name: "Muninn".to_string(),
        recipe: iskworks_core::BuildRecipe::Manufacturing(captured_recipe(
            80_000,
            &[
                (80_810, "Nanotransistor Component", 1),
                (80_820, "Plasmonic Component", 1),
            ],
            (80_500, "Muninn", 1),
        )),
        runs: 1,
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
    }
}

#[tokio::test]
async fn execution_plan_reaction_chain_stages_match_the_production_prerequisite_topology() {
    // Ship(Manufacturing) -> Component(Manufacturing) ->
    // Nanotransistors(Reaction) -> Neo Mercurite(Reaction) -> Buy(34).
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let mut root = muninn_root(workspace_id, owner_id);
    root.recipe = iskworks_core::BuildRecipe::Manufacturing(captured_recipe(
        80_900,
        &[(80_910, "Component", 1)],
        (80_950, "Ship", 1),
    ));
    let root_id = root.id;

    let component = linked_child_with_draft(
        workspace_id,
        owner_id,
        root_id,
        80_910,
        1,
        captured_recipe(
            80_911,
            &[(80_920, "Nanotransistors", 5)],
            (80_910, "Component", 1),
        ),
        vec![reaction_resolution(80_920, 80_921)],
    );
    let component_id = component.id;
    let nanotransistors = linked_reaction_child(
        workspace_id,
        owner_id,
        component_id,
        80_920,
        1,
        captured_reaction_formula(
            80_921,
            &[(80_930, "Neo Mercurite", 10)],
            (80_920, "Nanotransistors", 1),
        ),
        vec![reaction_resolution(80_930, 80_931)],
        None,
    );
    let nanotransistors_id = nanotransistors.id;
    let neo_mercurite = linked_reaction_child(
        workspace_id,
        owner_id,
        nanotransistors_id,
        80_930,
        1,
        captured_reaction_formula(
            80_931,
            &[(34, "Tritanium", 2)],
            (80_930, "Neo Mercurite", 1),
        ),
        Vec::new(),
        None,
    );
    let neo_mercurite_id = neo_mercurite.id;

    let (app, _inv) = execution_plan_app_with(
        root,
        owner_id,
        vec![component, nanotransistors, neo_mercurite],
        vec![],
        Arc::new(MuninnLikeSde),
        std::collections::HashMap::new(),
    );
    let (status, json) = post_execution_plan(
        app,
        root_id,
        ship_overlay(
            1,
            serde_json::json!([
                {"typeId": 80_910, "recipe": {"mode": "manufacturing", "blueprintTypeId": 80_911}}
            ]),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json:?}");

    assert_eq!(
        occurrence_for_build(&json, neo_mercurite_id).unwrap()["stage"],
        0
    );
    assert_eq!(
        occurrence_for_build(&json, nanotransistors_id).unwrap()["stage"],
        1
    );
    assert_eq!(
        occurrence_for_build(&json, component_id).unwrap()["stage"],
        2
    );
    assert_eq!(occurrence_for_build(&json, root_id).unwrap()["stage"], 3);
    assert_eq!(
        json["stages"].as_array().unwrap().len(),
        4,
        "four distinct dependency stages"
    );
    // No readiness terminology anywhere on the wire.
    let raw = json.to_string();
    for forbidden in ["\"ready\"", "\"blocked\"", "\"unlocks\"", "\"waiting\""] {
        assert!(
            !raw.contains(forbidden),
            "must not introduce readiness terminology: {forbidden}"
        );
    }
}

// ---------------------------------------------------------------------
// Typed warnings passed through unchanged; ordinary incomplete
// cost evidence is a 200, never an HTTP failure.
// ---------------------------------------------------------------------

#[tokio::test]
async fn execution_plan_incomplete_cost_is_200_with_typed_warnings_not_strings() {
    let (parent, _ws, owner_id) = rifter_root(1);
    let parent_id = parent.id;
    // No facility, no market prices -- cost is necessarily incomplete.
    let (app, _inv) = execution_plan_app(parent, owner_id, Vec::new(), vec![]);
    let (status, json) =
        post_execution_plan(app, parent_id, overlay(1, serde_json::json!([]))).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "ordinary incomplete cost evidence must never be an HTTP failure"
    );
    assert_eq!(json["complete"], false);
    let warnings = json["warnings"].as_array().unwrap();
    assert!(!warnings.is_empty());
    // Typed warnings: each is an object with a `code`, never a bare
    // string -- the existing `CostWarning` serde shape, untouched.
    for warning in warnings {
        assert!(
            warning.is_object(),
            "warning must stay a typed object: {warning:?}"
        );
        assert!(warning.get("code").is_some());
    }
}

#[tokio::test]
async fn execution_plan_missing_build_is_a_404_not_a_persistence_leak() {
    let (parent, _ws, owner_id) = rifter_root(1);
    let (app, _inv) = execution_plan_app(parent, owner_id, Vec::new(), vec![]);
    let missing_id = BuildId::new();
    let (status, json) =
        post_execution_plan(app, missing_id, overlay(1, serde_json::json!([]))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Curated error body, never a raw persistence/debug string.
    assert!(json["error"].is_object());
}

// -------------------------------------------------------------------
// Free stock: acquisitions count only what open Epics haven't reserved
// -------------------------------------------------------------------

#[tokio::test]
async fn execution_plan_acquisitions_report_free_and_reserved_stock() {
    // Root needs 100 Tritanium; 100 on hand but 70 reserved by another
    // Epic -> 30 free, 70 to buy.
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let inventory = support::inventory::SeededInventoryRepository::new([(90_001, 2), (34, 100)])
        .with_reserved(34, 70);
    let (app, _inv) = execution_plan_app_with_inventory(
        parent,
        owner_id,
        linked,
        inventory,
        Arc::new(FixtureSdeRepository),
        std::collections::HashMap::new(),
    );
    let (status, json) =
        post_execution_plan(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);

    let tritanium = acquisition_for_type(&json, 34).expect("Tritanium must be acquired");
    assert_eq!(tritanium["requiredQuantity"], 100);
    assert_eq!(tritanium["availableQuantity"], 30);
    assert_eq!(tritanium["reservedQuantity"], 70);
    assert_eq!(tritanium["plannedInventoryQuantity"], 30);
    assert_eq!(tritanium["shortageQuantity"], 70);
}
