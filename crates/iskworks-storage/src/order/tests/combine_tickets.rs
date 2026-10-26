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
