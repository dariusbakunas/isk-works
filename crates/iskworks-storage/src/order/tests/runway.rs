use super::*;
use iskworks_core::{InventoryRunwayRepository, MaterialCategory};

// --- Inventory › Runway facts: `PgOrderRepository::runway_facts` ---

const MORPHITE: i64 = 11399;

/// A live (v3) Epic with the given Buy requirements; returns the order id
/// and its requirement ids in input order.
async fn live_epic(
    pool: &PgPool,
    repository: &PgOrderRepository,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    build_id: BuildId,
    requirements: Vec<iskworks_core::order::NewOrderRequirement>,
) -> (OrderId, Vec<OrderRequirementId>) {
    let snapshot = empty_price_snapshot();
    let order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    let (order, requirements) = repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements,
        })
        .await
        .unwrap();
    sqlx::query("UPDATE orders SET planning_snapshot_version = 3 WHERE id = $1")
        .bind(order.id.0)
        .execute(pool)
        .await
        .unwrap();
    (order.id, requirements.iter().map(|r| r.id).collect())
}

async fn seed_allocation(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    requirement_id: OrderRequirementId,
    type_id: i64,
    quantity: i64,
    state: &str,
) {
    let (released, consumed) = match state {
        "active" => ("NULL", "NULL"),
        "consumed" => ("NULL", "now()"),
        "released" => ("now()", "NULL"),
        _ => unreachable!(),
    };
    sqlx::query(&format!(
        "INSERT INTO inventory_allocations \
         (id, workspace_id, owner_id, type_id, quantity, order_requirement_id, created_at, \
          released_at, consumed_at) \
         VALUES ($1, $2, $3, $4, $5, $6, now(), {released}, {consumed})"
    ))
    .bind(Uuid::new_v4())
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(type_id)
    .bind(quantity)
    .bind(requirement_id.0)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_mineral_type(pool: &PgPool, import_id: Uuid, type_id: i64, name: &str) {
    sqlx::query(
        "INSERT INTO sde_categories (import_id, category_id, name_en, published) \
         VALUES ($1, 4, 'Material', true) ON CONFLICT DO NOTHING",
    )
    .bind(import_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_groups (import_id, group_id, name_en, category_id, published) \
         VALUES ($1, 18, 'Mineral', 4, true) ON CONFLICT DO NOTHING",
    )
    .bind(import_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_types (import_id, type_id, name_en, group_id, published) \
         VALUES ($1, $2, $3, 18, true)",
    )
    .bind(import_id)
    .bind(type_id)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn runway_facts_sum_allocations_by_lifecycle_and_pool_stock(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_mineral_type(&pool, import_id, 34, "Tritanium").await;
    let repository = PgOrderRepository::new(pool.clone());
    let (order_id, requirement_ids) = live_epic(
        &pool,
        &repository,
        workspace_id,
        owner_id,
        build_id,
        vec![
            requirement(34, RequirementKind::Buy, 100),
            requirement(35, RequirementKind::React, 40),
        ],
    )
    .await;
    seed_allocation(
        &pool,
        workspace_id,
        owner_id,
        requirement_ids[0],
        34,
        30,
        "active",
    )
    .await;
    seed_allocation(
        &pool,
        workspace_id,
        owner_id,
        requirement_ids[0],
        34,
        20,
        "consumed",
    )
    .await;
    seed_allocation(
        &pool,
        workspace_id,
        owner_id,
        requirement_ids[0],
        34,
        10,
        "released",
    )
    .await;
    seed_inventory_balance(&pool, workspace_id, owner_id, 34, 50).await;

    let facts = repository.runway_facts(workspace_id).await.unwrap();

    assert_eq!(facts.epics.len(), 1);
    assert_eq!(facts.epics[0].order_id, order_id);
    // React requirements are internal production steps, not demand.
    assert_eq!(facts.requirements.len(), 1);
    let requirement = &facts.requirements[0];
    assert_eq!(
        (
            requirement.type_id,
            requirement.required,
            requirement.reserved,
            requirement.consumed
        ),
        (34, 100, 30, 20)
    );
    assert_eq!(facts.pools.len(), 1);
    assert_eq!(
        (facts.pools[0].on_hand, facts.pools[0].active_reserved),
        (50, 30)
    );
    assert_eq!(facts.types.len(), 1);
    assert_eq!(facts.types[0].name, "Tritanium");
    assert_eq!(facts.types[0].category, MaterialCategory::Minerals);
    assert!(facts.event_consumption.is_empty());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn runway_facts_skip_closed_and_pre_v3_epics_and_other_workspaces(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let (other_workspace_id, other_owner_id) = fixture_second_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let other_build_id = fixture_build(&pool, other_workspace_id, other_owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());
    let buy = || vec![requirement(MORPHITE, RequirementKind::Buy, 10)];

    let (live, _) = live_epic(&pool, &repository, workspace_id, owner_id, build_id, buy()).await;
    let (canceled, _) =
        live_epic(&pool, &repository, workspace_id, owner_id, build_id, buy()).await;
    let (archived, _) =
        live_epic(&pool, &repository, workspace_id, owner_id, build_id, buy()).await;
    let (frozen_v1, _) =
        live_epic(&pool, &repository, workspace_id, owner_id, build_id, buy()).await;
    live_epic(
        &pool,
        &repository,
        other_workspace_id,
        other_owner_id,
        other_build_id,
        buy(),
    )
    .await;
    for (sql, id) in [
        (
            "UPDATE orders SET canceled_at = now() WHERE id = $1",
            canceled,
        ),
        (
            "UPDATE orders SET archived_at = now() WHERE id = $1",
            archived,
        ),
        (
            "UPDATE orders SET planning_snapshot_version = 1 WHERE id = $1",
            frozen_v1,
        ),
    ] {
        sqlx::query(sql).bind(id.0).execute(&pool).await.unwrap();
    }

    let facts = repository.runway_facts(workspace_id).await.unwrap();

    let epics: Vec<OrderId> = facts.epics.iter().map(|e| e.order_id).collect();
    assert_eq!(epics, vec![live]);
    assert_eq!(facts.requirements.len(), 1);
    assert_eq!(facts.requirements[0].order_id, live);
    // Not in the SDE fixture: the captured name stands in, category "Other".
    assert_eq!(facts.types[0].name, "Tritanium");
    assert_eq!(facts.types[0].category, MaterialCategory::Other);
}
