use super::*;

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn start_ticket_transitions_todo_to_in_progress(pool: PgPool) {
    let (workspace_id, owner_id, _import_id) = fixture_workspace(&pool).await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(NewTicket {
            order_id: None,
            notes: String::new(),
            assignee_character_id: None,
            id: TicketId::new(),
            workspace_id,
            owner_id,
            kind: TicketKind::Acquisition,
            type_id: Some(44),
            captured_name: "Vexor".to_string(),
            quantity: Some(1),
            source_build_id: None,
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
    assert_eq!(ticket.status, TicketStatus::Todo);

    let started = repository
        .start_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(started.status, TicketStatus::InProgress);
}

/// Completing an Acquisition ticket posts no `Purchase` event and infers no
/// `actual_unit_cost` -- it is a bare `status = 'complete'` write, exactly
/// like any other kind. The only
/// way to post inventory is the explicit `record_ticket_acquisition` action
/// (see `record_acquisition_after_completing_a_ticket_posts_exactly_one_purchase`
/// below for that path proven independent of this one).
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn completing_an_acquisition_ticket_does_not_post_inventory(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 44, "Vexor").await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(NewTicket {
            order_id: None,
            notes: String::new(),
            assignee_character_id: None,
            id: TicketId::new(),
            workspace_id,
            owner_id,
            kind: TicketKind::Acquisition,
            type_id: Some(44),
            captured_name: "Vexor".to_string(),
            quantity: Some(3),
            source_build_id: None,
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
    repository
        .start_ticket(workspace_id, ticket.id)
        .await
        .unwrap();

    let completed = repository
        .complete_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(completed.status, TicketStatus::Complete);
    assert_eq!(
        completed.actual_unit_cost, None,
        "completion no longer infers/prices anything"
    );
    assert_eq!(completed.actual_line_total, None);

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
    assert!(
        repository
            .list_ticket_inventory_recordings(ticket.id)
            .await
            .unwrap()
            .is_empty(),
        "completion never creates a recording row"
    );
}

/// Manufacturing/Reaction counterpart of the test above: completing no
/// longer posts a `Consumption` per prerequisite.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn completing_a_manufacturing_ticket_does_not_consume_materials(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_sde_type(&pool, import_id, 37164, "Isogen").await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 37164, 100).await;
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
    // Force straight to InProgress -- this test is about `complete_ticket`
    // specifically, not about how a ticket with blockers gets there.
    repository
        .set_ticket_status(workspace_id, ticket.id, TicketStatus::InProgress)
        .await
        .unwrap();
    let balance_before = inventory_balance(&pool, workspace_id, owner_id, 37164).await;

    let completed = repository
        .complete_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(completed.status, TicketStatus::Complete);

    assert_eq!(inventory_event_count(&pool).await, 0);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 37164).await,
        balance_before,
        "no Consumption was posted"
    );
    assert!(allocation_snapshot(&pool).await.is_empty());
    assert!(repository
        .list_ticket_inventory_recordings(ticket.id)
        .await
        .unwrap()
        .is_empty());
}

/// A Ticket workflow operation must never mutate another Ticket's status:
/// completing a fulfiller leaves its dependent exactly where it was.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn completing_a_production_tickets_fulfilling_ticket_does_not_unblock_it(pool: PgPool) {
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

    let (acquisition_ticket, _) = repository
        .create_ticket(NewTicket {
            order_id: None,
            notes: String::new(),
            assignee_character_id: None,
            id: TicketId::new(),
            workspace_id,
            owner_id,
            kind: TicketKind::Acquisition,
            type_id: Some(37164),
            captured_name: "Isogen".to_string(),
            quantity: Some(50),
            source_build_id: None,
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

    repository
        .link_ticket_prerequisite_to_ticket(prerequisites[0].id, acquisition_ticket.id, 50)
        .await
        .unwrap();

    repository
        .start_ticket(workspace_id, acquisition_ticket.id)
        .await
        .unwrap();
    repository
        .complete_ticket(workspace_id, acquisition_ticket.id)
        .await
        .unwrap();

    let manufacturing_ticket = repository
        .get_ticket(workspace_id, manufacturing_ticket.id)
        .await
        .unwrap();
    assert_eq!(
        manufacturing_ticket.status,
        TicketStatus::Todo,
        "completing a fulfiller must not rewrite its dependent's workflow status"
    );
}

/// Symmetric case for `cancel_ticket`: canceling a fulfiller performs no
/// cross-ticket write -- the dependent's persisted workflow status is
/// untouched, whatever it happens to be.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn canceling_a_fulfilling_ticket_does_not_revert_its_dependents_workflow_status(
    pool: PgPool,
) {
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

    // Put the dependent in a definite lane via the workflow-only primitive,
    // same as a person dragging the card.
    repository
        .set_ticket_status(workspace_id, manufacturing_ticket.id, TicketStatus::Todo)
        .await
        .unwrap();

    repository
        .cancel_ticket(workspace_id, fulfiller.id)
        .await
        .unwrap();

    let manufacturing_ticket = repository
        .get_ticket(workspace_id, manufacturing_ticket.id)
        .await
        .unwrap();
    assert_eq!(
        manufacturing_ticket.status,
        TicketStatus::Todo,
        "canceling a fulfiller must not revert its dependent's workflow status"
    );
}

/// Completing an Acquisition ticket leaves
/// it fully unrecorded -- recording is the only accounting path, and it
/// works identically whether it happens before or after completion.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn completing_an_acquisition_ticket_leaves_recording_not_recorded_until_explicitly_recorded(
    pool: PgPool,
) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 44, "Vexor").await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            44,
            "Vexor",
            3,
        ))
        .await
        .unwrap();
    repository
        .start_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    let completed = repository
        .complete_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(completed.status, TicketStatus::Complete);
    assert!(repository
        .list_ticket_inventory_recordings(ticket.id)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(inventory_event_count(&pool).await, 0);

    // The only accounting path: recording, which works from Complete same
    // as from any other status.
    let outcome = repository
        .record_ticket_acquisition(workspace_id, ticket.id, acquisition_input(3, Some("4.00")))
        .await
        .unwrap();
    assert!(outcome.created);
    assert_eq!(inventory_event_count(&pool).await, 1);
    let after = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(
        after.status,
        TicketStatus::Complete,
        "recording never touches workflow status"
    );
}

/// Double-accounting regression (Acquisition): recording first, then
/// completing, must never post a second Purchase.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn completing_a_ticket_after_recording_acquisition_posts_no_additional_purchase(
    pool: PgPool,
) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 44, "Vexor").await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            44,
            "Vexor",
            3,
        ))
        .await
        .unwrap();
    repository
        .start_ticket(workspace_id, ticket.id)
        .await
        .unwrap();

    repository
        .record_ticket_acquisition(workspace_id, ticket.id, acquisition_input(3, Some("4.00")))
        .await
        .unwrap();
    let events_after_recording = inventory_event_count(&pool).await;
    assert_eq!(events_after_recording, 1);
    let balance_after_recording = inventory_balance(&pool, workspace_id, owner_id, 44).await;
    let recordings_after_recording = repository
        .list_ticket_inventory_recordings(ticket.id)
        .await
        .unwrap()
        .len();

    repository
        .complete_ticket(workspace_id, ticket.id)
        .await
        .unwrap();

    assert_eq!(
        inventory_event_count(&pool).await,
        events_after_recording,
        "completing after recording posts no additional Purchase"
    );
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 44).await,
        balance_after_recording
    );
    assert_eq!(
        repository
            .list_ticket_inventory_recordings(ticket.id)
            .await
            .unwrap()
            .len(),
        recordings_after_recording,
        "completing after recording creates no additional recording row"
    );
}

/// Double-accounting regression (Manufacturing): same as above for
/// production -- recording first, then completing, posts nothing more.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn completing_a_ticket_after_recording_production_posts_no_additional_events(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    for (type_id, name) in [(34_i64, "Material A"), (20185, "Product")] {
        seed_sde_type(&pool, import_id, type_id, name).await;
    }
    seed_inventory_balance_with_cost(
        &pool,
        workspace_id,
        owner_id,
        34,
        "Material A",
        100,
        "1000.0000",
    )
    .await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            20185,
            "Product",
            build_id,
            vec![prod_prereq(34, "Material A")],
            None,
        ))
        .await
        .unwrap();
    repository
        .set_ticket_status(workspace_id, ticket.id, TicketStatus::InProgress)
        .await
        .unwrap();

    repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 10, &[(34, 100)], "0"),
        )
        .await
        .unwrap();
    let events_after_recording = inventory_event_count(&pool).await;
    assert!(events_after_recording > 0);
    let balance_after_recording = inventory_balance(&pool, workspace_id, owner_id, 34).await;
    let recordings_after_recording = repository
        .list_ticket_inventory_recordings(ticket.id)
        .await
        .unwrap()
        .len();

    repository
        .complete_ticket(workspace_id, ticket.id)
        .await
        .unwrap();

    assert_eq!(
        inventory_event_count(&pool).await,
        events_after_recording,
        "completing after recording posts no additional Consumption/ProductionOutput"
    );
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        balance_after_recording
    );
    assert_eq!(
        repository
            .list_ticket_inventory_recordings(ticket.id)
            .await
            .unwrap()
            .len(),
        recordings_after_recording
    );
}

/// Canceling a ticket after it was recorded is not an accounting
/// reversal -- the recording, the posted events, and the resulting balance
/// all survive; only `status` changes.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn canceling_a_ticket_after_recording_acquisition_preserves_the_recording_and_inventory(
    pool: PgPool,
) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 44, "Vexor").await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            44,
            "Vexor",
            3,
        ))
        .await
        .unwrap();

    repository
        .record_ticket_acquisition(workspace_id, ticket.id, acquisition_input(3, Some("4.00")))
        .await
        .unwrap();
    let events_before = inventory_event_count(&pool).await;
    let balance_before = inventory_balance(&pool, workspace_id, owner_id, 44).await;
    let recordings_before = repository
        .list_ticket_inventory_recordings(ticket.id)
        .await
        .unwrap();

    let canceled = repository
        .cancel_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(canceled.status, TicketStatus::Canceled);

    assert_eq!(inventory_event_count(&pool).await, events_before);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 44).await,
        balance_before
    );
    assert_eq!(
        repository
            .list_ticket_inventory_recordings(ticket.id)
            .await
            .unwrap(),
        recordings_before,
        "cancellation is not an accounting reversal -- the recording history stands"
    );
}

/// Manufacturing counterpart of the test above.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn canceling_a_ticket_after_recording_production_preserves_the_recording_and_inventory(
    pool: PgPool,
) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    for (type_id, name) in [(34_i64, "Material A"), (20185, "Product")] {
        seed_sde_type(&pool, import_id, type_id, name).await;
    }
    seed_inventory_balance_with_cost(
        &pool,
        workspace_id,
        owner_id,
        34,
        "Material A",
        100,
        "1000.0000",
    )
    .await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            20185,
            "Product",
            build_id,
            vec![prod_prereq(34, "Material A")],
            None,
        ))
        .await
        .unwrap();
    repository
        .set_ticket_status(workspace_id, ticket.id, TicketStatus::InProgress)
        .await
        .unwrap();

    repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 10, &[(34, 100)], "0"),
        )
        .await
        .unwrap();
    let events_before = inventory_event_count(&pool).await;
    let balance_before = inventory_balance(&pool, workspace_id, owner_id, 34).await;
    let recordings_before = repository
        .list_ticket_inventory_recordings(ticket.id)
        .await
        .unwrap();

    let canceled = repository
        .cancel_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(canceled.status, TicketStatus::Canceled);

    assert_eq!(inventory_event_count(&pool).await, events_before);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        balance_before
    );
    assert_eq!(
        repository
            .list_ticket_inventory_recordings(ticket.id)
            .await
            .unwrap(),
        recordings_before
    );
}

/// Full lifecycle-neutrality matrix: for each supported kind, drives
/// `start_ticket` / `complete_ticket` / `set_ticket_status` (the
/// `Complete -> InProgress` leg, which only that primitive allows) /
/// `cancel_ticket` in sequence and asserts, throughout, that no inventory
/// event, allocation, or recording was ever created, and that an unrelated
/// control ticket and the shared Build are byte-identical before and
/// after. Every kind starts `Todo` (workflow status is user-controlled and
/// independent of prerequisites), so the same transition sequence applies
/// uniformly.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn ticket_lifecycle_across_every_kind_never_mutates_inventory_recordings_or_unrelated_domains(
    pool: PgPool,
) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_sde_type(&pool, import_id, 44, "Vexor").await;
    let repository = PgOrderRepository::new(pool.clone());

    let control = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            44,
            "Vexor",
            1,
        ))
        .await
        .unwrap()
        .0;
    let build_before: (i64, DateTime<Utc>) =
        sqlx::query_as("SELECT revision, updated_at FROM builds WHERE id = $1")
            .bind(build_id.0)
            .fetch_one(&pool)
            .await
            .unwrap();

    for kind in [
        TicketKind::Generic,
        TicketKind::Acquisition,
        TicketKind::Manufacturing,
        TicketKind::Reaction,
    ] {
        let new_ticket = match kind {
            TicketKind::Generic => NewTicket {
                order_id: None,
                notes: String::new(),
                assignee_character_id: None,
                id: TicketId::new(),
                workspace_id,
                owner_id,
                kind: TicketKind::Generic,
                type_id: None,
                captured_name: "Move blueprints".to_string(),
                quantity: None,
                source_build_id: None,
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
            },
            TicketKind::Acquisition => {
                new_acquisition_ticket_no_estimate(workspace_id, owner_id, 44, "Vexor", 2)
            }
            // Every new ticket starts `Todo` regardless of prerequisites,
            // so the same Todo -> ... transition sequence applies to every
            // kind.
            TicketKind::Manufacturing | TicketKind::Reaction => mfg_ticket(
                workspace_id,
                owner_id,
                kind,
                20185,
                "Product",
                build_id,
                Vec::new(),
                None,
            ),
        };
        let (ticket, _) = repository.create_ticket(new_ticket).await.unwrap();
        assert_eq!(ticket.status, TicketStatus::Todo, "{kind:?} starts Todo");

        let started = repository
            .start_ticket(workspace_id, ticket.id)
            .await
            .unwrap();
        assert_eq!(started.status, TicketStatus::InProgress);

        let completed = repository
            .complete_ticket(workspace_id, ticket.id)
            .await
            .unwrap();
        assert_eq!(completed.status, TicketStatus::Complete);

        let reopened = repository
            .set_ticket_status(workspace_id, ticket.id, TicketStatus::InProgress)
            .await
            .unwrap();
        assert_eq!(reopened.status, TicketStatus::InProgress);

        let canceled = repository
            .cancel_ticket(workspace_id, ticket.id)
            .await
            .unwrap();
        assert_eq!(canceled.status, TicketStatus::Canceled);

        assert!(
            repository
                .list_ticket_inventory_recordings(ticket.id)
                .await
                .unwrap()
                .is_empty(),
            "{kind:?}: no recording was ever created by workflow moves alone"
        );
    }

    assert_eq!(inventory_event_count(&pool).await, 0);
    assert!(allocation_snapshot(&pool).await.is_empty());

    let control_after = repository
        .get_ticket(workspace_id, control.id)
        .await
        .unwrap();
    assert_eq!(
        control_after.status, control.status,
        "unrelated ticket untouched"
    );
    assert_eq!(control_after.updated_at, control.updated_at);

    let build_after: (i64, DateTime<Utc>) =
        sqlx::query_as("SELECT revision, updated_at FROM builds WHERE id = $1")
            .bind(build_id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        build_after, build_before,
        "Build untouched by any Ticket lifecycle move"
    );
}

/// Exercises the exact query path `apps/iskworks-api/src/routes/orders.rs`'s
/// `list_order_tickets` route depends on for its "blocked by" derivation:
/// a Manufacturing ticket's own unmet prerequisite, and the non-canceled
/// fulfillment linked to it, both come back correctly shaped.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn list_ticket_prerequisite_fulfillments_returns_the_right_rows_for_a_ticket_with_an_unmet_prerequisite(
    pool: PgPool,
) {
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

    let (acquisition_ticket, _) = repository
        .create_ticket(NewTicket {
            order_id: None,
            notes: String::new(),
            assignee_character_id: None,
            id: TicketId::new(),
            workspace_id,
            owner_id,
            kind: TicketKind::Acquisition,
            type_id: Some(37164),
            captured_name: "Isogen".to_string(),
            quantity: Some(50),
            source_build_id: None,
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

    repository
        .link_ticket_prerequisite_to_ticket(prerequisites[0].id, acquisition_ticket.id, 50)
        .await
        .unwrap();

    let fulfillments = repository
        .list_ticket_prerequisite_fulfillments(prerequisites[0].id)
        .await
        .unwrap();
    assert_eq!(fulfillments.len(), 1);
    assert_eq!(fulfillments[0].fulfilling_ticket_id, acquisition_ticket.id);
    assert_eq!(fulfillments[0].allocated_quantity, 50);

    let refetched_prerequisites = repository
        .list_ticket_prerequisites(manufacturing_ticket.id)
        .await
        .unwrap();
    assert_eq!(refetched_prerequisites.len(), 1);
    assert_eq!(refetched_prerequisites[0].kind, RequirementKind::Buy);
    assert_eq!(refetched_prerequisites[0].source_build_id, None);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn linking_a_ticket_with_a_different_type_id_is_rejected(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let snapshot = empty_price_snapshot();
    let repository = PgOrderRepository::new(pool.clone());

    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let (_, requirements) = repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: vec![requirement(34, RequirementKind::Buy, 1)],
        })
        .await
        .unwrap();

    let (mismatched_ticket, _) = repository
        .create_ticket(NewTicket {
            order_id: None,
            notes: String::new(),
            assignee_character_id: None,
            id: TicketId::new(),
            workspace_id,
            owner_id,
            kind: TicketKind::Acquisition,
            type_id: Some(44), // Vexor, not the requirement's Tritanium (34).
            captured_name: "Vexor".to_string(),
            quantity: Some(1),
            source_build_id: None,
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

    let result = repository
        .link_order_requirement_to_ticket(requirements[0].id, mismatched_ticket.id, 1)
        .await;
    assert!(matches!(result, Err(OrderError::TicketTypeMismatch)));
}
