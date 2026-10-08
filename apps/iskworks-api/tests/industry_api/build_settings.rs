use super::*;

// --- Build-ID-addressable component-resolution mutation (nested BUY<->BUILD) ---

// --- Build-ID-addressable "common settings" patches (unified inspector) ---

async fn patch_build(
    app: &axum::Router,
    build_id: BuildId,
    suffix: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/builds/{}/{suffix}", build_id.0))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let value = if status == StatusCode::OK {
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
    } else {
        serde_json::Value::Null
    };
    (status, value)
}

#[tokio::test]
async fn set_build_blueprint_selection_targets_the_named_build_never_the_root() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = hull_parent(workspace_id, owner_id, 1, None);
    let parent_id = parent.id;
    let child = hull_child_resolving_pyerite(workspace_id, owner_id, parent_id, 2);
    let child_id = child.id;
    let app = app_with_mutable_build_tree(owner_id, vec![parent, child]);

    // Research the linked child's blueprint in place -- by the child's id.
    let (status, _) = patch_build(
        &app,
        child_id,
        "blueprint-selection",
        serde_json::json!({
            "expectedRevision": 1,
            "blueprintSelection": {
                "mode": "manual", "kind": "original",
                "materialEfficiency": 10, "timeEfficiency": 20,
                "licensedRuns": null, "notes": ""
            },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let child_after = get_build_json(&app, child_id).await;
    let selection = &child_after["draftPlanning"]["input"]["blueprintSelection"];
    assert_eq!(selection["materialEfficiency"], 10);
    assert_eq!(selection["timeEfficiency"], 20);

    // The root Build's own planning input is untouched.
    let root_after = get_build_json(&app, parent_id).await;
    assert!(root_after["draftPlanning"]["input"]["blueprintSelection"].is_null());
    assert_eq!(root_after["revision"], 1);
}

// Multi-BPC job split: picking an observed copy freezes its licensed runs
// with kind/ME/TE -- the per-job run limit the planner splits by.
#[tokio::test]
async fn set_build_blueprint_selection_captures_an_observed_copys_licensed_runs() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = hull_parent(workspace_id, owner_id, 1, None);
    let parent_id = parent.id;
    let child = hull_child_resolving_pyerite(workspace_id, owner_id, parent_id, 2);
    let child_id = child.id;
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
        material_efficiency: 2,
        time_efficiency: 4,
        licensed_runs: Some(1),
        location_id: 60_003_760,
        location_flag: "Hangar".to_string(),
        location_name: None,
        observed_at: now,
        imported_at: now,
    };
    let app = app_with_mutable_build_tree_facilities_and_observations(
        owner_id,
        vec![parent, child],
        std::collections::HashMap::new(),
        std::collections::HashMap::from([(observation_id, observation)]),
    );

    let (status, _) = patch_build(
        &app,
        child_id,
        "blueprint-selection",
        serde_json::json!({
            "expectedRevision": 1,
            "blueprintSelection": {"mode": "observedAsset", "observationId": observation_id},
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let child_after = get_build_json(&app, child_id).await;
    let selection = &child_after["draftPlanning"]["input"]["blueprintSelection"];
    assert_eq!(selection["kind"], "copy");
    assert_eq!(selection["materialEfficiency"], 2);
    assert_eq!(selection["licensedRuns"], 1);
}

#[tokio::test]
async fn set_build_blueprint_selection_rejects_an_out_of_range_efficiency() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = hull_parent(workspace_id, owner_id, 1, None);
    let parent_id = parent.id;
    let child = hull_child_resolving_pyerite(workspace_id, owner_id, parent_id, 2);
    let child_id = child.id;
    let app = app_with_mutable_build_tree(owner_id, vec![parent, child]);

    let (status, _) = patch_build(
        &app,
        child_id,
        "blueprint-selection",
        serde_json::json!({
            "expectedRevision": 1,
            "blueprintSelection": {
                "mode": "manual", "kind": "original",
                "materialEfficiency": 15, "timeEfficiency": 0,
                "licensedRuns": null, "notes": ""
            },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn set_build_facility_writes_only_the_recipe_appropriate_slot() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = hull_parent(workspace_id, owner_id, 1, None);
    let parent_id = parent.id;
    let child = hull_child_resolving_pyerite(workspace_id, owner_id, parent_id, 2);
    let child_id = child.id;
    let app = app_with_mutable_build_tree(owner_id, vec![parent, child]);

    let facility_id = uuid::Uuid::new_v4();
    let (status, _) = patch_build(
        &app,
        child_id,
        "facility",
        serde_json::json!({
            "expectedRevision": 1,
            "facilityProfileId": facility_id,
            // A legacy client may still send this; it must be accepted and ignored.
            "expectedProfileRevision": 4,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let child_after = get_build_json(&app, child_id).await;
    let input = &child_after["draftPlanning"]["input"];
    // Manufacturing Build -> the manufacturing slot is set, the reaction slot stays clear.
    assert_eq!(
        input["manufacturingFacility"]["facilityProfileId"],
        facility_id.to_string()
    );
    assert!(
        input["manufacturingFacility"]
            .get("expectedProfileRevision")
            .is_none(),
        "the vestigial revision field is neither stored nor emitted"
    );
    assert!(input["reactionFacility"].is_null());
}

#[tokio::test]
async fn set_build_pricing_persists_scope_and_strategy_on_the_named_build() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = hull_parent(workspace_id, owner_id, 1, None);
    let parent_id = parent.id;
    let child = hull_child_resolving_pyerite(workspace_id, owner_id, parent_id, 2);
    let child_id = child.id;
    let app = app_with_mutable_build_tree(owner_id, vec![parent, child]);

    let (status, _) = patch_build(
        &app,
        child_id,
        "pricing",
        serde_json::json!({
            "expectedRevision": 1,
            "materialScope": { "regionId": 10_000_043, "locationId": null },
            "outputScope": { "regionId": 10_000_002, "locationId": null },
            "materialPricingPolicy": "acquireQuantityFromSellOrders",
            "outputPricingPolicy": "liquidateQuantityIntoBuyOrders",
            "facilityEivManual": true,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let input = get_build_json(&app, child_id).await["draftPlanning"]["input"].clone();
    assert_eq!(
        input["materialPricingPolicy"],
        "acquireQuantityFromSellOrders"
    );
    assert_eq!(
        input["outputPricingPolicy"],
        "liquidateQuantityIntoBuyOrders"
    );
    assert_eq!(input["materialScope"]["regionId"], 10_000_043);
    assert_eq!(input["facilityEivManual"], true);
    // The root Build's revision is untouched by a child-only patch.
    assert_eq!(get_build_json(&app, parent_id).await["revision"], 1);
}

#[tokio::test]
async fn set_build_settings_rejects_a_stale_expected_revision() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = hull_parent(workspace_id, owner_id, 1, None);
    let parent_id = parent.id;
    let child = hull_child_resolving_pyerite(workspace_id, owner_id, parent_id, 2);
    let child_id = child.id;
    let app = app_with_mutable_build_tree(owner_id, vec![parent, child]);

    let (status, _) = patch_build(
        &app,
        child_id,
        "pricing",
        serde_json::json!({
            "expectedRevision": 99,
            "materialScope": { "regionId": 10_000_043, "locationId": null },
            "outputScope": { "regionId": 10_000_002, "locationId": null },
            "materialPricingPolicy": "highestBuy",
            "outputPricingPolicy": "lowestSell",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

// --- Discrete-output surplus ---------------------------------------------
//
// Parent bp 92_010 consumes 92_001 (SDE recipe: 250/run); child bp 92_002
// makes 92_001 in lots of 500/run. `create_or_reuse_linked_build` derives
// the parent's authoritative requirement from the *embedded* CapturedRecipe
// (via derive_effective_root_requirements), so the run-boundary test moves
// it to 250 / 500 / 501 there; the graph endpoint re-captures from the SDE
// recipe, so the surplus test reads its 250 straight through.

/// A persisted linked child producing 92_001 (bp 92_002, 500/run).
fn batch_child(
    workspace_id: iskworks_core::WorkspaceId,
    owner_id: OwnerId,
    parent_id: BuildId,
    runs: u64,
) -> Build {
    let now = chrono::Utc::now();
    fixture_link(
        Build {
            id: BuildId::new(),
            workspace_id,
            owner_id,
            name: "Batched Component build".to_string(),
            recipe: iskworks_core::BuildRecipe::Manufacturing(iskworks_core::CapturedRecipe {
                source_sde_dataset_id: uuid::Uuid::new_v4(),
                source_sde_version: "test".to_string(),
                blueprint_type_id: 92_002,
                blueprint_name: "Batched Component Blueprint".to_string(),
                duration_seconds_per_run: Some(300),
                materials: vec![iskworks_core::CapturedRecipeLine {
                    type_id: 34,
                    type_name: "Tritanium".to_string(),
                    quantity_per_run: 10,
                    sort_order: 0,
                }],
                products: vec![iskworks_core::CapturedRecipeLine {
                    type_id: 92_001,
                    type_name: "Batched Component".to_string(),
                    quantity_per_run: 500,
                    sort_order: 0,
                }],
                fingerprint: "recipe".to_string(),
            }),
            runs,
            notes: String::new(),
            revision: 1,
            created_at: now,
            updated_at: now,
            draft_planning: None,
            recipe_currency: iskworks_core::RecipeCurrency::Current,
            active_sde_version: Some("test".to_string()),
            product_category_name: None,
            product_group_name: None,
            selected_blueprint_origin: None,
            has_owned_blueprint: false,
        },
        parent_id,
        92_001,
    )
}

async fn create_linked_runs(app: axum::Router, parent_id: BuildId) -> u64 {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/builds/{}/linked-builds", parent_id.0))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"componentTypeId": 92001}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    body["runs"].as_u64().unwrap()
}

#[tokio::test]
async fn linked_build_surplus_is_only_the_unavoidable_discrete_output_remainder() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();

    // required 250, child output/run 500 -> 1 run.
    let parent = batch_parent(workspace_id, owner_id, 250);
    let parent_id = parent.id;
    let runs = create_linked_runs(
        app_with_linked_build_fixture(parent.clone(), owner_id, Vec::new()),
        parent_id,
    )
    .await;
    assert_eq!(runs, 1, "ceil(250 / 500) = 1, never 2");

    // The graph reports the discrete surplus explicitly, from the persisted
    // child's real runs -- not hidden, not inflated.
    let mut child = batch_child(workspace_id, owner_id, parent_id, runs);
    child.draft_planning = Some(walkable_draft_planning(chrono::Utc::now()));
    let graph = graph_app(
        parent,
        owner_id,
        vec![child],
        std::collections::BTreeMap::new(),
    );
    let overlay = serde_json::json!({
        "recipe": { "mode": "manufacturing", "blueprintTypeId": 92_010 },
        "runs": 1,
        "pricingSelections": [],
        "componentResolutions": [
            { "typeId": 92_001, "recipe": { "mode": "manufacturing", "blueprintTypeId": 92_002 } }
        ],
        "fulfillmentScopes": [],
    });
    let (status, json) = post_graph(graph, parent_id, overlay).await;
    assert_eq!(status, StatusCode::OK);
    let node = prod_child(&json["root"]);
    let required = node["requiredQuantity"].as_i64().unwrap();
    let producing = node["producingQuantity"].as_i64().unwrap();
    let surplus = node["surplus"].as_i64().unwrap();
    assert_eq!(required, 250);
    assert_eq!(producing, 500); // 1 run x 500/run
    assert_eq!(surplus, 250); // 500 - 250
    assert!(producing >= required, "never under-produce the requirement");
    assert!(
        producing - required < 500,
        "surplus is strictly less than one lot (output/run)"
    );
}

#[tokio::test]
async fn create_linked_build_inherits_the_parents_market_scopes() {
    let owner_id = OwnerId::new();
    let mut parent =
        parent_build_resolving_hull_section_to_build(iskworks_core::WorkspaceId::new(), owner_id);
    let rens_scope = iskworks_core::MarketScope {
        region_id: 10_000_030,
        location_id: Some(60_004_588),
    };
    parent.draft_planning.as_mut().unwrap().input.material_scope = rens_scope;
    parent.draft_planning.as_mut().unwrap().input.output_scope = rens_scope;
    let parent_id = parent.id;
    let app = app_with_linked_build_fixture(parent, owner_id, Vec::new());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/builds/{}/linked-builds", parent_id.0))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"componentTypeId": 90001}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(
        body["draftPlanning"]["input"]["materialScope"]["regionId"],
        10_000_030
    );
    assert_eq!(
        body["draftPlanning"]["input"]["materialScope"]["locationId"],
        60_004_588
    );
    assert_eq!(
        body["draftPlanning"]["input"]["outputScope"]["regionId"],
        10_000_030
    );
}

#[tokio::test]
async fn create_linked_build_inherits_the_parents_own_facility_with_no_per_row_override() {
    let owner_id = OwnerId::new();
    let mut parent =
        parent_build_resolving_hull_section_to_build(iskworks_core::WorkspaceId::new(), owner_id);
    let facility_id = iskworks_core::FacilityProfileId::new();
    // The row itself (90_001) has no `facility_override` -- only the
    // parent's own shared manufacturing slot is set.
    parent
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .manufacturing_facility = Some(iskworks_core::FacilityPreviewCommand {
        facility_profile_id: facility_id,
        blueprint_me: 8,
        blueprint_te: 16,
        estimated_item_value: Some("500000".to_string()),
    });
    let parent_id = parent.id;
    // The parent's shared manufacturing slot must resolve now that the
    // linked-build sizing path derives the parent's authoritative
    // (ME/facility-adjusted) requirement -- a zero-bonus profile keeps the
    // resulting run count identical while still exercising inheritance.
    let mut profile = fixture_facility_profile(iskworks_core::FacilityRole::Manufacturing);
    profile.id = facility_id;
    let app = app_with_linked_build_fixture_facilities(
        parent,
        owner_id,
        Vec::new(),
        std::collections::HashMap::from([(facility_id, profile)]),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/builds/{}/linked-builds", parent_id.0))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"componentTypeId": 90001}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    let facility = &body["draftPlanning"]["input"]["manufacturingFacility"];
    assert_eq!(facility["facilityProfileId"], facility_id.0.to_string());
    assert!(
        facility.get("expectedProfileRevision").is_none(),
        "the vestigial revision field is no longer emitted: {facility}"
    );
    // The parent's own blueprint ME/TE and EIV are about its own job, not
    // the child's -- the child resolves those for itself.
    assert_eq!(facility["blueprintMe"], 0);
    assert_eq!(facility["blueprintTe"], 0);
    assert_eq!(facility["estimatedItemValue"], serde_json::Value::Null);
}

#[tokio::test]
async fn create_linked_build_rejects_when_component_is_not_build_resolved() {
    let owner_id = OwnerId::new();
    let mut parent =
        parent_build_resolving_hull_section_to_build(iskworks_core::WorkspaceId::new(), owner_id);
    parent.draft_planning = None;
    let parent_id = parent.id;
    let app = app_with_linked_build_fixture(parent, owner_id, Vec::new());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/builds/{}/linked-builds", parent_id.0))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"componentTypeId": 90001}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_build_leaves_parent_fields_null_for_a_build_with_no_parent() {
    let owner_id = OwnerId::new();
    let parent =
        parent_build_resolving_hull_section_to_build(iskworks_core::WorkspaceId::new(), owner_id);
    let parent_id = parent.id;
    let app = app_with_linked_build_fixture(parent, owner_id, Vec::new());

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/builds/{}", parent_id.0))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["parentBuildName"], serde_json::Value::Null);
}

#[tokio::test]
async fn list_builds_excludes_linked_builds() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    let parent_id = parent.id;
    let child = linked_child_build(workspace_id, owner_id, parent_id, 90_001, 2);
    let child_id = child.id;
    let app = app_with_linked_build_fixture(parent, owner_id, vec![child]);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/builds")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    let ids: Vec<String> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|build| build["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&parent_id.0.to_string()));
    assert!(!ids.contains(&child_id.0.to_string()));

    // Read-model extras for the Builds library reach the wire as camelCase.
    let parent_json = body
        .as_array()
        .unwrap()
        .iter()
        .find(|build| build["id"] == parent_id.0.to_string())
        .unwrap();
    assert_eq!(parent_json["productCategoryName"], "Ship");
    assert_eq!(parent_json["productGroupName"], "Battleship");
    assert_eq!(
        parent_json["selectedBlueprintOrigin"],
        serde_json::Value::Null
    );
    assert_eq!(parent_json["hasOwnedBlueprint"], true);
}

#[tokio::test]
async fn build_and_price_source_lists_have_explicit_empty_arrays() {
    for uri in ["/api/builds", "/api/price-sources"] {
        let response = app()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body.as_ref(), b"[]");
    }
}

#[tokio::test]
async fn optimistic_concurrency_conflict_is_structured() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/price-sources")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"name":"Home Market","description":"","isDefault":true}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["error"]["code"], "revision_conflict");
    assert_eq!(body["error"]["retryable"], false);
}

#[tokio::test]
async fn create_candidate_preview_route_requires_a_structured_candidate_body() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/build-plans/candidate-preview")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
