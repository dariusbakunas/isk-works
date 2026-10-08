use super::*;

// --- Inventory reporting: `PgProductionRepository::reserved_quantity` ---
//
// The Inventory page's "Reserved"/"Available" columns (and the Build
// worksheet's coverage preview) read this query via
// `AppState::reserved_quantity`. Exercised here rather than duplicating
// this file's Order/Ticket fixture helpers elsewhere -- the point is to
// prove the query reflects exactly the allocation lifecycle these
// fixtures already produce, scoped correctly, with no other definition of
// "reserved" competing with it.

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reserved_quantity_with_no_allocations_is_zero(pool: PgPool) {
    let (workspace_id, owner_id, _import_id) = fixture_workspace(&pool).await;
    let production = crate::PgProductionRepository::new(pool.clone());

    let reserved = production
        .reserved_quantity(workspace_id, owner_id, 34)
        .await
        .unwrap();
    assert_eq!(reserved, 0);
}

/// Inserts one Order-owned, active `inventory_allocations` row. Order
/// creation itself never allocates, so the read-model tests below seed this
/// directly to exercise `reserved_quantity`'s summing/filtering -- live
/// code for any legacy row that migration `202609040001` didn't reach.
async fn seed_legacy_order_allocation(
    pool: &PgPool,
    repository: &PgOrderRepository,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    build_id: BuildId,
    type_id: i64,
    quantity: i64,
) -> Uuid {
    let snapshot = empty_price_snapshot();
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let (_, requirements) = repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements: vec![requirement(type_id, RequirementKind::Buy, quantity as u64)],
        })
        .await
        .unwrap();
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO inventory_allocations \
         (id, workspace_id, owner_id, type_id, quantity, order_requirement_id, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, now())",
    )
    .bind(id)
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(type_id)
    .bind(quantity)
    .bind(requirements[0].id.0)
    .execute(pool)
    .await
    .unwrap();
    id
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reserved_quantity_reflects_a_legacy_active_allocation(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 34, 500).await;
    let repository = PgOrderRepository::new(pool.clone());
    seed_legacy_order_allocation(
        &pool,
        &repository,
        workspace_id,
        owner_id,
        build_id,
        34,
        300,
    )
    .await;

    let production = crate::PgProductionRepository::new(pool.clone());
    assert_eq!(
        production
            .reserved_quantity(workspace_id, owner_id, 34)
            .await
            .unwrap(),
        300
    );
}

/// The Inventory list's batched read sums exactly what `reserved_quantity`
/// sums for each type, and omits a type with nothing reserved.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reserved_quantities_matches_reserved_quantity_per_type(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());
    for (type_id, quantity) in [(34, 300), (34, 200), (35, 40)] {
        seed_legacy_order_allocation(
            &pool,
            &repository,
            workspace_id,
            owner_id,
            build_id,
            type_id,
            quantity,
        )
        .await;
    }
    let released = seed_legacy_order_allocation(
        &pool,
        &repository,
        workspace_id,
        owner_id,
        build_id,
        35,
        1_000,
    )
    .await;
    sqlx::query("UPDATE inventory_allocations SET released_at = now() WHERE id = $1")
        .bind(released)
        .execute(&pool)
        .await
        .unwrap();

    let production = crate::PgProductionRepository::new(pool.clone());
    let batched = production
        .reserved_quantities(workspace_id, owner_id, &[34, 35, 36])
        .await
        .unwrap();
    assert_eq!(
        batched,
        std::collections::BTreeMap::from([(34, 500), (35, 40)])
    );
    for type_id in [34, 35, 36] {
        assert_eq!(
            batched.get(&type_id).copied().unwrap_or(0),
            production
                .reserved_quantity(workspace_id, owner_id, type_id)
                .await
                .unwrap()
        );
    }
}

// A Ticket reserves nothing, so creating one (with a prerequisite, plentiful stock) leaves
// `reserved_quantity` at zero.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn creating_a_manufacturing_ticket_does_not_raise_reserved_quantity(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 37164, 1400).await;
    let order_repository = PgOrderRepository::new(pool.clone());
    order_repository
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

    let production = crate::PgProductionRepository::new(pool.clone());
    let reserved = production
        .reserved_quantity(workspace_id, owner_id, 37164)
        .await
        .unwrap();
    assert_eq!(reserved, 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reserved_quantity_sums_multiple_legacy_active_allocations(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 34, 500).await;
    let repository = PgOrderRepository::new(pool.clone());
    seed_legacy_order_allocation(
        &pool,
        &repository,
        workspace_id,
        owner_id,
        build_id,
        34,
        300,
    )
    .await;
    seed_legacy_order_allocation(
        &pool,
        &repository,
        workspace_id,
        owner_id,
        build_id,
        34,
        200,
    )
    .await;

    let production = crate::PgProductionRepository::new(pool.clone());
    assert_eq!(
        production
            .reserved_quantity(workspace_id, owner_id, 34)
            .await
            .unwrap(),
        500
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reserved_quantity_excludes_a_released_legacy_allocation(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 34, 500).await;
    let repository = PgOrderRepository::new(pool.clone());
    let alloc_id = seed_legacy_order_allocation(
        &pool,
        &repository,
        workspace_id,
        owner_id,
        build_id,
        34,
        300,
    )
    .await;
    sqlx::query("UPDATE inventory_allocations SET released_at = now() WHERE id = $1")
        .bind(alloc_id)
        .execute(&pool)
        .await
        .unwrap();

    let production = crate::PgProductionRepository::new(pool.clone());
    assert_eq!(
        production
            .reserved_quantity(workspace_id, owner_id, 34)
            .await
            .unwrap(),
        0
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reserved_quantity_excludes_a_consumed_legacy_allocation(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 34, 500).await;
    let repository = PgOrderRepository::new(pool.clone());
    let alloc_id = seed_legacy_order_allocation(
        &pool,
        &repository,
        workspace_id,
        owner_id,
        build_id,
        34,
        500,
    )
    .await;
    sqlx::query("UPDATE inventory_allocations SET consumed_at = now() WHERE id = $1")
        .bind(alloc_id)
        .execute(&pool)
        .await
        .unwrap();

    let production = crate::PgProductionRepository::new(pool.clone());
    assert_eq!(
        production
            .reserved_quantity(workspace_id, owner_id, 34)
            .await
            .unwrap(),
        0
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reserved_quantity_excludes_legacy_allocations_for_another_owner(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let other_owner_id = fixture_second_owner(&pool, workspace_id).await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 34, 500).await;
    let repository = PgOrderRepository::new(pool.clone());
    seed_legacy_order_allocation(
        &pool,
        &repository,
        workspace_id,
        owner_id,
        build_id,
        34,
        300,
    )
    .await;

    let production = crate::PgProductionRepository::new(pool.clone());
    assert_eq!(
        production
            .reserved_quantity(workspace_id, other_owner_id, 34)
            .await
            .unwrap(),
        0
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reserved_quantity_excludes_legacy_allocations_for_another_type(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 34, 500).await;
    let repository = PgOrderRepository::new(pool.clone());
    seed_legacy_order_allocation(
        &pool,
        &repository,
        workspace_id,
        owner_id,
        build_id,
        34,
        300,
    )
    .await;

    let production = crate::PgProductionRepository::new(pool.clone());
    assert_eq!(
        production
            .reserved_quantity(workspace_id, owner_id, 999) // never allocated
            .await
            .unwrap(),
        0
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reserved_quantity_excludes_legacy_allocations_for_another_workspace(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 34, 500).await;
    let repository = PgOrderRepository::new(pool.clone());
    seed_legacy_order_allocation(
        &pool,
        &repository,
        workspace_id,
        owner_id,
        build_id,
        34,
        300,
    )
    .await;

    let (other_workspace_id, other_owner_id) = fixture_second_workspace(&pool).await;
    let production = crate::PgProductionRepository::new(pool.clone());
    assert_eq!(
        production
            .reserved_quantity(other_workspace_id, other_owner_id, 34)
            .await
            .unwrap(),
        0
    );
}
