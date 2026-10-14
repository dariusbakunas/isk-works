use super::*;

// ─── Canonical manual Ticket creation: Generic + assignee ─────────────────
// `TicketKind::Generic` -- a standalone organizational work item with no
// item/quantity/execution/recording contract -- plus `assignee_character_id`,
// the connected-character FK every manually created Ticket may optionally
// carry.

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn generic_ticket_created_standalone_has_no_relationships(pool: PgPool) {
    let (workspace_id, owner_id, _import_id) = fixture_workspace(&pool).await;
    let repository = PgOrderRepository::new(pool.clone());

    let (ticket, prerequisites) = repository
        .create_ticket(generic_ticket(
            workspace_id,
            owner_id,
            "Move blueprints to C-J6MT",
        ))
        .await
        .unwrap();

    assert_eq!(ticket.kind, TicketKind::Generic);
    assert_eq!(ticket.type_id, None);
    assert_eq!(ticket.quantity, None);
    assert_eq!(ticket.source_build_id, None);
    assert_eq!(ticket.execution_snapshot, None);
    assert_eq!(ticket.order_id, None);
    assert_eq!(ticket.assignee_character_id, None);
    assert_eq!(ticket.notes, "");
    assert_eq!(ticket.status, TicketStatus::Todo);
    assert!(prerequisites.is_empty());

    // Genuinely no side effects of any kind.
    assert!(allocation_snapshot(&pool).await.is_empty());
    assert_eq!(inventory_event_count(&pool).await, 0);
    let fulfillment_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM order_requirement_fulfillments")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(fulfillment_count, 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn generic_ticket_with_epic_and_assignee_persists_both(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());

    let snapshot = empty_price_snapshot();
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let (order, _) = repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: vec![],
        })
        .await
        .unwrap();
    let character = fixture_character(&pool, workspace_id, owner_id, "Alt One").await;

    let mut new_ticket = generic_ticket(workspace_id, owner_id, "Move blueprints to C-J6MT");
    new_ticket.order_id = Some(order.id);
    new_ticket.assignee_character_id = Some(character);
    new_ticket.notes = "Use covert route".to_string();
    let (ticket, _) = repository.create_ticket(new_ticket).await.unwrap();

    assert_eq!(ticket.order_id, Some(order.id));
    assert_eq!(ticket.assignee_character_id, Some(character));
    assert_eq!(ticket.notes, "Use covert route");
    let reloaded = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(reloaded.order_id, Some(order.id));
    assert_eq!(reloaded.assignee_character_id, Some(character));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn clearing_the_assignee_via_metadata_update_leaves_everything_else_unchanged(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());

    let snapshot = empty_price_snapshot();
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let (order, _) = repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: vec![],
        })
        .await
        .unwrap();
    let character = fixture_character(&pool, workspace_id, owner_id, "Alt One").await;

    let mut new_ticket = generic_ticket(workspace_id, owner_id, "Move blueprints");
    new_ticket.order_id = Some(order.id);
    new_ticket.assignee_character_id = Some(character);
    let (ticket, _) = repository.create_ticket(new_ticket).await.unwrap();

    let cleared = repository
        .update_ticket_metadata(
            workspace_id,
            ticket.id,
            TicketMetadataUpdate {
                captured_name: None,
                notes: None,
                assignee_character_id: Some(None),
            },
        )
        .await
        .unwrap();

    assert_eq!(cleared.order_id, Some(order.id), "the Epic never changes");
    assert_eq!(cleared.assignee_character_id, None);
    // Everything else survives untouched -- the Ticket itself, not merely
    // the relationships.
    assert_eq!(cleared.id, ticket.id);
    assert_eq!(cleared.captured_name, "Move blueprints");
    assert_eq!(cleared.status, ticket.status);
    assert_eq!(cleared.kind, TicketKind::Generic);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reassigning_a_ticket_via_metadata_update_touches_nothing_else(pool: PgPool) {
    let (workspace_id, owner_id, _import_id) = fixture_workspace(&pool).await;
    let repository = PgOrderRepository::new(pool.clone());
    let character_a = fixture_character(&pool, workspace_id, owner_id, "Alt One").await;
    let character_b = fixture_character(&pool, workspace_id, owner_id, "Alt Two").await;

    let mut new_ticket = generic_ticket(workspace_id, owner_id, "Move blueprints");
    new_ticket.assignee_character_id = Some(character_a);
    let (ticket, _) = repository.create_ticket(new_ticket).await.unwrap();
    let status_before = ticket.status;

    let reassigned = repository
        .update_ticket_metadata(
            workspace_id,
            ticket.id,
            TicketMetadataUpdate {
                captured_name: None,
                notes: None,
                assignee_character_id: Some(Some(character_b)),
            },
        )
        .await
        .unwrap();

    assert_eq!(reassigned.assignee_character_id, Some(character_b));
    assert_eq!(reassigned.status, status_before);
    assert_eq!(
        reassigned.order_id, None,
        "reassignment never touches Epic membership"
    );
    assert_eq!(reassigned.captured_name, "Move blueprints");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn disconnecting_a_character_is_a_soft_status_change_and_does_not_clear_the_assignee(
    pool: PgPool,
) {
    // `eve_connections`' own "disconnect" is `status = 'disconnected'`,
    // never a row delete (see `PgInventoryRepository::disconnect`) -- so
    // the FK stays intact and the assignee reference survives exactly as
    // set. This is deliberately different from a hard delete (tested
    // next): the Ticket still remembers *who* it was assigned to even
    // after their connection lapses.
    let (workspace_id, owner_id, _import_id) = fixture_workspace(&pool).await;
    let repository = PgOrderRepository::new(pool.clone());
    let character = fixture_character(&pool, workspace_id, owner_id, "Alt One").await;

    let mut new_ticket = generic_ticket(workspace_id, owner_id, "Move blueprints");
    new_ticket.assignee_character_id = Some(character);
    let (ticket, _) = repository.create_ticket(new_ticket).await.unwrap();

    sqlx::query(
        "UPDATE eve_connections SET status = 'disconnected', disconnected_at = now() WHERE id = $1",
    )
    .bind(character.0)
    .execute(&pool)
    .await
    .unwrap();

    let reloaded = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(reloaded.assignee_character_id, Some(character));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn hard_deleting_a_connected_character_sets_the_assignee_to_null_and_does_not_delete_the_ticket(
    pool: PgPool,
) {
    let (workspace_id, owner_id, _import_id) = fixture_workspace(&pool).await;
    let repository = PgOrderRepository::new(pool.clone());
    let character = fixture_character(&pool, workspace_id, owner_id, "Alt One").await;

    let mut new_ticket = generic_ticket(workspace_id, owner_id, "Move blueprints");
    new_ticket.assignee_character_id = Some(character);
    let (ticket, _) = repository.create_ticket(new_ticket).await.unwrap();

    // `eve_connections` has no hard-delete route today either -- this
    // proves the FK's `ON DELETE SET NULL` behavior directly, for
    // whenever one is added.
    sqlx::query("DELETE FROM eve_connections WHERE id = $1")
        .bind(character.0)
        .execute(&pool)
        .await
        .unwrap();

    let reloaded = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(
        reloaded.assignee_character_id, None,
        "the ticket survives, with the assignee cleared -- never cascade-deleted"
    );
    assert_eq!(reloaded.id, ticket.id);
    assert_eq!(reloaded.captured_name, "Move blueprints");
}
