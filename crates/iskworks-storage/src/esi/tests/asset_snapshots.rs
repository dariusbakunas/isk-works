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
            0,
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

/// One `esi_asset_observations` row, in column order.
type StoredAsset = (
    i64,
    i64,
    i64,
    i64,
    String,
    String,
    bool,
    Option<bool>,
    Value,
    String,
);

/// Writes more assets than one batched INSERT holds, so the snapshot spans
/// several statements. Every column must still land exactly as the per-row
/// insert wrote it.
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn a_snapshot_larger_than_one_insert_batch_keeps_every_asset(pool: PgPool) {
    let repository = PgEsiRepository::new(pool.clone());
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let connection = fixture_connection(&pool, workspace_id, owner_id, "Hoarder").await;
    let run = repository
        .start_sync(&connection, EsiSyncKind::Assets)
        .await
        .unwrap();
    let count = 2 * crate::esi::assets::ASSET_INSERT_BATCH_ROWS + 7;
    let assets: Vec<AssetObservation> = (1..=count as i64)
        .map(|item_id| AssetObservation {
            item_id,
            type_id: 34 + item_id % 5,
            quantity: item_id * 3,
            // Every tenth asset sits inside the previous one.
            location_id: if item_id % 10 == 0 {
                item_id - 1
            } else {
                60_003_760
            },
            location_type: if item_id % 10 == 0 { "item" } else { "station" }.to_string(),
            location_flag: format!("Flag{}", item_id % 3),
            is_singleton: item_id % 2 == 0,
            is_blueprint_copy: match item_id % 3 {
                0 => None,
                1 => Some(true),
                _ => Some(false),
            },
            raw: json!({ "item_id": item_id }),
        })
        .collect();

    repository
        .complete_assets(&run, 3, &assets, Some("etag-1"), 0)
        .await
        .unwrap();

    let stored: Vec<StoredAsset> = sqlx::query_as(
        "SELECT o.source_item_id, o.type_id, o.quantity, o.location_id, o.location_type,
                    o.location_flag, o.is_singleton, o.is_blueprint_copy, o.raw_payload,
                    o.source_checksum
             FROM esi_asset_observations o
             JOIN esi_asset_snapshots s ON s.id = o.snapshot_id
             WHERE s.connection_id = $1 AND s.active
             ORDER BY o.source_item_id",
    )
    .bind(connection.id.0)
    .fetch_all(&pool)
    .await
    .unwrap();
    let expected: Vec<_> = assets
        .iter()
        .map(|asset| {
            let payload = serde_json::to_value(asset).unwrap();
            let checksum = crate::esi::sync_runs::checksum(&payload);
            (
                asset.item_id,
                asset.type_id,
                asset.quantity,
                asset.location_id,
                asset.location_type.clone(),
                asset.location_flag.clone(),
                asset.is_singleton,
                asset.is_blueprint_copy,
                payload,
                checksum,
            )
        })
        .collect();
    assert_eq!(stored.len(), count);
    assert_eq!(stored, expected);
    assert_eq!(
        snapshot_rows(&pool, &connection).await,
        vec![("complete".to_string(), true, count as i64, count as i64)]
    );
    let row_count: i64 = sqlx::query_scalar(
        "SELECT row_count FROM esi_asset_snapshots WHERE connection_id = $1 AND active",
    )
    .bind(connection.id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row_count, count as i64);
}

fn stock(item_id: i64) -> AssetObservation {
    AssetObservation {
        item_id,
        type_id: 34,
        quantity: 100,
        location_id: 60_003_760,
        location_type: "station".to_string(),
        location_flag: "Hangar".to_string(),
        is_singleton: false,
        is_blueprint_copy: None,
        raw: json!({}),
    }
}

async fn snapshot_rows(
    pool: &PgPool,
    connection: &ConnectedCharacter,
) -> Vec<(String, bool, i64, i64)> {
    sqlx::query_as(
        "SELECT s.status, s.active,
                (SELECT count(*) FROM esi_asset_observations o WHERE o.snapshot_id = s.id),
                (SELECT count(*) FROM esi_asset_hierarchy h WHERE h.snapshot_id = s.id)
         FROM esi_asset_snapshots s WHERE s.connection_id = $1 ORDER BY s.observed_at",
    )
    .bind(connection.id.0)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// A superseded snapshot as the pre-retention code left it behind: inactive,
/// with its observations and hierarchy still in place.
async fn backlog_snapshot(
    repository: &PgEsiRepository,
    pool: &PgPool,
    connection: &ConnectedCharacter,
    observed_at: DateTime<Utc>,
    status: &str,
) {
    let run = repository
        .start_sync(connection, EsiSyncKind::Assets)
        .await
        .unwrap();
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO esi_asset_snapshots (id,connection_id,sync_run_id,observed_at,completed_at,status,page_count,row_count,active)
         VALUES ($1,$2,$3,$4,$4,$5,1,1,false)",
    )
    .bind(id)
    .bind(connection.id.0)
    .bind(run.id.0)
    .bind(observed_at)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
    if status == "complete" {
        sqlx::query(
            "INSERT INTO esi_asset_observations (snapshot_id,source_item_id,type_id,quantity,location_id,location_type,location_flag,is_singleton,raw_payload,source_checksum)
             VALUES ($1,1,34,1,60003760,'station','Hangar',false,'{}','fixture')",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("SELECT refresh_esi_asset_hierarchy($1)")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn completing_an_asset_snapshot_deletes_the_snapshots_it_supersedes(pool: PgPool) {
    let repository = PgEsiRepository::new(pool.clone());
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let connection = fixture_connection(&pool, workspace_id, owner_id, "Hauler").await;
    let other = fixture_connection(&pool, workspace_id, owner_id, "Miner").await;

    let run = repository
        .start_sync(&other, EsiSyncKind::Assets)
        .await
        .unwrap();
    repository
        .complete_assets(&run, 1, &[stock(9)], None, 0)
        .await
        .unwrap();
    let run = repository
        .start_sync(&connection, EsiSyncKind::Assets)
        .await
        .unwrap();
    repository
        .complete_assets(&run, 1, &[stock(1), stock(2)], None, 0)
        .await
        .unwrap();
    let run = repository
        .start_sync(&connection, EsiSyncKind::Assets)
        .await
        .unwrap();
    repository
        .mark_incomplete_asset_sync(&run, 2, &[stock(1)], "stopped")
        .await
        .unwrap();
    let run = repository
        .start_sync(&connection, EsiSyncKind::Assets)
        .await
        .unwrap();
    repository
        .complete_assets(&run, 1, &[stock(1), stock(2), stock(3)], None, 0)
        .await
        .unwrap();

    assert_eq!(
        snapshot_rows(&pool, &connection).await,
        vec![("complete".to_string(), true, 3, 3)]
    );
    assert_eq!(
        snapshot_rows(&pool, &other).await,
        vec![("complete".to_string(), true, 1, 1)]
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn asset_snapshot_sweep_clears_the_backlog_but_keeps_each_connections_newest(pool: PgPool) {
    let repository = PgEsiRepository::new(pool.clone());
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let hauler = fixture_connection(&pool, workspace_id, owner_id, "Hauler").await;
    let miner = fixture_connection(&pool, workspace_id, owner_id, "Miner").await;
    let now = crate::db_now();
    let hours = chrono::Duration::hours;

    for age in [9, 8, 7] {
        backlog_snapshot(&repository, &pool, &hauler, now - hours(age), "complete").await;
    }
    // Completing a sync drops the connection's earlier failed attempts.
    backlog_snapshot(&repository, &pool, &hauler, now - hours(6), "incomplete").await;
    let run = repository
        .start_sync(&hauler, EsiSyncKind::Assets)
        .await
        .unwrap();
    repository
        .complete_assets(&run, 1, &[stock(1), stock(2)], None, 0)
        .await
        .unwrap();
    // A failed sync newer than the active snapshot is what the sync status
    // reports, so it stays.
    backlog_snapshot(&repository, &pool, &hauler, now + hours(1), "incomplete").await;
    // A connection that never completed keeps its newest attempt only.
    backlog_snapshot(&repository, &pool, &miner, now - hours(3), "incomplete").await;
    backlog_snapshot(&repository, &pool, &miner, now - hours(2), "incomplete").await;

    let first = repository
        .prune_superseded_asset_snapshots(2, 1)
        .await
        .unwrap();
    assert_eq!(
        (first.rows_deleted, first.chunks_run, first.drained),
        (2, 1, false)
    );
    let rest = repository
        .prune_superseded_asset_snapshots(2, 10)
        .await
        .unwrap();
    assert_eq!((rest.rows_deleted, rest.drained), (2, true));

    assert_eq!(
        snapshot_rows(&pool, &hauler).await,
        vec![
            ("complete".to_string(), true, 2, 2),
            ("incomplete".to_string(), false, 0, 0),
        ]
    );
    assert_eq!(
        snapshot_rows(&pool, &miner).await,
        vec![("incomplete".to_string(), false, 0, 0)]
    );
    let again = repository
        .prune_superseded_asset_snapshots(2, 10)
        .await
        .unwrap();
    assert_eq!((again.rows_deleted, again.drained), (0, true));
}
