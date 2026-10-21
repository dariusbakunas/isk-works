use super::*;

/// The `*_for_*` reads used by list views return exactly what the per-id
/// reads do, concatenated in id order, so a list view can swap one for the
/// other without changing a response.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn batched_ticket_reads_match_the_per_ticket_reads(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    for (type_id, name) in [(34, "Tritanium"), (35, "Pyerite"), (20185, "Product")] {
        seed_sde_type(&pool, import_id, type_id, name).await;
    }
    let repository = PgOrderRepository::new(pool.clone());
    let (acquisition, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
        ))
        .await
        .unwrap();
    for quantity in [60, 40] {
        repository
            .record_ticket_acquisition(
                workspace_id,
                acquisition.id,
                acquisition_input(quantity, Some("4")),
            )
            .await
            .unwrap();
    }
    let (producer, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            20185,
            "Product",
            build_id,
            vec![prod_prereq(34, "Tritanium"), prod_prereq(35, "Pyerite")],
            None,
        ))
        .await
        .unwrap();
    repository
        .record_ticket_production(
            workspace_id,
            producer.id,
            production_input(1, 20185, 10, &[(34, 50)], "0"),
        )
        .await
        .unwrap();
    let (untouched, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            35,
            "Pyerite",
            5,
        ))
        .await
        .unwrap();
    let producer_prerequisites = repository
        .list_ticket_prerequisites(producer.id)
        .await
        .unwrap();
    for prerequisite in &producer_prerequisites {
        let fulfilling = if prerequisite.type_id == 34 {
            acquisition.id
        } else {
            untouched.id
        };
        repository
            .link_ticket_prerequisite_to_ticket(prerequisite.id, fulfilling, 5)
            .await
            .unwrap();
    }

    let mut ticket_ids = vec![acquisition.id, producer.id, untouched.id];
    ticket_ids.sort_by_key(|id| id.0);

    let mut prerequisites = Vec::new();
    let mut recordings = Vec::new();
    for ticket_id in &ticket_ids {
        prerequisites.extend(
            repository
                .list_ticket_prerequisites(*ticket_id)
                .await
                .unwrap(),
        );
        recordings.extend(
            repository
                .list_ticket_inventory_recordings(*ticket_id)
                .await
                .unwrap(),
        );
    }
    assert_eq!(prerequisites.len(), 2);
    assert_eq!(recordings.len(), 3);
    assert!(recordings
        .iter()
        .all(|recording| !recording.effects.is_empty()));
    assert_eq!(
        repository
            .list_ticket_prerequisites_for_tickets(&ticket_ids)
            .await
            .unwrap(),
        prerequisites
    );
    assert_eq!(
        repository
            .list_ticket_inventory_recordings_for_tickets(&ticket_ids)
            .await
            .unwrap(),
        recordings
    );

    let mut prerequisite_ids: Vec<_> = producer_prerequisites.iter().map(|p| p.id).collect();
    prerequisite_ids.sort_by_key(|id| id.0);
    let mut fulfillments = Vec::new();
    for prerequisite_id in &prerequisite_ids {
        fulfillments.extend(
            repository
                .list_ticket_prerequisite_fulfillments(*prerequisite_id)
                .await
                .unwrap(),
        );
    }
    assert_eq!(fulfillments.len(), 2);
    assert_eq!(
        repository
            .list_ticket_prerequisite_fulfillments_for_prerequisites(&prerequisite_ids)
            .await
            .unwrap(),
        fulfillments
    );

    assert!(repository
        .list_ticket_prerequisites_for_tickets(&[])
        .await
        .unwrap()
        .is_empty());
    assert!(repository
        .list_ticket_inventory_recordings_for_tickets(&[])
        .await
        .unwrap()
        .is_empty());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn batched_order_reads_match_the_per_order_reads(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    seed_sde_type(&pool, import_id, 35, "Pyerite").await;
    let repository = PgOrderRepository::new(pool.clone());

    let mut order_ids = Vec::new();
    let mut ticket_ids = Vec::new();
    for _ in 0..2 {
        let snapshot = empty_price_snapshot();
        let (order, requirements) = repository
            .create_order(NewOrder {
                order: draft_order(workspace_id, owner_id, build_id, snapshot.id),
                price_snapshot: snapshot,
                requirements: vec![
                    requirement(34, RequirementKind::Buy, 100),
                    requirement(35, RequirementKind::Buy, 50),
                ],
            })
            .await
            .unwrap();
        for requirement in &requirements {
            let mut new_ticket = new_acquisition_ticket(
                workspace_id,
                owner_id,
                requirement.type_id,
                "Mineral",
                requirement.required_quantity,
                None,
                None,
            );
            new_ticket.order_id = Some(order.id);
            let (ticket, _) = repository.create_ticket(new_ticket).await.unwrap();
            repository
                .link_order_requirement_to_ticket(
                    requirement.id,
                    ticket.id,
                    requirement.required_quantity,
                )
                .await
                .unwrap();
            ticket_ids.push(ticket.id);
        }
        order_ids.push(order.id);
    }
    order_ids.sort_by_key(|id| id.0);

    let mut requirements = Vec::new();
    let mut tickets = Vec::new();
    let mut operations = Vec::new();
    for order_id in &order_ids {
        requirements.extend(repository.list_order_requirements(*order_id).await.unwrap());
        tickets.extend(
            repository
                .list_tickets_for_order(workspace_id, *order_id)
                .await
                .unwrap(),
        );
        operations.extend(
            repository
                .list_order_plan_operations(*order_id)
                .await
                .unwrap(),
        );
    }
    assert_eq!(requirements.len(), 4);
    assert_eq!(tickets.len(), 4);
    assert_eq!(
        repository
            .list_order_requirements_for_orders(&order_ids)
            .await
            .unwrap(),
        requirements
    );
    assert_eq!(
        repository
            .list_tickets_for_orders(workspace_id, &order_ids)
            .await
            .unwrap(),
        tickets
    );
    assert_eq!(
        repository
            .list_order_plan_operations_for_orders(&order_ids)
            .await
            .unwrap(),
        operations
    );

    let mut requirement_ids: Vec<_> = requirements.iter().map(|r| r.id).collect();
    requirement_ids.sort_by_key(|id| id.0);
    let mut fulfillments = Vec::new();
    for requirement_id in &requirement_ids {
        fulfillments.extend(
            repository
                .list_order_requirement_fulfillments(*requirement_id)
                .await
                .unwrap(),
        );
    }
    assert_eq!(fulfillments.len(), 4);
    assert_eq!(
        repository
            .list_order_requirement_fulfillments_for_requirements(&requirement_ids)
            .await
            .unwrap(),
        fulfillments
    );

    // `get_tickets` answers what `get_ticket` would, skipping unknown ids.
    let mut asked = ticket_ids[..3].to_vec();
    asked.push(TicketId::new());
    let mut expected = Vec::new();
    for ticket_id in &ticket_ids[..3] {
        expected.push(
            repository
                .get_ticket(workspace_id, *ticket_id)
                .await
                .unwrap(),
        );
    }
    expected.sort_by(|a, b| a.display_id.cmp(&b.display_id));
    assert_eq!(
        repository.get_tickets(workspace_id, &asked).await.unwrap(),
        expected
    );
    let (other_workspace, _) = fixture_second_workspace(&pool).await;
    assert!(repository
        .get_tickets(other_workspace, &ticket_ids)
        .await
        .unwrap()
        .is_empty());
}
