use super::*;

use iskworks_core::order::{
    NewOrderPlan, NewOrderRequirement, NewPlanOperation, NewPlanReservation, Order, OrderError,
    OrderRequirementId, PlanOperationEvidence, PlanOperationId, RequirementKind,
    ReservationShortfall,
};

// ─────────────────────────────────────────────────────────────────────────
// Epic reservations: `create_order_plan` with a reservation request reserves
// each requirement's frozen reuse from free stock, under the balance lock,
// all or nothing.
// ─────────────────────────────────────────────────────────────────────────

/// A minimal version-3 plan for `build`: one root operation and one Buy
/// requirement per `(type_id, required, reused)`.
fn plan_for(
    fx: &Fixture,
    build: &Build,
    requirements: &[(i64, u64, u64)],
    reserve: bool,
) -> NewOrderPlan {
    let now = chrono::Utc::now();
    let price_snapshot = iskworks_core::PriceSnapshot {
        id: iskworks_core::PriceSnapshotId(Uuid::new_v4()),
        price_source_id: None,
        source_name: "fixture".to_string(),
        source_revision: 1,
        created_at: now,
        items: Vec::new(),
    };
    let root_key = format!("root:{}", build.id.0);
    let order = Order {
        id: OrderId(Uuid::new_v4()),
        workspace_id: fx.workspace_id,
        owner_id: fx.owner_id,
        source_build_id: Some(build.id),
        source_build_revision: build.revision,
        display_name: "Reservation probe".to_string(),
        runs: 1,
        recipe_fingerprint: "fp".to_string(),
        price_snapshot_id: price_snapshot.id,
        estimated_material_cost: iskworks_core::Money::zero(),
        expected_revenue: None,
        estimated_margin: None,
        missing_price_count: 0,
        created_at: now,
        updated_at: now,
        started_at: None,
        completed_at: None,
        canceled_at: None,
        archived_at: None,
        planning_snapshot_version: 3,
    };
    let root_op = NewPlanOperation {
        id: PlanOperationId::new(),
        occurrence_key: root_key.clone(),
        parent_occurrence_key: None,
        build_id: build.id,
        activity: iskworks_core::build_materials::MaterialActivity::Manufacturing,
        runs: 1,
        persisted_runs: 1,
        product_type_id: 5876,
        product_name: "Rifter".to_string(),
        output_per_run: 1,
        produced_quantity: 1,
        blueprint_or_formula_type_id: 6830,
        material_component_cost: None,
        own_installation_cost: None,
        total_production_cost: None,
        complete: false,
        consumed_quantity: None,
        surplus_quantity: None,
        surplus_retained_basis: None,
        evidence: PlanOperationEvidence {
            effective_me: Some(0),
            effective_te: Some(0),
            job_count: 1,
            recipe_currency: RecipeCurrency::Current,
            installation: None,
            warnings: Vec::new(),
        },
    };
    let requirements = requirements
        .iter()
        .map(|&(type_id, required, reused)| NewOrderRequirement {
            id: OrderRequirementId::new(),
            type_id,
            captured_name: format!("Type {type_id}"),
            kind: RequirementKind::Buy,
            source_build_id: None,
            required_quantity: required,
            fulfillment_scope: FulfillmentScope::Missing,
            reused_quantity: reused,
            estimated_unit_cost: None,
            estimated_line_total: None,
            reused_line_total: None,
            operation_occurrence_key: Some(root_key.clone()),
            child_occurrence_key: None,
            inventory_unit_basis: None,
            child_produced_quantity: None,
            child_consumed_quantity: None,
            child_surplus_quantity: None,
            child_surplus_retained_basis: None,
            child_consumed_cost: None,
            dependency_id: None,
            price_evidence: None,
        })
        .collect();
    NewOrderPlan {
        order,
        price_snapshot,
        operations: vec![root_op],
        requirements,
        tickets: Vec::new(),
        reservation: reserve.then(|| NewPlanReservation {
            stages: std::collections::BTreeMap::from([(root_key, 0)]),
        }),
    }
}

/// `(type_id, quantity, reason)` of every active allocation, by type.
async fn active_allocations(pool: &PgPool) -> Vec<(i64, i64, String)> {
    sqlx::query_as(
        "SELECT type_id, quantity, reason FROM inventory_allocations \
         WHERE released_at IS NULL AND consumed_at IS NULL ORDER BY type_id, quantity",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn order_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*)::bigint FROM orders")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_order_plan_reserves_each_requirements_frozen_reuse(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 1_000, 1_000).await;
    seed_balance(&pool, &fx, 35, "Pyerite", 200, 200).await;
    let inventory_before = inventory_fingerprint(&pool).await;

    let result = PgOrderRepository::new(pool.clone())
        .create_order_plan(plan_for(
            &fx,
            &build,
            &[(34, 1_000, 600), (35, 200, 0)],
            true,
        ))
        .await
        .unwrap();

    assert_eq!(
        result.reservations.len(),
        1,
        "zero-reuse rows reserve nothing"
    );
    assert_eq!(
        result.reservations[0].requirement_id,
        result
            .requirements
            .iter()
            .find(|r| r.type_id == 34)
            .unwrap()
            .id
    );
    assert_eq!(
        active_allocations(&pool).await,
        vec![(34, 600, "epic_create".to_string())]
    );
    assert_eq!(
        inventory_fingerprint(&pool).await,
        inventory_before,
        "reserving posts no event and changes no balance"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_order_plan_refuses_reuse_another_epic_reserved_and_persists_nothing(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 1_000, 1_000).await;
    let repository = PgOrderRepository::new(pool.clone());
    repository
        .create_order_plan(plan_for(&fx, &build, &[(34, 1_000, 600)], true))
        .await
        .unwrap();
    let orders_before = order_count(&pool).await;

    let error = repository
        .create_order_plan(plan_for(&fx, &build, &[(34, 1_000, 600)], true))
        .await
        .unwrap_err();

    match error {
        OrderError::ReservationShortfall(shortfalls) => assert_eq!(
            shortfalls,
            vec![ReservationShortfall {
                type_id: 34,
                wanted: 600,
                free: 400
            }]
        ),
        other => panic!("expected a reservation shortfall, got {other:?}"),
    }
    assert_eq!(order_count(&pool).await, orders_before, "no partial Epic");
    assert_eq!(
        active_allocations(&pool).await,
        vec![(34, 600, "epic_create".to_string())],
        "the first Epic's reservation is untouched"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_epics_cannot_both_reserve_the_last_units(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 1_000, 1_000).await;
    let first = PgOrderRepository::new(pool.clone());
    let second = PgOrderRepository::new(pool.clone());

    let (a, b) = tokio::join!(
        first.create_order_plan(plan_for(&fx, &build, &[(34, 1_000, 600)], true)),
        second.create_order_plan(plan_for(&fx, &build, &[(34, 1_000, 600)], true)),
    );

    let outcomes = [a.is_ok(), b.is_ok()];
    assert_eq!(
        outcomes.iter().filter(|ok| **ok).count(),
        1,
        "exactly one Epic reserves the 600: {outcomes:?}"
    );
    let failure = if a.is_err() { a } else { b };
    assert!(matches!(failure, Err(OrderError::ReservationShortfall(_))));
    assert_eq!(
        active_allocations(&pool).await,
        vec![(34, 600, "epic_create".to_string())]
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_order_plan_without_a_reservation_request_reserves_nothing(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 1_000, 1_000).await;

    let result = PgOrderRepository::new(pool.clone())
        .create_order_plan(plan_for(&fx, &build, &[(34, 1_000, 600)], false))
        .await
        .unwrap();

    assert!(result.reservations.is_empty());
    assert_eq!(allocation_count(&pool).await, 0);
}

// ─────────────────────────────────────────────────────────────────────────
// Routes: `POST /api/builds/:id/orders/preview` and Create Epic's
// `reservation` request.
// ─────────────────────────────────────────────────────────────────────────

async fn preview_reuse(fx: &Fixture, build: &Build) -> std::collections::BTreeMap<i64, u64> {
    let (status, body) = post_json(
        &fx.app,
        &format!("/api/builds/{}/orders/preview", build.id.0),
        command_json_for_build(build),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "preview: {body}");
    reuse_map(&body["reuse"])
}

fn reuse_map(lines: &Value) -> std::collections::BTreeMap<i64, u64> {
    lines
        .as_array()
        .unwrap()
        .iter()
        .map(|line| {
            (
                line["typeId"].as_i64().unwrap(),
                line["quantity"].as_u64().unwrap(),
            )
        })
        .collect()
}

async fn create_reserving_order(
    fx: &Fixture,
    build: &Build,
    expected_reuse: &[(i64, u64)],
) -> (StatusCode, Value) {
    let mut body = command_json_for_build(build);
    body["reservation"] = serde_json::json!({
        "expectedReuse": expected_reuse
            .iter()
            .map(|(type_id, quantity)| serde_json::json!({"typeId": type_id, "quantity": quantity}))
            .collect::<Vec<_>>(),
    });
    post_json(&fx.app, &format!("/api/builds/{}/orders", build.id.0), body).await
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_epic_reserves_what_it_previewed_and_a_second_plan_sees_only_free_stock(
    pool: PgPool,
) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;
    // Rifter needs 1,000 Tritanium and 200 Pyerite; 600 Tritanium on hand.
    seed_balance(&pool, &fx, 34, "Tritanium", 600, 600).await;

    let preview = preview_reuse(&fx, &build).await;
    assert_eq!(preview, std::collections::BTreeMap::from([(34, 600)]));
    assert_eq!(allocation_count(&pool).await, 0, "preview reserves nothing");

    let (status, order) = create_reserving_order(&fx, &build, &[(34, 600)]).await;
    assert_eq!(status, StatusCode::CREATED, "body: {order}");
    assert!(order.get("reuseIncreased").is_none());
    assert_eq!(requirement_of(&order, 34)["reusedQuantity"], 600);
    assert_eq!(
        active_allocations(&pool).await,
        vec![(34, 600, "epic_create".to_string())]
    );

    // The issue's scenario: the next plan no longer counts that stock.
    assert!(
        preview_reuse(&fx, &build).await.is_empty(),
        "another Epic's reserved Tritanium is not free"
    );
    let (status, second) = create_reserving_order(&fx, &build, &[]).await;
    assert_eq!(status, StatusCode::CREATED, "body: {second}");
    assert_eq!(requirement_of(&second, 34)["reusedQuantity"], 0);
    assert_eq!(requirement_of(&second, 34)["freshQuantity"], 1_000);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_epic_refuses_less_reuse_than_previewed_with_a_fresh_preview(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 600, 600).await;
    let orders_before = order_count(&pool).await;

    // The dialog showed 800 (stock has since dropped to 600).
    let (status, body) = create_reserving_order(&fx, &build, &[(34, 800)]).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "reservation_drift");
    assert_eq!(
        reuse_map(&body["error"]["preview"]["reuse"]),
        std::collections::BTreeMap::from([(34, 600)]),
        "the fresh preview rides along so the dialog can refresh in place"
    );
    assert_eq!(
        body["error"]["decreased"],
        serde_json::json!([{"typeId": 34, "expected": 800, "now": 600}])
    );
    assert_eq!(body["error"]["shortfalls"], serde_json::json!([]));
    assert_eq!(order_count(&pool).await, orders_before, "nothing created");
    assert_eq!(allocation_count(&pool).await, 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_epic_accepts_more_reuse_than_previewed_and_reports_it(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 600, 600).await;

    // The dialog showed 400 (an asset sync has since brought in more).
    let (status, order) = create_reserving_order(&fx, &build, &[(34, 400)]).await;

    assert_eq!(status, StatusCode::CREATED, "body: {order}");
    assert_eq!(
        order["reuseIncreased"],
        serde_json::json!([{"typeId": 34, "expected": 400, "now": 600}])
    );
    assert_eq!(
        active_allocations(&pool).await,
        vec![(34, 600, "epic_create".to_string())]
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_epic_without_a_reservation_request_reserves_nothing(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 600, 600).await;

    let (status, order) = create_order(&fx.app, &build).await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(requirement_of(&order, 34)["reusedQuantity"], 600);
    assert_eq!(allocation_count(&pool).await, 0);
}

/// The freeze draws root and child demand from one shared pool, so an
/// Epic never plans to reuse more of a type than is free -- the old
/// "root and child both freeze the same stock" duplicate cannot happen,
/// and reserving such an Epic never trips a shortfall on its own.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn shared_raw_material_reuse_never_exceeds_free_stock(pool: PgPool) {
    let fx = fixture(&pool).await;
    // Root: 5 Fabricated Component + 2,000 Tritanium; child (5 runs):
    // 300 Pyerite + 500 Tritanium. 2,500 Tritanium demanded, 2,200 on hand.
    let parent = assembly_with_built_component(&fx, 1, 90100, &[]).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 2_200, 2_200).await;

    let preview = preview_reuse(&fx, &parent).await;
    assert_eq!(preview.get(&34), Some(&2_200), "capped at the shared pool");

    let expected: Vec<(i64, u64)> = preview.into_iter().collect();
    let (status, order) = create_reserving_order(&fx, &parent, &expected).await;
    assert_eq!(status, StatusCode::CREATED, "body: {order}");
    let reserved: i64 = active_allocations(&pool)
        .await
        .iter()
        .filter(|(type_id, _, _)| *type_id == 34)
        .map(|(_, quantity, _)| quantity)
        .sum();
    assert_eq!(reserved, 2_200);
}

// ─────────────────────────────────────────────────────────────────────────
// `GET /api/orders/:id/coverage`: the Epic view's live read model.
// ─────────────────────────────────────────────────────────────────────────

async fn get_json(fx: &Fixture, path: &str) -> (StatusCode, Value) {
    let response = fx
        .app
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    (status, body_json(response).await)
}

// ─────────────────────────────────────────────────────────────────────────
// `GET /api/orders/:id/execution-plan`: the frozen Epic in the Plan view's
// own shape, plus the Epic overlay.
// ─────────────────────────────────────────────────────────────────────────

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn epic_execution_plan_stages_the_frozen_plan_like_the_build_plan(pool: PgPool) {
    let fx = fixture(&pool).await;
    // Root: 5 Fabricated Component + 2,000 Tritanium; child (5 runs):
    // 300 Pyerite + 500 Tritanium. 1,000 Tritanium on hand.
    let parent = assembly_with_built_component(&fx, 1, 90100, &[]).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 1_000, 1_000).await;
    let preview = preview_reuse(&fx, &parent).await;
    let expected: Vec<(i64, u64)> = preview.into_iter().collect();
    let (status, order) = create_reserving_order(&fx, &parent, &expected).await;
    assert_eq!(status, StatusCode::CREATED, "body: {order}");
    let order_id = order["id"].as_str().unwrap();

    let (status, body) = get_json(&fx, &format!("/api/orders/{order_id}/execution-plan")).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let plan = &body["plan"];

    // Build order: the component's stage first, the root last.
    let stages = plan["stages"].as_array().unwrap();
    assert_eq!(stages.len(), 2);
    let root_id = plan["rootNodeId"].as_str().unwrap();
    assert_eq!(stages[1]["nodeIds"], serde_json::json!([root_id]));
    let component_id = stages[0]["nodeIds"][0].as_str().unwrap();
    let component = plan["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == component_id)
        .unwrap();
    assert_eq!(component["outputTypeId"], 90100);
    assert_eq!(component["projectedRuns"], 5);
    assert_eq!(component["consumers"][0]["nodeId"], root_id);

    // Tritanium is bought by both operations; the Epic reserved all 1,000.
    let tritanium = plan["acquisitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|line| line["typeId"] == 34)
        .expect("Tritanium still to source");
    assert_eq!(tritanium["requiredQuantity"], 2_500);
    assert_eq!(tritanium["plannedInventoryQuantity"], 1_000);
    assert_eq!(tritanium["shortageQuantity"], 1_500);
    assert_eq!(tritanium["availableQuantity"], 0);
    assert_eq!(
        tritanium["reservedQuantity"], 0,
        "the Epic's own hold is not someone else's"
    );

    let epic = &body["epic"];
    assert_eq!(epic["orderId"], order["id"]);
    assert_eq!(epic["acquisitions"]["34"]["reserved"], 1_000);
    assert_eq!(epic["acquisitions"]["34"]["remainingNeed"], 1_500);
    assert_eq!(epic["nodes"][root_id]["ticketStatus"], "todo");
    assert!(epic["nodes"][component_id]["ticketDisplayId"].is_string());
}

// ─────────────────────────────────────────────────────────────────────────
// Releasing: cancel / archive free the Epic's reservations; restore
// doesn't take them back.
// ─────────────────────────────────────────────────────────────────────────

async fn reserved_rifter_epic(pool: &PgPool, fx: &Fixture) -> String {
    let build = rifter(fx).await;
    seed_balance(pool, fx, 34, "Tritanium", 600, 600).await;
    let (status, order) = create_reserving_order(fx, &build, &[(34, 600)]).await;
    assert_eq!(status, StatusCode::CREATED, "body: {order}");
    order["id"].as_str().unwrap().to_string()
}

async fn released_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM inventory_allocations WHERE released_at IS NOT NULL",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn canceling_an_epic_releases_its_reservations_without_touching_inventory(pool: PgPool) {
    let fx = fixture(&pool).await;
    let order_id = reserved_rifter_epic(&pool, &fx).await;
    let inventory_before = inventory_fingerprint(&pool).await;

    let (status, body) = post_json(
        &fx.app,
        &format!("/api/orders/{order_id}/cancel"),
        Value::Null,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(active_allocations(&pool).await.is_empty());
    assert_eq!(released_count(&pool).await, 1);
    assert_eq!(inventory_fingerprint(&pool).await, inventory_before);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn archiving_releases_and_restoring_does_not_re_reserve(pool: PgPool) {
    let fx = fixture(&pool).await;
    let order_id = reserved_rifter_epic(&pool, &fx).await;
    let inventory_before = inventory_fingerprint(&pool).await;
    let (_, detail) = get_json(&fx, &format!("/api/orders/{order_id}")).await;
    assert_eq!(
        detail["inventory"],
        serde_json::json!({"plannedReuse": 600, "reserved": 600, "used": 0})
    );

    let (status, body) = post_json(
        &fx.app,
        &format!("/api/orders/{order_id}/archive"),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(active_allocations(&pool).await.is_empty());
    let (_, detail) = get_json(&fx, &format!("/api/orders/{order_id}")).await;
    assert_eq!(detail["inventory"]["reserved"], 0);

    let (status, body) = post_json(
        &fx.app,
        &format!("/api/orders/{order_id}/restore"),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(
        active_allocations(&pool).await.is_empty(),
        "restore reserves nothing"
    );
    assert_eq!(inventory_fingerprint(&pool).await, inventory_before);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_refused_cancel_releases_nothing(pool: PgPool) {
    let fx = fixture(&pool).await;
    let order_id = reserved_rifter_epic(&pool, &fx).await;
    let (status, _) = post_json(
        &fx.app,
        &format!("/api/orders/{order_id}/cancel"),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let released = released_count(&pool).await;

    let (status, _) = post_json(
        &fx.app,
        &format!("/api/orders/{order_id}/cancel"),
        Value::Null,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(released_count(&pool).await, released);
}

// ─────────────────────────────────────────────────────────────────────────
// `POST /api/orders/:id/reserve`: explicit top-up.
// ─────────────────────────────────────────────────────────────────────────

async fn reserve(fx: &Fixture, order_id: &str) -> (StatusCode, Value) {
    post_json(
        &fx.app,
        &format!("/api/orders/{order_id}/reserve"),
        Value::Null,
    )
    .await
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reserve_inventory_takes_back_a_restored_epics_reuse(pool: PgPool) {
    let fx = fixture(&pool).await;
    let order_id = reserved_rifter_epic(&pool, &fx).await;
    post_json(
        &fx.app,
        &format!("/api/orders/{order_id}/archive"),
        Value::Null,
    )
    .await;
    post_json(
        &fx.app,
        &format!("/api/orders/{order_id}/restore"),
        Value::Null,
    )
    .await;

    let (status, body) = reserve(&fx, &order_id).await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(
        body["reserved"],
        serde_json::json!([{"typeId": 34, "typeName": "Tritanium", "quantity": 600}])
    );
    assert_eq!(body["shortfalls"], serde_json::json!([]));
    assert_eq!(
        active_allocations(&pool).await,
        vec![(34, 600, "epic_top_up".to_string())]
    );

    // Already holding its full reuse: a repeat reserves nothing.
    let (status, body) = reserve(&fx, &order_id).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["reserved"], serde_json::json!([]));
    assert_eq!(active_allocations(&pool).await.len(), 1);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reserve_inventory_takes_what_is_free_and_reports_the_rest(pool: PgPool) {
    let fx = fixture(&pool).await;
    let order_id = reserved_rifter_epic(&pool, &fx).await;
    post_json(
        &fx.app,
        &format!("/api/orders/{order_id}/archive"),
        Value::Null,
    )
    .await;
    post_json(
        &fx.app,
        &format!("/api/orders/{order_id}/restore"),
        Value::Null,
    )
    .await;
    // Meanwhile 400 of the 600 Tritanium left the hangar.
    sqlx::query("UPDATE inventory_balances SET quantity = 200 WHERE type_id = 34")
        .execute(&pool)
        .await
        .unwrap();

    let (status, body) = reserve(&fx, &order_id).await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["reserved"][0]["quantity"], 200);
    assert_eq!(
        body["shortfalls"],
        serde_json::json!([{"typeId": 34, "wanted": 600, "free": 200}])
    );
    assert_eq!(
        active_allocations(&pool).await,
        vec![(34, 200, "epic_top_up".to_string())]
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_canceled_epic_cannot_reserve(pool: PgPool) {
    let fx = fixture(&pool).await;
    let order_id = reserved_rifter_epic(&pool, &fx).await;
    post_json(
        &fx.app,
        &format!("/api/orders/{order_id}/cancel"),
        Value::Null,
    )
    .await;

    let (status, body) = reserve(&fx, &order_id).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "order_not_reservable");
    assert!(active_allocations(&pool).await.is_empty());
}

// ─────────────────────────────────────────────────────────────────────────
// Recording production: own reservations first, then free stock, and
// never another Epic's reservation without `takeFrom`.
// ─────────────────────────────────────────────────────────────────────────

async fn root_ticket_id(fx: &Fixture, order_id: &str) -> String {
    let tickets = list_tickets(&fx.app).await;
    root_ticket_for_order(&tickets, order_id)["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn record_rifter(
    fx: &Fixture,
    ticket_id: &str,
    inputs: &[(i64, u64)],
    take_from: Value,
) -> (StatusCode, Value) {
    post_json(
        &fx.app,
        &format!("/api/tickets/{ticket_id}/record-production"),
        serde_json::json!({
            "idempotencyKey": Uuid::new_v4(),
            "runsCompleted": 1,
            "output": {"typeId": 5876, "quantity": 1},
            "inputs": inputs
                .iter()
                .map(|(type_id, quantity)| serde_json::json!({"typeId": type_id, "quantity": quantity}))
                .collect::<Vec<_>>(),
            "installationCost": "0",
            "takeFrom": take_from,
        }),
    )
    .await
}

/// `(quantity, consumed?, released?, reason)` of every Tritanium allocation.
async fn tritanium_allocations(pool: &PgPool) -> Vec<(i64, bool, bool, String)> {
    sqlx::query_as(
        "SELECT quantity, consumed_at IS NOT NULL, released_at IS NOT NULL, reason \
         FROM inventory_allocations WHERE type_id = 34 ORDER BY quantity, reason",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Epic A reserves 600 Tritanium + 200 Pyerite; 400 more Tritanium then
/// arrives and Epic B reserves it. Physical Tritanium: 1,000, all reserved.
async fn two_epics_sharing_tritanium(pool: &PgPool, fx: &Fixture) -> (String, String) {
    let build = rifter(fx).await;
    seed_balance(pool, fx, 34, "Tritanium", 600, 600).await;
    seed_balance(pool, fx, 35, "Pyerite", 200, 200).await;
    let (status, a) = create_reserving_order(fx, &build, &[(34, 600), (35, 200)]).await;
    assert_eq!(status, StatusCode::CREATED, "body: {a}");
    sqlx::query(
        "UPDATE inventory_balances SET quantity = 1000, total_historical_cost = 1000 WHERE type_id = 34",
    )
    .execute(pool)
    .await
    .unwrap();
    let (status, b) = create_reserving_order(fx, &build, &[(34, 400)]).await;
    assert_eq!(status, StatusCode::CREATED, "body: {b}");
    (
        a["id"].as_str().unwrap().to_string(),
        b["id"].as_str().unwrap().to_string(),
    )
}

/// The spec's worked example: A needs 1,000, holds 600, nothing is free,
/// and B holds the other 400. Recording must not silently use B's 400.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn recording_refuses_to_use_another_epics_reservation(pool: PgPool) {
    let fx = fixture(&pool).await;
    let (a, b) = two_epics_sharing_tritanium(&pool, &fx).await;
    let ticket = root_ticket_id(&fx, &a).await;
    let inventory_before = inventory_fingerprint(&pool).await;

    let (status, body) = record_rifter(
        &fx,
        &ticket,
        &[(34, 1_000), (35, 200)],
        serde_json::json!([]),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
    let error = &body["error"];
    assert_eq!(error["code"], "insufficient_available");
    assert_eq!(error["shortages"].as_array().unwrap().len(), 1);
    let shortage = &error["shortages"][0];
    assert_eq!(shortage["typeId"], 34);
    assert_eq!(shortage["typeName"], "Tritanium");
    assert_eq!(shortage["needed"], 1_000);
    assert_eq!(shortage["own"], 600);
    assert_eq!(shortage["free"], 0);
    assert_eq!(shortage["holders"][0]["orderId"], b.as_str());
    assert_eq!(shortage["holders"][0]["quantity"], 400);
    assert!(shortage["holders"][0]["displayName"].is_string());
    // Nothing was recorded.
    assert_eq!(inventory_fingerprint(&pool).await, inventory_before);
    assert_eq!(active_allocations(&pool).await.len(), 3);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn recording_takes_from_a_named_epic_when_allowed(pool: PgPool) {
    let fx = fixture(&pool).await;
    let (a, b) = two_epics_sharing_tritanium(&pool, &fx).await;
    let ticket = root_ticket_id(&fx, &a).await;

    let (status, body) = record_rifter(
        &fx,
        &ticket,
        &[(34, 1_000), (35, 200)],
        serde_json::json!([{"orderId": b, "typeId": 34}]),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    assert_eq!(
        tritanium_allocations(&pool).await,
        vec![
            (400, false, true, "epic_create".to_string()),
            (600, true, false, "epic_create".to_string()),
        ],
        "A's 600 consumed by the recording; B's 400 released to it"
    );
    assert!(active_allocations(&pool).await.is_empty());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn recording_uses_own_reservations_first_then_free_stock(pool: PgPool) {
    let fx = fixture(&pool).await;
    let order_id = reserved_rifter_epic(&pool, &fx).await; // 600 Tritanium held
    seed_balance(&pool, &fx, 35, "Pyerite", 200, 200).await;
    sqlx::query(
        "UPDATE inventory_balances SET quantity = 1000, total_historical_cost = 1000 WHERE type_id = 34",
    )
    .execute(&pool)
    .await
    .unwrap();
    let ticket = root_ticket_id(&fx, &order_id).await;

    let (status, body) =
        record_rifter(&fx, &ticket, &[(34, 700), (35, 200)], serde_json::json!([])).await;

    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    assert_eq!(
        tritanium_allocations(&pool).await,
        vec![(600, true, false, "epic_create".to_string())],
        "own 600 consumed, the other 100 came from free stock"
    );
    let left: i64 =
        sqlx::query_scalar("SELECT quantity FROM inventory_balances WHERE type_id = 34")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(left, 300);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn recording_part_of_a_reservation_splits_it(pool: PgPool) {
    let fx = fixture(&pool).await;
    let order_id = reserved_rifter_epic(&pool, &fx).await; // 600 Tritanium held
    seed_balance(&pool, &fx, 35, "Pyerite", 200, 200).await;
    let ticket = root_ticket_id(&fx, &order_id).await;

    let (status, body) =
        record_rifter(&fx, &ticket, &[(34, 250), (35, 200)], serde_json::json!([])).await;

    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    assert_eq!(
        tritanium_allocations(&pool).await,
        vec![
            (250, true, false, "epic_create".to_string()),
            (350, false, false, "epic_create".to_string()),
        ]
    );
}

/// D14: a canceled Epic holds nothing, so its ticket draws free stock only
/// -- and still can't use another Epic's reservation.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_canceled_epics_ticket_draws_free_stock_only(pool: PgPool) {
    let fx = fixture(&pool).await;
    let (a, b) = two_epics_sharing_tritanium(&pool, &fx).await;
    let (status, _) = post_json(&fx.app, &format!("/api/orders/{a}/cancel"), Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    let ticket = root_ticket_id(&fx, &a).await;

    let (status, body) = record_rifter(
        &fx,
        &ticket,
        &[(34, 1_000), (35, 200)],
        serde_json::json!([]),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
    let shortage = &body["error"]["shortages"][0];
    assert_eq!(shortage["own"], 0);
    assert_eq!(shortage["free"], 600);
    assert_eq!(shortage["holders"][0]["orderId"], b.as_str());

    let (status, body) =
        record_rifter(&fx, &ticket, &[(34, 600), (35, 200)], serde_json::json!([])).await;
    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    assert_eq!(
        active_allocations(&pool).await,
        vec![(34, 400, "epic_create".to_string())],
        "B's reservation is untouched"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Reverting a recording restores what it consumed.
// ─────────────────────────────────────────────────────────────────────────

async fn revert(fx: &Fixture, ticket_id: &str, recording: &Value) -> (StatusCode, Value) {
    let recording_id = recording["recording"]["id"].as_str().unwrap();
    post_json(
        &fx.app,
        &format!("/api/tickets/{ticket_id}/recordings/{recording_id}/revert"),
        Value::Null,
    )
    .await
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reverting_a_recording_restores_the_reservations_it_used(pool: PgPool) {
    let fx = fixture(&pool).await;
    let order_id = reserved_rifter_epic(&pool, &fx).await; // 600 Tritanium held
    seed_balance(&pool, &fx, 35, "Pyerite", 200, 200).await;
    let ticket = root_ticket_id(&fx, &order_id).await;
    let (status, recording) =
        record_rifter(&fx, &ticket, &[(34, 250), (35, 200)], serde_json::json!([])).await;
    assert_eq!(status, StatusCode::CREATED, "body: {recording}");

    let (status, body) = revert(&fx, &ticket, &recording).await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    let held: i64 = active_allocations(&pool)
        .await
        .iter()
        .filter(|(type_id, _, _)| *type_id == 34)
        .map(|(_, quantity, _)| quantity)
        .sum();
    assert_eq!(held, 600, "the Epic holds its full 600 again");
    assert_eq!(
        tritanium_allocations(&pool).await,
        vec![
            (250, false, false, "epic_create".to_string()),
            (350, false, false, "epic_create".to_string()),
        ]
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reverting_does_not_hand_taken_stock_back_to_its_epic(pool: PgPool) {
    let fx = fixture(&pool).await;
    let (a, b) = two_epics_sharing_tritanium(&pool, &fx).await;
    let ticket = root_ticket_id(&fx, &a).await;
    let (status, recording) = record_rifter(
        &fx,
        &ticket,
        &[(34, 1_000), (35, 200)],
        serde_json::json!([{"orderId": b, "typeId": 34}]),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body: {recording}");

    let (status, body) = revert(&fx, &ticket, &recording).await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(
        tritanium_allocations(&pool).await,
        vec![
            (400, false, true, "epic_create".to_string()),
            (600, false, false, "epic_create".to_string()),
        ],
        "A's 600 active again; B's 400 stays released, so it is free stock"
    );
    let physical: i64 =
        sqlx::query_scalar("SELECT quantity FROM inventory_balances WHERE type_id = 34")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(physical, 1_000);
}
