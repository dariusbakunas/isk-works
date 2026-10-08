use super::*;

// Item 9's invariant, and the Manual-mode twin of the same latent bug: a
// blueprint's `licensedRuns` is execution-readiness evidence, never a
// planning-validity gate. Previously, `capture_blueprint_snapshot_
// for_preview` validated licensed-runs sufficiency against the Build's real
// (here, dynamically large) run count -- so a Manual BPC assumption with
// fewer licensed runs than the current run count would have hard-failed
// this exact preview with `CopyRunsInsufficient`, the same failure class
// that broke a real Muninn build via its `ObservedAsset` child. Root-level
// `Manual` never depended on ESI, so this proves the fix isn't scoped only
// to `ObservedAsset`.
#[tokio::test]
async fn candidate_preview_is_not_blocked_by_a_manual_blueprints_licensed_runs_falling_short_of_the_current_run_count(
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

    // 200 runs requested; the Manual BPC assumption names only 10 licensed
    // runs. Planning must still calculate all 200 runs' worth of materials.
    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 200,
        "pricingSelections": [],
        "blueprintSelection": {
            "mode": "manual",
            "kind": "copy",
            "materialEfficiency": 10,
            "timeEfficiency": 20,
            "licensedRuns": 10,
            "notes": "",
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

    assert_eq!(response.status(), StatusCode::OK);
    let json: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    // Root's own Tritanium demand scales with the full 200 runs (100/run),
    // ME10-adjusted (9/10) -- proving materials were actually computed for
    // 200 runs, not silently capped to the 10 licensed ones.
    let tritanium = json["candidate"]["materialLines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|line| line["typeId"] == 34)
        .unwrap();
    assert_eq!(tritanium["totalQuantity"], 100 * 200 * 9 / 10);
}

// Multi-BPC job split: 5 runs on 1-run copies are 5 jobs, each rounded on
// its own. Rifter Hull Section is 2/run at ME10: one 5-run job would need
// ceil(10 x 0.9) = 9, five 1-run jobs need 5 x ceil(2 x 0.9) = 10. The
// planned duration is one (parallel) job: 600 s x 0.8 (TE20).
#[tokio::test]
async fn candidate_preview_rounds_materials_per_bpc_job() {
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
    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 5,
        "pricingSelections": [],
        "blueprintSelection": {
            "mode": "manual",
            "kind": "copy",
            "materialEfficiency": 10,
            "timeEfficiency": 20,
            "licensedRuns": 1,
            "notes": "",
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

    assert_eq!(response.status(), StatusCode::OK);
    let json: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    let line = |type_id: i64| {
        json["candidate"]["materialLines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|line| line["typeId"] == type_id)
            .unwrap()
            .clone()
    };
    assert_eq!(line(90_001)["totalQuantity"], 10);
    assert_eq!(line(34)["totalQuantity"], 450);
    assert_eq!(
        json["candidate"]["blueprint"]["plannedDurationSeconds"],
        480
    );
}

#[tokio::test]
async fn candidate_preview_honors_component_resolutions_but_leaves_build_resolved_cost_unknown_without_a_linked_build(
) {
    let source_id = PriceSourceId::new();
    let price_source = PriceSource {
        id: source_id,
        workspace_id: iskworks_core::WorkspaceId::new(),
        name: "Home Market".to_string(),
        description: String::new(),
        kind: PriceSourceKind::Manual,
        revision: 1,
        item_count: 4,
        recent_build_count: 0,
        items: vec![
            price_item(34, "Tritanium", "4.1250"),
            price_item(35, "Pyerite", "8.5000"),
            price_item(90_001, "Rifter Hull Section", "50.0000"),
            price_item(5_876, "Rifter", "100000.0000"),
        ],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "manualPriceListId": source_id,
        "expectedManualPriceListRevision": 1,
        "pricingSelections": [],
        "facility": null,
        "componentResolutions": [
            {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
        ]
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

    assert_eq!(response.status(), StatusCode::OK);
    let json: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();

    let material_lines = json["candidate"]["materialLines"].as_array().unwrap();
    // Worksheets are single-level: the root's raw materials (Tritanium +
    // Rifter Hull Section) stay exactly two rows -- Tritanium a buy leaf,
    // Rifter Hull Section a build-resolved row -- and Hull Section's own
    // material (Pyerite) never appears as a new row here (it lives only on
    // Hull Section's own linked build's own worksheet).
    assert_eq!(material_lines.len(), 2);

    let tritanium = material_lines
        .iter()
        .find(|line| line["typeId"] == 34)
        .unwrap();
    assert_eq!(tritanium["totalQuantity"], 100);
    assert_eq!(
        tritanium["contributions"],
        serde_json::json!([{"parentTypeId": null, "parentTypeName": "Rifter", "quantity": 100}])
    );

    let hull_section = material_lines
        .iter()
        .find(|line| line["typeId"] == 90_001)
        .unwrap();
    // demand 2 (root needs 2), 1 per run -> 2 runs, produced 2, no surplus.
    assert_eq!(hull_section["totalQuantity"], 2);
    assert_eq!(
        hull_section["contributions"],
        serde_json::json!([{"parentTypeId": null, "parentTypeName": "Rifter", "quantity": 2}])
    );
    assert_eq!(hull_section["isBuildResolved"], true);
    // No facility of either kind was selected -- installation cost stays
    // unknown, not silently zero.
    assert_eq!(hull_section["installationCost"], serde_json::Value::Null);
    // This request has no buildId (a brand-new, unsaved candidate can't
    // have any linked children yet) -- a build-resolved row's own cost
    // only ever comes from its own linked build now, never a market price
    // (that would misrepresent what actually building it costs), so with
    // no linked build to ask, it stays unknown too.
    assert_eq!(hull_section["missing"], true);
    assert_eq!(hull_section["unitPrice"], serde_json::Value::Null);
    assert_eq!(hull_section["lineTotal"], serde_json::Value::Null);

    assert_eq!(json["candidate"]["pricingComplete"], false);
    // Only Tritanium's own market price is known: 100*4.1250 = 412.50.
    // Hull Section's own cost is missing, so it contributes nothing here.
    assert_eq!(json["candidate"]["estimatedMaterialCost"], "412.5000");
}

#[tokio::test]
async fn candidate_preview_prices_a_buy_resolved_root_material_from_an_order_book_source_alongside_a_build_resolved_one(
) {
    // An order-book-backed source (ESI/eve client export) derives prices
    // per-request via `derive_market_price_items`, unlike a Manual source
    // (whose `.items` are a static, pre-populated list). Proves order-book
    // pricing still works correctly for a plain Buy-resolved root material
    // (Tritanium) even when another root material (Rifter Hull Section) is
    // build-resolved in the same request -- and that the build-resolved
    // one correctly stays unpriced (no linked build to ask, and a
    // build-resolved row never uses a market/order-book price regardless).
    //
    // Uses `EveClientMarketExport` rather than `EsiMarketOrders` so this
    // stays a fast, fake-repository-backed test: `EsiMarketOrders` sources
    // also register live ESI market coverage (`register_selection_market_coverage`
    // in routes/builds.rs, requiring a real `PublicMarketService`), which is
    // exercised separately by the ignored Postgres/ESI-backed suite.
    let source_id = PriceSourceId::new();
    let price_source = PriceSource {
        id: source_id,
        workspace_id: iskworks_core::WorkspaceId::new(),
        name: "Jita 4-4".to_string(),
        description: String::new(),
        kind: PriceSourceKind::EveClientMarketExport,
        revision: 1,
        item_count: 0,
        recent_build_count: 0,
        items: Vec::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let market_items = std::collections::BTreeMap::from([
        (34, price_item(34, "Tritanium", "4.1250")),
        (35, price_item(35, "Pyerite", "8.5000")),
        (90_001, price_item(90_001, "Rifter Hull Section", "50.0000")),
        (5_876, price_item(5_876, "Rifter", "100000.0000")),
    ]);

    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "manualPriceListId": source_id,
        "expectedManualPriceListRevision": 1,
        "pricingSelections": [],
        "facility": null,
        "componentResolutions": [
            {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
        ]
    })
    .to_string();

    let response = app_with_order_book_price_source_and_sde(price_source, market_items)
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

    let status = response.status();
    let json: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(status, StatusCode::OK, "{json:?}");

    let material_lines = json["candidate"]["materialLines"].as_array().unwrap();
    let tritanium = material_lines
        .iter()
        .find(|line| line["typeId"] == 34)
        .unwrap();
    assert_eq!(tritanium["missing"], false);
    assert_eq!(tritanium["unitPrice"], "4.1250");

    let hull_section = material_lines
        .iter()
        .find(|line| line["typeId"] == 90_001)
        .unwrap();
    assert_eq!(hull_section["isBuildResolved"], true);
    assert_eq!(hull_section["missing"], true);
    assert_eq!(hull_section["unitPrice"], serde_json::Value::Null);

    assert_eq!(json["candidate"]["pricingComplete"], false);
}

/// The core proof for "a Build-resolved row's cost comes from its own
/// linked build": Rifter Hull Section (90_001) has an active linked build whose
/// own draft resolves live to a real material cost (Pyerite, priced from
/// its own price source) -- previewing the parent (`buildId` in the
/// request) must show Hull Section's own row costed from that live
/// recomputation, not a market price for Hull Section itself (none is even
/// configured in this fixture's price source, proving it's genuinely not
/// being used).
#[tokio::test]
async fn candidate_preview_of_an_existing_build_costs_a_build_resolved_row_from_its_own_linked_builds_live_material_cost(
) {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let source_id = PriceSourceId::new();
    let price_source = PriceSource {
        id: source_id,
        workspace_id,
        name: "Home Market".to_string(),
        description: String::new(),
        kind: PriceSourceKind::Manual,
        revision: 1,
        item_count: 2,
        recent_build_count: 0,
        items: vec![
            price_item(34, "Tritanium", "4.1250"),
            price_item(35, "Pyerite", "8.5000"),
            // Deliberately no price for 90_001 (Rifter Hull Section) itself
            // -- proves its row's cost can only be coming from the linked
            // build's own recomputation, not a market lookup that doesn't
            // even have a price to fall back on.
        ],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    let parent_id = parent.id;
    // The child's own `total_production_cost` (and therefore this
    // row's cost) needs the child's own installation to be *complete* --
    // give it a real facility (`fixture_facility_profile` carries a 5%
    // system cost index, everything else zero) rather than leaving
    // installation unconfigured (which correctly reports the row as
    // incomplete rather than silently treating installation as free).
    let child_facility_id = iskworks_core::FacilityProfileId::new();
    let child_facility = fixture_facility_profile(iskworks_core::FacilityRole::Manufacturing);
    let mut child = linked_child_build(workspace_id, owner_id, parent_id, 90_001, 2);
    child.draft_planning = Some(iskworks_core::DraftPlanningSnapshot {
        input: iskworks_core::DraftPlanningInput {
            material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            manual_price_list_id: Some(source_id),
            expected_manual_price_list_revision: Some(1),
            material_pricing_policy: iskworks_core::MarketPricingPolicy::HighestBuy,
            output_pricing_policy: iskworks_core::MarketPricingPolicy::LowestSell,
            pricing_selections: Vec::new(),
            blueprint_selection: None,
            manufacturing_facility: Some(iskworks_core::FacilityPreviewCommand {
                facility_profile_id: child_facility_id,
                blueprint_me: 0,
                blueprint_te: 0,
                estimated_item_value: None,
            }),
            reaction_facility: None,
            facility_eiv_manual: false,
            component_resolutions: Vec::new(),
            fulfillment_scopes: Vec::new(),
        },
        updated_at: chrono::Utc::now(),
    });

    let app = build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            linked_builds: std::sync::Mutex::new(vec![child]),
            price_source: Some(price_source),
            market_items: std::collections::BTreeMap::new(),
            facility_profiles: std::collections::HashMap::from([(
                child_facility_id,
                child_facility,
            )]),
            blueprint_observations: std::collections::HashMap::new(),
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        .with_inventory_repository(Arc::new(EmptyInventoryRepository))
        // `Missing` is the default fulfillment scope now, so a preview with
        // a `buildId` always needs a working `coverage()` lookup, even when
        // the test isn't exercising fulfillment scope itself.
        .with_production_repository(Arc::new(FixtureProductionRepository {
            type_id: 0,
            available_to_this_build: 0,
            average_historical_unit_cost: None,
        }))
        // The child operation's EIV = adjusted_price[35] * 50/run *
        // 2 runs. Pyerite's adjusted price happens to equal its market price
        // here purely to keep the arithmetic simple -- the two are resolved
        // from entirely independent repositories in `BuildCostProjection`.
        .with_adjusted_price_repository(Arc::new(FixtureAdjustedPriceRepository {
            prices: std::collections::BTreeMap::from([(35, rust_decimal::Decimal::new(85, 1))]),
        })),
    );

    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "manualPriceListId": source_id,
        "expectedManualPriceListRevision": 1,
        "pricingSelections": [],
        "buildId": parent_id.0,
        "componentResolutions": [
            {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
        ]
    })
    .to_string();

    let response = app
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

    let status = response.status();
    let json: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(status, StatusCode::OK, "{json:?}");

    let material_lines = json["candidate"]["materialLines"].as_array().unwrap();
    let tritanium = material_lines
        .iter()
        .find(|line| line["typeId"] == 34)
        .unwrap();
    assert_eq!(tritanium["unitPrice"], "4.1250");
    // 100 * 4.1250 = 412.50
    assert_eq!(tritanium["lineTotal"], "412.5000");

    let hull_section = material_lines
        .iter()
        .find(|line| line["typeId"] == 90_001)
        .unwrap();
    assert_eq!(hull_section["isBuildResolved"], true);
    assert_eq!(hull_section["missing"], false);
    // The linked child's own *total production cost* (material +
    // its own installation, never the parent's), consumed in full since the
    // child was dynamically sized to produce exactly what the parent needs
    // (2 == 2, no surplus): child material 100 Pyerite * 8.5000 = 850.0000;
    // child EIV = 8.5000 * 50/run * 2 runs = 850.0000; child installation =
    // 850.0000 * 5% system index = 42.5000; child total = 892.5000.
    assert_eq!(hull_section["lineTotal"], "892.5000");
    // A Build/Reaction row carries no synthetic "unit price" --
    // there is no single market price for a self-produced item. The
    // equivalent evidence is `planningEvidence.childUnitProductionCost`.
    assert_eq!(hull_section["unitPrice"], serde_json::Value::Null);
    let evidence = &hull_section["planningEvidence"];
    assert_eq!(evidence["childProducedQuantity"], 2);
    assert_eq!(evidence["childConsumedQuantity"], 2);
    assert_eq!(evidence["childUnitProductionCost"], "446.2500"); // 892.5 / 2
    assert_eq!(evidence["childConsumedCost"], "892.5000");
    assert_eq!(evidence["childSurplusQuantity"], 0);
    assert_eq!(evidence["childSurplusRetainedBasis"], "0.0000");

    // The root's own installation is still unconfigured (this test only
    // wires a facility for the *child*), so the overall candidate stays
    // incomplete -- but every boundary's own cost (what this test actually
    // proves) is fully known regardless.
    assert_eq!(json["candidate"]["pricingComplete"], false);
    // Tritanium (412.50) + Hull Section's own total production cost
    // (892.50) -- never the child's installation counted a second time on
    // top of that, and never the whole child job charged if there were
    // surplus (there isn't, here).
    assert_eq!(json["candidate"]["estimatedMaterialCost"], "1305.0000");
}

/// **Canonical surplus-display regression**: a
/// Build/Reaction row must never show the user the full child job cost when
/// the child overproduces -- only its *consumed* share.
///
/// (The companion installation-double-count guarantee is proven
/// exhaustively at the pure `BuildCostProjection` level in
/// `crates/iskworks-core/src/build_cost/tests.rs`'s
/// `child_installation_counted_once_and_only_in_child` -- root R and child C
/// at *different* facilities/indices, asserting `root.total_production_cost
/// == child_consumed_cost + R`, never `+ C` again. Reproducing that at the
/// HTTP layer would additionally need a root facility selected *and* a
/// build-resolved component sharing its slot, which routes through a facility-rig "effective requirements"
/// recalculation this fixture set doesn't model; the pure unit test is the
/// right layer for the arithmetic guarantee regardless.)
///
/// Otherwise identical to
/// `candidate_preview_of_an_existing_build_costs_a_build_resolved_row_from_its_own_linked_builds_live_material_cost`
/// (same price source, same child facility/adjusted-price setup, same 50
/// Pyerite/run), except the child blueprint here produces 500 Hull
/// Sections/run instead of 1 -- the parent's 2 required are covered by a
/// single dynamically-sized run (never the 2 runs the 1-per-run case
/// needed), producing an unavoidable 498-unit surplus. Persisted child
/// `runs` is deliberately a stale 5, proving that number is never used
/// either.
#[tokio::test]
async fn candidate_preview_shows_only_the_consumed_share_of_a_surplus_producing_child() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let source_id = PriceSourceId::new();
    let price_source = PriceSource {
        id: source_id,
        workspace_id,
        name: "Home Market".to_string(),
        description: String::new(),
        kind: PriceSourceKind::Manual,
        revision: 1,
        item_count: 2,
        recent_build_count: 0,
        items: vec![
            price_item(34, "Tritanium", "4.1250"),
            price_item(35, "Pyerite", "8.5000"),
        ],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    let parent_id = parent.id;
    let child_facility_id = iskworks_core::FacilityProfileId::new();
    let child_facility = fixture_facility_profile(iskworks_core::FacilityRole::Manufacturing);
    // Child blueprint produces 500 Hull Sections/run from 50 Pyerite/run --
    // the parent's 2 required are covered by a *single* dynamically-sized
    // run, producing an unavoidable 498-unit surplus.
    let mut child = linked_child_build(workspace_id, owner_id, parent_id, 90_001, 5);
    let iskworks_core::BuildRecipe::Manufacturing(recipe) = &mut child.recipe else {
        unreachable!("linked_child_build always returns a Manufacturing recipe")
    };
    recipe.products[0].quantity_per_run = 500;
    child.draft_planning = Some(iskworks_core::DraftPlanningSnapshot {
        input: iskworks_core::DraftPlanningInput {
            material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            manual_price_list_id: Some(source_id),
            expected_manual_price_list_revision: Some(1),
            material_pricing_policy: iskworks_core::MarketPricingPolicy::HighestBuy,
            output_pricing_policy: iskworks_core::MarketPricingPolicy::LowestSell,
            pricing_selections: Vec::new(),
            blueprint_selection: None,
            manufacturing_facility: Some(iskworks_core::FacilityPreviewCommand {
                facility_profile_id: child_facility_id,
                blueprint_me: 0,
                blueprint_te: 0,
                estimated_item_value: None,
            }),
            reaction_facility: None,
            facility_eiv_manual: false,
            component_resolutions: Vec::new(),
            fulfillment_scopes: Vec::new(),
        },
        updated_at: chrono::Utc::now(),
    });

    let app = build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            linked_builds: std::sync::Mutex::new(vec![child]),
            price_source: Some(price_source),
            market_items: std::collections::BTreeMap::new(),
            facility_profiles: std::collections::HashMap::from([(
                child_facility_id,
                child_facility,
            )]),
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
        .with_adjusted_price_repository(Arc::new(FixtureAdjustedPriceRepository {
            prices: std::collections::BTreeMap::from([(35, rust_decimal::Decimal::new(85, 1))]),
        })),
    );

    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "manualPriceListId": source_id,
        "expectedManualPriceListRevision": 1,
        "pricingSelections": [],
        "buildId": parent_id.0,
        "componentResolutions": [
            {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
        ]
    })
    .to_string();

    let response = app
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

    let status = response.status();
    let json: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(status, StatusCode::OK, "{json:?}");

    let material_lines = json["candidate"]["materialLines"].as_array().unwrap();
    let hull_section = material_lines
        .iter()
        .find(|line| line["typeId"] == 90_001)
        .unwrap();
    assert_eq!(hull_section["isBuildResolved"], true);
    assert_eq!(hull_section["missing"], false);
    // Required 2 needs only 1 dynamically-sized run (500/run >= 2), never
    // the 2 runs the 1-per-run case needed: child material 50 Pyerite *
    // 8.5000 = 425.0000; child EIV = 8.5000 * 50/run * 1 run = 425.0000;
    // child installation = 425.0000 * 5% = 21.2500; child total = 446.2500.
    // Produced 500, consumed 2 (the parent's requirement) -- this row must
    // show only 2/500 of that: round(446.25 * 2 / 500, 4) = 1.7850. Never
    // the full 446.2500 child job.
    assert_eq!(hull_section["lineTotal"], "1.7850");
    assert_eq!(hull_section["unitPrice"], serde_json::Value::Null);
    let evidence = &hull_section["planningEvidence"];
    assert_eq!(evidence["childProducedQuantity"], 500);
    assert_eq!(evidence["childConsumedQuantity"], 2);
    assert_eq!(evidence["childUnitProductionCost"], "0.8925"); // 446.25 / 500
    assert_eq!(evidence["childConsumedCost"], "1.7850");
    assert_eq!(evidence["childSurplusQuantity"], 498);
    assert_eq!(evidence["childSurplusRetainedBasis"], "444.4650"); // 446.25 - 1.785
}

/// **InvalidRecipe regression** (root facility + fully-costed
/// Build-resolved component): `POST /build-plans/candidate-preview` must not
/// 400 with `recipe is invalid` on a candidate where `pricingComplete` is
/// `true` while a Build/Reaction row's `unitPrice` is `null` (the correct,
/// deliberate representation of a self-produced component's cost). Also
/// exercises, in one fixture: root and child facilities at *different* cost
/// indices, and a deliberately stale persisted child `runs` (5) that dynamic
/// sizing must override to 2. Rig behavior gets its own dedicated test below
/// (`..._with_a_root_rig_configured`) rather than hand-computed numbers here,
/// since a rig's material reduction folds into the facility's own reported
/// requirement quantities upstream of `calculation_evidence` -- not worth
/// re-deriving by hand in this fixture.
#[tokio::test]
async fn candidate_preview_succeeds_with_root_facility_and_fully_costed_build_resolved_component() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let source_id = PriceSourceId::new();
    let price_source = PriceSource {
        id: source_id,
        workspace_id,
        name: "Home Market".to_string(),
        description: String::new(),
        kind: PriceSourceKind::Manual,
        revision: 1,
        item_count: 2,
        recent_build_count: 0,
        items: vec![
            price_item(34, "Tritanium", "4.1250"),
            price_item(35, "Pyerite", "8.5000"),
        ],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    let parent_id = parent.id;
    let root_facility_id = iskworks_core::FacilityProfileId::new();
    let root_facility = iskworks_core::IndustryFacilityProfile {
        manual_system_cost_index: Some(rust_decimal::Decimal::new(10, 2)), // 10%, different from the child's 5%.
        ..fixture_facility_profile(iskworks_core::FacilityRole::Manufacturing)
    };
    let child_facility_id = iskworks_core::FacilityProfileId::new();
    // Default fixture index is 5% -- deliberately different from the root's
    // 10% above, so a mix-up between parent/child facility evidence would be
    // visible in the resulting numbers.
    let child_facility = fixture_facility_profile(iskworks_core::FacilityRole::Manufacturing);
    // Persisted runs (5) deliberately stale -- dynamic sizing must still
    // size the child to the 2 the parent actually needs.
    let mut child = linked_child_build(workspace_id, owner_id, parent_id, 90_001, 5);
    child.draft_planning = Some(iskworks_core::DraftPlanningSnapshot {
        input: iskworks_core::DraftPlanningInput {
            material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            manual_price_list_id: Some(source_id),
            expected_manual_price_list_revision: Some(1),
            material_pricing_policy: iskworks_core::MarketPricingPolicy::HighestBuy,
            output_pricing_policy: iskworks_core::MarketPricingPolicy::LowestSell,
            pricing_selections: Vec::new(),
            blueprint_selection: None,
            manufacturing_facility: Some(iskworks_core::FacilityPreviewCommand {
                facility_profile_id: child_facility_id,
                blueprint_me: 0,
                blueprint_te: 0,
                estimated_item_value: None,
            }),
            reaction_facility: None,
            facility_eiv_manual: false,
            component_resolutions: Vec::new(),
            fulfillment_scopes: Vec::new(),
        },
        updated_at: chrono::Utc::now(),
    });

    let app = build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            linked_builds: std::sync::Mutex::new(vec![child]),
            price_source: Some(price_source),
            market_items: std::collections::BTreeMap::new(),
            facility_profiles: std::collections::HashMap::from([
                (root_facility_id, root_facility),
                (child_facility_id, child_facility),
            ]),
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
        .with_adjusted_price_repository(Arc::new(FixtureAdjustedPriceRepository {
            prices: std::collections::BTreeMap::from([
                (34, rust_decimal::Decimal::new(413, 2)),
                (90_001, rust_decimal::Decimal::new(500, 2)),
                (35, rust_decimal::Decimal::new(85, 1)),
            ]),
        })),
    );

    // The root's facility comes from *this* (unsaved-overlay) request, same
    // as any other live-preview field -- not from the parent's own persisted
    // `draftPlanning` snapshot (which carries none for this fixture at all).
    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "manualPriceListId": source_id,
        "expectedManualPriceListRevision": 1,
        "pricingSelections": [],
        "buildId": parent_id.0,
        "manufacturingFacility": {
            "facilityProfileId": root_facility_id.0,
            "blueprintMe": 0,
            "blueprintTe": 0,
            "estimatedItemValue": "1"
        },
        "componentResolutions": [
            {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
        ]
    })
    .to_string();

    let response = app
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

    let status = response.status();
    let json: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    // Before the fix: 400 `{"error":{"code":"validation_failed","message":"recipe is invalid"}}`.
    assert_eq!(status, StatusCode::OK, "{json:?}");
    assert_eq!(json["candidate"]["pricingComplete"], true);

    let material_lines = json["candidate"]["materialLines"].as_array().unwrap();
    let hull_section = material_lines
        .iter()
        .find(|line| line["typeId"] == 90_001)
        .unwrap();
    assert_eq!(hull_section["isBuildResolved"], true);
    // Never a synthesized per-unit price for a self-produced row -- the
    // `unitPrice: null` semantic holds here too.
    assert_eq!(hull_section["unitPrice"], serde_json::Value::Null);
    assert!(hull_section["lineTotal"].is_string());
    assert_eq!(hull_section["lineTotal"], "892.5000"); // 2/2 consumed, no surplus.
    assert!(hull_section["planningEvidence"].is_object());
    assert_eq!(
        hull_section["planningEvidence"]["childUnitProductionCost"],
        "446.2500"
    );

    // `calculationEvidence` exists and is internally consistent: the
    // Build-resolved row's 892.5000 held constant across all three stages
    // (never repriced by the root's own ME/structure discount, which is
    // exactly 1.0/1.0 here -- blueprintMe 0, root material_reduction_percent
    // 0 -- so this also confirms the *Buy* row (Tritanium, 412.5000) is
    // unaffected).
    let material_cost = &json["calculationEvidence"]["materialCost"];
    assert_eq!(material_cost["complete"], true);
    assert_eq!(material_cost["baseMarketValue"], "1305.0000"); // 412.5 + 892.5
    assert_eq!(material_cost["afterBlueprintMe"], "1305.0000");
    assert_eq!(material_cost["afterStructure"], "1305.0000");
    assert_eq!(material_cost["adjustedMaterialCost"], "1305.0000");
}

/// **InvalidRecipe fix regression** (root rig + Build-resolved component):
/// closes the investigation's earlier mistaken "facility-rig recalculation"
/// attribution -- a rig on the root facility has no bearing on this bug (the
/// trigger is purely `pricingComplete: true` + a `null` `unitPrice`), and
/// candidate-preview must succeed with one configured. Doesn't hand-compute
/// the Buy row's exact rig-reduced quantity (that arithmetic lives entirely
/// upstream, in the facility's own requirement computation, untouched by
/// this fix) -- only that the Build-resolved row's own contribution stays
/// held constant across all three evidence stages regardless.
#[tokio::test]
async fn candidate_preview_succeeds_with_a_root_rig_configured_and_a_build_resolved_component() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let source_id = PriceSourceId::new();
    let price_source = PriceSource {
        id: source_id,
        workspace_id,
        name: "Home Market".to_string(),
        description: String::new(),
        kind: PriceSourceKind::Manual,
        revision: 1,
        item_count: 2,
        recent_build_count: 0,
        items: vec![
            price_item(34, "Tritanium", "4.1250"),
            price_item(35, "Pyerite", "8.5000"),
        ],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    let parent_id = parent.id;
    let root_facility_id = iskworks_core::FacilityProfileId::new();
    let root_facility = iskworks_core::IndustryFacilityProfile {
        rigs: vec![iskworks_core::FacilityRig {
            slot_number: 1,
            type_id: 46_640,
            type_name: "Standup L-Set Basic Material Efficiency I".to_string(),
            material_reduction_percent: rust_decimal::Decimal::new(2, 0), // 2%.
            time_reduction_percent: rust_decimal::Decimal::ZERO,
            applicability: Default::default(),
        }],
        ..fixture_facility_profile(iskworks_core::FacilityRole::Manufacturing)
    };
    let child_facility_id = iskworks_core::FacilityProfileId::new();
    let child_facility = fixture_facility_profile(iskworks_core::FacilityRole::Manufacturing);
    let mut child = linked_child_build(workspace_id, owner_id, parent_id, 90_001, 2);
    child.draft_planning = Some(iskworks_core::DraftPlanningSnapshot {
        input: iskworks_core::DraftPlanningInput {
            material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            manual_price_list_id: Some(source_id),
            expected_manual_price_list_revision: Some(1),
            material_pricing_policy: iskworks_core::MarketPricingPolicy::HighestBuy,
            output_pricing_policy: iskworks_core::MarketPricingPolicy::LowestSell,
            pricing_selections: Vec::new(),
            blueprint_selection: None,
            manufacturing_facility: Some(iskworks_core::FacilityPreviewCommand {
                facility_profile_id: child_facility_id,
                blueprint_me: 0,
                blueprint_te: 0,
                estimated_item_value: None,
            }),
            reaction_facility: None,
            facility_eiv_manual: false,
            component_resolutions: Vec::new(),
            fulfillment_scopes: Vec::new(),
        },
        updated_at: chrono::Utc::now(),
    });

    let app = build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            linked_builds: std::sync::Mutex::new(vec![child]),
            price_source: Some(price_source),
            market_items: std::collections::BTreeMap::new(),
            facility_profiles: std::collections::HashMap::from([
                (root_facility_id, root_facility),
                (child_facility_id, child_facility),
            ]),
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
        .with_adjusted_price_repository(Arc::new(FixtureAdjustedPriceRepository {
            prices: std::collections::BTreeMap::from([
                (34, rust_decimal::Decimal::new(413, 2)),
                (90_001, rust_decimal::Decimal::new(500, 2)),
                (35, rust_decimal::Decimal::new(85, 1)),
            ]),
        })),
    );

    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "manualPriceListId": source_id,
        "expectedManualPriceListRevision": 1,
        "pricingSelections": [],
        "buildId": parent_id.0,
        "manufacturingFacility": {
            "facilityProfileId": root_facility_id.0,
            "blueprintMe": 0,
            "blueprintTe": 0,
            "estimatedItemValue": "1"
        },
        "componentResolutions": [
            {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
        ]
    })
    .to_string();

    let response = app
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

    let status = response.status();
    let json: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(status, StatusCode::OK, "{json:?}");
    assert_eq!(json["candidate"]["pricingComplete"], true);

    let material_lines = json["candidate"]["materialLines"].as_array().unwrap();
    let hull_section = material_lines
        .iter()
        .find(|line| line["typeId"] == 90_001)
        .unwrap();
    assert_eq!(hull_section["unitPrice"], serde_json::Value::Null);
    // The Build-resolved row's own cost is unaffected by the root's rig --
    // it's the child's own job, priced from the child's own facility.
    assert_eq!(hull_section["lineTotal"], "892.5000");

    let material_cost = &json["calculationEvidence"]["materialCost"];
    assert_eq!(material_cost["complete"], true);
    assert_eq!(material_cost["rigMultiplier"], "0.98");
    // The Build-resolved row's 892.5000 contribution is identical at every
    // stage regardless of whatever the rig does to the Buy row's own
    // quantity -- this is what this fix actually guarantees.
    let base = material_cost["baseMarketValue"]
        .as_str()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap();
    let after_me = material_cost["afterBlueprintMe"]
        .as_str()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap();
    let after_structure = material_cost["afterStructure"]
        .as_str()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap();
    // blueprintMe is 0 here, so base and afterBlueprintMe coincide; only
    // afterStructure can differ, by however much the rig reduces the Buy
    // row's own reported base quantity -- always by less than one whole
    // Tritanium unit's price (4.125), since the rig is only 2%.
    assert_eq!(base, after_me);
    assert!((after_structure - base).abs() < 4.125);
}

/// **InvalidRecipe fix regression**: a Build-resolved component whose linked
/// child has *no* facility at all must leave the candidate incomplete
/// (`pricingComplete: false`) -- never crash with `InvalidRecipe`. This is
/// `stage_cost`'s pre-existing `!pricing_complete -> Ok(None)` short-circuit,
/// confirmed still reachable after the fix.
#[tokio::test]
async fn candidate_preview_stays_incomplete_not_invalid_when_build_child_has_no_facility() {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let source_id = PriceSourceId::new();
    let price_source = PriceSource {
        id: source_id,
        workspace_id,
        name: "Home Market".to_string(),
        description: String::new(),
        kind: PriceSourceKind::Manual,
        revision: 1,
        item_count: 1,
        recent_build_count: 0,
        items: vec![price_item(34, "Tritanium", "4.1250")],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    let parent_id = parent.id;
    let root_facility_id = iskworks_core::FacilityProfileId::new();
    let root_facility = fixture_facility_profile(iskworks_core::FacilityRole::Manufacturing);
    // Linked child exists (with an explicit, walkable draft) but has no
    // facility configured at all -- its own installation, and therefore its
    // own `total_production_cost`, stays incomplete.
    let mut child = linked_child_build(workspace_id, owner_id, parent_id, 90_001, 2);
    child.draft_planning = Some(iskworks_core::DraftPlanningSnapshot {
        input: iskworks_core::DraftPlanningInput {
            material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            manual_price_list_id: Some(source_id),
            expected_manual_price_list_revision: Some(1),
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
        updated_at: chrono::Utc::now(),
    });

    let app = build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            linked_builds: std::sync::Mutex::new(vec![child]),
            price_source: Some(price_source),
            market_items: std::collections::BTreeMap::new(),
            facility_profiles: std::collections::HashMap::from([(root_facility_id, root_facility)]),
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
        .with_adjusted_price_repository(Arc::new(FixtureAdjustedPriceRepository {
            prices: std::collections::BTreeMap::from([(34, rust_decimal::Decimal::new(413, 2))]),
        })),
    );

    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "manualPriceListId": source_id,
        "expectedManualPriceListRevision": 1,
        "pricingSelections": [],
        "buildId": parent_id.0,
        "manufacturingFacility": {
            "facilityProfileId": root_facility_id.0,
            "blueprintMe": 0,
            "blueprintTe": 0,
            "estimatedItemValue": "1"
        },
        "componentResolutions": [
            {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
        ]
    })
    .to_string();

    let response = app
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

    let status = response.status();
    let json: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(status, StatusCode::OK, "{json:?}");
    assert_eq!(json["candidate"]["pricingComplete"], false);
    assert_eq!(
        json["calculationEvidence"]["materialCost"]["complete"],
        false
    );
    assert_eq!(
        json["calculationEvidence"]["materialCost"]["baseMarketValue"],
        serde_json::Value::Null
    );
}

/// **InvalidRecipe fix regression**: a Reaction-resolved component (not
/// Manufacturing) with a fully-costed linked child must succeed exactly like
/// the Manufacturing case -- `is_build_resolved` (what the fix actually
/// checks) makes no distinction between the two resolution kinds.
#[tokio::test]
async fn candidate_preview_succeeds_with_root_facility_and_fully_costed_reaction_resolved_component(
) {
    let owner_id = OwnerId::new();
    let workspace_id = iskworks_core::WorkspaceId::new();
    let source_id = PriceSourceId::new();
    let price_source = PriceSource {
        id: source_id,
        workspace_id,
        name: "Home Market".to_string(),
        description: String::new(),
        kind: PriceSourceKind::Manual,
        revision: 1,
        item_count: 2,
        recent_build_count: 0,
        items: vec![
            price_item(34, "Tritanium", "4.1250"),
            price_item(35, "Pyerite", "8.5000"),
        ],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let parent = with_saved_resolution(
        parent_build_resolving_hull_section_to_build(workspace_id, owner_id),
        90_001,
        iskworks_core::RecipeSelection::Reaction {
            reaction_formula_type_id: 90_003,
        },
    );
    let parent_id = parent.id;
    let root_facility_id = iskworks_core::FacilityProfileId::new();
    let root_facility = fixture_facility_profile(iskworks_core::FacilityRole::Manufacturing);
    let child_facility_id = iskworks_core::FacilityProfileId::new();
    let child_facility = fixture_facility_profile(iskworks_core::FacilityRole::Reaction);

    let now = chrono::Utc::now();
    // A reaction-resolved child for 90_001 via formula 90_003 (Pyerite
    // 50/run -> Hull Section 1/run, same shape as the manufacturing route).
    let child = fixture_link(
        Build {
            id: BuildId::new(),
            workspace_id,
            owner_id,
            name: "Rifter Hull Section reaction".to_string(),
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
                fingerprint: "rf".to_string(),
            }),
            runs: 2,
            notes: String::new(),
            revision: 1,
            created_at: now,
            updated_at: now,
            draft_planning: Some(iskworks_core::DraftPlanningSnapshot {
                input: iskworks_core::DraftPlanningInput {
                    material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
                    output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
                    manual_price_list_id: Some(source_id),
                    expected_manual_price_list_revision: Some(1),
                    material_pricing_policy: iskworks_core::MarketPricingPolicy::HighestBuy,
                    output_pricing_policy: iskworks_core::MarketPricingPolicy::LowestSell,
                    pricing_selections: Vec::new(),
                    blueprint_selection: None,
                    manufacturing_facility: None,
                    reaction_facility: Some(iskworks_core::ReactionFacilityPreviewCommand {
                        facility_profile_id: child_facility_id,
                        estimated_item_value: None,
                    }),
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

    let app = build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            linked_builds: std::sync::Mutex::new(vec![child]),
            price_source: Some(price_source),
            market_items: std::collections::BTreeMap::new(),
            facility_profiles: std::collections::HashMap::from([
                (root_facility_id, root_facility),
                (child_facility_id, child_facility),
            ]),
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
        .with_adjusted_price_repository(Arc::new(FixtureAdjustedPriceRepository {
            prices: std::collections::BTreeMap::from([
                (34, rust_decimal::Decimal::new(413, 2)),
                (90_001, rust_decimal::Decimal::new(500, 2)),
                (35, rust_decimal::Decimal::new(85, 1)),
            ]),
        })),
    );

    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "manualPriceListId": source_id,
        "expectedManualPriceListRevision": 1,
        "pricingSelections": [],
        "buildId": parent_id.0,
        "manufacturingFacility": {
            "facilityProfileId": root_facility_id.0,
            "blueprintMe": 0,
            "blueprintTe": 0,
            "estimatedItemValue": "1"
        },
        "componentResolutions": [
            {"typeId": 90_001, "recipe": {"mode": "reaction", "reactionFormulaTypeId": 90_003}}
        ]
    })
    .to_string();

    let response = app
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

    let status = response.status();
    let json: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(status, StatusCode::OK, "{json:?}");
    assert_eq!(json["candidate"]["pricingComplete"], true);

    let material_lines = json["candidate"]["materialLines"].as_array().unwrap();
    let hull_section = material_lines
        .iter()
        .find(|line| line["typeId"] == 90_001)
        .unwrap();
    assert_eq!(hull_section["isBuildResolved"], true);
    assert_eq!(hull_section["unitPrice"], serde_json::Value::Null);
    assert_eq!(hull_section["lineTotal"], "892.5000");

    let material_cost = &json["calculationEvidence"]["materialCost"];
    assert_eq!(material_cost["complete"], true);
    assert_eq!(material_cost["baseMarketValue"], "1305.0000");
    assert_eq!(material_cost["afterBlueprintMe"], "1305.0000");
    assert_eq!(material_cost["afterStructure"], "1305.0000");
}
