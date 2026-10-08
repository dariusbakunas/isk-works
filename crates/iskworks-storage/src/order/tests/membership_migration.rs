use super::*;

// ─── Migration backfill (202609040002_ticket_order_membership.sql) ───────
// The migration's two backfill UPDATEs run once, automatically, against an
// empty database when `#[sqlx::test]` applies every migration up front --
// there's nothing to observe on an empty DB. These tests instead seed data
// shaped like it predates `tickets.order_id` (created with `order_id =
// NULL`, exactly what a pre-migration row looks like once the column
// exists) and replay the migration's own backfill SQL verbatim against it,
// proving its set-based logic picks only the unambiguous cases and leaves
// every ambiguous/unrelated one `NULL`.

/// The exact two backfill `UPDATE`s from
/// `migrations/202609040002_ticket_order_membership.sql` (the column/index
/// DDL already ran once when `sqlx::test` applied every migration, so only
/// the backfill logic itself is replayed here, against manually-seeded
/// "legacy" rows).
async fn replay_ticket_order_membership_backfill(pool: &PgPool) {
    sqlx::query(
        r#"
        UPDATE tickets t
        SET order_id = unambiguous.order_id
        FROM (
          SELECT
            orf.ticket_id,
            min(oreq.order_id::text)::uuid AS order_id,
            count(DISTINCT oreq.order_id) AS distinct_orders
          FROM order_requirement_fulfillments orf
          JOIN order_requirements oreq ON oreq.id = orf.order_requirement_id
          GROUP BY orf.ticket_id
        ) unambiguous
        WHERE t.id = unambiguous.ticket_id
          AND unambiguous.distinct_orders = 1
        "#,
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        UPDATE tickets t
        SET order_id = unambiguous.order_id
        FROM (
          SELECT
            ticket.id AS ticket_id,
            min(candidate_order.id::text)::uuid AS order_id,
            count(DISTINCT candidate_order.id) AS distinct_orders
          FROM tickets ticket
          JOIN orders candidate_order ON candidate_order.source_build_id = ticket.source_build_id
          WHERE ticket.kind IN ('manufacturing', 'reaction')
            AND ticket.order_id IS NULL
          GROUP BY ticket.id
        ) unambiguous
        WHERE t.id = unambiguous.ticket_id
          AND unambiguous.distinct_orders = 1
        "#,
    )
    .execute(pool)
    .await
    .unwrap();
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn backfill_assigns_a_ticket_with_one_unambiguous_requirement_link(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());

    let snapshot = empty_price_snapshot();
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let (order, requirements) = repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: vec![requirement(34, RequirementKind::Buy, 100)],
        })
        .await
        .unwrap();

    // A "legacy" ticket -- created with no `order_id`, only a fulfillment
    // link, exactly what a pre-migration requirement ticket looks like.
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
    repository
        .link_order_requirement_to_ticket(requirements[0].id, ticket.id, 100)
        .await
        .unwrap();
    assert_eq!(
        repository
            .get_ticket(workspace_id, ticket.id)
            .await
            .unwrap()
            .order_id,
        None,
        "sanity: not yet backfilled"
    );

    replay_ticket_order_membership_backfill(&pool).await;

    let reloaded = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(reloaded.order_id, Some(order.id));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn backfill_leaves_a_ticket_linked_to_two_distinct_orders_ambiguous(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());

    let snapshot_1 = empty_price_snapshot();
    let order_1 = draft_order(workspace_id, owner_id, build_id, snapshot_1.id);
    let (_order_1, requirements_1) = repository
        .create_order(NewOrder {
            order: order_1,
            price_snapshot: snapshot_1,
            requirements: vec![requirement(34, RequirementKind::Buy, 100)],
        })
        .await
        .unwrap();

    let snapshot_2 = empty_price_snapshot();
    let order_2 = draft_order(workspace_id, owner_id, build_id, snapshot_2.id);
    let (_order_2, requirements_2) = repository
        .create_order(NewOrder {
            order: order_2,
            price_snapshot: snapshot_2,
            requirements: vec![requirement(34, RequirementKind::Buy, 100)],
        })
        .await
        .unwrap();

    // One ticket, reused (`link_ticket_to_requirement`'s underlying
    // mechanism) to fulfill *both* Orders' equivalent requirements --
    // genuinely ambiguous historical membership.
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
    repository
        .link_order_requirement_to_ticket(requirements_1[0].id, ticket.id, 100)
        .await
        .unwrap();
    repository
        .link_order_requirement_to_ticket(requirements_2[0].id, ticket.id, 100)
        .await
        .unwrap();

    replay_ticket_order_membership_backfill(&pool).await;

    let reloaded = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(
        reloaded.order_id, None,
        "never arbitrarily assigned to one of the two candidate Orders"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn backfill_leaves_a_standalone_legacy_ticket_null(pool: PgPool) {
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

    replay_ticket_order_membership_backfill(&pool).await;

    let reloaded = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(
        reloaded.order_id, None,
        "NULL is a legitimate, expected outcome"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn backfill_assigns_an_unambiguous_root_ticket_by_source_build_id(pool: PgPool) {
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

    // A "legacy" root ticket -- no requirement fulfillment at all (the
    // root ticket never fulfills one), only a shared `source_build_id`
    // with exactly one Order.
    let root_ticket = mfg_ticket(
        workspace_id,
        owner_id,
        TicketKind::Manufacturing,
        5876,
        "Rifter",
        build_id,
        Vec::new(),
        Some(1),
    );
    let (ticket, _) = repository.create_ticket(root_ticket).await.unwrap();

    replay_ticket_order_membership_backfill(&pool).await;

    let reloaded = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(reloaded.order_id, Some(order.id));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn backfill_leaves_a_root_ticket_null_when_two_orders_share_its_source_build(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());

    // Two Orders from the same Build -- a root ticket with only that
    // `source_build_id` cannot be attributed to either one.
    let snapshot_1 = empty_price_snapshot();
    let order_1 = draft_order(workspace_id, owner_id, build_id, snapshot_1.id);
    repository
        .create_order(NewOrder {
            order: order_1,
            price_snapshot: snapshot_1,
            requirements: vec![],
        })
        .await
        .unwrap();
    let snapshot_2 = empty_price_snapshot();
    let order_2 = draft_order(workspace_id, owner_id, build_id, snapshot_2.id);
    repository
        .create_order(NewOrder {
            order: order_2,
            price_snapshot: snapshot_2,
            requirements: vec![],
        })
        .await
        .unwrap();

    let root_ticket = mfg_ticket(
        workspace_id,
        owner_id,
        TicketKind::Manufacturing,
        5876,
        "Rifter",
        build_id,
        Vec::new(),
        Some(1),
    );
    let (ticket, _) = repository.create_ticket(root_ticket).await.unwrap();

    replay_ticket_order_membership_backfill(&pool).await;

    let reloaded = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(
        reloaded.order_id, None,
        "genuinely ambiguous historical root-ticket membership is left NULL, never guessed"
    );
}
