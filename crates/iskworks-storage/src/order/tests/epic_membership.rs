use super::*;

// ─── Explicit Ticket -> Order/Epic membership ─────────────────────────────
// `tickets.order_id` is the authoritative answer to "which Order/Epic
// organizationally contains this ticket?" -- independent of
// `source_build_id` (which Build a ticket executes) and of
// `order_requirement_fulfillments` (which frozen requirement(s) a ticket
// contributes to/fulfills, possibly for a *different* Order -- see
// `link_order_requirement_to_ticket`).

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn standalone_ticket_has_no_epic_membership(pool: PgPool) {
    let (workspace_id, owner_id, _import_id) = fixture_workspace(&pool).await;
    let repository = PgOrderRepository::new(pool.clone());

    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
            None,
            None,
        ))
        .await
        .unwrap();

    assert_eq!(ticket.order_id, None);
    let reloaded = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(reloaded.order_id, None);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn ticket_created_for_an_order_persists_explicit_epic_membership(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());
    let snapshot = empty_price_snapshot();
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let (order, _) = repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: vec![requirement(34, RequirementKind::Buy, 100)],
        })
        .await
        .unwrap();

    let mut new_ticket =
        new_acquisition_ticket(workspace_id, owner_id, 34, "Tritanium", 100, None, None);
    new_ticket.order_id = Some(order.id);
    let (ticket, _) = repository.create_ticket(new_ticket).await.unwrap();

    assert_eq!(ticket.order_id, Some(order.id));
    let reloaded = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(reloaded.order_id, Some(order.id));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn list_tickets_for_order_returns_only_that_orders_tickets(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());

    let snapshot_1 = empty_price_snapshot();
    let order_1 = draft_order(workspace_id, owner_id, build_id, snapshot_1.id);
    let (order_1, _) = repository
        .create_order(NewOrder {
            order: order_1,
            price_snapshot: snapshot_1,
            requirements: vec![],
        })
        .await
        .unwrap();

    let snapshot_2 = empty_price_snapshot();
    let order_2 = draft_order(workspace_id, owner_id, build_id, snapshot_2.id);
    let (order_2, _) = repository
        .create_order(NewOrder {
            order: order_2,
            price_snapshot: snapshot_2,
            requirements: vec![],
        })
        .await
        .unwrap();

    let mut ticket_a =
        new_acquisition_ticket(workspace_id, owner_id, 34, "Tritanium", 100, None, None);
    ticket_a.order_id = Some(order_1.id);
    let (ticket_a, _) = repository.create_ticket(ticket_a).await.unwrap();

    let mut ticket_b =
        new_acquisition_ticket(workspace_id, owner_id, 35, "Pyerite", 200, None, None);
    ticket_b.order_id = Some(order_1.id);
    let (ticket_b, _) = repository.create_ticket(ticket_b).await.unwrap();

    let mut ticket_c =
        new_acquisition_ticket(workspace_id, owner_id, 36, "Mexallon", 300, None, None);
    ticket_c.order_id = Some(order_2.id);
    let (ticket_c, _) = repository.create_ticket(ticket_c).await.unwrap();

    // A standalone ticket -- no Epic at all.
    let standalone = new_acquisition_ticket(workspace_id, owner_id, 37, "Isogen", 400, None, None);
    let (ticket_d, _) = repository.create_ticket(standalone).await.unwrap();

    let order_1_tickets = repository
        .list_tickets_for_order(workspace_id, order_1.id)
        .await
        .unwrap();
    let ids: std::collections::HashSet<_> = order_1_tickets.iter().map(|t| t.id).collect();
    assert_eq!(ids, [ticket_a.id, ticket_b.id].into_iter().collect());
    assert!(!ids.contains(&ticket_c.id));
    assert!(!ids.contains(&ticket_d.id));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deleting_an_order_sets_its_tickets_order_id_to_null_and_does_not_delete_them(
    pool: PgPool,
) {
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

    let mut new_ticket =
        new_acquisition_ticket(workspace_id, owner_id, 34, "Tritanium", 100, None, None);
    new_ticket.order_id = Some(order.id);
    let (ticket, _) = repository.create_ticket(new_ticket).await.unwrap();
    assert_eq!(ticket.order_id, Some(order.id));

    // Orders have no hard-delete route today (only archive/restore) -- this
    // proves the FK's `ON DELETE SET NULL` behavior directly, for whenever
    // one is added, and as a safety net for any future administrative
    // cleanup that does delete an Order row.
    sqlx::query("DELETE FROM orders WHERE id = $1")
        .bind(order.id.0)
        .execute(&pool)
        .await
        .unwrap();

    let reloaded = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(
        reloaded.order_id, None,
        "the ticket survives, with membership cleared -- never cascade-deleted"
    );
    assert_eq!(reloaded.id, ticket.id);
    assert_eq!(reloaded.status, ticket.status);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn epic_membership_is_independent_of_requirement_fulfillment_linkage(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());

    let snapshot_1 = empty_price_snapshot();
    let order_1 = draft_order(workspace_id, owner_id, build_id, snapshot_1.id);
    let (order_1, _) = repository
        .create_order(NewOrder {
            order: order_1,
            price_snapshot: snapshot_1,
            requirements: vec![],
        })
        .await
        .unwrap();

    let snapshot_2 = empty_price_snapshot();
    let order_2 = draft_order(workspace_id, owner_id, build_id, snapshot_2.id);
    let (order_2, requirements_2) = repository
        .create_order(NewOrder {
            order: order_2,
            price_snapshot: snapshot_2,
            requirements: vec![requirement(34, RequirementKind::Buy, 100)],
        })
        .await
        .unwrap();
    let requirement_2 = requirements_2[0].id;

    // A ticket that belongs to Epic 1 ...
    let mut new_ticket =
        new_acquisition_ticket(workspace_id, owner_id, 34, "Tritanium", 100, None, None);
    new_ticket.order_id = Some(order_1.id);
    let (ticket, _) = repository.create_ticket(new_ticket).await.unwrap();

    // ... is reused (the mechanism behind `link_ticket_to_requirement`) to
    // fulfill Epic 2's own requirement for the same material.
    repository
        .link_order_requirement_to_ticket(requirement_2, ticket.id, 100)
        .await
        .unwrap();

    // Fulfilling another Epic's requirement never moves organizational
    // membership -- the ticket still belongs to Epic 1 alone.
    let reloaded = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(reloaded.order_id, Some(order_1.id));

    let epic_1_tickets = repository
        .list_tickets_for_order(workspace_id, order_1.id)
        .await
        .unwrap();
    assert_eq!(
        epic_1_tickets.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![ticket.id]
    );
    let epic_2_tickets = repository
        .list_tickets_for_order(workspace_id, order_2.id)
        .await
        .unwrap();
    assert!(
        epic_2_tickets.is_empty(),
        "fulfillment linkage never grants Epic membership"
    );
}
