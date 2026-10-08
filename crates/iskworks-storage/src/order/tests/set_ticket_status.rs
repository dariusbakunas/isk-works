use super::*;

// ─── `set_ticket_status` is a bare, reversible, side-effect-free workflow
// write ──────────────────────────────────────────────────────────────────
//
// The invariant these prove: PATCHing / dragging a ticket status changes
// `tickets.status` (and `updated_at`) and nothing else -- no
// `inventory_events`, no change to `inventory_allocations`, no cascade onto
// dependent tickets, no Build mutation -- and every transition is legal in
// both directions.

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn set_ticket_status_moves_a_ticket_both_directions(pool: PgPool) {
    let (workspace_id, owner_id, _import_id) = fixture_workspace(&pool).await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            44,
            "Vexor",
            3,
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(ticket.status, TicketStatus::Todo);

    // Every transition between the four workflow states is legal in both
    // directions -- there is no state machine.
    for target in [
        TicketStatus::InProgress,
        TicketStatus::Complete,
        // Backward and sideways all work the same.
        TicketStatus::InProgress,
        TicketStatus::Todo,
        TicketStatus::Canceled,
        TicketStatus::Todo,
    ] {
        let updated = repository
            .set_ticket_status(workspace_id, ticket.id, target)
            .await
            .unwrap();
        assert_eq!(updated.status, target);
    }

    // No verb ever posted anything or reserved anything.
    assert_eq!(inventory_event_count(&pool).await, 0);
    assert!(allocation_snapshot(&pool).await.is_empty());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn set_ticket_status_on_an_acquisition_ticket_creates_zero_inventory_events(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    // Seed the type so a *Purchase* posting would succeed if one were
    // (wrongly) attempted -- the assertion is meaningful only if the
    // absence of an event isn't just a rejected posting.
    seed_sde_type(&pool, import_id, 44, "Vexor").await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            44,
            "Vexor",
            3,
            None,
            None,
        ))
        .await
        .unwrap();

    for target in [
        TicketStatus::InProgress,
        TicketStatus::Complete,
        TicketStatus::InProgress,
        TicketStatus::Todo,
    ] {
        repository
            .set_ticket_status(workspace_id, ticket.id, target)
            .await
            .unwrap();
    }

    assert_eq!(inventory_event_count(&pool).await, 0);
    let balance: Option<i64> = sqlx::query_scalar(
        "SELECT quantity FROM inventory_balances \
         WHERE workspace_id = $1 AND owner_id = $2 AND type_id = 44",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(balance, None, "no balance row should have been created");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn set_ticket_status_on_a_manufacturing_ticket_leaves_allocations_and_events_untouched(
    pool: PgPool,
) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_sde_type(&pool, import_id, 37164, "Isogen").await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 37164, 100).await;
    let repository = PgOrderRepository::new(pool.clone());

    // An Order (which reserves nothing -- Order lifecycle is
    // inventory-neutral) alongside the ticket, so this test proves a ticket
    // status move disturbs no allocation even when other rows exist.
    let snapshot = empty_price_snapshot();
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: vec![requirement(37164, RequirementKind::Buy, 40)],
        })
        .await
        .unwrap();

    // The manufacturing ticket reserves nothing: a prerequisite
    // is pure outstanding work. It still starts Todo (workflow status is
    // user-controlled) even though 100 Isogen is in stock.
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
    assert_eq!(ticket.status, TicketStatus::Todo);
    assert_eq!(prerequisites[0].reused_quantity, 0);
    assert_eq!(prerequisites[0].fresh_quantity, 50);

    let allocations_before = allocation_snapshot(&pool).await;
    assert!(
        allocations_before.is_empty(),
        "neither Order nor Ticket creation reserves anything"
    );
    assert_eq!(inventory_event_count(&pool).await, 0);

    for target in [
        TicketStatus::Todo,
        TicketStatus::InProgress,
        TicketStatus::Complete,
        TicketStatus::InProgress,
        TicketStatus::Todo,
    ] {
        repository
            .set_ticket_status(workspace_id, ticket.id, target)
            .await
            .unwrap();
    }

    assert_eq!(
        allocation_snapshot(&pool).await,
        allocations_before,
        "a ticket status move may not release, consume, or add any allocation -- not even the Order's"
    );
    assert_eq!(
        inventory_event_count(&pool).await,
        0,
        "a ticket status move may not post any inventory event"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn set_ticket_status_never_cascades_to_dependent_tickets(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_sde_type(&pool, import_id, 37164, "Isogen").await;
    let repository = PgOrderRepository::new(pool.clone());

    // A manufacturing ticket whose single prerequisite is fed by an
    // acquisition ticket. Moving the fulfiller through its workflow states
    // must never write the dependent's status -- dependency state is
    // reported (via `derive_ticket_blockers`), never enforced on status.
    let (manufacturing_ticket, prerequisites) = repository
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
    assert_eq!(manufacturing_ticket.status, TicketStatus::Todo);

    let (fulfiller, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            37164,
            "Isogen",
            50,
            None,
            None,
        ))
        .await
        .unwrap();
    repository
        .link_ticket_prerequisite_to_ticket(prerequisites[0].id, fulfiller.id, 50)
        .await
        .unwrap();

    for target in [TicketStatus::InProgress, TicketStatus::Complete] {
        repository
            .set_ticket_status(workspace_id, fulfiller.id, target)
            .await
            .unwrap();
    }

    let manufacturing_after = repository
        .get_ticket(workspace_id, manufacturing_ticket.id)
        .await
        .unwrap();
    assert_eq!(
        manufacturing_after.status,
        TicketStatus::Todo,
        "moving the fulfiller through its lanes must never rewrite its dependent's workflow status"
    );
    assert_eq!(inventory_event_count(&pool).await, 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn set_ticket_status_leaves_the_source_build_untouched(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_sde_type(&pool, import_id, 37164, "Isogen").await;
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

    let build_before: (i64, DateTime<Utc>) =
        sqlx::query_as("SELECT revision, updated_at FROM builds WHERE id = $1")
            .bind(build_id.0)
            .fetch_one(&pool)
            .await
            .unwrap();

    repository
        .set_ticket_status(workspace_id, ticket.id, TicketStatus::Complete)
        .await
        .unwrap();

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
async fn set_ticket_status_is_idempotent_for_a_no_op_write(pool: PgPool) {
    let (workspace_id, owner_id, _import_id) = fixture_workspace(&pool).await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            44,
            "Vexor",
            1,
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(ticket.status, TicketStatus::Todo);

    let again = repository
        .set_ticket_status(workspace_id, ticket.id, TicketStatus::Todo)
        .await
        .unwrap();
    assert_eq!(again.status, TicketStatus::Todo);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn set_ticket_status_rejects_a_ticket_from_another_workspace(pool: PgPool) {
    let (workspace_id, owner_id, _import_id) = fixture_workspace(&pool).await;
    let (other_workspace_id, _other_owner_id) = fixture_second_workspace(&pool).await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            44,
            "Vexor",
            1,
            None,
            None,
        ))
        .await
        .unwrap();

    let error = repository
        .set_ticket_status(other_workspace_id, ticket.id, TicketStatus::InProgress)
        .await
        .unwrap_err();
    assert!(matches!(error, OrderError::TicketNotFound));

    // The ticket in its real workspace is unchanged.
    let unchanged = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(unchanged.status, TicketStatus::Todo);
}
