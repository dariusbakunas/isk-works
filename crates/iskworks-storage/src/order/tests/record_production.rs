use super::*;

// ─── Explicit inventory recording -- production ───────────────────────────
//
// One `record_ticket_production` call = one immutable recording row + N
// Consumption events + (when output > 0) one ProductionOutput event, in one
// transaction. Output basis = Σ abs(consumption cost) + installation cost
// (Known quality). Touches nothing organizational; idempotent on
// `(ticket_id, idempotency_key)`.

/// Every event of `kind`, ordered by `(type_id, sequence)`:
/// `(quantity_delta, total_cost_delta, cost_quality, ticket_inventory_recording_id)`.
async fn events_of_kind(pool: &PgPool, kind: &str) -> Vec<(i64, Decimal, String, Option<Uuid>)> {
    sqlx::query_as(
        "SELECT quantity_delta, total_cost_delta, cost_quality, ticket_inventory_recording_id \
         FROM inventory_events WHERE event_kind = $1 \
         ORDER BY workspace_id, owner_id, type_id, sequence",
    )
    .bind(kind)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_ticket_persists_the_execution_snapshot_without_touching_inventory(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 34, 5_000).await;
    let repository = PgOrderRepository::new(pool.clone());
    let balance_before = inventory_balance(&pool, workspace_id, owner_id, 34).await;

    let expected = iskworks_core::TaskExecutionSnapshot {
        runs: 100,
        blueprint: None,
        facility: None,
        duration_seconds: Some(43_200),
        installation_cost: None,
        material_value: Some(Money::parse("987654.0000").unwrap()),
    };
    let mut new_ticket = mfg_ticket(
        workspace_id,
        owner_id,
        TicketKind::Manufacturing,
        20185,
        "Product",
        build_id,
        vec![prod_prereq(34, "Material A")],
        None,
    );
    new_ticket.execution_snapshot = Some(expected.clone());

    let (ticket, _) = repository.create_ticket(new_ticket).await.unwrap();

    // The frozen plan round-trips unchanged through the JSONB column.
    assert_eq!(ticket.execution_snapshot.as_ref(), Some(&expected));
    let reloaded = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(reloaded.execution_snapshot.as_ref(), Some(&expected));

    // Freezing the plan is still zero inventory movement.
    assert!(allocation_snapshot(&pool).await.is_empty());
    assert_eq!(inventory_event_count(&pool).await, 0);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        balance_before
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_acquisition_ticket_persists_a_null_execution_snapshot(pool: PgPool) {
    let (workspace_id, owner_id, _import_id) = fixture_workspace(&pool).await;
    let repository = PgOrderRepository::new(pool.clone());

    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            1_000,
            None,
            None,
        ))
        .await
        .unwrap();

    assert!(ticket.execution_snapshot.is_none());
    let reloaded = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert!(reloaded.execution_snapshot.is_none());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_posts_consumptions_and_output_at_batch_basis(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    for (type_id, name) in [
        (34_i64, "Material A"),
        (35, "Material B"),
        (20185, "Product"),
    ] {
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
    let before = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();

    let outcome = repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 10, &[(34, 100), (35, 50)], "500"),
        )
        .await
        .unwrap();

    assert!(outcome.created);
    assert_eq!(outcome.recording.runs_completed, Some(1));
    assert_eq!(
        outcome.recording.installation_cost,
        Some(Money::parse("500").unwrap())
    );
    assert_eq!(outcome.recording.output_type_id, Some(20185));
    assert_eq!(outcome.recording.output_quantity, Some(10));
    // No plan captured -> requested defaults to recorded, so Recorded.
    assert_eq!(outcome.summary.state, RecordingState::Recorded);

    // Inputs fully drawn.
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        Some((0, "0.0000".parse::<Decimal>().unwrap(), 2))
    );
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 35).await,
        Some((0, "0.0000".parse::<Decimal>().unwrap(), 2))
    );
    let consumptions = events_of_kind(&pool, "consumption").await;
    assert_eq!(consumptions.len(), 2);
    for (quantity_delta, total_cost_delta, cost_quality, recording_id) in &consumptions {
        assert!(*quantity_delta < 0);
        assert_eq!(*total_cost_delta, "-1000.0000".parse::<Decimal>().unwrap());
        assert_eq!(cost_quality, "known");
        assert_eq!(*recording_id, Some(outcome.recording.id.0));
    }

    // Output: batch basis 1000 + 1000 + 500 = 2500 over 10 units.
    let outputs = events_of_kind(&pool, "production_output").await;
    assert_eq!(outputs.len(), 1);
    let (quantity_delta, total_cost_delta, cost_quality, recording_id) = &outputs[0];
    assert_eq!(*quantity_delta, 10);
    assert_eq!(*total_cost_delta, "2500.0000".parse::<Decimal>().unwrap());
    assert_eq!(cost_quality, "known");
    assert_eq!(*recording_id, Some(outcome.recording.id.0));
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 20185).await,
        Some((10, "2500.0000".parse::<Decimal>().unwrap(), 1))
    );

    // Nothing organizational moved.
    let after = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(after.status, before.status);
    assert_eq!(after.updated_at, before.updated_at);
    assert_eq!(after.archived_at, before.archived_at);
    assert_eq!(recording_count(&pool).await, 1);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_adds_to_an_existing_output_balance(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    for (type_id, name) in [
        (34_i64, "Material A"),
        (35, "Material B"),
        (20185, "Product"),
    ] {
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
    // Product already on hand: 5 units, basis 500 (avg 100).
    seed_inventory_balance_with_cost(
        &pool,
        workspace_id,
        owner_id,
        20185,
        "Product",
        5,
        "500.0000",
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

    repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 10, &[(34, 100), (35, 50)], "500"),
        )
        .await
        .unwrap();

    // 5 @ 500 + 10 @ 2500 => 15 units, total 3000, avg 200.
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 20185).await,
        Some((15, "3000.0000".parse::<Decimal>().unwrap(), 2))
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_consumption_basis_is_the_actual_weighted_average_removed(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    for (type_id, name) in [(34_i64, "Material A"), (20185, "Product")] {
        seed_sde_type(&pool, import_id, type_id, name).await;
    }
    // A: qty 100, basis 1000 => avg 10.
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

    let outcome = repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 4, &[(34, 40)], "0"),
        )
        .await
        .unwrap();

    // Consume 40 @ avg 10 = 400 removed; A left with 60 @ 600.
    let consumptions = events_of_kind(&pool, "consumption").await;
    assert_eq!(consumptions[0].1, "-400.0000".parse::<Decimal>().unwrap());
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        Some((60, "600.0000".parse::<Decimal>().unwrap(), 2))
    );
    // Output basis = 400 consumed + 0 install = 400 over 4 units.
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 20185).await,
        Some((4, "400.0000".parse::<Decimal>().unwrap(), 1))
    );
    assert_eq!(outcome.recording.installation_cost, Some(Money::zero()));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_surplus_output_shares_the_full_batch_basis(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    for (type_id, name) in [
        (34_i64, "Material A"),
        (35, "Material B"),
        (20185, "Product"),
    ] {
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
    // Ticket "needs" 10 units (quantity), but the batch produced 50.
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

    repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 50, &[(34, 100), (35, 50)], "500"),
        )
        .await
        .unwrap();

    // All 50 produced units enter inventory at the same per-unit basis:
    // batch basis 2500 / 50 = 50 each. The extra 40 are ordinary stock.
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 20185).await,
        Some((50, "2500.0000".parse::<Decimal>().unwrap(), 1))
    );
    let (_, _, _, avg): (i64, Decimal, Decimal, Option<Decimal>) = sqlx::query_as(
        "SELECT quantity_delta, total_cost_delta, resulting_total_cost, resulting_average_cost \
         FROM inventory_events WHERE event_kind = 'production_output'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(avg, Some("50.0000".parse::<Decimal>().unwrap()));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_partial_runs_reach_recorded(pool: PgPool) {
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
        1_000_000,
        "1000000.0000",
    )
    .await;
    let repository = PgOrderRepository::new(pool.clone());
    // Plan: 100 runs (captured in the execution snapshot).
    let (ticket, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            20185,
            "Product",
            build_id,
            vec![prod_prereq(34, "Material A")],
            Some(100),
        ))
        .await
        .unwrap();

    let first = repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(40, 20185, 1, &[(34, 1)], "0"),
        )
        .await
        .unwrap();
    assert_eq!(first.summary.state, RecordingState::PartiallyRecorded);
    assert_eq!(first.summary.recorded_quantity, 40);
    assert_eq!(first.summary.remaining_quantity, 60);

    let second = repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(35, 20185, 1, &[(34, 1)], "0"),
        )
        .await
        .unwrap();
    assert_eq!(second.summary.state, RecordingState::PartiallyRecorded);
    assert_eq!(second.summary.recorded_quantity, 75);

    let third = repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(30, 20185, 1, &[(34, 1)], "0"),
        )
        .await
        .unwrap();
    assert_eq!(third.summary.state, RecordingState::Recorded);
    assert_eq!(third.summary.recorded_quantity, 105);
    assert_eq!(third.summary.remaining_quantity, 0);
    assert_eq!(third.summary.surplus_quantity, 5);

    assert_eq!(recording_count(&pool).await, 3);
    assert_eq!(events_of_kind(&pool, "consumption").await.len(), 3);
    assert_eq!(events_of_kind(&pool, "production_output").await.len(), 3);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_is_idempotent_on_a_repeated_key(pool: PgPool) {
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
        1000,
        "10000.0000",
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

    let key = Uuid::new_v4();
    let mut body = production_input(5, 20185, 10, &[(34, 50)], "100");
    body.idempotency_key = key;
    let first = repository
        .record_ticket_production(workspace_id, ticket.id, body)
        .await
        .unwrap();
    assert!(first.created);
    let balance_after_first = inventory_balance(&pool, workspace_id, owner_id, 20185).await;
    let material_after_first = inventory_balance(&pool, workspace_id, owner_id, 34).await;

    // Replay: same key, different body -- post nothing.
    let mut replay_body = production_input(999, 20185, 999, &[(34, 999)], "9999");
    replay_body.idempotency_key = key;
    let replay = repository
        .record_ticket_production(workspace_id, ticket.id, replay_body)
        .await
        .unwrap();
    assert!(!replay.created);
    assert_eq!(replay.recording.id, first.recording.id);
    assert_eq!(replay.recording.runs_completed, Some(5));

    assert_eq!(recording_count(&pool).await, 1);
    assert_eq!(events_of_kind(&pool, "consumption").await.len(), 1);
    assert_eq!(events_of_kind(&pool, "production_output").await.len(), 1);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 20185).await,
        balance_after_first
    );
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        material_after_first
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_with_different_keys_creates_two_recordings(pool: PgPool) {
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
        1000,
        "10000.0000",
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
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 5, &[(34, 10)], "0"),
        )
        .await
        .unwrap();
    repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 5, &[(34, 10)], "0"),
        )
        .await
        .unwrap();

    assert_eq!(recording_count(&pool).await, 2);
    assert_eq!(events_of_kind(&pool, "consumption").await.len(), 2);
    assert_eq!(events_of_kind(&pool, "production_output").await.len(), 2);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_rolls_back_entirely_when_the_output_post_fails(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    // Seed the input type but NOT the product type -- `verify_active_type`
    // inside the ProductionOutput post fails after the Consumption has run.
    seed_sde_type(&pool, import_id, 34, "Material A").await;
    seed_inventory_balance_with_cost(
        &pool,
        workspace_id,
        owner_id,
        34,
        "Material A",
        1000,
        "10000.0000",
    )
    .await;
    let material_before = inventory_balance(&pool, workspace_id, owner_id, 34).await;
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

    let error = repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 10, &[(34, 50)], "500"),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, OrderError::Persistence(_)));

    assert_eq!(recording_count(&pool).await, 0);
    assert_eq!(inventory_event_count(&pool).await, 0);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        material_before
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_fails_when_an_input_is_not_in_stock(pool: PgPool) {
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
        30,
        "300.0000",
    )
    .await;
    let material_before = inventory_balance(&pool, workspace_id, owner_id, 34).await;
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

    let error = repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 10, &[(34, 50)], "0"),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, OrderError::InsufficientInventory));

    assert_eq!(recording_count(&pool).await, 0);
    assert_eq!(inventory_event_count(&pool).await, 0);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        material_before
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_rejects_an_output_type_that_is_not_the_ticket_product(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    for (type_id, name) in [(34_i64, "Material A"), (20185, "Product"), (99999, "Other")] {
        seed_sde_type(&pool, import_id, type_id, name).await;
    }
    seed_inventory_balance_with_cost(
        &pool,
        workspace_id,
        owner_id,
        34,
        "Material A",
        1000,
        "10000.0000",
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

    let error = repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 99999, 10, &[(34, 10)], "0"),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, OrderError::RecordingOutputTypeMismatch));
    assert_eq!(recording_count(&pool).await, 0);
    assert_eq!(inventory_event_count(&pool).await, 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_rejects_an_input_that_is_not_a_prerequisite(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    for (type_id, name) in [
        (34_i64, "Material A"),
        (77, "Unrelated"),
        (20185, "Product"),
    ] {
        seed_sde_type(&pool, import_id, type_id, name).await;
    }
    seed_inventory_balance_with_cost(
        &pool,
        workspace_id,
        owner_id,
        77,
        "Unrelated",
        1000,
        "10000.0000",
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

    let error = repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 10, &[(77, 10)], "0"),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, OrderError::RecordingInputNotAPrerequisite));
    assert_eq!(recording_count(&pool).await, 0);
    assert_eq!(inventory_event_count(&pool).await, 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_records_a_reaction_ticket(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    for (type_id, name) in [
        (16634_i64, "Tritanium Bar"),
        (16644, "Pyerite Bar"),
        (16663, "Fernite Carbide"),
    ] {
        seed_sde_type(&pool, import_id, type_id, name).await;
    }
    seed_inventory_balance_with_cost(
        &pool,
        workspace_id,
        owner_id,
        16634,
        "Tritanium Bar",
        200,
        "2000.0000",
    )
    .await;
    seed_inventory_balance_with_cost(
        &pool,
        workspace_id,
        owner_id,
        16644,
        "Pyerite Bar",
        200,
        "4000.0000",
    )
    .await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Reaction,
            16663,
            "Fernite Carbide",
            build_id,
            vec![
                prod_prereq(16634, "Tritanium Bar"),
                prod_prereq(16644, "Pyerite Bar"),
            ],
            None,
        ))
        .await
        .unwrap();

    let outcome = repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 16663, 20, &[(16634, 100), (16644, 100)], "0"),
        )
        .await
        .unwrap();
    assert!(outcome.created);
    // consumed: 100 @ (2000/200=10) = 1000; 100 @ (4000/200=20) = 2000 => 3000 over 20 units.
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 16663).await,
        Some((20, "3000.0000".parse::<Decimal>().unwrap(), 1))
    );
    assert_eq!(
        events_of_kind(&pool, "production_output").await[0].2,
        "known"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_rejects_an_acquisition_ticket(pool: PgPool) {
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

    let error = repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 34, 10, &[], "0"),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        OrderError::RecordingRequiresProductionTicket
    ));
    assert_eq!(recording_count(&pool).await, 0);
    assert_eq!(inventory_event_count(&pool).await, 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_rejects_a_ticket_from_another_workspace(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let (other_workspace_id, _other_owner_id) = fixture_second_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_sde_type(&pool, import_id, 34, "Material A").await;
    seed_sde_type(&pool, import_id, 20185, "Product").await;
    seed_inventory_balance_with_cost(
        &pool,
        workspace_id,
        owner_id,
        34,
        "Material A",
        1000,
        "10000.0000",
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

    let error = repository
        .record_ticket_production(
            other_workspace_id,
            ticket.id,
            production_input(1, 20185, 10, &[(34, 10)], "0"),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, OrderError::TicketNotFound));
    assert_eq!(recording_count(&pool).await, 0);
    assert_eq!(inventory_event_count(&pool).await, 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_is_independent_of_ticket_status(pool: PgPool) {
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
        1_000_000,
        "1000000.0000",
    )
    .await;
    let repository = PgOrderRepository::new(pool.clone());

    for status in [
        TicketStatus::Todo,
        TicketStatus::InProgress,
        TicketStatus::Complete,
    ] {
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
            .set_ticket_status(workspace_id, ticket.id, status)
            .await
            .unwrap();
        let before = repository
            .get_ticket(workspace_id, ticket.id)
            .await
            .unwrap();

        let outcome = repository
            .record_ticket_production(
                workspace_id,
                ticket.id,
                production_input(1, 20185, 5, &[(34, 10)], "0"),
            )
            .await
            .unwrap();
        assert!(outcome.created, "recording must succeed from {status:?}");

        let after = repository
            .get_ticket(workspace_id, ticket.id)
            .await
            .unwrap();
        assert_eq!(after.status, status);
        assert_eq!(after.updated_at, before.updated_at);
        assert_eq!(after.archived_at, before.archived_at);
    }
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_does_not_recheck_dependent_tickets(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    for (type_id, name) in [(34_i64, "Material A"), (20185, "Component")] {
        seed_sde_type(&pool, import_id, type_id, name).await;
    }
    seed_inventory_balance_with_cost(
        &pool,
        workspace_id,
        owner_id,
        34,
        "Material A",
        1_000_000,
        "1000000.0000",
    )
    .await;
    let repository = PgOrderRepository::new(pool.clone());

    // Dependent: a manufacturing ticket with an unmet prerequisite fed by
    // the production ticket `component`.
    let (dependent, dependent_prereqs) = repository
        .create_ticket(NewTicket {
            order_id: None,
            notes: String::new(),
            assignee_character_id: None,
            id: TicketId::new(),
            workspace_id,
            owner_id,
            kind: TicketKind::Manufacturing,
            type_id: Some(30000),
            captured_name: "Assembly".to_string(),
            quantity: Some(1),
            source_build_id: Some(build_id),
            estimated_unit_cost: None,
            estimated_line_total: None,
            market_region_id: None,
            market_location_id: None,
            price_source_id: None,
            execution_snapshot: None,
            prerequisites: vec![ticket_prerequisite(20185, 50)],
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
    assert_eq!(dependent.status, TicketStatus::Todo);

    let (component, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            20185,
            "Component",
            build_id,
            vec![prod_prereq(34, "Material A")],
            None,
        ))
        .await
        .unwrap();
    repository
        .link_ticket_prerequisite_to_ticket(dependent_prereqs[0].id, component.id, 50)
        .await
        .unwrap();

    let dependent_before = repository
        .get_ticket(workspace_id, dependent.id)
        .await
        .unwrap();

    repository
        .record_ticket_production(
            workspace_id,
            component.id,
            production_input(1, 20185, 50, &[(34, 100)], "0"),
        )
        .await
        .unwrap();

    let dependent_after = repository
        .get_ticket(workspace_id, dependent.id)
        .await
        .unwrap();
    assert_eq!(dependent_after.status, TicketStatus::Todo);
    assert_eq!(dependent_after.updated_at, dependent_before.updated_at);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_with_zero_output_consumes_but_posts_no_output(pool: PgPool) {
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
        1000,
        "10000.0000",
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

    let outcome = repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 0, &[(34, 50)], "500"),
        )
        .await
        .unwrap();

    assert_eq!(outcome.recording.output_quantity, Some(0));
    assert_eq!(events_of_kind(&pool, "consumption").await.len(), 1);
    assert_eq!(events_of_kind(&pool, "production_output").await.len(), 0);
    // Input drawn; no Product balance row created.
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        Some((950, "9500.0000".parse::<Decimal>().unwrap(), 2))
    );
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 20185).await,
        None
    );

    repository
        .revert_ticket_inventory_recording(workspace_id, ticket.id, outcome.recording.id)
        .await
        .unwrap();
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        Some((1000, "10000.0000".parse::<Decimal>().unwrap(), 3))
    );
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 20185).await,
        None
    );
    let reversals = events_of_kind(&pool, "reversal").await;
    assert_eq!(reversals.len(), 1);
    assert_eq!(reversals[0].0, 50);
    assert_eq!(reversals[0].1, "500.0000".parse().unwrap());
}
