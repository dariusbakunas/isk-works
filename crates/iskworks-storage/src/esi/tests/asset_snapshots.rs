use super::*;

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn completed_asset_snapshot_resolves_its_container_hierarchy(pool: PgPool) {
    let repository = PgEsiRepository::new(pool.clone());
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let connection = fixture_connection(&pool, workspace_id, owner_id, "Hauler").await;
    let run = repository
        .start_sync(&connection, EsiSyncKind::Assets)
        .await
        .unwrap();
    let asset = |item_id, location_id, location_type: &str| AssetObservation {
        item_id,
        type_id: 34,
        quantity: 1,
        location_id,
        location_type: location_type.to_string(),
        location_flag: "Hangar".to_string(),
        is_singleton: false,
        is_blueprint_copy: None,
        raw: json!({}),
    };
    // Station <- ship 1 <- container 2 <- item 3; item 4 sits in a missing parent.
    repository
        .complete_assets(
            &run,
            1,
            &[
                asset(1, 60_003_760, "station"),
                asset(2, 1, "item"),
                asset(3, 2, "item"),
                asset(4, 99, "item"),
            ],
            None,
        )
        .await
        .unwrap();
    let rows: Vec<(i64, i64, String, Option<i64>, i32, String)> = sqlx::query_as(
        "SELECT source_item_id, effective_location_id, effective_location_type,
                parent_item_id, hierarchy_depth, hierarchy_state
         FROM asset_browser_current WHERE workspace_id=$1 ORDER BY source_item_id",
    )
    .bind(workspace_id.0)
    .fetch_all(&pool)
    .await
    .unwrap();
    let station = || "station".to_string();
    assert_eq!(
        rows,
        vec![
            (1, 60_003_760, station(), None, 0, "resolved".to_string()),
            (2, 60_003_760, station(), Some(1), 1, "resolved".to_string()),
            (3, 60_003_760, station(), Some(2), 2, "resolved".to_string()),
            (
                4,
                99,
                "item".to_string(),
                None,
                0,
                "missing_parent".to_string()
            ),
        ]
    );
}
