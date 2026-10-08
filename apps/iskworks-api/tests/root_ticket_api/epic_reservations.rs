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

fn coverage_line<'a>(coverage: &'a Value, order: &Value, type_id: i64) -> &'a Value {
    let requirement_id = requirement_of(order, type_id)["id"].clone();
    coverage["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|line| line["requirementId"] == requirement_id)
        .unwrap_or_else(|| panic!("no coverage line for type {type_id}"))
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn epic_coverage_reports_held_needed_and_free_stock_per_requirement(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;
    // Rifter: 1,000 Tritanium, 200 Pyerite. 600 Tritanium on hand, reserved
    // by this Epic.
    seed_balance(&pool, &fx, 34, "Tritanium", 600, 600).await;
    let (status, order) = create_reserving_order(&fx, &build, &[(34, 600)]).await;
    assert_eq!(status, StatusCode::CREATED, "body: {order}");
    // Pyerite arrives after the Epic was created: free, not reserved.
    seed_balance(&pool, &fx, 35, "Pyerite", 50, 50).await;

    let order_id = order["id"].as_str().unwrap();
    let (status, coverage) = get_json(&fx, &format!("/api/orders/{order_id}/coverage")).await;
    assert_eq!(status, StatusCode::OK, "body: {coverage}");
    assert_eq!(coverage["orderId"], order["id"]);

    let tritanium = coverage_line(&coverage, &order, 34);
    assert_eq!(tritanium["reserved"], 600);
    assert_eq!(tritanium["consumed"], 0);
    assert_eq!(tritanium["remainingNeed"], 400);
    assert_eq!(
        tritanium["freeAvailable"], 0,
        "the Epic's own 600 are not free"
    );
    assert_eq!(tritanium["freeCoverable"], 0);

    let pyerite = coverage_line(&coverage, &order, 35);
    assert_eq!(pyerite["reserved"], 0);
    assert_eq!(pyerite["remainingNeed"], 200);
    assert_eq!(pyerite["freeAvailable"], 50);
    assert_eq!(pyerite["freeCoverable"], 50);

    // The Epic detail exposes each operation's ticket status, so finished
    // stages can show as done.
    let (status, detail) = get_json(&fx, &format!("/api/orders/{order_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        detail["productionPlan"]["operations"][0]["ticketStatus"],
        "todo"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn epic_coverage_of_an_unknown_epic_is_not_found(pool: PgPool) {
    let fx = fixture(&pool).await;
    let (status, _) = get_json(&fx, &format!("/api/orders/{}/coverage", Uuid::new_v4())).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
