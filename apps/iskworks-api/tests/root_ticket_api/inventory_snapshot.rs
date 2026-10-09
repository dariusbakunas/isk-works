use super::*;

// ─────────────────────────────────────────────────────────────────────────
// Inventory-aware Epic snapshotting
//
// Create Epic reads live inventory coverage and freezes intended reuse onto
// each requirement -- WITHOUT reserving or mutating inventory. A frozen
// `reusedQuantity` is planning evidence, not a reservation.
// ─────────────────────────────────────────────────────────────────────────

/// Mark `type_ids` as explicitly `Full`-scoped on the build's planning
/// input, so Create Epic must ignore inventory for them. A scope is
/// sourcing, so it is saved through the Build route (the canonical write).
async fn set_full_scope(fx: &Fixture, build: &Build, type_ids: &[i64]) -> Build {
    let now = chrono::Utc::now();
    let mut snapshot = draft_planning(now, fx.price_list_id);
    snapshot.input.fulfillment_scopes = type_ids
        .iter()
        .map(|&type_id| FulfillmentScopeOverride {
            type_id,
            scope: FulfillmentScope::Full,
        })
        .collect();
    let response = fx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/builds/{}", build.id.0))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "expectedRevision": build.revision,
                        "name": build.name,
                        "recipe": iskworks_core::recipe_selection_of(&build.recipe),
                        "runs": build.runs,
                        "notes": build.notes,
                        "draftPlanning": snapshot.input,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::OK, "set_full_scope: {body}");
    fx.industry
        .get_build(fx.workspace_id, build.id)
        .await
        .unwrap()
}

/// Full + partial inventory coverage: bulk creation skips fresh==0,
/// InventorySatisfied requirements get no ticket, and the Epic cost keeps
/// the inventory basis -- never zero.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn epic_freezes_missing_scope_coverage_full_and_partial(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;

    // Tritanium fully covers 1000 need, at avg cost 3 (< market 5).
    seed_balance(&pool, &fx, 34, "Tritanium", 5_000, 15_000).await;
    // Pyerite: only 50 of the 200 need, at avg cost 5 (< market 10).
    seed_balance(&pool, &fx, 35, "Pyerite", 50, 250).await;

    let (status, order) = create_order(&fx.app, &build).await;
    assert_eq!(status, StatusCode::CREATED, "body: {order}");

    let tritanium = requirement_of(&order, 34);
    assert_eq!(tritanium["fulfillmentScope"], "missing");
    assert_eq!(tritanium["reusedQuantity"], 1000);
    assert_eq!(tritanium["freshQuantity"], 0);
    assert_eq!(tritanium["state"], "inventorySatisfied");
    money_eq(&tritanium["reusedLineTotal"], "3000.0000");

    let pyerite = requirement_of(&order, 35);
    assert_eq!(pyerite["fulfillmentScope"], "missing");
    assert_eq!(pyerite["reusedQuantity"], 50);
    assert_eq!(pyerite["freshQuantity"], 150);
    assert_eq!(pyerite["state"], "needsAction");
    money_eq(&pyerite["reusedLineTotal"], "250.0000");

    // Epic material cost keeps the frozen inventory basis and is NOT zero:
    //   Tritanium 1000 @ 3 (all reused)          = 3000
    //   Pyerite    50 @ 5 (reused) + 150 @ 10    =  250 + 1500 = 1750
    money_eq(&order["estimatedMaterialCost"], "4750.0000");

    // A fully-covered requirement gets no ticket via direct creation...
    let tritanium_id = tritanium["id"].as_str().unwrap();
    let order_id = order["id"].as_str().unwrap();
    let (status, _) = post_json(
        &fx.app,
        &format!("/api/orders/{order_id}/requirements/{tritanium_id}/tickets"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "direct ticket creation for an InventorySatisfied requirement is rejected"
    );

    // ...and bulk creation silently skips it, still making the Pyerite one.
    let pyerite_id = pyerite["id"].as_str().unwrap().to_string();
    let (status, tickets) = post_json(
        &fx.app,
        &format!("/api/orders/{order_id}/tickets/bulk"),
        serde_json::json!({ "requirementIds": [tritanium_id, pyerite_id] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {tickets}");
    let tickets = tickets.as_array().unwrap();
    assert_eq!(tickets.len(), 1, "only the Pyerite shortage becomes work");
    assert_eq!(tickets[0]["typeId"], 35);
    assert_eq!(
        tickets[0]["quantity"], 150,
        "sized to the shortage, not 200"
    );
    // Ticket estimate is the fresh portion only: 150 @ market 10.
    money_eq(&tickets[0]["estimatedLineTotal"], "1500.0000");
}

/// Zero inventory -- default `Missing` scope, nothing on hand: the
/// whole requirement is fresh and a full-size ticket is created.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn epic_with_no_inventory_freezes_missing_scope_entirely_fresh(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;

    let (status, order) = create_order(&fx.app, &build).await;
    assert_eq!(status, StatusCode::CREATED);

    let tritanium = requirement_of(&order, 34);
    assert_eq!(tritanium["fulfillmentScope"], "missing");
    assert_eq!(tritanium["reusedQuantity"], 0);
    assert_eq!(tritanium["freshQuantity"], 1000);
    assert_eq!(tritanium["state"], "needsAction");
    assert_eq!(tritanium["reusedLineTotal"], Value::Null);

    let order_id = order["id"].as_str().unwrap();
    let tritanium_id = tritanium["id"].as_str().unwrap();
    let (status, ticket) = post_json(
        &fx.app,
        &format!("/api/orders/{order_id}/requirements/{tritanium_id}/tickets"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(ticket["quantity"], 1000);
}

/// Explicit `Full` scope ignores stock entirely: full fresh
/// quantity, `needsAction`, a full-size acquisition ticket even though the
/// balance more than covers it.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn epic_full_scope_ignores_inventory(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;
    let build = set_full_scope(&fx, &build, &[34]).await;

    seed_balance(&pool, &fx, 34, "Tritanium", 5_000, 25_000).await;

    let (status, order) = create_order(&fx.app, &build).await;
    assert_eq!(status, StatusCode::CREATED, "body: {order}");

    let tritanium = requirement_of(&order, 34);
    assert_eq!(tritanium["fulfillmentScope"], "full");
    assert_eq!(tritanium["reusedQuantity"], 0);
    assert_eq!(tritanium["freshQuantity"], 1000);
    assert_eq!(tritanium["state"], "needsAction");
    // Pyerite has no override -> still default Missing (here, zero on hand).
    assert_eq!(requirement_of(&order, 35)["fulfillmentScope"], "missing");

    let order_id = order["id"].as_str().unwrap();
    let tritanium_id = tritanium["id"].as_str().unwrap();
    let (status, ticket) = post_json(
        &fx.app,
        &format!("/api/orders/{order_id}/requirements/{tritanium_id}/tickets"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        ticket["quantity"], 1000,
        "full requirement despite the stock"
    );
}

/// Create Epic reads inventory but writes none: event count, every
/// balance's quantity / cost basis / revision, and the allocation table are
/// byte-identical before and after. The single most important regression.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn creating_an_epic_touches_no_inventory(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 5_000, 15_000).await;
    seed_balance(&pool, &fx, 35, "Pyerite", 5_000, 30_000).await;

    let before = inventory_fingerprint(&pool).await;
    let allocations_before = allocation_count(&pool).await;

    let (status, _order) = create_order(&fx.app, &build).await;
    assert_eq!(status, StatusCode::CREATED);

    assert_eq!(
        inventory_fingerprint(&pool).await,
        before,
        "Epic creation posted an event or changed a balance"
    );
    // It reserves what it reuses (Tritanium + Pyerite) -- a claim, not a
    // ledger change.
    assert_eq!(allocation_count(&pool).await, allocations_before + 2);
}

/// No double-planning: every Epic reserves what it freezes as reuse, so a
/// second Epic created right after the first freezes only the stock the
/// first left free (issue #4).
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_second_epic_freezes_only_the_stock_the_first_left_free(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;
    // Only 1.5x one build's Tritanium need on hand.
    seed_balance(&pool, &fx, 34, "Tritanium", 1_500, 7_500).await;

    let (status_a, order_a) = create_order(&fx.app, &build).await;
    assert_eq!(status_a, StatusCode::CREATED);
    let (status_b, order_b) = create_order(&fx.app, &build).await;
    assert_eq!(status_b, StatusCode::CREATED);

    assert_eq!(requirement_of(&order_a, 34)["reusedQuantity"], 1000);
    assert_eq!(requirement_of(&order_b, 34)["reusedQuantity"], 500);

    assert_eq!(allocation_count(&pool).await, 2);
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM inventory_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(events, 0);
}

/// The root product itself is explicit production intent, never
/// inventory-netted: owning a Rifter does not shrink or skip the root
/// Manufacturing ticket. Only the recipe *inputs* are inventory-aware.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn root_output_is_not_netted_against_inventory(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;
    seed_balance(&pool, &fx, 5876, "Rifter", 3, 1_500_000).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 5_000, 25_000).await;

    let (status, order) = create_order(&fx.app, &build).await;
    assert_eq!(status, StatusCode::CREATED);

    let root = root_ticket_for(&list_tickets(&fx.app).await, build.id).clone();
    assert_eq!(root["kind"], "manufacturing");
    assert_eq!(
        root["quantity"], 1,
        "still building one Rifter -- the owned Rifter never nets the root output"
    );

    // ...but the root recipe's Tritanium input IS netted, on the Epic
    // requirement and on the mirrored root-ticket prerequisite.
    assert_eq!(requirement_of(&order, 34)["reusedQuantity"], 1000);
    let tritanium_prereq = root["prerequisites"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["typeId"] == 34)
        .unwrap();
    assert_eq!(tritanium_prereq["fulfillmentScope"], "missing");
    assert_eq!(tritanium_prereq["reusedQuantity"], 1000);
    assert_eq!(tritanium_prereq["freshQuantity"], 0);
}

/// The Epic snapshot is immutable: changing inventory or the
/// source Build after creation never rewrites the frozen quantities/costs.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn epic_snapshot_is_stable_across_later_inventory_and_build_changes(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 600, 1_800).await; // avg 3, covers 600 of 1000

    let (status, order) = create_order(&fx.app, &build).await;
    assert_eq!(status, StatusCode::CREATED);
    let order_id = order["id"].as_str().unwrap().to_string();

    let frozen = requirement_of(&order, 34).clone();
    assert_eq!(frozen["reusedQuantity"], 600);
    assert_eq!(frozen["freshQuantity"], 400);
    money_eq(&frozen["reusedLineTotal"], "1800.0000");

    // Drain all the Tritanium, and bump the Build's runs.
    sqlx::query("UPDATE inventory_balances SET quantity = 0, total_historical_cost = 0, revision = revision + 1 WHERE type_id = 34")
        .execute(&pool)
        .await
        .unwrap();
    let reloaded = fx
        .industry
        .get_build(fx.workspace_id, build.id)
        .await
        .unwrap();
    set_full_scope(&fx, &reloaded, &[34]).await;

    // Re-read the Epic: the frozen requirement is unchanged.
    let response = fx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/orders/{order_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let after = body_json(response).await;
    let still = requirement_of(&after, 34);
    assert_eq!(still["reusedQuantity"], 600);
    assert_eq!(still["freshQuantity"], 400);
    money_eq(&still["reusedLineTotal"], "1800.0000");
    assert_eq!(still["fulfillmentScope"], "missing");
}
