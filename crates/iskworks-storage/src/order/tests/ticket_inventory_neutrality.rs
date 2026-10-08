use super::*;

// ─── A Ticket owns no inventory reservation ───────────────────────────────
//
// Invariant: creating / canceling / archiving / restoring a Ticket, or
// moving its status, touches no `inventory_allocations`, no
// `inventory_balances`, and no `inventory_events`. Order -> Inventory
// reservation (`create_order`) is a separate concern.
//
// Every ticket starts `Todo` regardless of stock or prerequisites.

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn manufacturing_ticket_creation_reserves_nothing_even_when_stock_covers_it(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    // Plenty of stock -- the ticket must still start Todo and reserve
    // nothing.
    seed_inventory_balance(&pool, workspace_id, owner_id, 37164, 1400).await;
    let repository = PgOrderRepository::new(pool.clone());

    let balance_before = inventory_balance(&pool, workspace_id, owner_id, 37164).await;

    let (ticket, prerequisites) = repository
        .create_ticket(NewTicket {
            order_id: None,
            notes: String::new(),
            assignee_character_id: None,
            id: TicketId::new(),
            workspace_id,
            owner_id,
            kind: TicketKind::Manufacturing,
            type_id: Some(20185),
            captured_name: "Crystalline Carbonide Armor Plate".to_string(),
            quantity: Some(3625),
            source_build_id: Some(build_id),
            estimated_unit_cost: None,
            estimated_line_total: None,
            market_region_id: None,
            market_location_id: None,
            price_source_id: None,
            execution_snapshot: None,
            prerequisites: vec![ticket_prerequisite(37164, 1400)],
            occurrence_key: None,
            parent_ticket_id: None,
            produced_quantity: None,
            material_component_cost: None,
            own_installation_cost: None,
            total_production_cost: None,
            plan_evidence: None,
        })
        .await
        .unwrap();

    assert_eq!(ticket.status, TicketStatus::Todo);
    assert_eq!(prerequisites[0].reused_quantity, 0);
    assert_eq!(prerequisites[0].fresh_quantity, 1400);
    assert!(
        allocation_snapshot(&pool).await.is_empty(),
        "ticket creation inserted an inventory_allocations row"
    );
    assert_eq!(inventory_event_count(&pool).await, 0);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 37164).await,
        balance_before
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn acquisition_ticket_creation_reserves_nothing(pool: PgPool) {
    let (workspace_id, owner_id, _import_id) = fixture_workspace(&pool).await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 34, 500).await;
    let repository = PgOrderRepository::new(pool.clone());
    let balance_before = inventory_balance(&pool, workspace_id, owner_id, 34).await;

    repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            5_000_000,
            None,
            None,
        ))
        .await
        .unwrap();

    assert!(allocation_snapshot(&pool).await.is_empty());
    assert_eq!(inventory_event_count(&pool).await, 0);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        balance_before
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn many_tickets_can_reference_the_same_scarce_stock_without_reserving_it(pool: PgPool) {
    // Tritanium = 1,000; two tickets each want 800. Both must be created
    // successfully, neither reserves anything, and reserved/available stay
    // at the unreserved balance. Proves a Ticket is not an inventory
    // reservation primitive.
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 34, 1_000).await;
    let repository = PgOrderRepository::new(pool.clone());
    let production = crate::PgProductionRepository::new(pool.clone());

    for _ in 0..2 {
        let (ticket, prerequisites) = repository
            .create_ticket(NewTicket {
                order_id: None,
                notes: String::new(),
                assignee_character_id: None,
                id: TicketId::new(),
                workspace_id,
                owner_id,
                kind: TicketKind::Manufacturing,
                type_id: Some(20185),
                captured_name: "Crystalline Carbonide Armor Plate".to_string(),
                quantity: Some(800),
                source_build_id: Some(build_id),
                estimated_unit_cost: None,
                estimated_line_total: None,
                market_region_id: None,
                market_location_id: None,
                price_source_id: None,
                execution_snapshot: None,
                prerequisites: vec![ticket_prerequisite(34, 800)],
                occurrence_key: None,
                parent_ticket_id: None,
                produced_quantity: None,
                material_component_cost: None,
                own_installation_cost: None,
                total_production_cost: None,
                plan_evidence: None,
            })
            .await
            .unwrap();
        assert_eq!(ticket.status, TicketStatus::Todo);
        assert_eq!(prerequisites[0].fresh_quantity, 800);
    }

    assert!(allocation_snapshot(&pool).await.is_empty());
    assert_eq!(inventory_event_count(&pool).await, 0);
    assert_eq!(
        production
            .reserved_quantity(workspace_id, owner_id, 34)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        repository
            .available_quantity(workspace_id, owner_id, 34)
            .await
            .unwrap(),
        1_000
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn bulk_manufacturing_ticket_creation_reserves_nothing(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 34, 10_000).await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 37164, 10_000).await;
    let repository = PgOrderRepository::new(pool.clone());

    // The route-level bulk path builds one `NewTicket` per requirement and
    // calls `create_ticket` for each; do the same here, with prerequisites.
    for type_id in [34_i64, 37164] {
        repository
            .create_ticket(NewTicket {
                order_id: None,
                notes: String::new(),
                assignee_character_id: None,
                id: TicketId::new(),
                workspace_id,
                owner_id,
                kind: TicketKind::Manufacturing,
                type_id: Some(20185),
                captured_name: "Crystalline Carbonide Armor Plate".to_string(),
                quantity: Some(500),
                source_build_id: Some(build_id),
                estimated_unit_cost: None,
                estimated_line_total: None,
                market_region_id: None,
                market_location_id: None,
                price_source_id: None,
                execution_snapshot: None,
                prerequisites: vec![ticket_prerequisite(type_id, 500)],
                occurrence_key: None,
                parent_ticket_id: None,
                produced_quantity: None,
                material_component_cost: None,
                own_installation_cost: None,
                total_production_cost: None,
                plan_evidence: None,
            })
            .await
            .unwrap();
    }

    let ticket_owned: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM inventory_allocations WHERE ticket_prerequisite_id IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ticket_owned, 0);
    assert_eq!(inventory_event_count(&pool).await, 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn canceling_a_ticket_touches_no_inventory(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 37164, 1_000).await;
    let repository = PgOrderRepository::new(pool.clone());

    let (ticket, _) = repository
        .create_ticket(NewTicket {
            order_id: None,
            notes: String::new(),
            assignee_character_id: None,
            id: TicketId::new(),
            workspace_id,
            owner_id,
            kind: TicketKind::Manufacturing,
            type_id: Some(20185),
            captured_name: "Crystalline Carbonide Armor Plate".to_string(),
            quantity: Some(100),
            source_build_id: Some(build_id),
            estimated_unit_cost: None,
            estimated_line_total: None,
            market_region_id: None,
            market_location_id: None,
            price_source_id: None,
            execution_snapshot: None,
            prerequisites: vec![ticket_prerequisite(37164, 50)],
            occurrence_key: None,
            parent_ticket_id: None,
            produced_quantity: None,
            material_component_cost: None,
            own_installation_cost: None,
            total_production_cost: None,
            plan_evidence: None,
        })
        .await
        .unwrap();

    let balance_before = inventory_balance(&pool, workspace_id, owner_id, 37164).await;

    let canceled = repository
        .cancel_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(canceled.status, TicketStatus::Canceled);

    assert!(allocation_snapshot(&pool).await.is_empty());
    assert_eq!(inventory_event_count(&pool).await, 0);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 37164).await,
        balance_before
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn archiving_and_restoring_a_ticket_touches_no_inventory(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 37164, 1_000).await;
    let repository = PgOrderRepository::new(pool.clone());

    let (ticket, _) = repository
        .create_ticket(NewTicket {
            order_id: None,
            notes: String::new(),
            assignee_character_id: None,
            id: TicketId::new(),
            workspace_id,
            owner_id,
            kind: TicketKind::Manufacturing,
            type_id: Some(20185),
            captured_name: "Crystalline Carbonide Armor Plate".to_string(),
            quantity: Some(100),
            source_build_id: Some(build_id),
            estimated_unit_cost: None,
            estimated_line_total: None,
            market_region_id: None,
            market_location_id: None,
            price_source_id: None,
            execution_snapshot: None,
            prerequisites: vec![ticket_prerequisite(37164, 50)],
            occurrence_key: None,
            parent_ticket_id: None,
            produced_quantity: None,
            material_component_cost: None,
            own_installation_cost: None,
            total_production_cost: None,
            plan_evidence: None,
        })
        .await
        .unwrap();

    let balance_before = inventory_balance(&pool, workspace_id, owner_id, 37164).await;
    let build_before: (i64, DateTime<Utc>) =
        sqlx::query_as("SELECT revision, updated_at FROM builds WHERE id = $1")
            .bind(build_id.0)
            .fetch_one(&pool)
            .await
            .unwrap();

    repository
        .archive_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    repository
        .restore_ticket(workspace_id, ticket.id)
        .await
        .unwrap();

    assert!(allocation_snapshot(&pool).await.is_empty());
    assert_eq!(inventory_event_count(&pool).await, 0);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 37164).await,
        balance_before
    );
    let build_after: (i64, DateTime<Utc>) =
        sqlx::query_as("SELECT revision, updated_at FROM builds WHERE id = $1")
            .bind(build_id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(build_before, build_after);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_order_reserves_nothing_even_with_a_frozen_reuse_intent(pool: PgPool) {
    // A frozen `reused_quantity` is planning evidence, never a
    // reservation -- no allocation row, and `reserved_quantity` stays zero.
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 34, 500).await;
    let repository = PgOrderRepository::new(pool.clone());
    let production = crate::PgProductionRepository::new(pool.clone());

    let snapshot = empty_price_snapshot();
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let (_, requirements) = repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: vec![requirement_reusing(34, RequirementKind::Buy, 300, 200)],
        })
        .await
        .unwrap();

    assert_eq!(requirements[0].reused_quantity, 200);
    assert_eq!(requirements[0].fresh_quantity, 100);
    let order_owned_active: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(quantity), 0)::bigint FROM inventory_allocations \
         WHERE order_requirement_id IS NOT NULL AND released_at IS NULL AND consumed_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(order_owned_active, 0);
    assert!(allocation_snapshot(&pool).await.is_empty());
    assert_eq!(
        production
            .reserved_quantity(workspace_id, owner_id, 34)
            .await
            .unwrap(),
        0
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn linking_a_requirement_to_a_ticket_twice_is_idempotent(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let snapshot = empty_price_snapshot();
    let repository = PgOrderRepository::new(pool.clone());

    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let (_, requirements) = repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: vec![requirement(44, RequirementKind::Buy, 1)],
        })
        .await
        .unwrap();
    let (ticket, _) = repository
        .create_ticket(NewTicket {
            order_id: None,
            notes: String::new(),
            assignee_character_id: None,
            id: TicketId::new(),
            workspace_id,
            owner_id,
            kind: TicketKind::Acquisition,
            type_id: Some(44),
            captured_name: "Vexor".to_string(),
            quantity: Some(1),
            source_build_id: None,
            estimated_unit_cost: None,
            estimated_line_total: None,
            market_region_id: None,
            market_location_id: None,
            price_source_id: None,
            execution_snapshot: None,
            prerequisites: Vec::new(),
            occurrence_key: None,
            parent_ticket_id: None,
            produced_quantity: None,
            material_component_cost: None,
            own_installation_cost: None,
            total_production_cost: None,
            plan_evidence: None,
        })
        .await
        .unwrap();

    let first = repository
        .link_order_requirement_to_ticket(requirements[0].id, ticket.id, 1)
        .await
        .unwrap();
    let second = repository
        .link_order_requirement_to_ticket(requirements[0].id, ticket.id, 1)
        .await
        .unwrap();
    assert_eq!(first.id, second.id);

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM order_requirement_fulfillments WHERE order_requirement_id = $1",
    )
    .bind(requirements[0].id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn linking_rejects_zero_allocated_quantity(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let snapshot = empty_price_snapshot();
    let repository = PgOrderRepository::new(pool.clone());

    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let (_, requirements) = repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: vec![requirement(44, RequirementKind::Buy, 1)],
        })
        .await
        .unwrap();
    let (ticket, _) = repository
        .create_ticket(NewTicket {
            order_id: None,
            notes: String::new(),
            assignee_character_id: None,
            id: TicketId::new(),
            workspace_id,
            owner_id,
            kind: TicketKind::Acquisition,
            type_id: Some(44),
            captured_name: "Vexor".to_string(),
            quantity: Some(1),
            source_build_id: None,
            estimated_unit_cost: None,
            estimated_line_total: None,
            market_region_id: None,
            market_location_id: None,
            price_source_id: None,
            execution_snapshot: None,
            prerequisites: Vec::new(),
            occurrence_key: None,
            parent_ticket_id: None,
            produced_quantity: None,
            material_component_cost: None,
            own_installation_cost: None,
            total_production_cost: None,
            plan_evidence: None,
        })
        .await
        .unwrap();

    let result = repository
        .link_order_requirement_to_ticket(requirements[0].id, ticket.id, 0)
        .await;
    assert!(matches!(result, Err(OrderError::InvalidQuantity)));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn linking_an_unknown_requirement_returns_not_found(pool: PgPool) {
    let (workspace_id, owner_id, _import_id) = fixture_workspace(&pool).await;
    let repository = PgOrderRepository::new(pool.clone());

    let (ticket, _) = repository
        .create_ticket(NewTicket {
            order_id: None,
            notes: String::new(),
            assignee_character_id: None,
            id: TicketId::new(),
            workspace_id,
            owner_id,
            kind: TicketKind::Acquisition,
            type_id: Some(44),
            captured_name: "Vexor".to_string(),
            quantity: Some(1),
            source_build_id: None,
            estimated_unit_cost: None,
            estimated_line_total: None,
            market_region_id: None,
            market_location_id: None,
            price_source_id: None,
            execution_snapshot: None,
            prerequisites: Vec::new(),
            occurrence_key: None,
            parent_ticket_id: None,
            produced_quantity: None,
            material_component_cost: None,
            own_installation_cost: None,
            total_production_cost: None,
            plan_evidence: None,
        })
        .await
        .unwrap();

    let result = repository
        .link_order_requirement_to_ticket(OrderRequirementId::new(), ticket.id, 1)
        .await;
    assert!(matches!(result, Err(OrderError::OrderRequirementNotFound)));
}
