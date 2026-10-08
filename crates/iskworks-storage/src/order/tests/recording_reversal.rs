use super::*;

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn ticket_inventory_recording_reversal_state_preserves_history_and_excludes_active_totals(
    pool: PgPool,
) {
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

    let original = repository
        .record_ticket_acquisition(workspace_id, ticket.id, acquisition_input(60, Some("4.00")))
        .await
        .unwrap()
        .recording;
    let before: (Option<i64>, Option<i64>, Option<Decimal>, String, String, DateTime<Utc>) =
        sqlx::query_as(
            "SELECT recorded_quantity, runs_completed, installation_cost, location_note, note, recorded_at \
             FROM ticket_inventory_recordings WHERE id = $1",
        )
        .bind(original.id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    let reverted_at = crate::db_now();
    sqlx::query("UPDATE ticket_inventory_recordings SET reverted_at = $1 WHERE id = $2")
        .bind(reverted_at)
        .bind(original.id.0)
        .execute(&pool)
        .await
        .unwrap();

    let corrected = repository
        .record_ticket_acquisition(workspace_id, ticket.id, acquisition_input(40, Some("4.00")))
        .await
        .unwrap();
    assert_eq!(corrected.summary.recorded_quantity, 40);
    assert_eq!(corrected.summary.remaining_quantity, 60);

    let history = repository
        .list_ticket_inventory_recordings(ticket.id)
        .await
        .unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].id, original.id);
    assert_eq!(history[0].reverted_at, Some(reverted_at));
    assert_eq!(history[0].status, TicketInventoryRecordingStatus::Reversed);
    assert_eq!(history[1].status, TicketInventoryRecordingStatus::Recorded);

    let after: (Option<i64>, Option<i64>, Option<Decimal>, String, String, DateTime<Utc>) =
        sqlx::query_as(
            "SELECT recorded_quantity, runs_completed, installation_cost, location_note, note, recorded_at \
             FROM ticket_inventory_recordings WHERE id = $1",
        )
        .bind(original.id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, before);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn revert_ticket_inventory_recording_acquisition_is_exact_one_time_and_rerecordable(
    pool: PgPool,
) {
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
    let original = repository
        .record_ticket_acquisition(
            workspace_id,
            ticket.id,
            acquisition_input(120, Some("4.2500")),
        )
        .await
        .unwrap();

    let reverted = repository
        .revert_ticket_inventory_recording(workspace_id, ticket.id, original.recording.id)
        .await
        .unwrap();
    assert_eq!(
        reverted.recording.status,
        TicketInventoryRecordingStatus::Reversed
    );
    assert_eq!(reverted.summary.recorded_quantity, 0);
    assert_eq!(reverted.summary.remaining_quantity, 100);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        Some((0, "0.0000".parse().unwrap(), 2))
    );
    let events: Vec<(String, i64, Decimal, Option<Uuid>)> = sqlx::query_as(
        "SELECT event_kind, quantity_delta, total_cost_delta, reverses_event_id \
         FROM inventory_events ORDER BY sequence",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].0, "purchase");
    assert_eq!(events[0].1, 120);
    assert_eq!(events[0].2, "510.0000".parse::<Decimal>().unwrap());
    assert_eq!(events[1].0, "reversal");
    assert_eq!(events[1].1, -120);
    assert_eq!(events[1].2, "-510.0000".parse::<Decimal>().unwrap());
    assert!(events[1].3.is_some());

    let second = repository
        .revert_ticket_inventory_recording(workspace_id, ticket.id, original.recording.id)
        .await
        .unwrap_err();
    assert!(matches!(second, OrderError::RecordingAlreadyReversed));
    assert_eq!(inventory_event_count(&pool).await, 2);

    let corrected = repository
        .record_ticket_acquisition(
            workspace_id,
            ticket.id,
            acquisition_input(100, Some("5.0000")),
        )
        .await
        .unwrap();
    assert_eq!(corrected.summary.recorded_quantity, 100);
    assert_eq!(corrected.summary.remaining_quantity, 0);
    assert_eq!(
        repository
            .get_ticket(workspace_id, ticket.id)
            .await
            .unwrap()
            .status,
        ticket.status
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn revert_ticket_inventory_recording_production_restores_every_exact_basis(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    for (type_id, name) in [(34, "Material A"), (35, "Material B"), (20185, "Product")] {
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
    seed_inventory_balance_with_cost(
        &pool,
        workspace_id,
        owner_id,
        35,
        "Material B",
        50,
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
            vec![prod_prereq(34, "Material A"), prod_prereq(35, "Material B")],
            None,
        ))
        .await
        .unwrap();
    let recorded = repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 10, &[(34, 100), (35, 50)], "500"),
        )
        .await
        .unwrap();

    repository
        .revert_ticket_inventory_recording(workspace_id, ticket.id, recorded.recording.id)
        .await
        .unwrap();

    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        Some((100, "1000.0000".parse().unwrap(), 3))
    );
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 35).await,
        Some((50, "1000.0000".parse().unwrap(), 3))
    );
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 20185).await,
        Some((0, "0.0000".parse().unwrap(), 2))
    );
    let reversals: Vec<(i64, Decimal, Option<Uuid>)> = sqlx::query_as(
        "SELECT quantity_delta, total_cost_delta, reverses_event_id FROM inventory_events \
         WHERE event_kind = 'reversal' ORDER BY quantity_delta",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(reversals.len(), 3);
    assert_eq!(reversals[0].0, -10);
    assert_eq!(reversals[0].1, "-2500.0000".parse::<Decimal>().unwrap());
    assert_eq!(
        reversals[1].1 + reversals[2].1,
        "2000.0000".parse::<Decimal>().unwrap()
    );
    assert!(reversals.iter().all(|row| row.2.is_some()));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn revert_ticket_inventory_recording_accepts_zero_cost_consumption_evidence(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    for (type_id, name) in [(34, "Zero Cost Material"), (20185, "Product")] {
        seed_sde_type(&pool, import_id, type_id, name).await;
    }
    seed_inventory_balance_with_cost(
        &pool,
        workspace_id,
        owner_id,
        34,
        "Zero Cost Material",
        100,
        "0.0000",
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
            vec![prod_prereq(34, "Zero Cost Material")],
            None,
        ))
        .await
        .unwrap();
    let recording = repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 10, &[(34, 100)], "0"),
        )
        .await
        .unwrap()
        .recording;

    repository
        .revert_ticket_inventory_recording(workspace_id, ticket.id, recording.id)
        .await
        .unwrap();
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        Some((100, "0.0000".parse().unwrap(), 3))
    );
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 20185).await,
        Some((0, "0.0000".parse().unwrap(), 2))
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn revert_ticket_inventory_recording_no_event_recording_uses_only_lifecycle_state(
    pool: PgPool,
) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_sde_type(&pool, import_id, 20185, "Product").await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            20185,
            "Product",
            build_id,
            vec![],
            None,
        ))
        .await
        .unwrap();
    let recorded = repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 0, &[], "0"),
        )
        .await
        .unwrap();
    assert_eq!(inventory_event_count(&pool).await, 0);

    let outcome = repository
        .revert_ticket_inventory_recording(workspace_id, ticket.id, recorded.recording.id)
        .await
        .unwrap();
    assert_eq!(
        outcome.recording.status,
        TicketInventoryRecordingStatus::Reversed
    );
    assert_eq!(outcome.summary.recorded_quantity, 0);
    assert_eq!(inventory_event_count(&pool).await, 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn revert_ticket_inventory_recording_concurrent_requests_commit_once(pool: PgPool) {
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
    let recording = repository
        .record_ticket_acquisition(workspace_id, ticket.id, acquisition_input(100, Some("4")))
        .await
        .unwrap()
        .recording;

    let left = repository.clone();
    let right = repository.clone();
    let (a, b) = tokio::join!(
        left.revert_ticket_inventory_recording(workspace_id, ticket.id, recording.id),
        right.revert_ticket_inventory_recording(workspace_id, ticket.id, recording.id),
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let error = if let Err(error) = a {
        error
    } else {
        b.unwrap_err()
    };
    assert!(matches!(error, OrderError::RecordingAlreadyReversed));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*)::bigint FROM inventory_events WHERE event_kind = 'reversal'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn ticket_and_generic_reversal_race_never_leaks_a_unique_constraint(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let orders = PgOrderRepository::new(pool.clone());
    let inventory = PgInventoryRepository::new(pool.clone());
    let (ticket, _) = orders
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
        ))
        .await
        .unwrap();
    let recording = orders
        .record_ticket_acquisition(workspace_id, ticket.id, acquisition_input(100, Some("4")))
        .await
        .unwrap()
        .recording;
    let key = InventoryItemKey {
        workspace_id,
        owner_id,
        type_id: 34,
    };
    let history = inventory.get_history(&key).await.unwrap();
    let original = history.events.last().unwrap();

    let left = orders.clone();
    let right = inventory.clone();
    let (ticket_result, generic_result) = tokio::join!(
        left.revert_ticket_inventory_recording(workspace_id, ticket.id, recording.id),
        right.reverse_latest(
            &key,
            original.id,
            history.balance.revision,
            "Concurrent correction".to_string(),
        ),
    );

    assert_eq!(
        usize::from(ticket_result.is_ok()) + usize::from(generic_result.is_ok()),
        1
    );
    if let Err(error) = ticket_result {
        assert!(matches!(error, OrderError::RecordingAlreadyReversed));
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM inventory_events WHERE reverses_event_id = $1"
        )
        .bind(original.id.0)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn revert_ticket_inventory_recording_rejects_downstream_use_without_partial_changes(
    pool: PgPool,
) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    for (type_id, name) in [(34, "Tritanium"), (20185, "Product")] {
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
    let recorded = repository
        .record_ticket_acquisition(
            workspace_id,
            acquisition.id,
            acquisition_input(100, Some("4")),
        )
        .await
        .unwrap();
    let (consumer, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            20185,
            "Product",
            build_id,
            vec![prod_prereq(34, "Tritanium")],
            None,
        ))
        .await
        .unwrap();
    repository
        .record_ticket_production(
            workspace_id,
            consumer.id,
            production_input(1, 20185, 0, &[(34, 50)], "0"),
        )
        .await
        .unwrap();
    let before_balance = inventory_balance(&pool, workspace_id, owner_id, 34).await;
    let before_events = inventory_event_count(&pool).await;

    let error = repository
        .revert_ticket_inventory_recording(workspace_id, acquisition.id, recorded.recording.id)
        .await
        .unwrap_err();
    assert!(matches!(error, OrderError::RecordingReversalInvalid));
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        before_balance
    );
    assert_eq!(inventory_event_count(&pool).await, before_events);
    assert!(repository
        .list_ticket_inventory_recordings(acquisition.id)
        .await
        .unwrap()[0]
        .reverted_at
        .is_none());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn revert_ticket_inventory_recording_production_rejects_consumed_output_before_restoring_inputs(
    pool: PgPool,
) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    for (type_id, name) in [
        (34, "Tritanium"),
        (20185, "Produced Item"),
        (20186, "Downstream Item"),
    ] {
        seed_sde_type(&pool, import_id, type_id, name).await;
    }
    seed_inventory_balance_with_cost(
        &pool,
        workspace_id,
        owner_id,
        34,
        "Tritanium",
        100,
        "1000.0000",
    )
    .await;
    let repository = PgOrderRepository::new(pool.clone());
    let (producer, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            20185,
            "Produced Item",
            build_id,
            vec![prod_prereq(34, "Tritanium")],
            None,
        ))
        .await
        .unwrap();
    let produced = repository
        .record_ticket_production(
            workspace_id,
            producer.id,
            production_input(1, 20185, 10, &[(34, 100)], "500"),
        )
        .await
        .unwrap();
    let (consumer, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            20186,
            "Downstream Item",
            build_id,
            vec![prod_prereq(20185, "Produced Item")],
            None,
        ))
        .await
        .unwrap();
    repository
        .record_ticket_production(
            workspace_id,
            consumer.id,
            production_input(1, 20186, 0, &[(20185, 5)], "0"),
        )
        .await
        .unwrap();

    let material_before = inventory_balance(&pool, workspace_id, owner_id, 34).await;
    let output_before = inventory_balance(&pool, workspace_id, owner_id, 20185).await;
    let events_before = inventory_event_count(&pool).await;
    let error = repository
        .revert_ticket_inventory_recording(workspace_id, producer.id, produced.recording.id)
        .await
        .unwrap_err();

    assert!(matches!(error, OrderError::RecordingReversalInvalid));
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 20185).await,
        output_before,
        "the recorded output cannot be removed after downstream consumption"
    );
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        material_before,
        "negative compensation runs first, so no consumed input is restored"
    );
    assert_eq!(inventory_event_count(&pool).await, events_before);
    assert!(repository
        .list_ticket_inventory_recordings(producer.id)
        .await
        .unwrap()[0]
        .reverted_at
        .is_none());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn revert_ticket_inventory_recording_rolls_back_an_earlier_output_compensation_when_a_later_input_post_fails(
    pool: PgPool,
) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    for (type_id, name) in [(34, "Material"), (20185, "Product")] {
        seed_sde_type(&pool, import_id, type_id, name).await;
    }
    seed_inventory_balance_with_cost(
        &pool,
        workspace_id,
        owner_id,
        34,
        "Material",
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
            vec![prod_prereq(34, "Material")],
            None,
        ))
        .await
        .unwrap();
    let recorded = repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 10, &[(34, 100)], "500"),
        )
        .await
        .unwrap();
    let input_before = inventory_balance(&pool, workspace_id, owner_id, 34).await;
    let output_before = inventory_balance(&pool, workspace_id, owner_id, 20185).await;
    sqlx::query(
        "CREATE FUNCTION fail_positive_ticket_reversal() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN IF NEW.event_kind = 'reversal' AND NEW.quantity_delta > 0 THEN \
         RAISE EXCEPTION 'forced later compensation failure'; END IF; RETURN NEW; END $$",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER fail_positive_ticket_reversal BEFORE INSERT ON inventory_events \
         FOR EACH ROW EXECUTE FUNCTION fail_positive_ticket_reversal()",
    )
    .execute(&pool)
    .await
    .unwrap();

    let error = repository
        .revert_ticket_inventory_recording(workspace_id, ticket.id, recorded.recording.id)
        .await
        .unwrap_err();
    assert!(matches!(error, OrderError::RecordingReversalInvalid));
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        input_before
    );
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 20185).await,
        output_before
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM inventory_events WHERE event_kind = 'reversal'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert!(repository
        .list_ticket_inventory_recordings(ticket.id)
        .await
        .unwrap()[0]
        .reverted_at
        .is_none());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn revert_ticket_inventory_recording_rejects_another_tickets_recording(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let repository = PgOrderRepository::new(pool.clone());
    let (first, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            10,
        ))
        .await
        .unwrap();
    let (second, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            10,
        ))
        .await
        .unwrap();
    let recording = repository
        .record_ticket_acquisition(workspace_id, first.id, acquisition_input(10, Some("1")))
        .await
        .unwrap()
        .recording;

    let error = repository
        .revert_ticket_inventory_recording(workspace_id, second.id, recording.id)
        .await
        .unwrap_err();
    assert!(matches!(error, OrderError::RecordingNotFound));
    assert_eq!(inventory_event_count(&pool).await, 1);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn revert_ticket_inventory_recording_rejects_mismatched_immutable_production_evidence(
    pool: PgPool,
) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    for (type_id, name) in [(34, "Material"), (20185, "Product")] {
        seed_sde_type(&pool, import_id, type_id, name).await;
    }
    seed_inventory_balance_with_cost(
        &pool,
        workspace_id,
        owner_id,
        34,
        "Material",
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
            vec![prod_prereq(34, "Material")],
            None,
        ))
        .await
        .unwrap();
    let recorded = repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 10, &[(34, 100)], "500"),
        )
        .await
        .unwrap();
    sqlx::query(
        "UPDATE inventory_events SET quantity_delta = 9 \
         WHERE ticket_inventory_recording_id = $1 AND event_kind = 'production_output'",
    )
    .bind(recorded.recording.id.0)
    .execute(&pool)
    .await
    .unwrap();

    let error = repository
        .revert_ticket_inventory_recording(workspace_id, ticket.id, recorded.recording.id)
        .await
        .unwrap_err();
    assert!(matches!(error, OrderError::RecordingEvidenceInvalid));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM inventory_events WHERE event_kind = 'reversal'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn revert_ticket_inventory_recording_uses_historical_name_after_active_sde_rename(
    pool: PgPool,
) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let repository = PgOrderRepository::new(pool.clone());
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
    let recording = repository
        .record_ticket_acquisition(workspace_id, ticket.id, acquisition_input(10, Some("4")))
        .await
        .unwrap()
        .recording;
    sqlx::query(
        "UPDATE sde_types SET name_en = 'Renamed Tritanium' WHERE import_id = $1 AND type_id = 34",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();

    repository
        .revert_ticket_inventory_recording(workspace_id, ticket.id, recording.id)
        .await
        .unwrap();
    let captured_name: String = sqlx::query_scalar(
        "SELECT captured_name FROM inventory_events WHERE event_kind = 'reversal'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(captured_name, "Tritanium");
}
