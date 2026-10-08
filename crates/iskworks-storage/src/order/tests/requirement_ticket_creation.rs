use super::*;
use iskworks_core::order::RequirementTicketCreation;

// ─── `create_ticket_for_order_requirement`: create + link, atomically ────
//
// The invariants these prove: minting a ticket for an Epic requirement and
// linking it (`order_requirement_fulfillments`) happen in one transaction --
// a failed link leaves no orphan ticket -- and at most one *active* minted
// ticket can exist per requirement, so a second (possibly concurrent) create
// reports the existing ticket instead of creating another one.

async fn order_with_buy_requirement(
    pool: &PgPool,
    repository: &PgOrderRepository,
) -> (WorkspaceId, OwnerId, OrderId, OrderRequirementId) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(pool).await;
    let build_id = fixture_build(pool, workspace_id, owner_id, import_id).await;
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
    (workspace_id, owner_id, order.id, requirements[0].id)
}

fn requirement_ticket(
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    order_id: OrderId,
) -> NewTicket {
    let mut new_ticket =
        new_acquisition_ticket(workspace_id, owner_id, 34, "Tritanium", 100, None, None);
    new_ticket.order_id = Some(order_id);
    new_ticket
}

async fn ticket_count(pool: &PgPool, order_id: OrderId) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM tickets WHERE order_id = $1")
        .bind(order_id.0)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_ticket_for_requirement_creates_and_links_the_ticket(pool: PgPool) {
    let repository = PgOrderRepository::new(pool.clone());
    let (workspace_id, owner_id, order_id, requirement_id) =
        order_with_buy_requirement(&pool, &repository).await;

    let outcome = repository
        .create_ticket_for_order_requirement(
            requirement_id,
            requirement_ticket(workspace_id, owner_id, order_id),
            100,
        )
        .await
        .unwrap();
    let RequirementTicketCreation::Created(ticket) = outcome else {
        panic!("expected a newly created ticket, got {outcome:?}");
    };
    assert_eq!(ticket.order_id, Some(order_id));
    assert_eq!(ticket.status, TicketStatus::Todo);

    let links = repository
        .list_order_requirement_fulfillments(requirement_id)
        .await
        .unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].ticket_id, ticket.id);
    assert_eq!(links[0].allocated_quantity, 100);
    assert_eq!(
        repository
            .get_ticket(workspace_id, ticket.id)
            .await
            .unwrap(),
        *ticket
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_failed_link_leaves_no_orphan_ticket(pool: PgPool) {
    let repository = PgOrderRepository::new(pool.clone());
    let (workspace_id, owner_id, order_id, requirement_id) =
        order_with_buy_requirement(&pool, &repository).await;

    // Make the *link* step fail after the ticket row has been inserted.
    sqlx::query(
        r#"
        CREATE FUNCTION fail_requirement_link() RETURNS trigger AS $$
        BEGIN
          RAISE EXCEPTION 'simulated link failure';
        END;
        $$ LANGUAGE plpgsql
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER fail_requirement_link BEFORE INSERT ON order_requirement_fulfillments \
         FOR EACH ROW EXECUTE FUNCTION fail_requirement_link()",
    )
    .execute(&pool)
    .await
    .unwrap();

    let new_ticket = requirement_ticket(workspace_id, owner_id, order_id);
    let ticket_id = new_ticket.id;
    let error = repository
        .create_ticket_for_order_requirement(requirement_id, new_ticket, 100)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("simulated link failure"),
        "unexpected error: {error}"
    );

    assert_eq!(ticket_count(&pool, order_id).await, 0, "no orphan ticket");
    assert!(matches!(
        repository.get_ticket(workspace_id, ticket_id).await,
        Err(OrderError::TicketNotFound)
    ));
    let claims: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM order_requirement_ticket_claims")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        claims, 0,
        "the requirement's claim rolled back with the ticket"
    );

    // The requirement is still free: once the link works again, a retry
    // creates (and links) the ticket.
    sqlx::query("DROP TRIGGER fail_requirement_link ON order_requirement_fulfillments")
        .execute(&pool)
        .await
        .unwrap();
    let retry = repository
        .create_ticket_for_order_requirement(
            requirement_id,
            requirement_ticket(workspace_id, owner_id, order_id),
            100,
        )
        .await
        .unwrap();
    assert!(matches!(retry, RequirementTicketCreation::Created(_)));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_second_create_for_the_same_requirement_reports_the_existing_ticket(pool: PgPool) {
    let repository = PgOrderRepository::new(pool.clone());
    let (workspace_id, owner_id, order_id, requirement_id) =
        order_with_buy_requirement(&pool, &repository).await;

    let RequirementTicketCreation::Created(first) = repository
        .create_ticket_for_order_requirement(
            requirement_id,
            requirement_ticket(workspace_id, owner_id, order_id),
            100,
        )
        .await
        .unwrap()
    else {
        panic!("first create must create");
    };

    let second = repository
        .create_ticket_for_order_requirement(
            requirement_id,
            requirement_ticket(workspace_id, owner_id, order_id),
            100,
        )
        .await
        .unwrap();
    assert_eq!(second, RequirementTicketCreation::AlreadyLinked(first.id));
    assert_eq!(ticket_count(&pool, order_id).await, 1);
    assert_eq!(
        repository
            .list_order_requirement_fulfillments(requirement_id)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_creates_for_the_same_requirement_yield_one_ticket(pool: PgPool) {
    let repository = PgOrderRepository::new(pool.clone());
    let (workspace_id, owner_id, order_id, requirement_id) =
        order_with_buy_requirement(&pool, &repository).await;

    let (left, right) = tokio::join!(
        repository.create_ticket_for_order_requirement(
            requirement_id,
            requirement_ticket(workspace_id, owner_id, order_id),
            100,
        ),
        repository.create_ticket_for_order_requirement(
            requirement_id,
            requirement_ticket(workspace_id, owner_id, order_id),
            100,
        ),
    );
    let outcomes = [left.unwrap(), right.unwrap()];
    let created: Vec<_> = outcomes
        .iter()
        .filter_map(|outcome| match outcome {
            RequirementTicketCreation::Created(ticket) => Some(ticket.id),
            RequirementTicketCreation::AlreadyLinked(_) => None,
        })
        .collect();
    assert_eq!(
        created.len(),
        1,
        "exactly one request creates: {outcomes:?}"
    );
    assert!(outcomes
        .iter()
        .any(|outcome| *outcome == RequirementTicketCreation::AlreadyLinked(created[0])));
    assert_eq!(ticket_count(&pool, order_id).await, 1);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn canceling_the_ticket_frees_the_requirement_for_a_new_one(pool: PgPool) {
    let repository = PgOrderRepository::new(pool.clone());
    let (workspace_id, owner_id, order_id, requirement_id) =
        order_with_buy_requirement(&pool, &repository).await;

    let RequirementTicketCreation::Created(first) = repository
        .create_ticket_for_order_requirement(
            requirement_id,
            requirement_ticket(workspace_id, owner_id, order_id),
            100,
        )
        .await
        .unwrap()
    else {
        panic!("first create must create");
    };
    repository
        .set_ticket_status(workspace_id, first.id, TicketStatus::Canceled)
        .await
        .unwrap();

    let RequirementTicketCreation::Created(second) = repository
        .create_ticket_for_order_requirement(
            requirement_id,
            requirement_ticket(workspace_id, owner_id, order_id),
            100,
        )
        .await
        .unwrap()
    else {
        panic!("a canceled ticket must not block a new one");
    };
    assert_ne!(second.id, first.id);

    // The canceled link stays as history; the new ticket now holds the claim.
    let links = repository
        .list_order_requirement_fulfillments(requirement_id)
        .await
        .unwrap();
    assert_eq!(links.len(), 2);
    let third = repository
        .create_ticket_for_order_requirement(
            requirement_id,
            requirement_ticket(workspace_id, owner_id, order_id),
            100,
        )
        .await
        .unwrap();
    assert_eq!(third, RequirementTicketCreation::AlreadyLinked(second.id));
}

// ─── Migration 202610080001_unique_requirement_ticket_link.sql ──────────
// Replays the migration against links created the pre-fix way (separate
// create + link, so duplicates are possible) and checks the backfill.

const UNIQUE_REQUIREMENT_TICKET_LINK_MIGRATION: &str =
    include_str!("../../../../../migrations/202610080001_unique_requirement_ticket_link.sql");

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn migration_backfills_claims_from_the_earliest_active_link(pool: PgPool) {
    let repository = PgOrderRepository::new(pool.clone());
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let snapshot = empty_price_snapshot();
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let (order, requirements) = repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: vec![
                requirement(34, RequirementKind::Buy, 100),
                requirement(34, RequirementKind::Buy, 100),
            ],
        })
        .await
        .unwrap();
    let (duplicated, all_canceled) = (requirements[0].id, requirements[1].id);

    sqlx::raw_sql("DROP TABLE order_requirement_ticket_claims")
        .execute(&pool)
        .await
        .unwrap();

    // Pre-fix shape: a canceled first ticket, then two racing active ones.
    let mut linked = Vec::new();
    for requirement_id in [duplicated, duplicated, duplicated, all_canceled] {
        let (ticket, _) = repository
            .create_ticket(requirement_ticket(workspace_id, owner_id, order.id))
            .await
            .unwrap();
        repository
            .link_order_requirement_to_ticket(requirement_id, ticket.id, 100)
            .await
            .unwrap();
        linked.push(ticket.id);
    }
    for canceled in [linked[0], linked[3]] {
        repository
            .set_ticket_status(workspace_id, canceled, TicketStatus::Canceled)
            .await
            .unwrap();
    }

    sqlx::raw_sql(UNIQUE_REQUIREMENT_TICKET_LINK_MIGRATION)
        .execute(&pool)
        .await
        .unwrap();

    let claims: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT order_requirement_id, ticket_id FROM order_requirement_ticket_claims",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        claims,
        vec![(duplicated.0, linked[1].0)],
        "earliest active link wins; an all-canceled requirement stays unclaimed"
    );
    // Nothing was deleted.
    assert_eq!(ticket_count(&pool, order.id).await, 4);
    assert_eq!(
        repository
            .create_ticket_for_order_requirement(
                duplicated,
                requirement_ticket(workspace_id, owner_id, order.id),
                100,
            )
            .await
            .unwrap(),
        RequirementTicketCreation::AlreadyLinked(linked[1])
    );
}
