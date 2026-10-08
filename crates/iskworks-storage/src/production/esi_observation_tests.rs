use super::*;

/// Fixtures insert snapshots directly, so resolve their hierarchy the way
/// `complete_assets` does before they are read.
async fn resolve_asset_hierarchies(tx: &mut sqlx::Transaction<'_, Postgres>) {
    sqlx::query(
        "SELECT refresh_esi_asset_hierarchy(id) FROM esi_asset_snapshots WHERE status='complete'",
    )
    .execute(&mut **tx)
    .await
    .expect("asset hierarchy resolved");
}

/// Exercises the three scope decisions in `list_esi_observations`
/// together against real Postgres, since none of them are visible to
/// a fake-repository route test: a corp hangar seen by two connected
/// characters (source_item_id 9001) must be deduped rather than
/// summed; a singleton ship and a blueprint copy must be excluded
/// entirely; and a `disconnected` character's still-active snapshot
/// must still count (staleness is surfaced via `observed_at`, not
/// filtered out).
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn dedupes_across_connections_and_excludes_singleton_and_blueprint_copy_rows(pool: PgPool) {
    let workspace_id = WorkspaceId(Uuid::new_v4());
    let owner_id = Uuid::new_v4();
    let connection_a = Uuid::new_v4();
    let connection_b = Uuid::new_v4();
    let sync_run_a = Uuid::new_v4();
    let sync_run_b = Uuid::new_v4();
    let snapshot_a = Uuid::new_v4();
    let snapshot_b = Uuid::new_v4();
    let observed_a = crate::db_now() - chrono::Duration::hours(2);
    let observed_b = crate::db_now();
    let mut tx = pool.begin().await.expect("transaction begins");

    sqlx::query("INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'ESI observation test',$2,$3,$3)")
        .bind(workspace_id.0).bind(owner_id).bind(observed_b).execute(&mut *tx).await.expect("workspace");
    sqlx::query("INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','ESI Owner',true,$3,$3)")
        .bind(owner_id).bind(workspace_id.0).bind(observed_b).execute(&mut *tx).await.expect("owner");
    sqlx::query("INSERT INTO eve_connections (id,workspace_id,owner_id,eve_character_id,character_name,status,granted_scopes,connected_at,updated_at) VALUES ($1,$2,$3,9001,'Connected Alt','connected','{}',$4,$4)")
        .bind(connection_a).bind(workspace_id.0).bind(owner_id).bind(observed_b).execute(&mut *tx).await.expect("connection a");
    sqlx::query("INSERT INTO eve_connections (id,workspace_id,owner_id,eve_character_id,character_name,status,granted_scopes,connected_at,updated_at,disconnected_at) VALUES ($1,$2,$3,9002,'Disconnected Alt','disconnected','{}',$4,$4,$4)")
        .bind(connection_b).bind(workspace_id.0).bind(owner_id).bind(observed_b).execute(&mut *tx).await.expect("connection b");
    sqlx::query("INSERT INTO esi_sync_runs (id,workspace_id,owner_id,connection_id,requested_kind,status,phase,started_at,completed_at,summary) VALUES ($1,$2,$3,$4,'assets','succeeded','complete',$5,$5,'test')")
        .bind(sync_run_a).bind(workspace_id.0).bind(owner_id).bind(connection_a).bind(observed_a).execute(&mut *tx).await.expect("sync run a");
    sqlx::query("INSERT INTO esi_sync_runs (id,workspace_id,owner_id,connection_id,requested_kind,status,phase,started_at,completed_at,summary) VALUES ($1,$2,$3,$4,'assets','succeeded','complete',$5,$5,'test')")
        .bind(sync_run_b).bind(workspace_id.0).bind(owner_id).bind(connection_b).bind(observed_b).execute(&mut *tx).await.expect("sync run b");
    sqlx::query("INSERT INTO esi_asset_snapshots (id,connection_id,sync_run_id,observed_at,completed_at,status,page_count,row_count,active) VALUES ($1,$2,$3,$4,$4,'complete',1,3,true)")
        .bind(snapshot_a).bind(connection_a).bind(sync_run_a).bind(observed_a).execute(&mut *tx).await.expect("snapshot a");
    sqlx::query("INSERT INTO esi_asset_snapshots (id,connection_id,sync_run_id,observed_at,completed_at,status,page_count,row_count,active) VALUES ($1,$2,$3,$4,$4,'complete',1,2,true)")
        .bind(snapshot_b).bind(connection_b).bind(sync_run_b).bind(observed_b).execute(&mut *tx).await.expect("snapshot b");
    // Connection A: a shared corp hangar item (9001, Tritanium),
    // a fitted/singleton ship (must be excluded), and a blueprint
    // copy (must be excluded -- is_blueprint_copy IS NOT NULL here).
    sqlx::query(
        "INSERT INTO esi_asset_observations
               (snapshot_id,source_item_id,type_id,quantity,location_id,location_type,location_flag,is_singleton,is_blueprint_copy,raw_payload,source_checksum)
             VALUES
               ($1,9001,34,27075,60003760,'station','Hangar',false,NULL,'{}','tritanium'),
               ($1,9010,587,1,60003760,'station','Hangar',true,NULL,'{}','fitted-rifter'),
               ($1,9011,781,1,60003760,'station','Hangar',false,true,'{}','bpc')",
    )
    .bind(snapshot_a)
    .execute(&mut *tx)
    .await
    .expect("connection a observations");
    // Connection B (disconnected): the *same* physical Tritanium
    // stack (9001, seen via a shared corp hangar -- must dedupe
    // against connection A's row, not sum to 54150) plus a genuinely
    // different item (9002, Pyerite).
    sqlx::query(
        "INSERT INTO esi_asset_observations
               (snapshot_id,source_item_id,type_id,quantity,location_id,location_type,location_flag,is_singleton,is_blueprint_copy,raw_payload,source_checksum)
             VALUES
               ($1,9001,34,27075,60003760,'station','Hangar',false,NULL,'{}','tritanium'),
               ($1,9002,35,500,60003760,'station','Hangar',false,NULL,'{}','pyerite')",
    )
    .bind(snapshot_b)
    .execute(&mut *tx)
    .await
    .expect("connection b observations");
    resolve_asset_hierarchies(&mut tx).await;
    tx.commit().await.expect("fixtures committed");

    let observations = PgProductionRepository::new(pool)
        .list_esi_observations(workspace_id, OwnerId(owner_id))
        .await
        .expect("observations load");

    assert_eq!(
        observations.len(),
        2,
        "singleton and BPC rows must be excluded"
    );
    let tritanium = observations.get(&34).expect("tritanium observed");
    assert_eq!(
        tritanium.quantity, 27_075,
        "the shared hangar item must be deduped, not summed to 54150"
    );
    assert_eq!(
        tritanium.observed_at, observed_b,
        "dedup keeps the freshest contributing observation"
    );
    let pyerite = observations.get(&35).expect("pyerite observed");
    assert_eq!(pyerite.quantity, 500);
    assert!(
        !observations.contains_key(&587),
        "a fitted/singleton ship must not appear"
    );
    assert!(
        !observations.contains_key(&781),
        "a blueprint copy must not appear"
    );
}

/// The discrepancy drill-down's core invariant, against real Postgres:
/// contributor quantities aggregated by (character, location) sum to
/// exactly what `list_esi_observations` reports for the same type --
/// using the exact numbers from the feature's own worked example (a
/// 125,000-unit Tritanium observation split Corvin/Jita 80k, Corvin/an
/// unresolved citadel 30k, and a second character's 15k that shares a
/// corp hangar item with Corvin's Jita stack and must dedupe against
/// it rather than double-count).
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn esi_holdings_aggregates_by_character_and_location_and_matches_the_summary_total(
    pool: PgPool,
) {
    let workspace_id = WorkspaceId(Uuid::new_v4());
    let owner_id = Uuid::new_v4();
    let connection_corvin = Uuid::new_v4();
    let connection_hauler = Uuid::new_v4();
    let sync_run_corvin = Uuid::new_v4();
    let sync_run_hauler = Uuid::new_v4();
    let snapshot_corvin = Uuid::new_v4();
    let snapshot_hauler = Uuid::new_v4();
    // Corvin's snapshot is the freshest, so his copy of the shared
    // Jita stack (source_item_id 9001) wins the dedup's `DISTINCT ON
    // ... ORDER BY observed_at DESC` -- Hauler Alt's older observation
    // of the exact same item must not also be counted.
    let observed_corvin = crate::db_now();
    let observed_hauler = crate::db_now() - chrono::Duration::hours(2);
    let jita = 60_003_760_i64;
    let unresolved_citadel = 1_040_000_000_001_i64;
    let resolved_citadel = 1_040_000_000_002_i64;
    let mut tx = pool.begin().await.expect("transaction begins");

    sqlx::query("INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'ESI holdings test',$2,$3,$3)")
        .bind(workspace_id.0).bind(owner_id).bind(observed_hauler).execute(&mut *tx).await.expect("workspace");
    sqlx::query("INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Holdings Owner',true,$3,$3)")
        .bind(owner_id).bind(workspace_id.0).bind(observed_hauler).execute(&mut *tx).await.expect("owner");
    sqlx::query("INSERT INTO eve_connections (id,workspace_id,owner_id,eve_character_id,character_name,status,granted_scopes,connected_at,updated_at) VALUES ($1,$2,$3,9101,'Corvin','connected','{}',$4,$4)")
        .bind(connection_corvin).bind(workspace_id.0).bind(owner_id).bind(observed_hauler).execute(&mut *tx).await.expect("connection corvin");
    sqlx::query("INSERT INTO eve_connections (id,workspace_id,owner_id,eve_character_id,character_name,status,granted_scopes,connected_at,updated_at) VALUES ($1,$2,$3,9102,'Hauler Alt','connected','{}',$4,$4)")
        .bind(connection_hauler).bind(workspace_id.0).bind(owner_id).bind(observed_hauler).execute(&mut *tx).await.expect("connection hauler");
    sqlx::query("INSERT INTO esi_sync_runs (id,workspace_id,owner_id,connection_id,requested_kind,status,phase,started_at,completed_at,summary) VALUES ($1,$2,$3,$4,'assets','succeeded','complete',$5,$5,'test')")
        .bind(sync_run_corvin).bind(workspace_id.0).bind(owner_id).bind(connection_corvin).bind(observed_corvin).execute(&mut *tx).await.expect("sync run corvin");
    sqlx::query("INSERT INTO esi_sync_runs (id,workspace_id,owner_id,connection_id,requested_kind,status,phase,started_at,completed_at,summary) VALUES ($1,$2,$3,$4,'assets','succeeded','complete',$5,$5,'test')")
        .bind(sync_run_hauler).bind(workspace_id.0).bind(owner_id).bind(connection_hauler).bind(observed_hauler).execute(&mut *tx).await.expect("sync run hauler");
    sqlx::query("INSERT INTO esi_asset_snapshots (id,connection_id,sync_run_id,observed_at,completed_at,status,page_count,row_count,active) VALUES ($1,$2,$3,$4,$4,'complete',1,3,true)")
        .bind(snapshot_corvin).bind(connection_corvin).bind(sync_run_corvin).bind(observed_corvin).execute(&mut *tx).await.expect("snapshot corvin");
    sqlx::query("INSERT INTO esi_asset_snapshots (id,connection_id,sync_run_id,observed_at,completed_at,status,page_count,row_count,active) VALUES ($1,$2,$3,$4,$4,'complete',1,2,true)")
        .bind(snapshot_hauler).bind(connection_hauler).bind(sync_run_hauler).bind(observed_hauler).execute(&mut *tx).await.expect("snapshot hauler");
    // Jita is an NPC station, resolved via the SDE (`sde_npc_stations`) --
    // exactly like the Assets browser resolves it, and NOT via
    // `market_location_names`, which only ever holds player-owned
    // structures resolved through an ESI structure lookup. The citadel
    // Corvin's second stack sits in is resolved through neither and must
    // degrade to a `None` location_name, not an error.
    let sde_import_id = Uuid::new_v4();
    sqlx::query("INSERT INTO sde_imports (id,source_version,source_label,source_checksum,status,active,started_at,completed_at) VALUES ($1,'test','test','esi-holdings-fixture','active',true,$2,$2)")
        .bind(sde_import_id).bind(observed_hauler).execute(&mut *tx).await.expect("sde import");
    sqlx::query("INSERT INTO sde_solar_systems (import_id,solar_system_id,name_en) VALUES ($1,30000142,'Jita')")
        .bind(sde_import_id).execute(&mut *tx).await.expect("jita solar system");
    sqlx::query("INSERT INTO sde_npc_stations (import_id,station_id,name_en,solar_system_id,owner_corporation_id,station_type_id) VALUES ($1,$2,'Jita IV - Moon 4 - Caldari Navy Assembly Plant',30000142,1000125,1531)")
        .bind(sde_import_id).bind(jita).execute(&mut *tx).await.expect("jita station");
    sqlx::query("INSERT INTO market_location_names (workspace_id,location_id,location_name,owner_id,solar_system_id,resolved_by_connection_id,resolved_at,updated_at) VALUES ($1,$2,'Some Citadel',1000125,30000144,$3,$4,$4)")
        .bind(workspace_id.0).bind(resolved_citadel).bind(connection_hauler).bind(observed_hauler).execute(&mut *tx).await.expect("resolved citadel name");
    // Corvin: 80,000 at Jita (source_item_id 9001, shared corp hangar
    // -- Hauler Alt also observes this exact item and must dedupe
    // against it), 30,000 at an unresolved citadel; plus a singleton
    // ship and a blueprint copy that must both be excluded entirely.
    sqlx::query(
        "INSERT INTO esi_asset_observations
               (snapshot_id,source_item_id,type_id,quantity,location_id,location_type,location_flag,is_singleton,is_blueprint_copy,raw_payload,source_checksum)
             VALUES
               ($1,9001,34,80000,$2,'station','Hangar',false,NULL,'{}','tritanium-jita'),
               ($1,9002,34,30000,$3,'structure','Hangar',false,NULL,'{}','tritanium-citadel'),
               ($1,9010,587,1,$2,'station','Hangar',true,NULL,'{}','fitted-rifter'),
               ($1,9011,781,1,$2,'station','Hangar',false,true,'{}','bpc')",
    )
    .bind(snapshot_corvin)
    .bind(jita)
    .bind(unresolved_citadel)
    .execute(&mut *tx)
    .await
    .expect("corvin observations");
    // Hauler Alt: the same Jita stack (9001, dedupes against Corvin's
    // row -- must not add another 80,000) plus a genuinely separate
    // 15,000 at a resolved citadel.
    sqlx::query(
        "INSERT INTO esi_asset_observations
               (snapshot_id,source_item_id,type_id,quantity,location_id,location_type,location_flag,is_singleton,is_blueprint_copy,raw_payload,source_checksum)
             VALUES
               ($1,9001,34,80000,$2,'station','Hangar',false,NULL,'{}','tritanium-jita'),
               ($1,9003,34,15000,$3,'structure','Hangar',false,NULL,'{}','tritanium-hauler')",
    )
    .bind(snapshot_hauler)
    .bind(jita)
    .bind(resolved_citadel)
    .execute(&mut *tx)
    .await
    .expect("hauler observations");
    resolve_asset_hierarchies(&mut tx).await;
    tx.commit().await.expect("fixtures committed");

    let repository = PgProductionRepository::new(pool);
    let updated = repository
        .set_esi_holding_reconciliation_inclusion(
            workspace_id,
            OwnerId(owner_id),
            SetEsiHoldingReconciliationInclusion {
                type_id: 34,
                eve_character_id: 9101,
                effective_location_id: unresolved_citadel,
                included: false,
            },
        )
        .await
        .expect("exclude Corvin citadel Tritanium through repository mutation");
    assert_eq!(updated.ignored_quantity, 30_000);
    assert_eq!(updated.included_quantity, 95_000);
    let holdings = repository
        .esi_holdings(workspace_id, OwnerId(owner_id), 34)
        .await
        .expect("holdings load");
    let summary = repository
        .list_esi_observations(workspace_id, OwnerId(owner_id))
        .await
        .expect("summary load");

    assert_eq!(holdings.observed_quantity, 125_000);
    assert_eq!(holdings.ignored_quantity, 30_000);
    assert_eq!(holdings.included_quantity, 95_000);
    assert_eq!(
        holdings
            .contributors
            .iter()
            .map(|c| c.quantity)
            .sum::<u64>(),
        holdings.observed_quantity,
        "contributor quantities must sum to exactly the displayed observed total"
    );
    let tritanium_summary = summary.get(&34).expect("tritanium in summary");
    assert_eq!(tritanium_summary.ignored_quantity, 30_000);
    assert_eq!(tritanium_summary.included_quantity, 95_000);
    assert_eq!(
        holdings.observed_quantity, tritanium_summary.quantity,
        "detail total must equal the summary's figure for the same type"
    );
    assert_eq!(
        holdings.observed_at,
        Some(tritanium_summary.observed_at),
        "detail freshness must equal the summary's figure for the same type"
    );

    assert_eq!(
        holdings.contributors.len(),
        3,
        "singleton and BPC rows must not produce contributor rows"
    );
    let jita_row = holdings
        .contributors
        .iter()
        .find(|c| c.character_name == "Corvin" && c.location_id == jita)
        .expect("corvin at jita");
    assert_eq!(
        jita_row.quantity, 80_000,
        "the shared hangar item must be deduped, not summed to 160,000"
    );
    assert_eq!(jita_row.eve_character_id, 9101);
    assert_eq!(
        jita_row.location_name.as_deref(),
        Some("Jita IV - Moon 4 - Caldari Navy Assembly Plant")
    );

    let unresolved_row = holdings
        .contributors
        .iter()
        .find(|c| c.character_name == "Corvin" && c.location_id == unresolved_citadel)
        .expect("corvin at the unresolved citadel");
    assert_eq!(unresolved_row.quantity, 30_000);
    assert_eq!(
        unresolved_row.location_name, None,
        "an unresolved location must degrade to None, not an error or a guessed name"
    );

    let hauler_row = holdings
        .contributors
        .iter()
        .find(|c| c.character_name == "Hauler Alt")
        .expect("hauler alt contributes separately");
    assert_eq!(hauler_row.quantity, 15_000);
    assert_eq!(hauler_row.location_name.as_deref(), Some("Some Citadel"));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn esi_holdings_is_scoped_to_the_requested_workspace(pool: PgPool) {
    let workspace_a = WorkspaceId(Uuid::new_v4());
    let workspace_b = WorkspaceId(Uuid::new_v4());
    let owner_a = Uuid::new_v4();
    let owner_b = Uuid::new_v4();
    let connection_a = Uuid::new_v4();
    let connection_b = Uuid::new_v4();
    let now = crate::db_now();
    let mut tx = pool.begin().await.expect("transaction begins");

    for (workspace_id, owner_id, connection_id, character_id, character_name) in [
        (
            workspace_a,
            owner_a,
            connection_a,
            9201_i64,
            "Workspace A Character",
        ),
        (
            workspace_b,
            owner_b,
            connection_b,
            9202_i64,
            "Workspace B Character",
        ),
    ] {
        let sync_run = Uuid::new_v4();
        let snapshot = Uuid::new_v4();
        sqlx::query("INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Workspace isolation test',$2,$3,$3)")
            .bind(workspace_id.0).bind(owner_id).bind(now).execute(&mut *tx).await.expect("workspace");
        sqlx::query("INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Owner',true,$3,$3)")
            .bind(owner_id).bind(workspace_id.0).bind(now).execute(&mut *tx).await.expect("owner");
        sqlx::query("INSERT INTO eve_connections (id,workspace_id,owner_id,eve_character_id,character_name,status,granted_scopes,connected_at,updated_at) VALUES ($1,$2,$3,$4,$5,'connected','{}',$6,$6)")
            .bind(connection_id).bind(workspace_id.0).bind(owner_id).bind(character_id).bind(character_name).bind(now).execute(&mut *tx).await.expect("connection");
        sqlx::query("INSERT INTO esi_sync_runs (id,workspace_id,owner_id,connection_id,requested_kind,status,phase,started_at,completed_at,summary) VALUES ($1,$2,$3,$4,'assets','succeeded','complete',$5,$5,'test')")
            .bind(sync_run).bind(workspace_id.0).bind(owner_id).bind(connection_id).bind(now).execute(&mut *tx).await.expect("sync run");
        sqlx::query("INSERT INTO esi_asset_snapshots (id,connection_id,sync_run_id,observed_at,completed_at,status,page_count,row_count,active) VALUES ($1,$2,$3,$4,$4,'complete',1,1,true)")
            .bind(snapshot).bind(connection_id).bind(sync_run).bind(now).execute(&mut *tx).await.expect("snapshot");
        sqlx::query(
            "INSERT INTO esi_asset_observations
                   (snapshot_id,source_item_id,type_id,quantity,location_id,location_type,location_flag,is_singleton,raw_payload,source_checksum)
                 VALUES ($1,$2,34,1000,60003760,'station','Hangar',false,'{}','tritanium')",
        )
        .bind(snapshot)
        .bind(if workspace_id == workspace_a { 8001_i64 } else { 8002_i64 })
        .execute(&mut *tx)
        .await
        .expect("observation");
    }
    resolve_asset_hierarchies(&mut tx).await;
    tx.commit().await.expect("fixtures committed");

    let holdings = PgProductionRepository::new(pool)
        .esi_holdings(workspace_a, OwnerId(owner_a), 34)
        .await
        .expect("holdings load");

    assert_eq!(holdings.observed_quantity, 1_000);
    assert_eq!(holdings.contributors.len(), 1);
    assert_eq!(
        holdings.contributors[0].character_name,
        "Workspace A Character"
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn esi_holdings_is_scoped_to_the_requested_owner_within_one_workspace(pool: PgPool) {
    let workspace_id = WorkspaceId(Uuid::new_v4());
    let owner_a = Uuid::new_v4();
    let owner_b = Uuid::new_v4();
    let connection_a = Uuid::new_v4();
    let connection_b = Uuid::new_v4();
    let now = crate::db_now();
    let mut tx = pool.begin().await.expect("transaction begins");

    sqlx::query("INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Owner isolation test',$2,$3,$3)")
        .bind(workspace_id.0).bind(owner_a).bind(now).execute(&mut *tx).await.expect("workspace");
    for (owner_id, connection_id, character_id, character_name, source_item_id, hidden) in [
        (
            owner_a,
            connection_a,
            9301_i64,
            "Owner A Character",
            8101_i64,
            true,
        ),
        (
            owner_b,
            connection_b,
            9302_i64,
            "Owner B Character",
            8102_i64,
            false,
        ),
    ] {
        let sync_run = Uuid::new_v4();
        let snapshot = Uuid::new_v4();
        sqlx::query("INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Owner',$3,$4,$4)")
            .bind(owner_id).bind(workspace_id.0).bind(hidden).bind(now).execute(&mut *tx).await.expect("owner");
        sqlx::query("INSERT INTO eve_connections (id,workspace_id,owner_id,eve_character_id,character_name,status,granted_scopes,connected_at,updated_at) VALUES ($1,$2,$3,$4,$5,'connected','{}',$6,$6)")
            .bind(connection_id).bind(workspace_id.0).bind(owner_id).bind(character_id).bind(character_name).bind(now).execute(&mut *tx).await.expect("connection");
        sqlx::query("INSERT INTO esi_sync_runs (id,workspace_id,owner_id,connection_id,requested_kind,status,phase,started_at,completed_at,summary) VALUES ($1,$2,$3,$4,'assets','succeeded','complete',$5,$5,'test')")
            .bind(sync_run).bind(workspace_id.0).bind(owner_id).bind(connection_id).bind(now).execute(&mut *tx).await.expect("sync run");
        sqlx::query("INSERT INTO esi_asset_snapshots (id,connection_id,sync_run_id,observed_at,completed_at,status,page_count,row_count,active) VALUES ($1,$2,$3,$4,$4,'complete',1,1,true)")
            .bind(snapshot).bind(connection_id).bind(sync_run).bind(now).execute(&mut *tx).await.expect("snapshot");
        sqlx::query(
            "INSERT INTO esi_asset_observations
                   (snapshot_id,source_item_id,type_id,quantity,location_id,location_type,location_flag,is_singleton,raw_payload,source_checksum)
                 VALUES ($1,$2,34,500,60003760,'station','Hangar',false,'{}','tritanium')",
        )
        .bind(snapshot)
        .bind(source_item_id)
        .execute(&mut *tx)
        .await
        .expect("observation");
    }
    resolve_asset_hierarchies(&mut tx).await;
    tx.commit().await.expect("fixtures committed");

    let holdings = PgProductionRepository::new(pool)
        .esi_holdings(workspace_id, OwnerId(owner_a), 34)
        .await
        .expect("holdings load");

    assert_eq!(holdings.observed_quantity, 500);
    assert_eq!(holdings.contributors.len(), 1);
    assert_eq!(holdings.contributors[0].character_name, "Owner A Character");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn esi_holdings_for_an_unobserved_type_is_empty_not_an_error(pool: PgPool) {
    let workspace_id = WorkspaceId(Uuid::new_v4());
    let owner_id = Uuid::new_v4();
    let now = crate::db_now();
    let mut tx = pool.begin().await.expect("transaction begins");
    sqlx::query("INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Empty holdings test',$2,$3,$3)")
        .bind(workspace_id.0).bind(owner_id).bind(now).execute(&mut *tx).await.expect("workspace");
    sqlx::query("INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Owner',true,$3,$3)")
        .bind(owner_id).bind(workspace_id.0).bind(now).execute(&mut *tx).await.expect("owner");
    tx.commit().await.expect("fixtures committed");

    let holdings = PgProductionRepository::new(pool)
        .esi_holdings(workspace_id, OwnerId(owner_id), 34)
        .await
        .expect("holdings load");

    assert_eq!(holdings.observed_quantity, 0);
    assert_eq!(holdings.observed_at, None);
    assert!(holdings.contributors.is_empty());
}

/// ESI reports a stacked item's `location_id` as its *direct* container --
/// for cargo sitting in a ship, that's the ship's own item id, not a
/// station. `esi_holdings` must resolve through that the same way the
/// Assets browser does (walking to `asset_browser_current`'s
/// `effective_location_id`), not show "Unknown location <ship item id>".
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn esi_holdings_resolves_through_a_containing_ship_to_its_docked_station(pool: PgPool) {
    let workspace_id = WorkspaceId(Uuid::new_v4());
    let owner_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let sync_run_id = Uuid::new_v4();
    let snapshot_id = Uuid::new_v4();
    let now = crate::db_now();
    let jita = 60_003_760_i64;
    let ship_item_id = 9501_i64;
    let mut tx = pool.begin().await.expect("transaction begins");

    sqlx::query("INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Nested container test',$2,$3,$3)")
        .bind(workspace_id.0).bind(owner_id).bind(now).execute(&mut *tx).await.expect("workspace");
    sqlx::query("INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Owner',true,$3,$3)")
        .bind(owner_id).bind(workspace_id.0).bind(now).execute(&mut *tx).await.expect("owner");
    sqlx::query("INSERT INTO eve_connections (id,workspace_id,owner_id,eve_character_id,character_name,status,granted_scopes,connected_at,updated_at) VALUES ($1,$2,$3,9101,'Corvin','connected','{}',$4,$4)")
        .bind(connection_id).bind(workspace_id.0).bind(owner_id).bind(now).execute(&mut *tx).await.expect("connection");
    sqlx::query("INSERT INTO esi_sync_runs (id,workspace_id,owner_id,connection_id,requested_kind,status,phase,started_at,completed_at,summary) VALUES ($1,$2,$3,$4,'assets','succeeded','complete',$5,$5,'test')")
        .bind(sync_run_id).bind(workspace_id.0).bind(owner_id).bind(connection_id).bind(now).execute(&mut *tx).await.expect("sync run");
    sqlx::query("INSERT INTO esi_asset_snapshots (id,connection_id,sync_run_id,observed_at,completed_at,status,page_count,row_count,active) VALUES ($1,$2,$3,$4,$4,'complete',1,2,true)")
        .bind(snapshot_id).bind(connection_id).bind(sync_run_id).bind(now).execute(&mut *tx).await.expect("snapshot");
    let sde_import_id = Uuid::new_v4();
    sqlx::query("INSERT INTO sde_imports (id,source_version,source_label,source_checksum,status,active,started_at,completed_at) VALUES ($1,'test','test','esi-holdings-nested-fixture','active',true,$2,$2)")
        .bind(sde_import_id).bind(now).execute(&mut *tx).await.expect("sde import");
    sqlx::query("INSERT INTO sde_solar_systems (import_id,solar_system_id,name_en) VALUES ($1,30000142,'Jita')")
        .bind(sde_import_id).execute(&mut *tx).await.expect("jita solar system");
    sqlx::query("INSERT INTO sde_npc_stations (import_id,station_id,name_en,solar_system_id,owner_corporation_id,station_type_id) VALUES ($1,$2,'Jita IV - Moon 4 - Caldari Navy Assembly Plant',30000142,1000125,1531)")
        .bind(sde_import_id).bind(jita).execute(&mut *tx).await.expect("jita station");
    // The ship itself: docked directly at Jita, a singleton hull -- not a
    // contributor row, but still needed as a parent to walk through.
    sqlx::query(
        "INSERT INTO esi_asset_observations
               (snapshot_id,source_item_id,type_id,quantity,location_id,location_type,location_flag,is_singleton,is_blueprint_copy,raw_payload,source_checksum)
             VALUES
               ($1,$2,640,1,$3,'station','Hangar',true,NULL,'{}','the-ship')",
    )
    .bind(snapshot_id)
    .bind(ship_item_id)
    .bind(jita)
    .execute(&mut *tx)
    .await
    .expect("ship observation");
    // 500 Tritanium in the ship's cargo hold -- location_id here is the
    // ship's item id, not Jita's.
    sqlx::query(
        "INSERT INTO esi_asset_observations
               (snapshot_id,source_item_id,type_id,quantity,location_id,location_type,location_flag,is_singleton,is_blueprint_copy,raw_payload,source_checksum)
             VALUES
               ($1,9502,34,500,$2,'item','Cargo',false,NULL,'{}','tritanium-in-cargo')",
    )
    .bind(snapshot_id)
    .bind(ship_item_id)
    .execute(&mut *tx)
    .await
    .expect("cargo observation");
    resolve_asset_hierarchies(&mut tx).await;
    tx.commit().await.expect("fixtures committed");

    let holdings = PgProductionRepository::new(pool)
        .esi_holdings(workspace_id, OwnerId(owner_id), 34)
        .await
        .expect("holdings load");

    assert_eq!(holdings.observed_quantity, 500);
    assert_eq!(holdings.contributors.len(), 1);
    let contributor = &holdings.contributors[0];
    assert_eq!(contributor.quantity, 500);
    assert_eq!(
        contributor.location_id, jita,
        "must resolve through the ship to its docked station, not the ship's own item id"
    );
    assert_eq!(
        contributor.location_name.as_deref(),
        Some("Jita IV - Moon 4 - Caldari Navy Assembly Plant")
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn inventory_reconciliation_exclusion_identity_is_durable_and_unique(pool: PgPool) {
    let workspace_id = Uuid::new_v4();
    let owner_a = Uuid::new_v4();
    let owner_b = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let now = crate::db_now();
    let mut tx = pool.begin().await.expect("transaction begins");
    sqlx::query("INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Exclusion test',$2,$3,$3)")
        .bind(workspace_id).bind(owner_a).bind(now).execute(&mut *tx).await.expect("workspace");
    sqlx::query("INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Owner A',true,$3,$3),($4,$2,'manual','Owner B',false,$3,$3)")
        .bind(owner_a).bind(workspace_id).bind(now).bind(owner_b).execute(&mut *tx).await.expect("owners");
    sqlx::query("INSERT INTO eve_connections (id,workspace_id,owner_id,eve_character_id,character_name,status,granted_scopes,connected_at,updated_at) VALUES ($1,$2,$3,90000001,'Valka','connected','{}',$4,$4)")
        .bind(connection_id).bind(workspace_id).bind(owner_a).bind(now).execute(&mut *tx).await.expect("connection");
    tx.commit().await.expect("fixtures committed");

    let insert = "INSERT INTO inventory_reconciliation_exclusions (workspace_id,owner_id,type_id,eve_character_id,effective_location_id,created_at) VALUES ($1,$2,$3,$4,$5,$6)";
    sqlx::query(insert)
        .bind(workspace_id)
        .bind(owner_a)
        .bind(28668_i64)
        .bind(90_000_001_i64)
        .bind(60_014_708_i64)
        .bind(now)
        .execute(&pool)
        .await
        .expect("first exclusion");
    assert!(
        sqlx::query(insert)
            .bind(workspace_id)
            .bind(owner_a)
            .bind(28668_i64)
            .bind(90_000_001_i64)
            .bind(60_014_708_i64)
            .bind(now)
            .execute(&pool)
            .await
            .is_err(),
        "exact duplicate must be rejected"
    );
    sqlx::query(insert)
        .bind(workspace_id)
        .bind(owner_a)
        .bind(34_i64)
        .bind(90_000_001_i64)
        .bind(60_014_708_i64)
        .bind(now)
        .execute(&pool)
        .await
        .expect("other item");
    sqlx::query(insert)
        .bind(workspace_id)
        .bind(owner_b)
        .bind(28668_i64)
        .bind(90_000_001_i64)
        .bind(60_014_708_i64)
        .bind(now)
        .execute(&pool)
        .await
        .expect("other owner");

    sqlx::query("DELETE FROM eve_connections WHERE id=$1")
        .bind(connection_id)
        .execute(&pool)
        .await
        .expect("connection deletion");
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM inventory_reconciliation_exclusions WHERE workspace_id=$1",
    )
    .bind(workspace_id)
    .fetch_one(&pool)
    .await
    .expect("exclusion count");
    assert_eq!(
        count, 3,
        "connection lifecycle must not cascade reconciliation policy"
    );
}
