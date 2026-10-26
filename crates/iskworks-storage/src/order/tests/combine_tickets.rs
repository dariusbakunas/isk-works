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
