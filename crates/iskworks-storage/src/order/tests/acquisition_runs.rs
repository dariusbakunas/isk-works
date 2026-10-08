use super::*;

// ─── Acquisition Run batching for standalone tickets ───────────────────────

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn creates_a_run_from_compatible_unbatched_acquisition_tickets(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    seed_sde_type(&pool, import_id, 35, "Pyerite").await;
    let source = fixture_price_source(&pool, workspace_id, "Jita").await;
    let repository = PgOrderRepository::new(pool.clone());

    let (a, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
            None,
            Some(source),
        ))
        .await
        .unwrap();
    let (b, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            35,
            "Pyerite",
            50,
            None,
            Some(source),
        ))
        .await
        .unwrap();

    let run = repository
        .create_order_acquisition_run(workspace_id, owner_id, None, vec![a.id, b.id])
        .await
        .unwrap();
    assert_eq!(run.price_source_id, Some(source));

    let members = repository
        .list_order_acquisition_run_tickets(workspace_id, run.id)
        .await
        .unwrap();
    assert_eq!(members.len(), 2);
    assert!(members
        .iter()
        .all(|ticket| ticket.acquisition_run_id == Some(run.id)));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn rejects_tickets_with_different_price_sources(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    seed_sde_type(&pool, import_id, 35, "Pyerite").await;
    let jita = fixture_price_source(&pool, workspace_id, "Jita").await;
    let amarr = fixture_price_source(&pool, workspace_id, "Amarr").await;
    let repository = PgOrderRepository::new(pool.clone());

    let (a, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
            None,
            Some(jita),
        ))
        .await
        .unwrap();
    let (b, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            35,
            "Pyerite",
            50,
            None,
            Some(amarr),
        ))
        .await
        .unwrap();

    let result = repository
        .create_order_acquisition_run(workspace_id, owner_id, None, vec![a.id, b.id])
        .await;
    assert!(matches!(
        result,
        Err(OrderError::AcquisitionRunCrossesIncompatibleLocation)
    ));
}

/// Batching compatibility keys off `(market_region_id,
/// market_location_id)` for a market-priced ticket, not `price_source_id`
/// -- two tickets frozen from the same market scope group together even
/// though their `price_source_id`s differ.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn creates_a_run_from_tickets_sharing_market_scope_despite_different_price_sources(
    pool: PgPool,
) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    seed_sde_type(&pool, import_id, 35, "Pyerite").await;
    let jita = fixture_price_source(&pool, workspace_id, "Jita").await;
    let amarr = fixture_price_source(&pool, workspace_id, "Amarr").await;
    let repository = PgOrderRepository::new(pool.clone());
    let scope = MarketScope {
        region_id: 10_000_002,
        location_id: Some(60_003_760),
    };

    let (a, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
            Some(scope),
            Some(jita),
        ))
        .await
        .unwrap();
    let (b, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            35,
            "Pyerite",
            50,
            Some(scope),
            Some(amarr),
        ))
        .await
        .unwrap();

    let run = repository
        .create_order_acquisition_run(workspace_id, owner_id, None, vec![a.id, b.id])
        .await
        .unwrap();
    assert_eq!(run.market_region_id, Some(scope.region_id));
    assert_eq!(run.market_location_id, scope.location_id);
    // Scope took priority over `price_source_id` as the batching key, so
    // the Run itself carries no manual-provenance source.
    assert_eq!(run.price_source_id, None);
}

/// The scope-based counterpart to `rejects_tickets_with_different_price_sources`
/// -- two market-priced tickets frozen from different regions can never
/// batch together, even though neither carries a `price_source_id` at all.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn rejects_tickets_with_different_market_scopes(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    seed_sde_type(&pool, import_id, 35, "Pyerite").await;
    let repository = PgOrderRepository::new(pool.clone());
    let jita = MarketScope {
        region_id: 10_000_002,
        location_id: Some(60_003_760),
    };
    let amarr = MarketScope {
        region_id: 10_000_043,
        location_id: Some(60_008_494),
    };

    let (a, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
            Some(jita),
            None,
        ))
        .await
        .unwrap();
    let (b, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            35,
            "Pyerite",
            50,
            Some(amarr),
            None,
        ))
        .await
        .unwrap();

    let result = repository
        .create_order_acquisition_run(workspace_id, owner_id, None, vec![a.id, b.id])
        .await;
    assert!(matches!(
        result,
        Err(OrderError::AcquisitionRunCrossesIncompatibleLocation)
    ));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn rejects_a_non_acquisition_ticket(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_sde_type(&pool, import_id, 20185, "Crystalline Carbonide Armor Plate").await;
    let source = fixture_price_source(&pool, workspace_id, "Jita").await;
    let repository = PgOrderRepository::new(pool.clone());

    let (manufacturing_ticket, _) = repository
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
            quantity: Some(10),
            source_build_id: Some(build_id),
            estimated_unit_cost: None,
            estimated_line_total: None,
            market_region_id: None,
            market_location_id: None,
            price_source_id: Some(source),
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
    // Every ticket starts Todo (the batchable status), so this exercises
    // the *kind* guard specifically, not the status guard.
    assert_eq!(manufacturing_ticket.status, TicketStatus::Todo);

    let result = repository
        .create_order_acquisition_run(workspace_id, owner_id, None, vec![manufacturing_ticket.id])
        .await;
    assert!(matches!(result, Err(OrderError::TicketNotBatchable)));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn rejects_an_already_batched_ticket(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let source = fixture_price_source(&pool, workspace_id, "Jita").await;
    let repository = PgOrderRepository::new(pool.clone());

    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
            None,
            Some(source),
        ))
        .await
        .unwrap();
    repository
        .create_order_acquisition_run(workspace_id, owner_id, None, vec![ticket.id])
        .await
        .unwrap();

    let result = repository
        .create_order_acquisition_run(workspace_id, owner_id, None, vec![ticket.id])
        .await;
    assert!(matches!(result, Err(OrderError::TicketNotBatchable)));
}

/// Starting a Run advances the Run's own state only. A member Ticket's
/// workflow `status` is user-controlled and this operation never touches
/// it -- the Run and the Ticket workflow lane are independent axes.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn starting_a_run_advances_the_run_only_and_never_a_member_tickets_status(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let source = fixture_price_source(&pool, workspace_id, "Jita").await;
    let repository = PgOrderRepository::new(pool.clone());

    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
            None,
            Some(source),
        ))
        .await
        .unwrap();
    let run = repository
        .create_order_acquisition_run(workspace_id, owner_id, None, vec![ticket.id])
        .await
        .unwrap();

    let started = repository
        .start_order_acquisition_run(workspace_id, owner_id, run.id)
        .await
        .unwrap();
    assert_eq!(
        started.status,
        iskworks_core::AcquisitionRunStatus::InProgress
    );

    let ticket = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(
        ticket.status,
        TicketStatus::Todo,
        "starting the Run must not rewrite the member Ticket's workflow status"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn recording_progress_allows_over_acquisition_and_posts_no_inventory(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let source = fixture_price_source(&pool, workspace_id, "Jita").await;
    let repository = PgOrderRepository::new(pool.clone());

    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            2985,
            None,
            Some(source),
        ))
        .await
        .unwrap();
    let run = repository
        .create_order_acquisition_run(workspace_id, owner_id, None, vec![ticket.id])
        .await
        .unwrap();
    repository
        .start_order_acquisition_run(workspace_id, owner_id, run.id)
        .await
        .unwrap();

    // Acquiring more than the ticket needs is valid input, not an error.
    repository
        .record_order_acquisition_progress(
            workspace_id,
            run.id,
            vec![iskworks_core::AcquisitionProgressUpdate {
                type_id: 34,
                acquired_quantity: 3000,
            }],
        )
        .await
        .unwrap();

    let items = repository
        .list_order_acquisition_run_items(workspace_id, run.id)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    // The real, uncapped total is retained as-is.
    assert_eq!(items[0].acquired_quantity, 3000);

    // Ticket-level acquired_quantity stays capped at its own need.
    let ticket = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(ticket.acquired_quantity, Some(2985));
    // Workflow status is user-controlled and untouched by Run operations --
    // creating and starting the Run left it at its `Todo` default, and
    // recording progress does not change it either.
    assert_eq!(ticket.status, TicketStatus::Todo);

    let balance: Option<i64> = sqlx::query_scalar(
        "SELECT quantity FROM inventory_balances WHERE workspace_id = $1 AND owner_id = $2 AND type_id = 34",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .fetch_optional(&pool)
    .await
    .unwrap();
    // No row at all -- recording progress must never touch inventory.
    assert_eq!(balance, None);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn completing_a_run_posts_one_purchase_for_the_full_acquired_total_including_surplus(
    pool: PgPool,
) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let source = fixture_price_source(&pool, workspace_id, "Jita").await;
    let repository = PgOrderRepository::new(pool.clone());

    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
            None,
            Some(source),
        ))
        .await
        .unwrap();
    let run = repository
        .create_order_acquisition_run(workspace_id, owner_id, None, vec![ticket.id])
        .await
        .unwrap();
    repository
        .start_order_acquisition_run(workspace_id, owner_id, run.id)
        .await
        .unwrap();
    repository
        .record_order_acquisition_progress(
            workspace_id,
            run.id,
            vec![iskworks_core::AcquisitionProgressUpdate {
                type_id: 34,
                acquired_quantity: 150,
            }],
        )
        .await
        .unwrap();

    let completed = repository
        .complete_order_acquisition_run(workspace_id, owner_id, run.id)
        .await
        .unwrap();
    assert_eq!(
        completed.status,
        iskworks_core::AcquisitionRunStatus::Complete
    );

    let ticket = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    // Run completion posts accounting but never moves the member Ticket's
    // workflow lane.
    assert_eq!(ticket.status, TicketStatus::Todo);
    assert_eq!(ticket.acquired_quantity, Some(100));

    let balance: i64 = sqlx::query_scalar(
        "SELECT quantity FROM inventory_balances WHERE workspace_id = $1 AND owner_id = $2 AND type_id = 34",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    // The full 150 delivered lands in inventory, not just the 100 needed --
    // the surplus 50 becomes ordinary unreserved stock, never discarded.
    assert_eq!(balance, 150);

    let purchase_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM inventory_events \
         WHERE workspace_id = $1 AND owner_id = $2 AND type_id = 34 AND event_kind = 'purchase'",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(purchase_events, 1);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn short_delivery_records_partial_acquired_quantity_without_touching_ticket_status(
    pool: PgPool,
) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let source = fixture_price_source(&pool, workspace_id, "Jita").await;
    let repository = PgOrderRepository::new(pool.clone());

    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
            None,
            Some(source),
        ))
        .await
        .unwrap();
    let run = repository
        .create_order_acquisition_run(workspace_id, owner_id, None, vec![ticket.id])
        .await
        .unwrap();
    repository
        .start_order_acquisition_run(workspace_id, owner_id, run.id)
        .await
        .unwrap();
    repository
        .record_order_acquisition_progress(
            workspace_id,
            run.id,
            vec![iskworks_core::AcquisitionProgressUpdate {
                type_id: 34,
                acquired_quantity: 60,
            }],
        )
        .await
        .unwrap();

    repository
        .complete_order_acquisition_run(workspace_id, owner_id, run.id)
        .await
        .unwrap();

    let ticket = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    // Run completion is an accounting action -- it records `acquired_quantity`
    // and posts the Purchase, but never moves the member Ticket's workflow
    // lane. The card stays where the user left it (`Todo` here).
    assert_eq!(ticket.status, TicketStatus::Todo);
    assert_eq!(ticket.acquired_quantity, Some(60));
}

/// A Run's own completion is an accounting action: it posts the Purchase and records
/// `acquired_quantity`, but never writes any member or dependent Ticket's
/// workflow status.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn completing_a_batched_acquisition_run_does_not_change_any_ticket_status(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_sde_type(&pool, import_id, 37164, "Isogen").await;
    let source = fixture_price_source(&pool, workspace_id, "Jita").await;
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
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            37164,
            "Isogen",
            50,
            None,
            Some(source),
        ))
        .await
        .unwrap();
    repository
        .link_ticket_prerequisite_to_ticket(prerequisites[0].id, acquisition_ticket.id, 50)
        .await
        .unwrap();

    let run = repository
        .create_order_acquisition_run(workspace_id, owner_id, None, vec![acquisition_ticket.id])
        .await
        .unwrap();
    repository
        .start_order_acquisition_run(workspace_id, owner_id, run.id)
        .await
        .unwrap();
    repository
        .record_order_acquisition_progress(
            workspace_id,
            run.id,
            vec![iskworks_core::AcquisitionProgressUpdate {
                type_id: 37164,
                acquired_quantity: 50,
            }],
        )
        .await
        .unwrap();
    repository
        .complete_order_acquisition_run(workspace_id, owner_id, run.id)
        .await
        .unwrap();

    // The delivered acquisition ticket stays in its own lane...
    let acquisition_after = repository
        .get_ticket(workspace_id, acquisition_ticket.id)
        .await
        .unwrap();
    assert_eq!(acquisition_after.status, TicketStatus::Todo);
    assert_eq!(acquisition_after.acquired_quantity, Some(50));

    // ...and its dependent's workflow status is untouched (its derived
    // blocker list may show the prerequisite as still unmet, since the
    // fulfiller never reached `Complete` -- that's read-side only).
    let manufacturing_ticket = repository
        .get_ticket(workspace_id, manufacturing_ticket.id)
        .await
        .unwrap();
    assert_eq!(manufacturing_ticket.status, TicketStatus::Todo);

    // The accounting side did happen: one Purchase was posted for the run.
    let purchase_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM inventory_events \
         WHERE workspace_id = $1 AND owner_id = $2 AND type_id = 37164 AND event_kind = 'purchase'",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(purchase_events, 1);
}
