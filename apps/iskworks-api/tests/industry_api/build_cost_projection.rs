use super::*;

/// Fixed market prices covering every Buy leaf + output type in the
/// `rifter_root` / `nested_chain` recipe chains, so a `Buy` boundary
/// resolves a fresh unit price. (Installation still needs an ESI service,
/// which the API fixtures do not wire -- see the module doc.)
fn fixture_prices() -> std::collections::BTreeMap<i64, iskworks_core::PriceSourceItem> {
    [
        price_item(34, "Tritanium", "5"),
        price_item(35, "Pyerite", "11"),
        price_item(90_001, "Rifter Hull Section", "800"),
        price_item(5_876, "Rifter", "5000000"),
    ]
    .into_iter()
    .map(|item| (item.type_id, item))
    .collect()
}

fn cost_app(
    parent: Build,
    owner_id: OwnerId,
    linked_builds: Vec<Build>,
    inventory: support::inventory::SeededInventoryRepository,
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
            market_items: fixture_prices(),
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        .with_inventory_repository(inventory.clone())
        .with_production_repository(Arc::new(FixtureProductionRepository {
            type_id: 0,
            available_to_this_build: 0,
            average_historical_unit_cost: None,
        })),
    );
    (router, inventory)
}

fn seeded(balances: Vec<(i64, u64)>) -> support::inventory::SeededInventoryRepository {
    support::inventory::SeededInventoryRepository::new(balances)
}

async fn post_cost(
    app: axum::Router,
    build_id: BuildId,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/builds/{}/cost-projection", build_id.0))
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

fn dec(value: &str) -> rust_decimal::Decimal {
    value.parse().unwrap()
}
fn money_of(value: &serde_json::Value) -> Option<rust_decimal::Decimal> {
    value.as_str().map(dec)
}

/// Every material-side conservation identity, checked against the JSON the
/// route returns (installation is skipped -- no ESI wired).
fn assert_material_conservation(projection: &serde_json::Value) {
    let boundaries = projection["boundaries"].as_array().unwrap();
    for boundary in boundaries {
        if !boundary["complete"].as_bool().unwrap() {
            continue;
        }
        let inv = money_of(&boundary["inventoryCost"]);
        let req = money_of(&boundary["requirementCost"]);
        match boundary["kind"].as_str().unwrap() {
            "buy" => {
                let fresh = money_of(&boundary["freshCost"]);
                assert_eq!(
                    inv.zip(fresh).map(|(a, b)| a + b),
                    req,
                    "buy boundary decomposition"
                );
            }
            "build" | "reaction" => {
                let consumed = money_of(&boundary["childConsumedCost"]);
                assert_eq!(
                    inv.zip(consumed).map(|(a, b)| a + b),
                    req,
                    "build boundary decomposition"
                );
                if let (Some(total), Some(consumed), Some(surplus)) = (
                    money_of(&boundary["childTotalProductionCost"]),
                    money_of(&boundary["childConsumedCost"]),
                    money_of(&boundary["childSurplusRetainedBasis"]),
                ) {
                    assert_eq!(consumed + surplus, total, "child production conservation");
                }
            }
            "fullyCovered" => assert_eq!(inv, req, "fully-covered boundary"),
            _ => {}
        }
    }

    for operation in projection["operations"].as_array().unwrap() {
        let material = money_of(&operation["materialComponentCost"]);
        if material.is_none() {
            continue;
        }
        let op_index = operation["opIndex"].as_u64().unwrap();
        let sum: rust_decimal::Decimal = boundaries
            .iter()
            .filter(|b| b["opIndex"].as_u64() == Some(op_index))
            .filter_map(|b| money_of(&b["requirementCost"]))
            .sum();
        assert_eq!(
            Some(sum),
            material,
            "op {op_index} Σ requirement == material"
        );
        let directs = money_of(&operation["directInventoryCost"]).unwrap()
            + money_of(&operation["directBuyCost"]).unwrap()
            + money_of(&operation["consumedChildCost"]).unwrap();
        assert_eq!(Some(directs), material, "op {op_index} directs sum");
    }

    // Root == root op total; never the naive sum of all op totals.
    let root_total = money_of(&projection["root"]["planningTotalProductionCost"]);
    let op0_total = projection["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["opIndex"].as_u64() == Some(0))
        .and_then(|o| money_of(&o["totalProductionCost"]));
    assert_eq!(root_total, op0_total, "root == root operation total");
}

// -- basic: a flat all-Buy Rifter -- root material cost fully resolves --
#[tokio::test]
async fn returns_an_allocation_aware_projection() {
    let (parent, _ws, owner_id) = rifter_root(1);
    let parent_id = parent.id;
    // No linked children, no Build resolutions => 34 and 90_001 are Buy
    // leaves priced from `fixture_prices()`.
    let (app, _inv) = cost_app(parent, owner_id, Vec::new(), seeded(vec![]));
    let (status, json) = post_cost(app, parent_id, overlay(1, serde_json::json!([]))).await;
    assert_eq!(status, StatusCode::OK);

    let ops = json["operations"].as_array().unwrap();
    assert_eq!(ops.len(), 1, "one operation -- the root Rifter job");
    assert_eq!(ops[0]["opIndex"], 0);
    let boundaries = json["boundaries"].as_array().unwrap();
    assert!(!boundaries.is_empty());
    assert!(boundaries.iter().all(|b| b["kind"] == "buy"));
    // Every Buy boundary priced => root material component cost resolves.
    // 6_830 recipe: Tritanium 100 @ 5 = 500 ; Hull Section 2 @ 800 = 1_600.
    assert_eq!(
        money_of(&ops[0]["materialComponentCost"]),
        Some(dec("2100"))
    );
    assert_eq!(money_of(&ops[0]["directBuyCost"]), Some(dec("2100")));
    assert_eq!(money_of(&ops[0]["directInventoryCost"]), Some(dec("0")));
    assert_material_conservation(&json);
    // installation still incomplete (no ESI) => operation + root incomplete.
    assert_eq!(ops[0]["ownInstallation"]["complete"], false);
    assert_eq!(json["complete"], false);
}

// -- Exactly one inventory read, and NO writes ------------------
#[tokio::test]
async fn performs_exactly_one_list_balances_and_no_inventory_writes() {
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let (app, inventory) = cost_app(
        parent,
        owner_id,
        linked,
        seeded(vec![(90_001, 1), (34, 400)]),
    );
    let (status, _json) = post_cost(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);
    // `SeededInventoryRepository` panics on any write, so reaching here at
    // all proves no inventory mutation occurred.
    assert_eq!(
        inventory.list_balances_call_count(),
        1,
        "one inventory snapshot for the whole cost projection"
    );
}

// -- nested tree: per-boundary + child conservation over the real walk --
#[tokio::test]
async fn nested_tree_conservation_holds_end_to_end() {
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    // Give the deep chain some real inventory basis so inventory_cost is
    // exercised on more than one boundary.
    let inventory = seeded(vec![(90_001, 1), (34, 400)])
        .with_unit_basis(34, "5.5")
        .with_unit_basis(90_001, "1000");
    let (app, _inv) = cost_app(parent, owner_id, linked, inventory);
    let (status, json) = post_cost(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);
    assert_material_conservation(&json);

    // Multi-operation tree: the naive sum of op totals must differ from the
    // root (double-count guard) whenever totals resolve. Installation is
    // incomplete here, so totals are null -- assert the guard on the
    // *material* rollup instead.
    let ops = json["operations"].as_array().unwrap();
    let root_material = money_of(&ops[0]["materialComponentCost"]);
    let naive: Option<rust_decimal::Decimal> = ops
        .iter()
        .map(|o| money_of(&o["materialComponentCost"]))
        .sum();
    if let (Some(root_material), Some(naive)) = (root_material, naive) {
        assert_ne!(
            root_material, naive,
            "summing every operation's material cost double-counts descendants"
        );
    }
}

// -- shared inventory across the tree: counted once -------------------
#[tokio::test]
async fn shared_inventory_is_never_double_valued() {
    // Type 34 (Tritanium) is demanded by the root recipe AND the Pyerite
    // grandchild. Seed 700 units with a basis; the global allocator gives
    // each boundary a disjoint slice.
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let inventory = seeded(vec![(90_001, 1), (34, 700)]).with_unit_basis(34, "5");
    let (app, _inv) = cost_app(parent, owner_id, linked, inventory);
    let (status, json) = post_cost(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);

    let total_34_inventory: rust_decimal::Decimal = json["boundaries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["typeId"].as_i64() == Some(34))
        .filter_map(|b| money_of(&b["inventoryCost"]))
        .sum();
    // At most 700 units * 5 ISK basis, never 2 * (full requirement) * 5.
    assert!(
        total_34_inventory <= dec("3500"),
        "type 34 inventory basis consumed = {total_34_inventory}, must be <= 700 * 5"
    );
    assert!(
        total_34_inventory > rust_decimal::Decimal::ZERO,
        "some Tritanium inventory was actually planned for reuse"
    );
    assert_material_conservation(&json);
}

// -- The unsaved overlay drives it, not persisted state ------
#[tokio::test]
async fn reflects_the_editor_overlay_not_persisted_runs() {
    // persisted runs 5, overlay runs 1.
    let (parent, linked, parent_id, owner_id) = nested_chain(5);
    let (app, _inv) = cost_app(parent, owner_id, linked, seeded(vec![(90_001, 1)]));
    let (status, json) = post_cost(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);
    let root = json["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["opIndex"].as_u64() == Some(0))
        .unwrap();
    assert_eq!(
        root["nodeRuns"], 1,
        "cost projection is at the overlay run count"
    );
}

// -- installation degrades to incomplete without an ESI service -------
#[tokio::test]
async fn installation_is_incomplete_without_adjusted_prices() {
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let (app, _inv) = cost_app(parent, owner_id, linked, seeded(vec![(90_001, 1)]));
    let (status, json) = post_cost(app, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["complete"], false);
    // Every operation reports its installation incomplete, but its
    // material-component cost still computes.
    for operation in json["operations"].as_array().unwrap() {
        assert_eq!(operation["ownInstallation"]["complete"], false);
    }
    let warnings = json["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w["code"] == "missingAdjustedPrice" || w["code"] == "noFacilitySelected"),
        "an installation-completeness warning is surfaced: {warnings:?}"
    );
    assert_material_conservation(&json);
}

// -- determinism -----------------------------------------------------
#[tokio::test]
async fn projection_is_deterministic() {
    let (parent, linked, parent_id, owner_id) = nested_chain(1);
    let (app_a, _a) = cost_app(
        parent.clone(),
        owner_id,
        linked.clone(),
        seeded(vec![(90_001, 1), (34, 250)]).with_unit_basis(34, "5"),
    );
    let (app_b, _b) = cost_app(
        parent,
        owner_id,
        linked,
        seeded(vec![(90_001, 1), (34, 250)]).with_unit_basis(34, "5"),
    );
    let (_s, first) = post_cost(app_a, parent_id, overlay(1, hull_section_built())).await;
    let (_s, again) = post_cost(app_b, parent_id, overlay(1, hull_section_built())).await;
    assert_eq!(first, again);
}
