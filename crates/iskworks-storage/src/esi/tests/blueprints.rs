use super::*;

fn blueprint_observation(type_id: i64) -> BlueprintAssetObservation {
    BlueprintAssetObservation {
        item_id: rand_character_id(),
        type_id,
        location_id: 60_003_760,
        location_flag: "Hangar".to_string(),
        material_efficiency: 0,
        time_efficiency: 0,
        runs: 0,
        quantity: -1,
        raw: Value::Null,
    }
}

/// A blueprint with a caller-controlled `item_id` (EVE's stable per-instance
/// id) so a test can re-sync "the same blueprint" and assert its row is
/// updated in place rather than duplicated.
fn blueprint_observation_item(item_id: i64, type_id: i64, me: i16) -> BlueprintAssetObservation {
    BlueprintAssetObservation {
        item_id,
        type_id,
        location_id: 60_003_760,
        location_flag: "Hangar".to_string(),
        material_efficiency: me,
        time_efficiency: 0,
        runs: 0,
        quantity: -1,
        raw: Value::Null,
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn syncing_one_connections_blueprints_does_not_hide_another_connections(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let esi_repository = PgEsiRepository::new(pool.clone());
    let industry_repository = PgIndustryRepository::new(pool.clone());

    let first = fixture_connection(&pool, workspace_id, owner_id, "First Character").await;
    let second = fixture_connection(&pool, workspace_id, owner_id, "Second Character").await;

    esi_repository
        .complete_blueprints(&first, &[blueprint_observation(17_323)], &[])
        .await
        .unwrap();
    // Syncing a second connected character under the same owner must not
    // touch the first character's blueprint rows (the delete-not-seen sweep
    // is scoped by connection_id).
    esi_repository
        .complete_blueprints(&second, &[blueprint_observation(40_672)], &[])
        .await
        .unwrap();

    let first_character_blueprint = industry_repository
        .list_blueprint_observations(workspace_id, owner_id, 17_323)
        .await
        .unwrap();
    assert_eq!(first_character_blueprint.len(), 1);

    let second_character_blueprint = industry_repository
        .list_blueprint_observations(workspace_id, owner_id, 40_672)
        .await
        .unwrap();
    assert_eq!(second_character_blueprint.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn re_syncing_a_blueprint_updates_its_row_in_place_keeping_the_same_id(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let esi = PgEsiRepository::new(pool.clone());
    let industry = PgIndustryRepository::new(pool.clone());
    let conn = fixture_connection(&pool, workspace_id, owner_id, "Researcher").await;

    esi.complete_blueprints(&conn, &[blueprint_observation_item(42, 17_323, 4)], &[])
        .await
        .unwrap();
    let first = industry
        .list_blueprint_observations(workspace_id, owner_id, 17_323)
        .await
        .unwrap();
    assert_eq!(first.len(), 1);
    let (id, imported_at) = (first[0].id, first[0].imported_at);
    assert_eq!(first[0].material_efficiency, 4);

    // Same blueprint, more research since.
    esi.complete_blueprints(&conn, &[blueprint_observation_item(42, 17_323, 8)], &[])
        .await
        .unwrap();
    let second = industry
        .list_blueprint_observations(workspace_id, owner_id, 17_323)
        .await
        .unwrap();
    assert_eq!(second.len(), 1, "re-sync must not duplicate the row");
    assert_eq!(second[0].id, id, "row id is stable across syncs");
    assert_eq!(second[0].material_efficiency, 8, "ME updated in place");
    assert_eq!(
        second[0].imported_at, imported_at,
        "imported_at is first-seen"
    );
    assert!(
        second[0].observed_at >= first[0].observed_at,
        "observed_at bumped"
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn a_blueprint_absent_from_a_later_sync_is_deleted(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let esi = PgEsiRepository::new(pool.clone());
    let industry = PgIndustryRepository::new(pool.clone());
    let conn = fixture_connection(&pool, workspace_id, owner_id, "Trader").await;

    esi.complete_blueprints(
        &conn,
        &[
            blueprint_observation_item(1, 17_323, 0),
            blueprint_observation_item(2, 40_672, 0),
        ],
        &[],
    )
    .await
    .unwrap();
    assert_eq!(
        industry
            .list_blueprint_observations(workspace_id, owner_id, 40_672)
            .await
            .unwrap()
            .len(),
        1
    );

    // Blueprint 2 has been sold -- the next full sync no longer reports it.
    esi.complete_blueprints(&conn, &[blueprint_observation_item(1, 17_323, 0)], &[])
        .await
        .unwrap();
    assert_eq!(
        industry
            .list_blueprint_observations(workspace_id, owner_id, 17_323)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        industry
            .list_blueprint_observations(workspace_id, owner_id, 40_672)
            .await
            .unwrap()
            .is_empty(),
        "the no-longer-reported blueprint is gone"
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn an_empty_sync_clears_only_that_connections_blueprints(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let esi = PgEsiRepository::new(pool.clone());
    let industry = PgIndustryRepository::new(pool.clone());
    let a = fixture_connection(&pool, workspace_id, owner_id, "Alpha").await;
    let b = fixture_connection(&pool, workspace_id, owner_id, "Bravo").await;

    esi.complete_blueprints(&a, &[blueprint_observation_item(10, 17_323, 0)], &[])
        .await
        .unwrap();
    esi.complete_blueprints(&b, &[blueprint_observation_item(20, 40_672, 0)], &[])
        .await
        .unwrap();

    esi.complete_blueprints(&a, &[], &[]).await.unwrap();

    assert!(
        industry
            .list_blueprint_observations(workspace_id, owner_id, 17_323)
            .await
            .unwrap()
            .is_empty(),
        "Alpha's blueprints are cleared"
    );
    assert_eq!(
        industry
            .list_blueprint_observations(workspace_id, owner_id, 40_672)
            .await
            .unwrap()
            .len(),
        1,
        "Bravo's blueprints are untouched"
    );
}
