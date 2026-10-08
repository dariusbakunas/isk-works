use super::*;

#[tokio::test]
async fn candidate_preview_blends_a_missing_scoped_buy_rows_cost_between_inventory_and_market() {
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

    let mut parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    parent
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .manual_price_list_id = Some(source_id);
    parent
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .expected_manual_price_list_revision = Some(1);
    // Tritanium (34) isn't in component_resolutions at all -- a plain
    // Buy row -- but scoping it Missing should still blend it against
    // inventory, same as a build-resolved row would.
    parent
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .fulfillment_scopes = vec![FulfillmentScopeOverride {
        type_id: 34,
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
            price_source: Some(price_source),
            market_items: std::collections::BTreeMap::new(),
            facility_profiles: std::collections::HashMap::new(),
            blueprint_observations: std::collections::HashMap::new(),
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        // `BuildCostProjection`'s allocation-aware inventory basis
        // comes from `InventoryRepository::list_balances`, not
        // `ProductionRepository::coverage` (coverage classification only) --
        // both must describe the same 40 units @ 2.0000 for the merged
        // planning-cost preview to blend inventory the way this test expects.
        .with_inventory_repository(Arc::new(
            support::inventory::SeededInventoryRepository::new(vec![(34, 40)])
                .with_unit_basis(34, "2.0"),
        ))
        .with_production_repository(Arc::new(FixtureProductionRepository {
            type_id: 34,
            available_to_this_build: 40,
            average_historical_unit_cost: Some(Money::parse("2.0000").unwrap()),
        })),
    );

    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "manualPriceListId": source_id,
        "expectedManualPriceListRevision": 1,
        "pricingSelections": [],
        "buildId": parent_id.0,
        "componentResolutions": [],
        "fulfillmentScopes": [{"typeId": 34, "scope": "missing"}]
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
    assert_eq!(tritanium["missing"], false);
    // Required 100, 40 already on hand: 60 missing @ 4.1250 market +
    // 40 reused @ 2.0000 inventory -- not 100 @ 4.1250.
    assert_eq!(tritanium["missingQuantity"], 60);
    assert_eq!(tritanium["reusedQuantity"], 40);
    assert_eq!(tritanium["reusedLineTotal"], "80.0000");
    assert_eq!(tritanium["lineTotal"], "327.5000");
    assert_eq!(tritanium["unitPrice"], "3.2750");
}

#[tokio::test]
async fn candidate_preview_blends_a_buy_rows_cost_against_inventory_by_default_with_no_explicit_scope(
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
        item_count: 1,
        recent_build_count: 0,
        items: vec![price_item(34, "Tritanium", "4.1250")],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let mut parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    parent
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .manual_price_list_id = Some(source_id);
    parent
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .expected_manual_price_list_revision = Some(1);
    // No `fulfillmentScopes` entry for Tritanium at all -- `Missing` is now
    // the default, so this should blend exactly like the explicitly-scoped
    // case above.
    let parent_id = parent.id;

    let app = build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            linked_builds: std::sync::Mutex::new(Vec::new()),
            price_source: Some(price_source),
            market_items: std::collections::BTreeMap::new(),
            facility_profiles: std::collections::HashMap::new(),
            blueprint_observations: std::collections::HashMap::new(),
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        // `BuildCostProjection`'s allocation-aware inventory basis
        // comes from `InventoryRepository::list_balances`, not
        // `ProductionRepository::coverage` (coverage classification only) --
        // both must describe the same 40 units @ 2.0000 for the merged
        // planning-cost preview to blend inventory the way this test expects.
        .with_inventory_repository(Arc::new(
            support::inventory::SeededInventoryRepository::new(vec![(34, 40)])
                .with_unit_basis(34, "2.0"),
        ))
        .with_production_repository(Arc::new(FixtureProductionRepository {
            type_id: 34,
            available_to_this_build: 40,
            average_historical_unit_cost: Some(Money::parse("2.0000").unwrap()),
        })),
    );

    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "manualPriceListId": source_id,
        "expectedManualPriceListRevision": 1,
        "pricingSelections": [],
        "buildId": parent_id.0,
        "componentResolutions": [],
        "fulfillmentScopes": []
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
    assert_eq!(tritanium["missingQuantity"], 60);
    assert_eq!(tritanium["reusedQuantity"], 40);
    assert_eq!(tritanium["reusedLineTotal"], "80.0000");
    assert_eq!(tritanium["lineTotal"], "327.5000");
    assert_eq!(tritanium["unitPrice"], "3.2750");
}

#[tokio::test]
async fn candidate_preview_prices_a_full_scoped_row_at_the_full_market_rate_despite_available_inventory(
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
        item_count: 1,
        recent_build_count: 0,
        items: vec![price_item(34, "Tritanium", "4.1250")],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let mut parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    parent
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .manual_price_list_id = Some(source_id);
    parent
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .expected_manual_price_list_revision = Some(1);
    parent
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .fulfillment_scopes = vec![FulfillmentScopeOverride {
        type_id: 34,
        scope: FulfillmentScope::Full,
    }];
    let parent_id = parent.id;

    let app = build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            linked_builds: std::sync::Mutex::new(Vec::new()),
            price_source: Some(price_source),
            market_items: std::collections::BTreeMap::new(),
            facility_profiles: std::collections::HashMap::new(),
            blueprint_observations: std::collections::HashMap::new(),
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        // 40 available, but explicitly Full-scoped -- must be ignored.
        .with_inventory_repository(Arc::new(
            support::inventory::SeededInventoryRepository::new(vec![(34, 40)])
                .with_unit_basis(34, "2.0"),
        ))
        .with_production_repository(Arc::new(FixtureProductionRepository {
            type_id: 34,
            available_to_this_build: 40,
            average_historical_unit_cost: Some(Money::parse("2.0000").unwrap()),
        })),
    );

    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "manualPriceListId": source_id,
        "expectedManualPriceListRevision": 1,
        "pricingSelections": [],
        "buildId": parent_id.0,
        "componentResolutions": [],
        "fulfillmentScopes": [{"typeId": 34, "scope": "full"}]
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
    // `BuildCostProjection` always reports an explicit inventory
    // quantity/cost per boundary (never omits the field) -- `0`/`"0.0000"`
    // for an explicitly Full-scoped row, never `null`.
    assert_eq!(tritanium["reusedQuantity"], 0);
    assert_eq!(tritanium["reusedLineTotal"], "0.0000");
    // `missingQuantity` is coverage-classification evidence (not part of
    // `BuildCostProjection`) -- still `null` for an explicitly Full-scoped
    // type, which `material_coverage()` excludes from the coverage map.
    assert_eq!(tritanium["missingQuantity"], serde_json::Value::Null);
    // Full 100 units at the plain market price -- inventory ignored.
    assert_eq!(tritanium["lineTotal"], "412.5000");
    assert_eq!(tritanium["unitPrice"], "4.1250");
}

/// The other half of the candidate-preview completeness contract: an active
/// linked build exists (so the row's cost isn't simply "no linked build at all,"
/// already covered elsewhere), but that linked build's own draft has never
/// had a price source configured -- its own live material cost can't be
/// computed at all, so the row must read as missing, not silently fall
/// back to a market price (there isn't one configured for 90_001 in this
/// fixture either, so a fallback would be impossible to distinguish from
/// this from the assertion alone if one existed -- the point is there's no
/// fallback path being taken).
#[tokio::test]
async fn candidate_preview_of_an_existing_build_leaves_a_row_unpriced_when_its_linked_build_has_no_price_source(
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
        item_count: 1,
        recent_build_count: 0,
        items: vec![price_item(34, "Tritanium", "4.1250")],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    let parent_id = parent.id;
    let mut child = linked_child_build(workspace_id, owner_id, parent_id, 90_001, 2);
    child.draft_planning = Some(iskworks_core::DraftPlanningSnapshot {
        input: iskworks_core::DraftPlanningInput {
            material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            manual_price_list_id: None, // never configured
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
            facility_profiles: std::collections::HashMap::new(),
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
    assert_eq!(hull_section["missing"], true);
    assert_eq!(hull_section["unitPrice"], serde_json::Value::Null);
    assert_eq!(hull_section["lineTotal"], serde_json::Value::Null);
    assert_eq!(json["candidate"]["pricingComplete"], false);
}

#[tokio::test]
async fn candidate_preview_resolves_a_selected_facility_of_the_non_root_kind_without_requiring_esi()
{
    // Root is manufacturing, and the one build-resolved sub-component (Hull
    // Section) is manufacturing-kind too -- but only a *reaction* facility
    // is selected here. `preview_plan` still resolves that reaction
    // profile (it isn't the root's own, but a later Task adds reaction
    // sub-components that would need it), while the manufacturing-kind
    // sub-component's own installation cost correctly stays unknown, and no
    // ESI call is ever attempted since nothing actually needs an EIV.
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
    let reaction_profile = fixture_facility_profile(iskworks_core::FacilityRole::Reaction);
    let facility_profiles =
        std::collections::HashMap::from([(reaction_profile.id, reaction_profile.clone())]);

    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "manualPriceListId": source_id,
        "expectedManualPriceListRevision": 1,
        "pricingSelections": [],
        "reactionFacility": {
            "facilityProfileId": reaction_profile.id.0,
            "estimatedItemValue": null
        },
        "componentResolutions": [
            {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
        ]
    })
    .to_string();

    let response = app_with_price_source_sde_and_facilities(price_source, facility_profiles)
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

    // The root is manufacturing, so the selected reaction facility is never
    // the root's own -- the top-level fields both stay null.
    assert_eq!(
        json["candidate"]["manufacturingFacility"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["candidate"]["reactionFacility"],
        serde_json::Value::Null
    );

    let material_lines = json["candidate"]["materialLines"].as_array().unwrap();
    let hull_section = material_lines
        .iter()
        .find(|line| line["typeId"] == 90_001)
        .unwrap();
    assert_eq!(hull_section["isBuildResolved"], true);
    assert_eq!(hull_section["installationCost"], serde_json::Value::Null);
}

#[tokio::test]
async fn candidate_preview_rejects_a_component_facility_override_with_the_wrong_role() {
    // The Hull Section sub-component resolves via a manufacturing recipe,
    // but its facility override points at a Reaction-role profile -- must
    // be rejected, the same way a role-mismatched shared slot already is.
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
    let reaction_profile = fixture_facility_profile(iskworks_core::FacilityRole::Reaction);
    let facility_profiles =
        std::collections::HashMap::from([(reaction_profile.id, reaction_profile.clone())]);

    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "manualPriceListId": source_id,
        "expectedManualPriceListRevision": 1,
        "pricingSelections": [],
        "componentResolutions": [
            {
                "typeId": 90_001,
                "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002},
                "facilityOverride": {
                    "facilityProfileId": reaction_profile.id.0
                }
            }
        ]
    })
    .to_string();

    let response = app_with_price_source_sde_and_facilities(price_source, facility_profiles)
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
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json:?}");
    assert_eq!(json["error"]["code"], "facility_calculation_failed");
}

#[tokio::test]
async fn candidate_preview_ignores_a_legacy_expected_profile_revision_on_a_component_override() {
    // A per-row `facility_override` is a live-Build reference by id, exactly
    // like the shared slots. An older client still sends an
    // `expectedProfileRevision` (here, a value that no longer matches any
    // revision); serde ignores it and the current profile is resolved,
    // reaching the EIV fetch (503 esi_not_configured with no ESI wired --
    // see the sibling test).
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
    let manufacturing_profile =
        fixture_facility_profile(iskworks_core::FacilityRole::Manufacturing);
    let facility_profiles = std::collections::HashMap::from([(
        manufacturing_profile.id,
        manufacturing_profile.clone(),
    )]);

    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "manualPriceListId": source_id,
        "expectedManualPriceListRevision": 1,
        "pricingSelections": [],
        "componentResolutions": [
            {
                "typeId": 90_001,
                "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002},
                "facilityOverride": {
                    "facilityProfileId": manufacturing_profile.id.0,
                    "expectedProfileRevision": manufacturing_profile.revision + 1
                }
            }
        ]
    })
    .to_string();

    let response = app_with_price_source_sde_and_facilities(price_source, facility_profiles)
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
    assert_ne!(
        json["error"]["code"], "facility_revision_conflict",
        "a stale override revision must no longer be rejected: {json:?}"
    );
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{json:?}");
    assert_eq!(json["error"]["code"], "esi_not_configured");
}

#[tokio::test]
async fn candidate_preview_attempts_eiv_for_an_override_only_row_with_no_shared_facility_selected()
{
    // No shared manufacturing/reaction facility is selected on the request
    // at all -- only the Hull Section sub-component carries its own
    // facility_override. Before this fix, `resolve_component_installation_eivs`'s
    // early-return guard skipped every row whenever both shared slots were
    // unselected, so this request would succeed with `installationCost:
    // null`. After the fix, the override-only row is recognized and an EIV
    // fetch is genuinely attempted -- and since this test's `AppState` has
    // no ESI configured, that surfaces as `503 esi_not_configured` instead,
    // proving the row was newly recognized (a real, observable side effect,
    // not an assertion on the EIV value itself -- there's no fake ESI
    // service in this test suite to produce one).
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
    let manufacturing_profile =
        fixture_facility_profile(iskworks_core::FacilityRole::Manufacturing);
    let facility_profiles = std::collections::HashMap::from([(
        manufacturing_profile.id,
        manufacturing_profile.clone(),
    )]);

    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "manualPriceListId": source_id,
        "expectedManualPriceListRevision": 1,
        "pricingSelections": [],
        "componentResolutions": [
            {
                "typeId": 90_001,
                "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002},
                "facilityOverride": {
                    "facilityProfileId": manufacturing_profile.id.0
                }
            }
        ]
    })
    .to_string();

    let response = app_with_price_source_sde_and_facilities(price_source, facility_profiles)
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
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{json:?}");
    assert_eq!(json["error"]["code"], "esi_not_configured");
}

#[tokio::test]
async fn candidate_preview_rejects_a_component_blueprint_selection_on_a_reaction_resolution() {
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
        "componentResolutions": [
            {
                "typeId": 90_001,
                "recipe": {"mode": "reaction", "reactionFormulaTypeId": 90_003},
                "blueprintSelection": {
                    "mode": "manual",
                    "kind": "original",
                    "materialEfficiency": 10,
                    "timeEfficiency": 0,
                    "licensedRuns": null,
                    "notes": ""
                }
            }
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

    let status = response.status();
    let json: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json:?}");
    assert_eq!(json["error"]["code"], "validation_failed");
}
