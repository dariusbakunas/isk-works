use super::*;

// ─── Combining tickets: persistence groundwork ────────────────────────────
// A combined ticket's members point at it through `merged_into_ticket_id`;
// its copies of their prerequisites carry the Epic each one serves.

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn merged_into_ticket_id_round_trips_and_clears_on_delete(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());
    let new_ticket = |name: &str| {
        mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Reaction,
            16_658,
            name,
            build_id,
            Vec::new(),
            Some(1),
        )
    };
    let (member, _) = repository
        .create_ticket(new_ticket("Hexite"))
        .await
        .unwrap();
    let (combined, _) = repository
        .create_ticket(new_ticket("Hexite"))
        .await
        .unwrap();
    assert_eq!(member.merged_into_ticket_id, None);

    sqlx::query("UPDATE tickets SET merged_into_ticket_id = $1 WHERE id = $2")
        .bind(combined.id.0)
        .bind(member.id.0)
        .execute(&pool)
        .await
        .unwrap();
    let reloaded = repository
        .get_ticket(workspace_id, member.id)
        .await
        .unwrap();
    assert_eq!(reloaded.merged_into_ticket_id, Some(combined.id));

    repository
        .delete_ticket(workspace_id, combined.id)
        .await
        .unwrap();
    let reloaded = repository
        .get_ticket(workspace_id, member.id)
        .await
        .unwrap();
    assert_eq!(reloaded.merged_into_ticket_id, None);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn prerequisite_epic_round_trips(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());
    let snapshot = empty_price_snapshot();
    let (order, _) = repository
        .create_order(NewOrder {
            order: draft_order(workspace_id, owner_id, build_id, snapshot.id),
            price_snapshot: snapshot,
            requirements: vec![requirement(34, RequirementKind::Buy, 100)],
        })
        .await
        .unwrap();

    let mut tagged = prod_prereq(34, "Tritanium");
    tagged.order_id = Some(order.id);
    let untagged = prod_prereq(35, "Pyerite");
    let (ticket, prerequisites) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            587,
            "Rifter",
            build_id,
            vec![tagged, untagged],
            Some(1),
        ))
        .await
        .unwrap();
    let created: Vec<_> = prerequisites
        .iter()
        .map(|p| (p.type_id, p.order_id))
        .collect();
    assert_eq!(created, vec![(34, Some(order.id)), (35, None)]);

    let reloaded = repository
        .list_ticket_prerequisites(ticket.id)
        .await
        .unwrap();
    let reloaded: Vec<_> = reloaded.iter().map(|p| (p.type_id, p.order_id)).collect();
    assert_eq!(reloaded, vec![(34, Some(order.id)), (35, None)]);
}

async fn claim(pool: &PgPool, order_id: OrderId, occurrence_key: &str, ticket_id: TicketId) {
    sqlx::query(
        "INSERT INTO order_operation_ticket_claims (order_id, occurrence_key, ticket_id, claimed_at) \
         VALUES ($1, $2, $3, now())",
    )
    .bind(order_id.0)
    .bind(occurrence_key)
    .bind(ticket_id.0)
    .execute(pool)
    .await
    .unwrap();
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn ticket_operations_come_from_claims_with_an_unclaimed_fallback(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());
    let mut orders = Vec::new();
    for _ in 0..2 {
        let snapshot = empty_price_snapshot();
        let (order, _) = repository
            .create_order(NewOrder {
                order: draft_order(workspace_id, owner_id, build_id, snapshot.id),
                price_snapshot: snapshot,
                requirements: vec![requirement(34, RequirementKind::Buy, 100)],
            })
            .await
            .unwrap();
        orders.push(order.id);
    }
    let mut orders_sorted = orders.clone();
    orders_sorted.sort();
    let step_ticket = |order_id: OrderId| {
        let mut ticket = mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Reaction,
            16_658,
            "Hexite",
            build_id,
            Vec::new(),
            Some(1),
        );
        ticket.order_id = Some(order_id);
        ticket.occurrence_key = Some("hexite".to_string());
        ticket
    };
    let (claimed, _) = repository
        .create_ticket(step_ticket(orders[0]))
        .await
        .unwrap();
    claim(&pool, orders[0], "hexite", claimed.id).await;
    let (unclaimed, _) = repository
        .create_ticket(step_ticket(orders[1]))
        .await
        .unwrap();
    let (combined, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Reaction,
            16_658,
            "Hexite",
            build_id,
            Vec::new(),
            Some(2),
        ))
        .await
        .unwrap();
    claim(&pool, orders[0], "hexite-2", combined.id).await;
    claim(&pool, orders[1], "hexite-2", combined.id).await;
    let (acquisition, _) = repository
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

    let operations = repository
        .list_ticket_operations(&[claimed.id, unclaimed.id, combined.id, acquisition.id])
        .await
        .unwrap();

    let op = |order_id: OrderId, key: &str| TicketOperation {
        order_id,
        occurrence_key: key.to_string(),
    };
    assert_eq!(operations[&claimed.id], vec![op(orders[0], "hexite")]);
    assert_eq!(operations[&unclaimed.id], vec![op(orders[1], "hexite")]);
    assert_eq!(
        operations[&combined.id],
        vec![
            op(orders_sorted[0], "hexite-2"),
            op(orders_sorted[1], "hexite-2")
        ]
    );
    assert!(!operations.contains_key(&acquisition.id));
}

/// An Epic with one 100-unit requirement of `type_id` for its `hexite`
/// operation, holding a 100-unit reservation for it. Returns the Epic and
/// the reservation id.
async fn epic_holding(
    pool: &PgPool,
    repository: &PgOrderRepository,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    build_id: BuildId,
    type_id: i64,
) -> (OrderId, Uuid) {
    let snapshot = empty_price_snapshot();
    let mut need = requirement(type_id, RequirementKind::Buy, 100);
    need.operation_occurrence_key = Some("hexite".to_string());
    let (order, _) = repository
        .create_order(NewOrder {
            order: draft_order(workspace_id, owner_id, build_id, snapshot.id),
            price_snapshot: snapshot,
            requirements: vec![need],
        })
        .await
        .unwrap();
    let requirement_id: Uuid =
        sqlx::query_scalar("SELECT id FROM order_requirements WHERE order_id = $1")
            .bind(order.id.0)
            .fetch_one(pool)
            .await
            .unwrap();
    let allocation_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO inventory_allocations \
         (id, workspace_id, owner_id, type_id, quantity, order_requirement_id, created_at, reason) \
         VALUES ($1, $2, $3, $4, 100, $5, now(), 'epic_create')",
    )
    .bind(allocation_id)
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(type_id)
    .bind(requirement_id)
    .execute(pool)
    .await
    .unwrap();
    (order.id, allocation_id)
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn recording_draws_from_every_epic_the_ticket_executes(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_sde_type(&pool, import_id, 16_658, "Hexite").await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let repository = PgOrderRepository::new(pool.clone());
    seed_inventory_balance_with_cost(&pool, workspace_id, owner_id, 34, "Tritanium", 300, "1500")
        .await;
    let mut epics = Vec::new();
    for _ in 0..3 {
        epics.push(epic_holding(&pool, &repository, workspace_id, owner_id, build_id, 34).await);
    }
    let (ticket, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Reaction,
            16_658,
            "Hexite",
            build_id,
            vec![prod_prereq(34, "Tritanium")],
            Some(2),
        ))
        .await
        .unwrap();
    claim(&pool, epics[0].0, "hexite", ticket.id).await;
    claim(&pool, epics[1].0, "hexite", ticket.id).await;

    // Free stock is 0: all 300 units are reserved. Only the two claimed
    // Epics' reservations are the ticket's own.
    repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(2, 16_658, 0, &[(34, 200)], "0"),
        )
        .await
        .unwrap();

    let consumed: Vec<(Uuid, bool)> = sqlx::query_as(
        "SELECT id, consumed_at IS NOT NULL FROM inventory_allocations \
         WHERE id = ANY($1) AND released_at IS NULL",
    )
    .bind(
        epics
            .iter()
            .map(|(_, allocation)| *allocation)
            .collect::<Vec<_>>(),
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let consumed: std::collections::HashMap<Uuid, bool> = consumed.into_iter().collect();
    assert!(consumed[&epics[0].1]);
    assert!(consumed[&epics[1].1]);
    assert!(!consumed[&epics[2].1], "an unclaimed Epic keeps its stock");
}
