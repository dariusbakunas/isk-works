use super::*;

/// `create_order` persists the caller's *frozen* inventory-reuse
/// snapshot as given -- including a non-zero `reused_quantity` -- and this
/// is not a reservation: no `inventory_allocations` row, no
/// `inventory_events` row, and the balance (quantity + cost basis +
/// revision) is byte-identical afterward. A non-zero frozen `reused_quantity`
/// is planning evidence, never a claim on physical stock. Do not "fix" this
/// by re-introducing an allocation write on Epic creation.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_order_persists_the_frozen_reuse_snapshot_without_touching_inventory(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let snapshot = empty_price_snapshot();
    seed_inventory_balance(&pool, workspace_id, owner_id, 34, 500).await;
    let balance_before = inventory_balance(&pool, workspace_id, owner_id, 34).await;

    let repository = PgOrderRepository::new(pool.clone());
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let order_id = order.id;
    let (persisted_order, requirements) = repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            // 800 needed, the Epic froze an intent to reuse 300 from stock.
            requirements: vec![requirement_reusing(34, RequirementKind::Buy, 800, 300)],
        })
        .await
        .unwrap();

    assert_eq!(persisted_order.id, order_id);
    assert_eq!(requirements.len(), 1);
    assert_eq!(requirements[0].required_quantity, 800);
    assert_eq!(requirements[0].fulfillment_scope, FulfillmentScope::Missing);
    assert_eq!(requirements[0].reused_quantity, 300);
    assert_eq!(requirements[0].fresh_quantity, 500);
    // Re-read from Postgres -- the frozen split round-trips.
    let reloaded = repository.list_order_requirements(order_id).await.unwrap();
    assert_eq!(reloaded[0].reused_quantity, 300);
    assert_eq!(reloaded[0].fresh_quantity, 500);
    assert_eq!(reloaded[0].fulfillment_scope, FulfillmentScope::Missing);

    // ...and NOT a reservation: no allocation row, no event, stock untouched.
    assert!(allocation_snapshot(&pool).await.is_empty());
    assert_eq!(inventory_event_count(&pool).await, 0);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        balance_before
    );
}

/// Regression: a Build priced purely from a market scope (no manual Price
/// Source) freezes a snapshot with `source_revision = 0` /
/// `price_source_id = None`. The original `captured_source_revision > 0`
/// check rejected that, so market-priced Builds could never create an
/// Epic (migration 202609060001 relaxed it to `>= 0`).
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_order_accepts_a_market_scope_snapshot_with_no_source_revision(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let snapshot = PriceSnapshot {
        id: PriceSnapshotId(Uuid::new_v4()),
        price_source_id: None,
        source_name: "Market".to_string(),
        source_revision: 0,
        created_at: crate::db_now(),
        items: Vec::new(),
    };
    let snapshot_id = snapshot.id;

    let repository = PgOrderRepository::new(pool.clone());
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let order_id = order.id;

    let (persisted_order, requirements) = repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: vec![requirement(34, RequirementKind::Buy, 800)],
        })
        .await
        .expect("market-scope snapshot with source_revision = 0 must persist");

    assert_eq!(persisted_order.id, order_id);
    assert_eq!(persisted_order.price_snapshot_id, snapshot_id);
    assert_eq!(requirements.len(), 1);

    let (captured_revision, captured_source_id): (i64, Option<Uuid>) = sqlx::query_as(
        "SELECT captured_source_revision, price_source_id FROM price_snapshots WHERE id = $1",
    )
    .bind(snapshot_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(captured_revision, 0);
    assert_eq!(captured_source_id, None);
}

/// `PriceSnapshotLine.market_region_id`/`market_location_id` persist
/// through to `price_snapshot_items`, per-line -- the material and output
/// roles can be priced from different scopes, so a single top-level
/// `PriceSnapshot` field cannot represent it.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_order_persists_each_snapshot_lines_own_market_scope(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let mut snapshot = empty_price_snapshot();
    snapshot.items = vec![
        iskworks_core::PriceSnapshotLine {
            type_id: 34,
            type_name: "Tritanium".to_string(),
            item_role: iskworks_core::PlannerItemRole::Material,
            selection_kind: iskworks_core::PricingSelectionKind::Default,
            manual_unit_price: None,
            price: Some(Money::parse("4.2500").unwrap()),
            pricing_policy: Some(iskworks_core::MarketPricingPolicy::HighestBuy),
            missing: false,
            source_note: String::new(),
            sort_order: 0,
            market_region_id: Some(10_000_002),
            market_location_id: Some(60_003_760),
        },
        iskworks_core::PriceSnapshotLine {
            type_id: 5_876,
            type_name: "Rifter".to_string(),
            item_role: iskworks_core::PlannerItemRole::Output,
            selection_kind: iskworks_core::PricingSelectionKind::Default,
            manual_unit_price: None,
            price: Some(Money::parse("100000").unwrap()),
            pricing_policy: Some(iskworks_core::MarketPricingPolicy::LowestSell),
            missing: false,
            source_note: String::new(),
            sort_order: 1,
            market_region_id: Some(10_000_030),
            market_location_id: Some(60_004_588),
        },
    ];

    let repository = PgOrderRepository::new(pool.clone());
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot.clone(),
            requirements: vec![requirement(34, RequirementKind::Buy, 800)],
        })
        .await
        .unwrap();

    let rows: Vec<(i64, Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT type_id, market_region_id, market_location_id FROM price_snapshot_items
         WHERE price_snapshot_id = $1 ORDER BY sort_order",
    )
    .bind(snapshot.id.0)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            (34, Some(10_000_002), Some(60_003_760)),
            (5_876, Some(10_000_030), Some(60_004_588)),
        ]
    );

    // `get_material_scope_for_snapshot` reads the *material* line's
    // scope (Jita, region 10_000_002) to freeze onto an Acquisition
    // ticket -- never the Output line's scope (region 10_000_030), since an
    // Acquisition ticket only ever concerns material acquisition.
    let material_scope = repository
        .get_material_scope_for_snapshot(snapshot.id)
        .await
        .unwrap();
    assert_eq!(
        material_scope,
        Some(MarketScope {
            region_id: 10_000_002,
            location_id: Some(60_003_760),
        })
    );
}

/// A snapshot with no resolved market scope at all (e.g. fully manual
/// pricing, or a historical snapshot from before market scopes) has nothing for
/// `get_material_scope_for_snapshot` to freeze -- `None`, not an error.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn get_material_scope_for_snapshot_is_none_when_no_material_line_has_a_scope(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let mut snapshot = empty_price_snapshot();
    snapshot.items = vec![iskworks_core::PriceSnapshotLine {
        type_id: 34,
        type_name: "Tritanium".to_string(),
        item_role: iskworks_core::PlannerItemRole::Material,
        selection_kind: iskworks_core::PricingSelectionKind::Manual,
        manual_unit_price: Some(Money::parse("4.2500").unwrap()),
        price: Some(Money::parse("4.2500").unwrap()),
        pricing_policy: None,
        missing: false,
        source_note: String::new(),
        sort_order: 0,
        market_region_id: None,
        market_location_id: None,
    }];

    let repository = PgOrderRepository::new(pool.clone());
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot.clone(),
            requirements: vec![requirement(34, RequirementKind::Buy, 800)],
        })
        .await
        .unwrap();

    let material_scope = repository
        .get_material_scope_for_snapshot(snapshot.id)
        .await
        .unwrap();
    assert_eq!(material_scope, None);
}

/// A `Full`-scoped requirement (inventory deliberately ignored) persists as
/// entirely fresh -- `reused_quantity == 0`, `fresh_quantity == required`.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_order_persists_a_full_scoped_requirement_as_entirely_fresh(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let snapshot = empty_price_snapshot();

    let repository = PgOrderRepository::new(pool.clone());
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let (_, requirements) = repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: vec![requirement(34, RequirementKind::Buy, 100)],
        })
        .await
        .unwrap();

    assert_eq!(requirements[0].fulfillment_scope, FulfillmentScope::Full);
    assert_eq!(requirements[0].reused_quantity, 0);
    assert_eq!(requirements[0].fresh_quantity, 100);
}

/// Double-planning is intentional: two Epics may each freeze
/// an intent to reuse the *same* physical stock, because a frozen
/// `reused_quantity` is planning evidence, not a reservation. Neither Epic
/// creation writes an allocation or event; availability stays at the full
/// physical amount. Surfacing the overlap is left to a separate
/// reconciliation/commitment feature.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn two_epics_may_freeze_the_same_stock_without_reserving_it(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 34, 840).await;
    let balance_before = inventory_balance(&pool, workspace_id, owner_id, 34).await;

    let repository = PgOrderRepository::new(pool.clone());

    // Two Epics, each needing 840 and each freezing a full-reuse intent.
    for _ in 0..2 {
        let snapshot = empty_price_snapshot();
        let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
        let (_, requirements) = repository
            .create_order(NewOrder {
                order,
                price_snapshot: snapshot,
                requirements: vec![requirement_reusing(34, RequirementKind::Buy, 840, 840)],
            })
            .await
            .unwrap();
        assert_eq!(requirements[0].reused_quantity, 840);
        assert_eq!(requirements[0].fresh_quantity, 0);
    }

    assert!(allocation_snapshot(&pool).await.is_empty());
    assert_eq!(inventory_event_count(&pool).await, 0);
    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        balance_before
    );
    assert_eq!(
        repository
            .available_quantity(workspace_id, owner_id, 34)
            .await
            .unwrap(),
        840
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn get_order_returns_order_not_found_for_an_unknown_id(pool: PgPool) {
    let (workspace_id, _owner_id, _import_id) = fixture_workspace(&pool).await;
    let repository = PgOrderRepository::new(pool);
    let result = repository.get_order(workspace_id, OrderId::new()).await;
    assert!(matches!(result, Err(OrderError::OrderNotFound)));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn acquisition_ticket_has_no_prerequisites_and_starts_todo(pool: PgPool) {
    let (workspace_id, owner_id, _import_id) = fixture_workspace(&pool).await;
    let repository = PgOrderRepository::new(pool.clone());

    let (ticket, prerequisites) = repository
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
    assert!(ticket.display_id.starts_with("ISK-"));
    assert!(prerequisites.is_empty());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn manufacturing_ticket_with_an_unmet_prerequisite_starts_todo_with_a_blocker(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());

    let (ticket, prerequisites) = repository
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
            quantity: Some(3625),
            source_build_id: Some(build_id),
            estimated_unit_cost: None,
            estimated_line_total: None,
            market_region_id: None,
            market_location_id: None,
            price_source_id: None,
            execution_snapshot: None,
            prerequisites: vec![ticket_prerequisite(37164, 1400)],
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
    assert_eq!(prerequisites.len(), 1);
    assert_eq!(prerequisites[0].fresh_quantity, 1400);
}
