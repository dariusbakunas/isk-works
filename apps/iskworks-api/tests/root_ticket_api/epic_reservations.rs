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
