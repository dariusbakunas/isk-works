use super::*;

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn start_order_transitions_to_in_progress(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let snapshot = empty_price_snapshot();
    let repository = PgOrderRepository::new(pool.clone());
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let order_id = order.id;
    let (order, _) = repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: Vec::new(),
        })
        .await
        .unwrap();
    assert!(order.started_at.is_none());

    let started = repository
        .start_order(workspace_id, order_id)
        .await
        .unwrap();
    assert!(started.started_at.is_some());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn start_order_rejects_an_already_started_order(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let snapshot = empty_price_snapshot();
    let repository = PgOrderRepository::new(pool.clone());
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let order_id = order.id;
    repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: Vec::new(),
        })
        .await
        .unwrap();
    repository
        .start_order(workspace_id, order_id)
        .await
        .unwrap();

    let result = repository.start_order(workspace_id, order_id).await;
    assert!(matches!(result, Err(OrderError::OrderNotStartable)));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn complete_order_consumes_nothing_and_posts_no_events(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let snapshot = empty_price_snapshot();
    seed_inventory_balance_with_cost(
        &pool,
        workspace_id,
        owner_id,
        34,
        "Tritanium",
        500,
        "1000.0000",
    )
    .await;
    let balance_before = inventory_balance(&pool, workspace_id, owner_id, 34).await;
    let repository = PgOrderRepository::new(pool.clone());
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let order_id = order.id;
    repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: vec![requirement(34, RequirementKind::Buy, 500)],
        })
        .await
        .unwrap();

    repository
        .start_order(workspace_id, order_id)
        .await
        .unwrap();
    let completed = repository
        .complete_order(workspace_id, order_id)
        .await
        .unwrap();

    // The ONLY effect: the organizational timestamp.
    assert!(completed.completed_at.is_some());
    // No consumption, no allocation, no balance or cost-basis movement.
    assert_eq!(inventory_event_count(&pool).await, 0);
    assert!(allocation_snapshot(&pool).await.is_empty());
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        balance_before
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn complete_order_rejects_an_order_that_was_never_started(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let snapshot = empty_price_snapshot();
    let repository = PgOrderRepository::new(pool.clone());
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let order_id = order.id;
    repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: Vec::new(),
        })
        .await
        .unwrap();

    let result = repository.complete_order(workspace_id, order_id).await;
    assert!(matches!(result, Err(OrderError::OrderNotCompletable)));
}

/// Seed stock with a real cost basis, capture every observable
/// Inventory fact, then create AND cancel an Order -- nothing about
/// Inventory may move.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_then_cancel_order_leaves_inventory_byte_identical(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_inventory_balance_with_cost(
        &pool,
        workspace_id,
        owner_id,
        34,
        "Tritanium",
        5_000,
        "12500.0000",
    )
    .await;

    let balance_before = inventory_balance(&pool, workspace_id, owner_id, 34).await;
    let events_before = inventory_event_count(&pool).await;
    let allocations_before = allocation_snapshot(&pool).await;

    let repository = PgOrderRepository::new(pool.clone());
    let snapshot = empty_price_snapshot();
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let order_id = order.id;
    repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: vec![requirement(34, RequirementKind::Buy, 3_000)],
        })
        .await
        .unwrap();

    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        balance_before
    );
    assert_eq!(inventory_event_count(&pool).await, events_before);
    assert_eq!(allocation_snapshot(&pool).await, allocations_before);

    let canceled = repository
        .cancel_order(workspace_id, order_id)
        .await
        .unwrap();
    assert!(canceled.canceled_at.is_some());

    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        balance_before
    );
    assert_eq!(inventory_event_count(&pool).await, events_before);
    assert_eq!(allocation_snapshot(&pool).await, allocations_before);
    assert!(allocation_snapshot(&pool).await.is_empty());
}

/// The full Order lifecycle -- create -> start -> complete, and
/// separately create -> cancel -- observes zero inventory delta end to end.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn the_full_order_lifecycle_is_inventory_neutral(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_inventory_balance_with_cost(
        &pool,
        workspace_id,
        owner_id,
        34,
        "Tritanium",
        5_000,
        "12500.0000",
    )
    .await;
    let repository = PgOrderRepository::new(pool.clone());

    let balance_before = inventory_balance(&pool, workspace_id, owner_id, 34).await;
    let events_before = inventory_event_count(&pool).await;

    // create -> start -> complete
    let snapshot_a = empty_price_snapshot();
    let order_a = draft_order(workspace_id, owner_id, build_id, snapshot_a.id);
    let order_a_id = order_a.id;
    repository
        .create_order(NewOrder {
            order: order_a,
            price_snapshot: snapshot_a,
            requirements: vec![requirement(34, RequirementKind::Buy, 1_000)],
        })
        .await
        .unwrap();
    repository
        .start_order(workspace_id, order_a_id)
        .await
        .unwrap();
    repository
        .complete_order(workspace_id, order_a_id)
        .await
        .unwrap();

    // create -> cancel
    let snapshot_b = empty_price_snapshot();
    let order_b = draft_order(workspace_id, owner_id, build_id, snapshot_b.id);
    let order_b_id = order_b.id;
    repository
        .create_order(NewOrder {
            order: order_b,
            price_snapshot: snapshot_b,
            requirements: vec![requirement(34, RequirementKind::Buy, 2_000)],
        })
        .await
        .unwrap();
    repository
        .cancel_order(workspace_id, order_b_id)
        .await
        .unwrap();

    // Persisted Inventory state -- not "no call was made" -- is unchanged.
    assert_eq!(
        inventory_event_count(&pool).await - events_before,
        0,
        "inventory_events delta must be zero across the whole lifecycle"
    );
    assert!(
        allocation_snapshot(&pool).await.is_empty(),
        "no inventory_allocations row was created"
    );
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        balance_before,
        "balance quantity, weighted-average basis and revision are all unchanged"
    );
}
