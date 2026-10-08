use super::*;

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_acquisition_cost_hierarchy(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let repository = PgOrderRepository::new(pool.clone());

    // 1. explicit unitCost -> Known.
    let (with_cost, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            10,
        ))
        .await
        .unwrap();
    repository
        .record_ticket_acquisition(
            workspace_id,
            with_cost.id,
            acquisition_input(10, Some("5.0")),
        )
        .await
        .unwrap();
    // balance is now qty 10, total 50, avg 5.0000.

    // 2. no explicit cost + the *ticket* carries an estimate -> Estimated.
    let (with_estimate, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            10,
            None,
            None,
        ))
        .await
        .unwrap(); // new_acquisition_ticket sets estimated_unit_cost = 10
    let outcome = repository
        .record_ticket_acquisition(workspace_id, with_estimate.id, acquisition_input(10, None))
        .await
        .unwrap();
    assert!(outcome.created);
    let after_estimate = purchase_events(&pool).await;
    let (_, total, quality, _, _, _) = after_estimate.last().unwrap();
    assert_eq!(quality, "estimated");
    assert_eq!(*total, "100.0000".parse::<Decimal>().unwrap()); // 10 @ ticket estimate 10

    // 3. no explicit cost, no ticket estimate, but the type now has a
    //    weighted average -> Estimated at that average.
    //    After step 1 (10 @ 5.00) + step 2 (10 @ 5.00 estimate applied to
    //    ticket estimate 10) the balance is qty 20, total 150.0000, so the
    //    current weighted average is 7.5000.
    let (bare, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            10,
        ))
        .await
        .unwrap();
    repository
        .record_ticket_acquisition(workspace_id, bare.id, acquisition_input(10, None))
        .await
        .unwrap();
    let after_avg = purchase_events(&pool).await;
    let (_, total, quality, _, _, _) = after_avg.last().unwrap();
    assert_eq!(quality, "estimated");
    assert_eq!(*total, "75.0000".parse::<Decimal>().unwrap()); // 10 @ 7.5000

    // 4. nothing available -> CostRequired, and it posts nothing. A fresh
    //    type_id in the same workspace: no explicit cost, no ticket
    //    estimate, no existing balance.
    seed_sde_type(&pool, import_id, 99, "Isogen").await;
    let (no_basis, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            99,
            "Isogen",
            10,
        ))
        .await
        .unwrap();
    let before_events = inventory_event_count(&pool).await;
    let before_recordings = recording_count(&pool).await;
    let error = repository
        .record_ticket_acquisition(workspace_id, no_basis.id, acquisition_input(10, None))
        .await
        .unwrap_err();
    assert!(matches!(error, OrderError::CostRequired));
    assert_eq!(inventory_event_count(&pool).await, before_events);
    assert_eq!(recording_count(&pool).await, before_recordings);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_acquisition_aggregates_partial_recordings_into_recorded(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100_000,
        ))
        .await
        .unwrap();

    let first = repository
        .record_ticket_acquisition(
            workspace_id,
            ticket.id,
            acquisition_input(40_000, Some("4.00")),
        )
        .await
        .unwrap();
    assert_eq!(first.summary.state, RecordingState::PartiallyRecorded);
    assert_eq!(first.summary.recorded_quantity, 40_000);
    assert_eq!(first.summary.remaining_quantity, 60_000);

    let second = repository
        .record_ticket_acquisition(
            workspace_id,
            ticket.id,
            acquisition_input(35_000, Some("4.10")),
        )
        .await
        .unwrap();
    assert_eq!(second.summary.state, RecordingState::PartiallyRecorded);
    assert_eq!(second.summary.recorded_quantity, 75_000);
    assert_eq!(second.summary.remaining_quantity, 25_000);
    assert_eq!(second.summary.surplus_quantity, 0);
    assert_eq!(recording_count(&pool).await, 2);
    assert_eq!(purchase_events(&pool).await.len(), 2);

    let third = repository
        .record_ticket_acquisition(
            workspace_id,
            ticket.id,
            acquisition_input(30_000, Some("3.95")),
        )
        .await
        .unwrap();
    assert_eq!(third.summary.state, RecordingState::Recorded);
    assert_eq!(third.summary.recorded_quantity, 105_000);
    assert_eq!(third.summary.remaining_quantity, 0);
    assert_eq!(third.summary.surplus_quantity, 5_000);
    assert_eq!(recording_count(&pool).await, 3);
    assert_eq!(purchase_events(&pool).await.len(), 3);

    // Weighted basis: 40000*4.00 + 35000*4.10 + 30000*3.95
    //   = 160000 + 143500 + 118500 = 422000.0000 over 105000 units.
    let balance = inventory_balance(&pool, workspace_id, owner_id, 34)
        .await
        .unwrap();
    assert_eq!(balance.0, 105_000);
    assert_eq!(balance.1, "422000.0000".parse::<Decimal>().unwrap());
    let (_, _, _, _, _, avg) = purchase_events(&pool).await.pop().unwrap();
    assert_eq!(avg, Some("4.0190".parse::<Decimal>().unwrap()));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_acquisition_is_idempotent_on_a_repeated_key(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
        ))
        .await
        .unwrap();

    let key = Uuid::new_v4();
    let first = repository
        .record_ticket_acquisition(
            workspace_id,
            ticket.id,
            RecordAcquisitionInput {
                idempotency_key: key,
                quantity: 100,
                unit_cost: Some(Money::parse("4.25").unwrap()),
                location_note: String::new(),
                note: String::new(),
                effective_at: crate::db_now(),
            },
        )
        .await
        .unwrap();
    assert!(first.created);

    let balance_after_first = inventory_balance(&pool, workspace_id, owner_id, 34).await;

    // Replay: same key, even a different body -- must post nothing.
    let replay = repository
        .record_ticket_acquisition(
            workspace_id,
            ticket.id,
            RecordAcquisitionInput {
                idempotency_key: key,
                quantity: 999,
                unit_cost: Some(Money::parse("1.00").unwrap()),
                location_note: "ignored".to_string(),
                note: String::new(),
                effective_at: crate::db_now(),
            },
        )
        .await
        .unwrap();
    assert!(!replay.created);
    assert_eq!(replay.recording.id, first.recording.id);
    assert_eq!(replay.recording.recorded_quantity, Some(100));
    assert_eq!(replay.summary.state, RecordingState::Recorded);

    assert_eq!(recording_count(&pool).await, 1);
    assert_eq!(purchase_events(&pool).await.len(), 1);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        balance_after_first
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_acquisition_with_different_keys_creates_separate_recordings(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
        ))
        .await
        .unwrap();

    repository
        .record_ticket_acquisition(workspace_id, ticket.id, acquisition_input(30, Some("4.00")))
        .await
        .unwrap();
    repository
        .record_ticket_acquisition(workspace_id, ticket.id, acquisition_input(30, Some("4.00")))
        .await
        .unwrap();

    assert_eq!(recording_count(&pool).await, 2);
    assert_eq!(purchase_events(&pool).await.len(), 2);
    assert_eq!(
        repository
            .list_ticket_inventory_recordings(ticket.id)
            .await
            .unwrap()
            .len(),
        2
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_acquisition_rolls_back_entirely_when_the_purchase_cannot_post(pool: PgPool) {
    let (workspace_id, owner_id, _import_id) = fixture_workspace(&pool).await;
    // Deliberately do NOT seed the SDE type -- `verify_active_type` inside
    // the Purchase posting fails *after* the recording row has been
    // inserted in the same transaction.
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
        ))
        .await
        .unwrap();

    let error = repository
        .record_ticket_acquisition(
            workspace_id,
            ticket.id,
            acquisition_input(100, Some("4.25")),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, OrderError::Persistence(_)));

    assert_eq!(recording_count(&pool).await, 0);
    assert_eq!(inventory_event_count(&pool).await, 0);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        None
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_acquisition_rejects_a_manufacturing_ticket(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
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

    let error = repository
        .record_ticket_acquisition(
            workspace_id,
            ticket.id,
            acquisition_input(100, Some("4.25")),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        OrderError::RecordingRequiresAcquisitionTicket
    ));
    assert_eq!(recording_count(&pool).await, 0);
    assert_eq!(inventory_event_count(&pool).await, 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_acquisition_rejects_a_reaction_ticket(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(NewTicket {
            order_id: None,
            notes: String::new(),
            assignee_character_id: None,
            id: TicketId::new(),
            workspace_id,
            owner_id,
            kind: TicketKind::Reaction,
            type_id: Some(16663),
            captured_name: "Fernite Carbide".to_string(),
            quantity: Some(100),
            source_build_id: Some(build_id),
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

    let error = repository
        .record_ticket_acquisition(
            workspace_id,
            ticket.id,
            acquisition_input(100, Some("4.25")),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        OrderError::RecordingRequiresAcquisitionTicket
    ));
    assert_eq!(recording_count(&pool).await, 0);
    assert_eq!(inventory_event_count(&pool).await, 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_acquisition_rejects_a_ticket_batched_into_a_run(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let price_source = fixture_price_source(&pool, workspace_id, "Jita 4-4").await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
            None,
            Some(price_source),
        ))
        .await
        .unwrap();
    repository
        .create_order_acquisition_run(workspace_id, owner_id, None, vec![ticket.id])
        .await
        .unwrap();

    let error = repository
        .record_ticket_acquisition(
            workspace_id,
            ticket.id,
            acquisition_input(100, Some("4.25")),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        OrderError::RecordingNotAllowedForBatchedTicket
    ));
    assert_eq!(recording_count(&pool).await, 0);
    assert_eq!(inventory_event_count(&pool).await, 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_acquisition_is_independent_of_ticket_status(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let repository = PgOrderRepository::new(pool.clone());

    for status in [
        TicketStatus::Todo,
        TicketStatus::InProgress,
        TicketStatus::Complete,
    ] {
        let (ticket, _) = repository
            .create_ticket(new_acquisition_ticket_no_estimate(
                workspace_id,
                owner_id,
                34,
                "Tritanium",
                10,
            ))
            .await
            .unwrap();
        repository
            .set_ticket_status(workspace_id, ticket.id, status)
            .await
            .unwrap();
        let before = repository
            .get_ticket(workspace_id, ticket.id)
            .await
            .unwrap();

        let outcome = repository
            .record_ticket_acquisition(workspace_id, ticket.id, acquisition_input(10, Some("4.00")))
            .await
            .unwrap();
        assert!(outcome.created, "recording must succeed from {status:?}");

        let after = repository
            .get_ticket(workspace_id, ticket.id)
            .await
            .unwrap();
        assert_eq!(after.status, status, "status changed from {status:?}");
        assert_eq!(after.status, before.status);
        assert_eq!(after.updated_at, before.updated_at);
        assert_eq!(after.archived_at, before.archived_at);
    }
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_acquisition_does_not_recheck_dependent_tickets(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_sde_type(&pool, import_id, 37164, "Isogen").await;
    let repository = PgOrderRepository::new(pool.clone());

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
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            37164,
            "Isogen",
            50,
        ))
        .await
        .unwrap();
    repository
        .link_ticket_prerequisite_to_ticket(prerequisites[0].id, fulfiller.id, 50)
        .await
        .unwrap();

    let dependent_before = repository
        .get_ticket(workspace_id, manufacturing_ticket.id)
        .await
        .unwrap();

    repository
        .record_ticket_acquisition(
            workspace_id,
            fulfiller.id,
            acquisition_input(50, Some("4.00")),
        )
        .await
        .unwrap();

    let dependent_after = repository
        .get_ticket(workspace_id, manufacturing_ticket.id)
        .await
        .unwrap();
    assert_eq!(dependent_after.status, TicketStatus::Todo);
    assert_eq!(dependent_after.status, dependent_before.status);
    assert_eq!(dependent_after.updated_at, dependent_before.updated_at);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_acquisition_rejects_a_ticket_from_another_workspace(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let (other_workspace_id, _other_owner_id) = fixture_second_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
        ))
        .await
        .unwrap();

    let error = repository
        .record_ticket_acquisition(
            other_workspace_id,
            ticket.id,
            acquisition_input(100, Some("4.25")),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, OrderError::TicketNotFound));
    assert_eq!(recording_count(&pool).await, 0);
    assert_eq!(inventory_event_count(&pool).await, 0);
}
