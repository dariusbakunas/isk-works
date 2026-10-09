use super::*;

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn delete_order_removes_frozen_plan_and_detaches_linked_tickets(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());

    let snapshot = empty_price_snapshot();
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let order_id = order.id;
    repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: vec![requirement(34, RequirementKind::Buy, 800)],
        })
        .await
        .unwrap();

    // A ticket organizationally linked to the Epic.
    let (ticket, _) = repository
        .create_ticket(NewTicket {
            order_id: Some(order_id),
            notes: String::new(),
            assignee_character_id: None,
            id: TicketId::new(),
            workspace_id,
            owner_id,
            kind: TicketKind::Acquisition,
            type_id: Some(34),
            captured_name: "Tritanium".to_string(),
            quantity: Some(800),
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
        .delete_order(workspace_id, order_id)
        .await
        .unwrap();

    assert!(matches!(
        repository.get_order(workspace_id, order_id).await,
        Err(OrderError::OrderNotFound)
    ));
    let detached = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(detached.order_id, None);
    let order_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM orders WHERE id = $1")
        .bind(order_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(order_rows, 0);
    let requirement_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM order_requirements WHERE order_id = $1")
            .bind(order_id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(requirement_rows, 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn delete_order_on_a_missing_order_is_not_found(pool: PgPool) {
    let (workspace_id, _owner_id, _import_id) = fixture_workspace(&pool).await;
    let repository = PgOrderRepository::new(pool.clone());
    assert!(matches!(
        repository.delete_order(workspace_id, OrderId::new()).await,
        Err(OrderError::OrderNotFound)
    ));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn delete_ticket_removes_an_acquisition_ticket(pool: PgPool) {
    let (workspace_id, owner_id, _import_id) = fixture_workspace(&pool).await;
    let repository = PgOrderRepository::new(pool.clone());

    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            44,
            "Vexor",
            1,
            None,
            None,
        ))
        .await
        .unwrap();

    repository
        .delete_ticket(workspace_id, ticket.id)
        .await
        .unwrap();

    assert!(matches!(
        repository.get_ticket(workspace_id, ticket.id).await,
        Err(OrderError::TicketNotFound)
    ));
    assert!(matches!(
        repository.delete_ticket(workspace_id, ticket.id).await,
        Err(OrderError::TicketNotFound)
    ));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deletion_ownership_acquisition_recording_survives_ticket_delete(pool: PgPool) {
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
    let outcome = repository
        .record_ticket_acquisition(
            workspace_id,
            ticket.id,
            RecordAcquisitionInput {
                idempotency_key: Uuid::new_v4(),
                quantity: 100,
                unit_cost: Some(Money::parse("4.25").unwrap()),
                location_note: String::new(),
                note: "durable purchase".to_string(),
                effective_at: crate::db_now(),
            },
        )
        .await
        .unwrap();
    let balance_before = inventory_balance(&pool, workspace_id, owner_id, 34).await;
    let event_before: (Uuid, i64, Decimal) = sqlx::query_as(
        "SELECT id, quantity_delta, total_cost_delta FROM inventory_events \
         WHERE ticket_inventory_recording_id = $1",
    )
    .bind(outcome.recording.id.0)
    .fetch_one(&pool)
    .await
    .unwrap();

    repository
        .delete_ticket(workspace_id, ticket.id)
        .await
        .unwrap();

    assert!(matches!(
        repository.get_ticket(workspace_id, ticket.id).await,
        Err(OrderError::TicketNotFound)
    ));
    let source_ticket_id: Uuid =
        sqlx::query_scalar("SELECT ticket_id FROM ticket_inventory_recordings WHERE id = $1")
            .bind(outcome.recording.id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(source_ticket_id, ticket.id.0);
    let event_after: (Uuid, i64, Decimal) = sqlx::query_as(
        "SELECT id, quantity_delta, total_cost_delta FROM inventory_events \
         WHERE ticket_inventory_recording_id = $1",
    )
    .bind(outcome.recording.id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(event_after, event_before);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        balance_before
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deletion_ownership_production_recording_survives_ticket_delete(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    seed_sde_type(&pool, import_id, 20185, "Product").await;
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
    let (ticket, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            20185,
            "Product",
            build_id,
            vec![prod_prereq(34, "Tritanium")],
            Some(1),
        ))
        .await
        .unwrap();
    let outcome = repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 10, &[(34, 100)], "500"),
        )
        .await
        .unwrap();
    let input_before = inventory_balance(&pool, workspace_id, owner_id, 34).await;
    let output_before = inventory_balance(&pool, workspace_id, owner_id, 20185).await;
    let events_before: Vec<(Uuid, String, i64, Decimal)> = sqlx::query_as(
        "SELECT id, event_kind, quantity_delta, total_cost_delta FROM inventory_events \
         WHERE ticket_inventory_recording_id = $1 ORDER BY sequence",
    )
    .bind(outcome.recording.id.0)
    .fetch_all(&pool)
    .await
    .unwrap();

    repository
        .delete_ticket(workspace_id, ticket.id)
        .await
        .unwrap();

    let source_ticket_id: Uuid =
        sqlx::query_scalar("SELECT ticket_id FROM ticket_inventory_recordings WHERE id = $1")
            .bind(outcome.recording.id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(source_ticket_id, ticket.id.0);
    let events_after: Vec<(Uuid, String, i64, Decimal)> = sqlx::query_as(
        "SELECT id, event_kind, quantity_delta, total_cost_delta FROM inventory_events \
         WHERE ticket_inventory_recording_id = $1 ORDER BY sequence",
    )
    .bind(outcome.recording.id.0)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(events_after, events_before);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        input_before
    );
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 20185).await,
        output_before
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deletion_ownership_epic_delete_detaches_ticket_and_keeps_recording(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());
    let snapshot = empty_price_snapshot();
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let order_id = order.id;
    let snapshot_id = snapshot.id;
    repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: vec![requirement(34, RequirementKind::Buy, 10)],
        })
        .await
        .unwrap();
    let mut new_ticket =
        new_acquisition_ticket_no_estimate(workspace_id, owner_id, 34, "Tritanium", 10);
    new_ticket.order_id = Some(order_id);
    let (ticket, _) = repository.create_ticket(new_ticket).await.unwrap();
    let outcome = repository
        .record_ticket_acquisition(
            workspace_id,
            ticket.id,
            RecordAcquisitionInput {
                idempotency_key: Uuid::new_v4(),
                quantity: 10,
                unit_cost: Some(Money::parse("5").unwrap()),
                location_note: String::new(),
                note: String::new(),
                effective_at: crate::db_now(),
            },
        )
        .await
        .unwrap();
    let balance_before = inventory_balance(&pool, workspace_id, owner_id, 34).await;

    repository
        .delete_order(workspace_id, order_id)
        .await
        .unwrap();

    let detached = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(detached.order_id, None);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM ticket_inventory_recordings WHERE id = $1"
        )
        .bind(outcome.recording.id.0)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM inventory_events WHERE ticket_inventory_recording_id = $1"
        )
        .bind(outcome.recording.id.0)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        balance_before
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM price_snapshots WHERE id = $1")
            .bind(snapshot_id.0)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0,
        "the frozen snapshot is Epic-owned once the Epic is deleted"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deletion_ownership_ticket_delete_cleans_workflow_links_only(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());
    let (dependent, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            20185,
            "Product",
            build_id,
            vec![prod_prereq(34, "Tritanium")],
            Some(1),
        ))
        .await
        .unwrap();
    let (fulfiller, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
        ))
        .await
        .unwrap();
    let prerequisites = repository
        .list_ticket_prerequisites(dependent.id)
        .await
        .unwrap();
    repository
        .link_ticket_prerequisite_to_ticket(prerequisites[0].id, fulfiller.id, 50)
        .await
        .unwrap();

    repository
        .delete_ticket(workspace_id, dependent.id)
        .await
        .unwrap();

    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM ticket_prerequisites WHERE ticket_id = $1"
        )
        .bind(dependent.id.0)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM ticket_prerequisite_fulfillments WHERE fulfilling_ticket_id = $1"
        )
        .bind(fulfiller.id.0)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        repository
            .get_ticket(workspace_id, fulfiller.id)
            .await
            .unwrap()
            .id,
        fulfiller.id
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM inventory_events")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM inventory_balances")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deletion_ownership_acquisition_run_survives_until_last_ticket_is_deleted(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    seed_sde_type(&pool, import_id, 35, "Pyerite").await;
    let source = fixture_price_source(&pool, workspace_id, "Jita").await;
    let repository = PgOrderRepository::new(pool.clone());
    let (first, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            10,
            None,
            Some(source),
        ))
        .await
        .unwrap();
    let (second, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            35,
            "Pyerite",
            10,
            None,
            Some(source),
        ))
        .await
        .unwrap();
    let run = repository
        .create_order_acquisition_run(workspace_id, owner_id, None, vec![first.id, second.id])
        .await
        .unwrap();

    repository
        .delete_ticket(workspace_id, first.id)
        .await
        .unwrap();
    assert_eq!(
        repository
            .get_order_acquisition_run(workspace_id, run.id)
            .await
            .unwrap()
            .id,
        run.id
    );
    assert_eq!(
        repository
            .get_ticket(workspace_id, second.id)
            .await
            .unwrap()
            .acquisition_run_id,
        Some(run.id)
    );

    repository
        .delete_ticket(workspace_id, second.id)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM acquisition_runs WHERE id = $1")
            .bind(run.id.0)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM acquisition_run_items WHERE acquisition_run_id = $1"
        )
        .bind(run.id.0)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deletion_ownership_production_records_once_after_source_build_delete(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    seed_sde_type(&pool, import_id, 20185, "Product").await;
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
    let order_repository = PgOrderRepository::new(pool.clone());
    let industry_repository = PgIndustryRepository::new(pool.clone());
    let (ticket, _) = order_repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            20185,
            "Product",
            build_id,
            vec![prod_prereq(34, "Tritanium")],
            Some(1),
        ))
        .await
        .unwrap();
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM builds WHERE id = $1")
        .bind(build_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    industry_repository
        .delete_build(
            workspace_id,
            build_id,
            u64::try_from(revision).unwrap(),
            false,
        )
        .await
        .unwrap();
    let detached = order_repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(detached.source_build_id, None);

    let key = Uuid::new_v4();
    let input = RecordProductionInput {
        idempotency_key: key,
        runs_completed: 1,
        installation_cost: Money::parse("500").unwrap(),
        output_type_id: 20185,
        output_quantity: 10,
        inputs: vec![iskworks_core::order::RecordProductionInputLine {
            type_id: 34,
            quantity: 100,
        }],
        location_note: String::new(),
        note: String::new(),
        effective_at: crate::db_now(),
        take_from: Vec::new(),
    };
    let first = order_repository
        .record_ticket_production(workspace_id, ticket.id, input.clone())
        .await
        .unwrap();
    let replay = order_repository
        .record_ticket_production(workspace_id, ticket.id, input)
        .await
        .unwrap();
    assert!(first.created);
    assert!(!replay.created);
    assert_eq!(replay.recording.id, first.recording.id);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM inventory_events WHERE ticket_inventory_recording_id = $1"
        )
        .bind(first.recording.id.0)
        .fetch_one(&pool)
        .await
        .unwrap(),
        2,
        "one consumption and one output are posted exactly once"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deletion_ownership_recorded_ticket_history_survives_source_build_delete(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    seed_sde_type(&pool, import_id, 20185, "Product").await;
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
    let order_repository = PgOrderRepository::new(pool.clone());
    let industry_repository = PgIndustryRepository::new(pool.clone());
    let (ticket, _) = order_repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            20185,
            "Product",
            build_id,
            vec![prod_prereq(34, "Tritanium")],
            Some(1),
        ))
        .await
        .unwrap();
    let recording = order_repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(1, 20185, 10, &[(34, 100)], "500"),
        )
        .await
        .unwrap()
        .recording;
    let events_before: Vec<(Uuid, String, i64, Decimal)> = sqlx::query_as(
        "SELECT id, event_kind, quantity_delta, total_cost_delta FROM inventory_events \
         WHERE ticket_inventory_recording_id = $1 ORDER BY sequence",
    )
    .bind(recording.id.0)
    .fetch_all(&pool)
    .await
    .unwrap();
    let input_before = inventory_balance(&pool, workspace_id, owner_id, 34).await;
    let output_before = inventory_balance(&pool, workspace_id, owner_id, 20185).await;
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM builds WHERE id = $1")
        .bind(build_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();

    industry_repository
        .delete_build(
            workspace_id,
            build_id,
            u64::try_from(revision).unwrap(),
            false,
        )
        .await
        .unwrap();

    assert_eq!(
        order_repository
            .get_ticket(workspace_id, ticket.id)
            .await
            .unwrap()
            .source_build_id,
        None
    );
    let events_after: Vec<(Uuid, String, i64, Decimal)> = sqlx::query_as(
        "SELECT id, event_kind, quantity_delta, total_cost_delta FROM inventory_events \
         WHERE ticket_inventory_recording_id = $1 ORDER BY sequence",
    )
    .bind(recording.id.0)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(events_after, events_before);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        input_before
    );
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 20185).await,
        output_before
    );

    order_repository
        .revert_ticket_inventory_recording(workspace_id, ticket.id, recording.id)
        .await
        .unwrap();
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        Some((100, "1000.0000".parse().unwrap(), 3)),
        "reversal uses retained ledger evidence after the source Build is gone"
    );
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 20185).await,
        Some((0, "0.0000".parse().unwrap(), 2))
    );
}

async fn fk_delete_action(pool: &PgPool, constraint_name: &str) -> Option<String> {
    sqlx::query_scalar(
        r#"
        SELECT CASE confdeltype
          WHEN 'a' THEN 'NO ACTION'
          WHEN 'r' THEN 'RESTRICT'
          WHEN 'c' THEN 'CASCADE'
          WHEN 'n' THEN 'SET NULL'
          WHEN 'd' THEN 'SET DEFAULT'
        END
        FROM pg_constraint
        WHERE contype = 'f' AND conname = $1
        "#,
    )
    .bind(constraint_name)
    .fetch_optional(pool)
    .await
    .unwrap()
}

async fn column_is_nullable(pool: &PgPool, table: &str, column: &str) -> bool {
    sqlx::query_scalar(
        "SELECT is_nullable = 'YES' FROM information_schema.columns \
         WHERE table_schema = 'public' AND table_name = $1 AND column_name = $2",
    )
    .bind(table)
    .bind(column)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deletion_ownership_schema_uses_provenance_constraints(pool: PgPool) {
    for constraint in [
        "orders_source_build_id_fkey",
        "order_plan_operations_build_id_fkey",
        "order_requirements_source_build_id_fkey",
        "tickets_source_build_id_fkey",
        "ticket_prerequisites_source_build_id_fkey",
        "price_snapshots_build_id_fkey",
    ] {
        assert_eq!(
            fk_delete_action(&pool, constraint).await.as_deref(),
            Some("SET NULL"),
            "{constraint} must detach historical provenance"
        );
    }
    assert_eq!(
        fk_delete_action(&pool, "ticket_inventory_recordings_ticket_id_fkey").await,
        None,
        "recording.ticket_id is an immutable source UUID, not a live FK"
    );
    assert!(
        !column_is_nullable(&pool, "ticket_inventory_recordings", "ticket_id").await,
        "historical Ticket UUID provenance must never be erased"
    );
    assert_eq!(
        fk_delete_action(&pool, "inventory_events_ticket_inventory_recording_id_fkey")
            .await
            .as_deref(),
        Some("RESTRICT"),
        "ledger events must continue protecting their recording"
    );

    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());
    let (production, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            20185,
            "Charon",
            build_id,
            vec![prod_prereq(34, "Tritanium")],
            Some(1),
        ))
        .await
        .unwrap();
    sqlx::query("UPDATE tickets SET source_build_id = NULL WHERE id = $1")
        .bind(production.id.0)
        .execute(&pool)
        .await
        .expect("a production snapshot remains valid without its source Build");

    let (acquisition, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            1,
            None,
            None,
        ))
        .await
        .unwrap();
    let invalid = sqlx::query("UPDATE tickets SET source_build_id = $1 WHERE id = $2")
        .bind(build_id.0)
        .bind(acquisition.id.0)
        .execute(&pool)
        .await;
    assert!(
        invalid.is_err(),
        "acquisition tickets must not acquire a production Build source"
    );
}
