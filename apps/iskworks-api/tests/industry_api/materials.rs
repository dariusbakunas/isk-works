use super::*;

// ---------------------------------------------------------------------------
// Build Materials aggregate (`POST /api/builds/:id/materials`).
// Reuses the graph fixtures: same `FixtureIndustryRepository` /
// `FixtureSdeRepository` recipe chain (Rifter 6_830 -> Hull Section 90_002 ->
// Pyerite 91_002 -> Tritanium 34), so `34` is demanded by the root directly
// *and*, when the chain is Build-resolved, deep in the tree -- the
// double-count regression.
// ---------------------------------------------------------------------------

// -- A / H / I / J / Q ------------------------------------------------------

#[tokio::test]
async fn materials_all_buy_root_aggregates_direct_recipe_materials_with_one_balance_read() {
    let (parent, _ws, owner_id) = rifter_root(1);
    let parent_id = parent.id;
    let (app, inventory) = materials_app(parent, owner_id, Vec::new(), vec![(34, 60)]);

    // Overlay resolves nothing -> both root materials (34, 90_001) are Buy.
    let (status, json) = post_materials(app, parent_id, overlay(1, serde_json::json!([]))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["buildId"], parent_id.0.to_string());
    assert!(json["generatedAt"].is_string());

    let tritanium = row(&json, 34).unwrap();
    assert_eq!(tritanium["requiredQuantity"], 100);
    assert_eq!(tritanium["availableQuantity"], 60);
    assert_eq!(tritanium["allocatedQuantity"], 60);
    assert_eq!(tritanium["shortageQuantity"], 40);
    assert_eq!(tritanium["fullyCovered"], false);

    let hull = row(&json, 90_001).unwrap();
    assert_eq!(hull["requiredQuantity"], 2);
    assert_eq!(hull["allocatedQuantity"], 0);
    assert_eq!(hull["shortageQuantity"], 2);

    // Invariants + no double-count + one inventory read.
    for line in json["rows"].as_array().unwrap() {
        assert_eq!(
            line["requiredQuantity"].as_u64().unwrap(),
            line["allocatedQuantity"].as_u64().unwrap()
                + line["shortageQuantity"].as_u64().unwrap()
        );
    }
    assert_eq!(inventory.list_balances_call_count(), 1);
}

// -- A / C / K -- a fully inventory-covered Buy leaf is a VISIBLE covered row
#[tokio::test]
async fn materials_covered_buy_leaf_is_a_visible_covered_row() {
    // Exact cover: Tritanium (34, required 100, on-hand 100) -> row, shortage 0.
    let (parent, _ws, owner_id) = rifter_root(1);
    let parent_id = parent.id;
    let (app, _inv) = materials_app(parent, owner_id, Vec::new(), vec![(34, 100)]);
    let (_s, json) = post_materials(app, parent_id, overlay(1, serde_json::json!([]))).await;
    let trit = row(&json, 34).unwrap();
    assert_eq!(trit["requiredQuantity"], 100);
    assert_eq!(trit["allocatedQuantity"], 100);
    assert_eq!(trit["shortageQuantity"], 0);
    assert_eq!(trit["fullyCovered"], true);
    assert_eq!(trit["strategy"], "buy");
    assert_eq!(row(&json, 90_001).unwrap()["shortageQuantity"], 2);
    assert_eq!(
        node_alloc(&json, parent_id, 34).unwrap()["resolution"],
        "buy"
    );
    assert_materials_invariants(&json);

    // Excess -- take is still capped at `required`; row shows availability
    // distinct from planned use.
    let (parent, _ws, owner_id) = rifter_root(1);
    let parent_id = parent.id;
    let (app, _inv) = materials_app(parent, owner_id, Vec::new(), vec![(34, 5_000)]);
    let (_s, json) = post_materials(app, parent_id, overlay(1, serde_json::json!([]))).await;
    let trit = row(&json, 34).unwrap();
    assert_eq!(trit["availableQuantity"], 5_000);
    assert_eq!(trit["allocatedQuantity"], 100);
    assert_eq!(trit["fullyCovered"], true);
}

// -- B / C ----------------------------------------------------------------

#[tokio::test]
async fn materials_overlay_buy_to_build_replaces_the_item_with_the_childs_inputs() {
    let (parent, ws, owner_id) = rifter_root(1);
    let parent_id = parent.id;
    // Child produces Hull Section (90_001) from Pyerite (35); 2 runs for the
    // root's demand of 2.
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
    let child_id = child.id;
    let (app, _inv) = materials_app(parent, owner_id, vec![child], vec![]);

    let (status, json) = post_materials(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);

    // The Build intermediate is visible AND its Buy inputs appear one level down.
    let hull = row(&json, 90_001).unwrap();
    assert_eq!(hull["requiredQuantity"], 2);
    assert_eq!(hull["strategy"], "build");
    assert_eq!(row(&json, 35).unwrap()["requiredQuantity"], 100);
    assert_eq!(row(&json, 35).unwrap()["strategy"], "buy");
    assert_eq!(row(&json, 34).unwrap()["requiredQuantity"], 100);
    assert_materials_invariants(&json);

    // Provenance: Pyerite is owned by the child node, one hop from the root.
    let pyerite_source = json["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["typeId"] == 35)
        .unwrap();
    assert_eq!(pyerite_source["buildId"], child_id.0.to_string());
    assert_eq!(
        pyerite_source["graphNodeId"],
        format!("build:{}", child_id.0)
    );
    assert_eq!(pyerite_source["provisional"], false);
    assert_eq!(pyerite_source["treePath"], serde_json::json!([90_001]));
}

// -- E / F / G -- the shared-material double-count regression -------------

// Item 13's central regression -- the exact failure topology that broke a
// real Muninn build at runs=4: a root Build-resolved intermediate (Hull
// Section) configured via an already-*captured* ObservedAsset selection
// whose physical blueprint has since vanished (sold/moved/desynced). At N
// the intermediate is fully covered by inventory and pruned -- the deep
// chain, and the blueprint selection itself, are never even inspected. At
// N+1 inventory falls short, the walker dynamically resizes and
// RE-PREVIEWS the intermediate's own linked Build, and that re-preview
// must succeed using the frozen ME/TE -- never touching the vanished
// observation, and never gated on its (irrelevant, since it's gone
// entirely) licensed-run count.
#[tokio::test]
async fn materials_muninn_shaped_boundary_survives_a_vanished_observed_blueprint_at_the_activation_run_count(
) {
    let missing_observation_id = uuid::Uuid::new_v4();
    let captured_selection = iskworks_core::BlueprintSelection::ObservedAsset {
        observation_id: missing_observation_id,
        kind: iskworks_core::BlueprintKind::Copy,
        material_efficiency: 10,
        time_efficiency: 20,
        licensed_runs: None,
    };

    // N: root needs 2 Hull Sections (2/run * 1 run); 2 on hand fully covers
    // it -> pruned. Same shape as
    // `materials_covered_build_intermediate_is_visible_but_child_pruned`'s
    // case (A), plus the vanished-blueprint dimension, which must have zero
    // effect here since the boundary is never walked.
    let (parent_n, mut linked_n, parent_id, owner_id) = nested_chain(1);
    linked_n[0]
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .blueprint_selection = Some(captured_selection.clone());
    let (app_n, _inv_n) = materials_app(parent_n, owner_id, linked_n, vec![(90_001, 2)]);
    let (status_n, json_n) =
        post_materials(app_n, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status_n, StatusCode::OK);
    let hull_n = row(&json_n, 90_001).unwrap();
    assert_eq!(hull_n["shortageQuantity"], 0);
    assert_eq!(hull_n["fullyCovered"], true);
    assert!(
        row(&json_n, 35).is_none(),
        "pruned: deep chain never walked"
    );

    // N+1 (root runs 3): root now needs 6 Hull Sections; the same 2 on hand
    // falls short by 4 -> the intermediate activates, dynamically resized
    // to 4 runs (its own persisted runs is 2 -- deliberately different, so
    // a stale/persisted figure couldn't accidentally pass this assertion).
    let (parent_n1, mut linked_n1, parent_id_n1, owner_id_n1) = nested_chain(1);
    linked_n1[0]
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .blueprint_selection = Some(captured_selection);
    let (app_n1, _inv_n1) = materials_app(parent_n1, owner_id_n1, linked_n1, vec![(90_001, 2)]);
    let (status_n1, json_n1) =
        post_materials(app_n1, parent_id_n1, overlay(3, hull_section_built())).await;
    assert_eq!(
        status_n1,
        StatusCode::OK,
        "activating a Build-resolved intermediate whose observed blueprint has \
         vanished must not fail the projection -- this is the exact class of \
         bug that broke Muninn at runs=4"
    );

    let hull_alloc = node_alloc(&json_n1, parent_id_n1, 90_001).unwrap();
    assert_eq!(hull_alloc["shortageQuantity"], 4);
    assert_eq!(
        hull_alloc["childRuns"], 4,
        "dynamically resized, not the persisted 2"
    );

    // ME10-derived Pyerite demand (50 * 4 runs * 9/10 = 180) -- proves the
    // frozen ME actually drove the child's own recursive re-preview, not
    // just a reported field.
    assert_eq!(row(&json_n1, 35).unwrap()["requiredQuantity"], 180);
    assert_materials_invariants(&json_n1);
}

#[tokio::test]
async fn materials_nested_build_aggregates_a_shared_type_across_root_and_deep_descendant() {
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let (app, _inv) = materials_app(parent, owner_id, linked, vec![]);

    let (status, json) = post_materials(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);

    // 34 demanded by the root (100) + the Pyerite grandchild (10 * 100 runs).
    assert_eq!(row(&json, 34).unwrap()["requiredQuantity"], 1_100);
    // The Build intermediates are visible rows in their own right.
    assert_eq!(row(&json, 90_001).unwrap()["strategy"], "build");
    assert_eq!(row(&json, 35).unwrap()["strategy"], "build");
    assert_eq!(row(&json, 35).unwrap()["requiredQuantity"], 100);
    assert_materials_invariants(&json);
}

#[tokio::test]
async fn materials_shared_type_is_allocated_once_across_the_tree() {
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let root_id = parent.id;
    let pyerite_child_id = linked[1].id;
    let (app, _inv) = materials_app(parent, owner_id, linked, vec![(34, 700)]);

    let (status, json) = post_materials(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);

    let tritanium = row(&json, 34).unwrap();
    assert_eq!(tritanium["requiredQuantity"], 1_100);
    assert_eq!(tritanium["availableQuantity"], 700);
    assert_eq!(
        tritanium["allocatedQuantity"], 700,
        "the 700 units are used once, not twice"
    );
    assert_eq!(tritanium["shortageQuantity"], 400);

    // Deterministic DFS pre-order: the root's own 34 draws first (100),
    // the grandchild takes the remaining 600 (shortage 400).
    assert_eq!(
        node_alloc(&json, root_id, 34).unwrap()["allocatedQuantity"],
        100
    );
    assert_eq!(
        node_alloc(&json, root_id, 34).unwrap()["shortageQuantity"],
        0
    );
    assert_eq!(
        node_alloc(&json, pyerite_child_id, 34).unwrap()["allocatedQuantity"],
        600
    );
    assert_eq!(
        node_alloc(&json, pyerite_child_id, 34).unwrap()["shortageQuantity"],
        400
    );

    // Sum of per-node allocations == the aggregate.
    let per_node: u64 = json["nodeAllocations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["typeId"] == 34)
        .map(|a| a["allocatedQuantity"].as_u64().unwrap())
        .sum();
    assert_eq!(per_node, 700);
}

// -- K / O / P ----------------------------------------------------------

#[tokio::test]
async fn materials_response_is_deterministic_across_repeated_requests() {
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let (app_a, _a) = materials_app(parent.clone(), owner_id, linked.clone(), vec![(34, 700)]);
    let (app_b, _b) = materials_app(parent, owner_id, linked, vec![(34, 700)]);

    let (_s, first) = post_materials(app_a, parent_id, overlay(1, hull_section_built())).await;
    let (_s, again) = post_materials(app_b, parent_id, overlay(1, hull_section_built())).await;

    assert_eq!(first["rows"], again["rows"]);
    assert_eq!(first["nodeAllocations"], again["nodeAllocations"]);
    assert_eq!(first["sources"], again["sources"]);
}

#[tokio::test]
async fn materials_changing_inventory_changes_allocation_not_required_demand() {
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let (app_full, _f) = materials_app(parent.clone(), owner_id, linked.clone(), vec![(34, 700)]);
    let (app_none, _n) = materials_app(parent, owner_id, linked, vec![]);

    let (_s, full) = post_materials(app_full, parent_id, overlay(1, hull_section_built())).await;
    let (_s, none) = post_materials(app_none, parent_id, overlay(1, hull_section_built())).await;

    assert_eq!(row(&full, 34).unwrap()["requiredQuantity"], 1_100);
    assert_eq!(row(&none, 34).unwrap()["requiredQuantity"], 1_100);
    assert_eq!(row(&full, 34).unwrap()["allocatedQuantity"], 700);
    assert_eq!(row(&none, 34).unwrap()["allocatedQuantity"], 0);
    assert_eq!(row(&none, 34).unwrap()["shortageQuantity"], 1_100);
}

// -- K -- Full scope transports through the overlay + never draws the pool

/// `PreviewBuildPlanCommand.fulfillmentScopes` -> `normalize_draft_planning`
/// -> `BuildTreeNode.draft_planning.input.fulfillment_scopes` ->
/// `NodePlanMaterials::from_revision` -> the allocating traversal -> API.
///
/// Root Tritanium (34) is `Full` (required 100 * 40 = 4,000); a second
/// Tritanium demand appears deep in the tree at `Missing` scope. The Full
/// contribution -- visited first in DFS pre-order -- must draw nothing,
/// leaving the whole 10,000 pool for the Missing demand.
///
/// The deep demand is now *dynamically sized*: root needs 80 Hull Sections
/// -> hull child at 80 runs -> 4,000 Pyerite -> Pyerite child at 4,000 runs
/// -> 40,000 Tritanium (Missing). So 34 aggregates to 4,000 + 40,000 =
/// 44,000; the pool covers 10,000 of the Missing 40,000. The persisted
/// Pyerite-child `runs` (600) is irrelevant.
#[tokio::test]
async fn materials_full_scope_contribution_never_consumes_the_pool() {
    let workspace_id = iskworks_core::WorkspaceId::new();
    let owner_id = OwnerId::new();
    // Root at 40 runs -> direct Tritanium demand 100 * 40 = 4,000.
    let mut parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    parent.runs = 40;
    let parent_id = parent.id;

    let hull_child = linked_child_with_draft(
        workspace_id,
        owner_id,
        parent_id,
        90_001,
        80,
        captured_recipe(
            90_002,
            &[(35, "Pyerite", 50)],
            (90_001, "Rifter Hull Section", 1),
        ),
        vec![resolution(35, 91_002)],
    );
    let hull_child_id = hull_child.id;
    // Persisted runs 600 -> the Pyerite grandchild's own revision demands
    // Tritanium 10 * 600 = 6,000 (Missing -- no fulfillment-scope override).
    let pyerite_child = linked_child_with_draft(
        workspace_id,
        owner_id,
        hull_child_id,
        35,
        600,
        captured_recipe(91_002, &[(34, "Tritanium", 10)], (35, "Pyerite", 1)),
        Vec::new(),
    );
    let pyerite_child_id = pyerite_child.id;

    let (app, _inv) = materials_app(
        parent,
        owner_id,
        vec![hull_child, pyerite_child],
        vec![(34, 10_000)],
    );

    // Overlay: Build-resolve 90_001, and mark the root's own Tritanium `Full`.
    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 40,
        "pricingSelections": [],
        "componentResolutions": [
            {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
        ],
        "fulfillmentScopes": [{"typeId": 34, "scope": "full"}],
    });
    let (status, json) = post_materials(app, parent_id, body).await;
    assert_eq!(status, StatusCode::OK);

    let tritanium = row(&json, 34).unwrap();
    assert_eq!(tritanium["requiredQuantity"], 44_000);
    assert_eq!(tritanium["availableQuantity"], 10_000);
    assert_eq!(tritanium["allocatedQuantity"], 10_000);
    assert_eq!(tritanium["shortageQuantity"], 34_000);
    assert_eq!(tritanium["fullyCovered"], false);

    let root_alloc = node_alloc(&json, parent_id, 34).unwrap();
    assert_eq!(root_alloc["scope"], "full");
    assert_eq!(
        root_alloc["allocatedQuantity"], 0,
        "Full never draws the pool"
    );
    assert_eq!(root_alloc["shortageQuantity"], 4_000);

    let deep_alloc = node_alloc(&json, pyerite_child_id, 34).unwrap();
    assert_eq!(deep_alloc["scope"], "missing");
    assert_eq!(
        deep_alloc["allocatedQuantity"], 10_000,
        "the Missing demand gets the whole pool"
    );
    assert_eq!(deep_alloc["shortageQuantity"], 30_000);

    // Source-level evidence carries the scope too.
    let source_scopes: std::collections::BTreeMap<String, &serde_json::Value> = json["sources"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["typeId"] == 34)
        .map(|s| (s["scope"].as_str().unwrap().to_string(), s))
        .collect();
    assert_eq!(source_scopes["full"]["allocatedQuantity"], 0);
    assert_eq!(source_scopes["missing"]["allocatedQuantity"], 10_000);
}

// -- L -- unresolved Build slot -----------------------------------------

#[tokio::test]
async fn materials_unresolved_build_slot_is_provisional_demand_with_a_warning() {
    let (parent, _ws, owner_id) = rifter_root(1);
    let parent_id = parent.id;
    // Overlay Build-resolves 90_001 but no linked build exists for it.
    let (app, _inv) = materials_app(parent, owner_id, Vec::new(), vec![]);

    let (status, json) = post_materials(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "an unresolved slot is a warning, not an error"
    );

    let hull = row(&json, 90_001).unwrap();
    assert_eq!(hull["requiredQuantity"], 2);
    let hull_source = json["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["typeId"] == 90_001)
        .unwrap();
    assert_eq!(hull_source["provisional"], true);
    assert!(json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w["code"] == "unresolvedBuild" && w["typeId"] == 90_001));
    // No fabricated descendant (Pyerite) demand.
    assert!(row(&json, 35).is_none());
}

// -- M -- a node with no reconstructable revision -> curated 422 --------

#[tokio::test]
async fn materials_missing_node_revision_returns_a_curated_incomplete_error() {
    let (parent, ws, owner_id) = rifter_root(1);
    let parent_id = parent.id;
    // `linked_child_build` has `draft_planning: None` -> its per-node
    // revision cannot be reconstructed.
    let child = linked_child_build(ws, owner_id, parent_id, 90_001, 2);
    let (app, _inv) = materials_app(parent, owner_id, vec![child], vec![]);

    let (status, json) = post_materials(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json["error"]["code"], "build_materials_incomplete");
    assert_eq!(json["error"]["retryable"], false);
    assert!(
        json.get("rows").is_none(),
        "no partial aggregate is returned"
    );
    // No SQL / path / debug leak.
    let message = json["error"]["message"].as_str().unwrap().to_lowercase();
    for marker in ["sqlx", "relation", "constraint", "buildid", "src/"] {
        assert!(!message.contains(marker), "leaked {marker:?}: {message}");
    }
}

// -- N -- post-modifier quantity + Graph parity ------------------------

#[tokio::test]
async fn materials_quantities_match_the_graph_for_the_same_overlay() {
    // A non-trivial run count so the quantities aren't 1:1 with the recipe.
    let (parent, _ws, owner_id) = rifter_root(3);
    let parent_id = parent.id;

    let (materials, _inv) = materials_app(parent.clone(), owner_id, Vec::new(), vec![]);
    let graph = graph_app(
        parent,
        owner_id,
        Vec::new(),
        std::collections::BTreeMap::new(),
    );

    let (ms, mjson) = post_materials(materials, parent_id, overlay(3, serde_json::json!([]))).await;
    let (gs, gjson) = post_graph(graph, parent_id, overlay(3, serde_json::json!([]))).await;
    assert_eq!(ms, StatusCode::OK);
    assert_eq!(gs, StatusCode::OK);

    // The Graph's root acquisition child for Tritanium and the Materials row
    // for Tritanium are both the same root `BuildPlanRevision` line.
    let graph_tritanium = gjson["root"]["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["typeId"] == 34)
        .unwrap();
    assert_eq!(
        row(&mjson, 34).unwrap()["requiredQuantity"],
        graph_tritanium["requiredQuantity"]
    );
    assert_eq!(row(&mjson, 34).unwrap()["requiredQuantity"], 300);
}

// -- R -- the overlay drives; nothing is persisted --------------------

#[tokio::test]
async fn materials_overlay_never_persists_the_sourcing_change() {
    let (parent, _ws, owner_id) = rifter_root(1);
    let parent_id = parent.id;
    let (app, _inv) = materials_app(parent, owner_id, Vec::new(), vec![]);

    // First: overlay Buy-resolves everything (differs from the persisted
    // draft, which Build-resolves 90_001).
    let (_s, first) =
        post_materials(app.clone(), parent_id, overlay(1, serde_json::json!([]))).await;
    assert!(
        row(&first, 90_001).is_some(),
        "overlay wins: 90_001 is a Buy row"
    );

    // Again, same overlay -> identical (no persisted mutation between calls).
    let (_s, again) = post_materials(app, parent_id, overlay(1, serde_json::json!([]))).await;
    assert_eq!(first["rows"], again["rows"]);
}
