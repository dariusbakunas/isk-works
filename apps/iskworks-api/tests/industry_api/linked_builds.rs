use super::*;

#[tokio::test]
async fn create_linked_build_sizes_runs_off_the_shortage_for_a_missing_scoped_row() {
    let owner_id = OwnerId::new();
    let mut parent =
        parent_build_resolving_hull_section_to_build(iskworks_core::WorkspaceId::new(), owner_id);
    parent
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .fulfillment_scopes = vec![FulfillmentScopeOverride {
        type_id: 90_001,
        scope: FulfillmentScope::Missing,
    }];
    let parent_id = parent.id;

    let app = build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            linked_builds: std::sync::Mutex::new(Vec::new()),
            price_source: None,
            market_items: std::collections::BTreeMap::new(),
            facility_profiles: std::collections::HashMap::new(),
            blueprint_observations: std::collections::HashMap::new(),
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        .with_inventory_repository(Arc::new(EmptyInventoryRepository))
        .with_production_repository(Arc::new(FixtureProductionRepository {
            type_id: 90_001,
            // Demand is 2 (root runs 1 x 2 per run); 1 already available,
            // so the linked build should be sized to the shortage (1 run),
            // not the full requirement (2 runs).
            available_to_this_build: 1,
            average_historical_unit_cost: None,
        })),
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
    assert_eq!(body["runs"], 1);
}

#[tokio::test]
async fn create_linked_build_sizes_runs_off_the_shortage_by_default_with_no_explicit_scope() {
    let owner_id = OwnerId::new();
    // No `fulfillment_scopes` entry at all for 90_001 -- `Missing` is now
    // the default, so this should size off the shortage exactly like the
    // explicitly-`Missing`-scoped case above.
    let parent =
        parent_build_resolving_hull_section_to_build(iskworks_core::WorkspaceId::new(), owner_id);
    let parent_id = parent.id;

    let app = build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            linked_builds: std::sync::Mutex::new(Vec::new()),
            price_source: None,
            market_items: std::collections::BTreeMap::new(),
            facility_profiles: std::collections::HashMap::new(),
            blueprint_observations: std::collections::HashMap::new(),
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        .with_inventory_repository(Arc::new(EmptyInventoryRepository))
        .with_production_repository(Arc::new(FixtureProductionRepository {
            type_id: 90_001,
            available_to_this_build: 1,
            average_historical_unit_cost: None,
        })),
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
    assert_eq!(body["runs"], 1);
}

// --- Linked-build run sizing: authoritative parent demand + resync -------
//
// The parent recipe (6_830) needs 90_001 x2/run; the child recipe (90_002)
// produces 90_001 x1/run. So the child's correct run count is exactly the
// parent's *authoritative* (ME/facility-adjusted) requirement for 90_001.
// At 100 parent runs: ME 0 -> 200, ME 10 -> ceil(200 x 0.9) = 180.

// --- Live-Build FacilityProfile revision model ---------------------------
//
// A FacilityProfile is mutable; a Build references one by id and always
// calculates against the profile's CURRENT settings. Editing the facility
// (and older persisted drafts that still carry an `expectedProfileRevision`
// key, now ignored) must not make the Build -- or a linked child -- stale,
// unreadable, or need re-saving.

#[tokio::test]
async fn worksheet_preview_ignores_a_legacy_expected_profile_revision_and_uses_current_settings() {
    let facility_id = iskworks_core::FacilityProfileId::new();
    // Profile is live at 10% material reduction.
    let profiles = std::collections::HashMap::from([(
        facility_id,
        manufacturing_facility_at(facility_id, 8, 10),
    )]);
    let app = app_with_price_source_sde_and_facilities(empty_manual_price_source(), profiles);

    // An older client still includes the removed `expectedProfileRevision`
    // key in the facility slot; serde must accept and ignore it.
    let json = post_preview(
        app,
        serde_json::json!({
            "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
            "runs": 1,
            "pricingSelections": [],
            "manufacturingFacility": {
                "facilityProfileId": facility_id.0,
                "expectedProfileRevision": 7,
                "blueprintMe": 0,
                "blueprintTe": 0,
                "estimatedItemValue": null
            },
        }),
    )
    .await;

    // Tritanium is 100/run; ceil(100 * 0.90) = 90 proves the current profile
    // drove the calculation (a raw 100 would mean the facility was ignored).
    let tritanium = json["materialLines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|line| line["typeId"] == 34)
        .unwrap();
    assert_eq!(tritanium["totalQuantity"], 90);
}

#[tokio::test]
async fn graph_linked_child_is_fully_enriched_after_a_facility_edit() {
    // The graph's per-child `snapshot_material_revision(...).ok()` silently
    // drops a child's cost + Buy-quantity enrichment when facility
    // resolution fails. With the revision guard gone it cannot fail on a
    // stale persisted revision, so the child stays fully enriched.
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let facility_id = iskworks_core::FacilityProfileId::new();

    // Parent Build-resolves 90_001; its shared slot references the facility.
    let mut parent = rifter_parent(workspace_id, owner_id);
    parent
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .manufacturing_facility = Some(iskworks_core::FacilityPreviewCommand {
        facility_profile_id: facility_id,
        blueprint_me: 0,
        blueprint_te: 0,
        estimated_item_value: None,
    });
    let parent_id = parent.id;

    // Linked Hull Section child (Pyerite 50/run), persisted runs 180, with
    // its OWN shared slot referencing the same facility -- this is the slot
    // the graph reads back via its per-child snapshot.
    let mut child = hull_section_linked_with_draft(
        workspace_id,
        owner_id,
        parent_id,
        180,
        90_002,
        pyerite_50(),
        None,
        Vec::new(),
    );
    child
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .manufacturing_facility = Some(iskworks_core::FacilityPreviewCommand {
        facility_profile_id: facility_id,
        blueprint_me: 0,
        blueprint_te: 0,
        estimated_item_value: None,
    });

    // Live profile: revision 8, 10% material reduction.
    let profiles = std::collections::HashMap::from([(
        facility_id,
        manufacturing_facility_at(facility_id, 8, 10),
    )]);
    // Root overlay carries no facility (keeps the root job off the ESI EIV
    // path); the stale slot under test is the CHILD's own persisted one,
    // which the graph reads back via its per-child `snapshot_material_revision`.
    let (status, json) = post_graph(
        graph_app_with_facilities(parent, owner_id, vec![child], chain_prices(), profiles),
        parent_id,
        overlay(
            100,
            serde_json::json!([
                {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
            ]),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json:?}");

    let child_node = prod_child(&json["root"]);
    // Root carries no facility in this overlay, so its own Hull Section
    // requirement is unreduced (2/run * 100 = 200) -- the child's PROJECTED
    // runs (`runs`/`producingQuantity`, never the persisted
    // value) size to meet exactly that, with the persisted 180 surfacing
    // only as `persistedRuns` (informational, see `RunsDiverged`).
    assert_eq!(child_node["producingQuantity"], 200, "projected runs * 1");
    assert_eq!(child_node["persistedRuns"], 180);
    // Not silently discarded: pre-fix the child's `snapshot_material_revision`
    // hit `resolve_facility_profile(child_facility, rev 7)` -> RevisionConflict
    // -> `.ok()` -> None, leaving the node with no cost and no Buy quantities.
    // No adjusted-price repository is wired in this fixture, so installation
    // (EIV) is genuinely unknown -- `costState` reflects total production
    // cost, but the materials-only figure is still fully enriched.
    assert_eq!(child_node["costState"], "incomplete", "{child_node:?}");
    assert!(child_node["estimatedCost"].is_null());
    assert!(
        child_node["materialComponentCost"].is_string(),
        "child material cost missing -> snapshot was silently dropped: {child_node:?}"
    );
    // Child's own rev-8 facility: Pyerite 50/run * 200 runs * 0.90 = 9000
    // (a raw 10000 would mean the facility was ignored rather than resolved).
    assert_eq!(line_qty(child_node, "buyMaterials", 35), 9_000);
}
