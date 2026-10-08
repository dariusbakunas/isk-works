use super::*;

// ===================================================================
// Build Graph market-evidence coherence  (Part A)
// ===================================================================
//
// Within one graph request every node on a scope prices from one
// completed evidence batch, resolved once up front -- a refresh landing
// mid-projection can't move a later node.

fn money(v: &str) -> iskworks_core::Money {
    iskworks_core::Money::parse(v).unwrap()
}

fn evidence_app(
    repo: std::sync::Arc<FixtureIndustryRepository>,
    owner_id: OwnerId,
) -> axum::Router {
    build_router(
        AppState::new(std::sync::Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(repo)
        .with_sde_repository(std::sync::Arc::new(FixtureSdeRepository))
        .with_inventory_repository(std::sync::Arc::new(EmptyInventoryRepository))
        .with_production_repository(std::sync::Arc::new(FixtureProductionRepository {
            type_id: 0,
            available_to_this_build: 0,
            average_historical_unit_cost: None,
        })),
    )
}

/// root Rifter -> linked Hull Section (90_002, ME 10) -> linked Pyerite
/// (91_002, ME 5). One market scope (Jita) throughout.
fn evidence_chain(
    workspace_id: iskworks_core::WorkspaceId,
    owner_id: OwnerId,
) -> (Build, Vec<Build>) {
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
    if let iskworks_core::BuildRecipe::Manufacturing(recipe) = &mut pyerite.recipe {
        recipe.products[0].type_id = 35;
        recipe.products[0].type_name = "Pyerite".to_string();
    }
    fixture_relink(&pyerite, 35);
    (parent, vec![hull, pyerite])
}

fn seed_live(repo: &FixtureIndustryRepository) {
    for (id, name, price) in [
        (34, "Tritanium", "5.0000"),
        (35, "Pyerite", "9.0000"),
        (90_001, "Rifter Hull Section", "60.0000"),
        (5_876, "Rifter", "100000.0000"),
    ] {
        repo.set_live_price(price_item(id, name, price));
    }
}

#[tokio::test]
async fn graph_response_carries_the_market_evidence_it_valued_against() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let (parent, linked) = evidence_chain(workspace_id, owner_id);
    let parent_id = parent.id;
    let repo = std::sync::Arc::new(FixtureIndustryRepository {
        build: Some(parent),
        linked_builds: std::sync::Mutex::new(linked),
        ..Default::default()
    });
    seed_live(&repo);

    let (status, json) = post_graph(
        evidence_app(repo.clone(), owner_id),
        parent_id,
        overlay(
            1,
            serde_json::json!([
                {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
            ]),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let evidence = json["marketEvidence"].as_array().unwrap();
    assert_eq!(evidence.len(), 1, "one scope -> one evidence entry");
    assert!(evidence[0]["asOf"].is_string());
    assert!(!evidence[0]["observationBatchIds"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(evidence[0]["scope"]["regionId"].is_number());

    // The scope was resolved exactly once for the whole 3-level graph.
    assert_eq!(repo.resolved_evidence_scopes().len(), 1);
}

#[tokio::test]
async fn every_graph_node_on_one_scope_shares_one_evidence_resolution() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let (parent, linked) = evidence_chain(workspace_id, owner_id);
    let parent_id = parent.id;
    let repo = std::sync::Arc::new(FixtureIndustryRepository {
        build: Some(parent),
        linked_builds: std::sync::Mutex::new(linked),
        ..Default::default()
    });
    seed_live(&repo);

    let (_, json) = post_graph(
        evidence_app(repo.clone(), owner_id),
        parent_id,
        overlay(
            1,
            serde_json::json!([
                {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
            ]),
        ),
    )
    .await;

    // root + Hull Section + Pyerite all price against Jita -> resolved once.
    let scopes = repo.resolved_evidence_scopes();
    assert_eq!(scopes.len(), 1);
    assert_eq!(scopes[0], iskworks_core::DEFAULT_MARKET_SCOPE);
    // None of these nodes has a facility, so every one is `costState:
    // incomplete` (installation unknown). Consuming a child whose own total
    // production cost is unknown makes the *parent's* material component
    // cost unknown too, cascading all the way to the root; only the
    // deepest node -- whose own boundaries are plain Buy leaves (Tritanium)
    // -- has a known material component cost, proving the one shared
    // evidence resolution actually priced every leaf.
    assert_eq!(
        json["root"]["costState"], "incomplete",
        "{:?}",
        json["root"]
    );
    assert!(json["root"]["materialComponentCost"].is_null());
    let hull = prod_child(&json["root"]);
    assert_eq!(hull["costState"], "incomplete", "{hull:?}");
    assert!(hull["materialComponentCost"].is_null());
    let pyerite = prod_child(hull);
    assert_eq!(pyerite["costState"], "incomplete", "{pyerite:?}");
    assert!(!pyerite["materialComponentCost"].is_null());
}

#[tokio::test]
async fn different_scopes_legitimately_resolve_different_evidence() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let (parent, mut linked) = evidence_chain(workspace_id, owner_id);
    let parent_id = parent.id;
    // Put the deep Pyerite build on a different market scope.
    let other_scope = iskworks_core::MarketScope {
        region_id: 10_000_043,
        location_id: Some(60_008_494),
    };
    if let Some(input) = linked[1].draft_planning.as_mut() {
        input.input.material_scope = other_scope;
        input.input.output_scope = other_scope;
    }
    let repo = std::sync::Arc::new(FixtureIndustryRepository {
        build: Some(parent),
        linked_builds: std::sync::Mutex::new(linked),
        ..Default::default()
    });
    seed_live(&repo);

    let (_, json) = post_graph(
        evidence_app(repo.clone(), owner_id),
        parent_id,
        overlay(
            1,
            serde_json::json!([
                {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
            ]),
        ),
    )
    .await;

    let mut scopes = repo.resolved_evidence_scopes();
    scopes.sort_by_key(|scope| scope.region_id);
    scopes.dedup();
    assert_eq!(scopes.len(), 2, "Jita for root/Hull, the other for Pyerite");
    assert_eq!(json["marketEvidence"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn a_refresh_completing_mid_projection_cannot_move_a_later_node_price() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let overlay_body = || {
        overlay(
            1,
            serde_json::json!([
                {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
            ]),
        )
    };

    // Control: no mid-request refresh.
    let (parent, linked) = evidence_chain(workspace_id, owner_id);
    let parent_id = parent.id;
    let control = std::sync::Arc::new(FixtureIndustryRepository {
        build: Some(parent),
        linked_builds: std::sync::Mutex::new(linked),
        ..Default::default()
    });
    seed_live(&control);
    let (_, control_json) =
        post_graph(evidence_app(control, owner_id), parent_id, overlay_body()).await;

    // Same fixture, but Tritanium jumps 5 -> 500 right after the first node
    // is priced.
    let (parent, linked) = evidence_chain(workspace_id, owner_id);
    let parent_id = parent.id;
    let refreshed = std::sync::Arc::new(FixtureIndustryRepository {
        build: Some(parent),
        linked_builds: std::sync::Mutex::new(linked),
        ..Default::default()
    });
    seed_live(&refreshed);
    refreshed.arm_mid_request_bump(price_item(34, "Tritanium", "500.0000"));
    let (_, json) = post_graph(
        evidence_app(refreshed.clone(), owner_id),
        parent_id,
        overlay_body(),
    )
    .await;

    // Every node's cost is byte-identical to the control -- the mid-request
    // price move never reached a pinned read.
    let refreshed_pyerite = &prod_child(prod_child(&json["root"]));
    let control_pyerite = &prod_child(prod_child(&control_json["root"]));
    assert_eq!(
        refreshed_pyerite["estimatedCost"],
        control_pyerite["estimatedCost"]
    );
    // No facility on this node -> installation unknown -> `incomplete`,
    // but still byte-identical to the control (the pinned read never
    // observed the mid-request price move).
    assert_eq!(refreshed_pyerite["costState"], "incomplete");
    assert_eq!(
        refreshed_pyerite["materialComponentCost"],
        control_pyerite["materialComponentCost"]
    );
    assert_eq!(
        json["root"]["estimatedCost"],
        control_json["root"]["estimatedCost"]
    );
    assert_eq!(
        prod_child(&json["root"])["estimatedCost"],
        prod_child(&control_json["root"])["estimatedCost"],
    );
    // Proof the bump really happened -- it just didn't reach the pinned read.
    assert_eq!(refreshed.live_price(34), Some(money("500.0000")));
}

#[tokio::test]
async fn manual_and_mixed_pricing_selections_survive_evidence_pinning() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let mut parent = rifter_parent(workspace_id, owner_id);
    let parent_id = parent.id;
    // A linked Hull Section that manually prices Pyerite at 3.0 and leaves
    // Tritanium (via a nested build) to the market.
    let mut hull = hull_section_linked_with_draft(
        workspace_id,
        owner_id,
        parent_id,
        10,
        90_002,
        pyerite_50(),
        Some(manual_bpc(10, 20)),
        Vec::new(),
    );
    if let Some(draft) = hull.draft_planning.as_mut() {
        draft.input.pricing_selections = vec![iskworks_core::ItemPricingSelectionInput {
            type_id: 35,
            role: iskworks_core::PlannerItemRole::Material,
            selection: iskworks_core::ItemPricingSelection::Manual {
                unit_price: "3.0000".to_string(),
            },
        }];
    }
    let _ = &mut parent;
    let repo = std::sync::Arc::new(FixtureIndustryRepository {
        build: Some(parent),
        linked_builds: std::sync::Mutex::new(vec![hull]),
        ..Default::default()
    });
    seed_live(&repo);
    // Even if the market moves, the manual price is authoritative.
    repo.arm_mid_request_bump(price_item(35, "Pyerite", "999.0000"));

    let (_, json) = post_graph(
        evidence_app(repo.clone(), owner_id),
        parent_id,
        overlay(
            1,
            serde_json::json!([
                {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
            ]),
        ),
    )
    .await;

    let hull_node = prod_child(&json["root"]);
    // Root need 2/run * 1 = 2 -> child's PROJECTED runs is 2, not its
    // persisted 10. qty 90 (50/run * 2 runs * ME10) at the *manual* 3.0 =
    // 270 -- never the mid-request market bump to 999.
    assert_eq!(hull_node["runs"], 2);
    assert_eq!(hull_node["persistedRuns"], 10);
    assert_eq!(line_qty(hull_node, "buyMaterials", 35), 90);
    // No facility on this node -> `estimatedCost` is incomplete; the
    // materials-only figure still reflects the manual price.
    assert!(hull_node["estimatedCost"].is_null());
    assert_eq!(hull_node["materialComponentCost"], "270.0000");
    assert_eq!(repo.live_price(35), Some(money("999.0000")));
}

#[tokio::test]
async fn graph_and_candidate_preview_match_when_pinned_to_the_same_evidence() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    // A level-1 linked Hull Section that Build-resolves nothing -- its
    // materials are plain buys, so a stand-alone candidate-preview can
    // reconcile it exactly.
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
        Vec::new(),
    );
    let repo = std::sync::Arc::new(FixtureIndustryRepository {
        build: Some(parent),
        linked_builds: std::sync::Mutex::new(vec![hull]),
        ..Default::default()
    });
    seed_live(&repo);

    let (_, graph) = post_graph(
        evidence_app(repo.clone(), owner_id),
        parent_id,
        overlay(
            1,
            serde_json::json!([
                {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
            ]),
        ),
    )
    .await;
    let graph_child = prod_child(&graph["root"]);
    // No facility on this node -> `estimatedCost` (total production cost)
    // is incomplete; pin the parity check to `materialComponentCost`
    // instead, and to the child's own PROJECTED runs (root need
    // is 2/run * 1 = 2, not the linked build's persisted 20).
    let graph_cost = graph_child["materialComponentCost"].clone();
    let child_runs = graph_child["runs"].as_u64().expect("child runs");
    assert_eq!(child_runs, 2, "root need 2/run * 1 run");
    let evidence = graph["marketEvidence"].clone();
    assert_ne!(graph_cost, serde_json::Value::String("0.0000".to_string()));
    assert_ne!(graph_cost, serde_json::Value::Null);

    // The market moves after the graph was valued. A candidate-preview
    // *pinned to the graph's evidence* must still match it exactly.
    repo.set_live_price(price_item(35, "Pyerite", "42.0000"));

    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002},
        "runs": child_runs,
        "pricingSelections": [],
        "blueprintSelection": {
            "mode": "manual", "kind": "copy",
            "materialEfficiency": 10, "timeEfficiency": 20,
            "licensedRuns": 100000, "notes": ""
        },
        "marketEvidence": evidence,
    });
    let response = evidence_app(repo.clone(), owner_id)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/build-plans/candidate-preview")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let ws: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();

    assert_eq!(
        ws["candidate"]["estimatedMaterialCost"], graph_cost,
        "pinned candidate-preview == graph node cost, despite the price move"
    );

    // And unpinned it would NOT match (proves the pin is what aligned them).
    let unpinned_body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002},
        "runs": child_runs,
        "pricingSelections": [],
        "blueprintSelection": {
            "mode": "manual", "kind": "copy",
            "materialEfficiency": 10, "timeEfficiency": 20,
            "licensedRuns": 100000, "notes": ""
        },
    });
    let unpinned = evidence_app(repo.clone(), owner_id)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/build-plans/candidate-preview")
                .header("content-type", "application/json")
                .body(Body::from(unpinned_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let unpinned: serde_json::Value =
        serde_json::from_slice(&to_bytes(unpinned.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_ne!(unpinned["candidate"]["estimatedMaterialCost"], graph_cost);
}
