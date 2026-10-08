use super::common::*;

/// `known_locations_in_region` reads `market_location_names` directly,
/// scoped by workspace and region -- no `price_sources` row is ever
/// created in this test, proving the read genuinely doesn't depend on
/// `PriceSource`. Seeds a second workspace's structure in the same
/// region to prove the read is workspace-scoped too, not just
/// region-scoped.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn known_locations_in_region_is_scoped_by_workspace_and_region(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let other_workspace_id = Uuid::new_v4();
    let other_owner_id = Uuid::new_v4();
    let now = crate::db_now();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Other Workspace',$2,$3,$3)",
    )
    .bind(other_workspace_id)
    .bind(other_owner_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Other Workspace',true,$3,$3)",
    )
    .bind(other_owner_id)
    .bind(other_workspace_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    // Two regions, each with one solar system -- The Forge (Jita) and
    // Insmother -- against the same active `sde_imports` row `fixture`
    // already created.
    let import_id: Uuid = sqlx::query_scalar("SELECT id FROM sde_imports WHERE active=true")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO sde_regions (import_id,region_id,name_en) VALUES ($1,10000002,'The Forge'),($1,10000009,'Insmother')",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_solar_systems (import_id,solar_system_id,name_en,region_id) VALUES ($1,30000142,'Jita',10000002),($1,30000772,'C-J6MT',10000009)",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();

    // A player structure in The Forge, known to `workspace_id`; a
    // second structure in the same region but a different workspace;
    // a third structure in Insmother (a different region entirely).
    sqlx::query(
        r#"
        INSERT INTO market_location_names
          (workspace_id,location_id,location_name,owner_id,solar_system_id,structure_type_id,resolved_by_connection_id,resolved_at,updated_at)
        VALUES
          ($1,1049588174021,'Perimeter - The Bar',1,30000142,35825,NULL,$4,$4),
          ($2,1049588174022,'Some Other Workspaces Citadel',1,30000142,35825,NULL,$4,$4),
          ($1,1049588174023,'A Structure In Insmother',1,30000772,35825,NULL,$4,$4)
        "#,
    )
    .bind(workspace_id.0)
    .bind(other_workspace_id)
    .bind(Uuid::new_v4())
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let forge = repository
        .known_locations_in_region(workspace_id, 10_000_002)
        .await
        .unwrap();
    assert_eq!(forge.len(), 1);
    assert_eq!(forge[0].location_id, 1_049_588_174_021);
    assert_eq!(forge[0].location_name, "Perimeter - The Bar");
    assert_eq!(forge[0].solar_system_id, 30_000_142);
    assert_eq!(forge[0].solar_system_name, "Jita");
    assert_eq!(forge[0].structure_type_id, Some(35_825));

    let insmother = repository
        .known_locations_in_region(workspace_id, 10_000_009)
        .await
        .unwrap();
    assert_eq!(insmother.len(), 1);
    assert_eq!(insmother[0].location_id, 1_049_588_174_023);

    // The other workspace's structure, though in the same region, must
    // never appear for `workspace_id`.
    assert!(!forge
        .iter()
        .any(|location| location.location_id == 1_049_588_174_022));

    // Confirmed zero `price_sources` rows exist anywhere -- this read
    // genuinely doesn't depend on `PriceSource`.
    let price_source_count: i64 = sqlx::query_scalar("SELECT count(*) FROM price_sources")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(price_source_count, 0);
}

/// `classify_location` is positive-evidence-only -- an id absent from
/// both the SDE NPC-station table and the resolved-structure registry
/// is `Unknown`, never assumed to be an `NpcStation`. A stale/malformed
/// `market_location_names` row for a real NPC-station id must never
/// override the SDE's own answer, and a resolved location with no
/// captured `structure_type_id` is `Unknown`, not `Structure`.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn classify_location_uses_positive_evidence_and_never_infers_npc_station_from_absence(
    pool: PgPool,
) {
    let (workspace_id, repository) = fixture(&pool).await;
    let now = crate::db_now();
    let import_id: Uuid = sqlx::query_scalar("SELECT id FROM sde_imports WHERE active=true")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO sde_regions (import_id,region_id,name_en) VALUES ($1,10000002,'The Forge')",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_solar_systems (import_id,solar_system_id,name_en,region_id) VALUES ($1,30000142,'Jita',10000002)",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_npc_stations (import_id,station_id,name_en,solar_system_id,owner_corporation_id,station_type_id) VALUES ($1,60003760,'Jita IV - Moon 4 - Caldari Navy Assembly Plant',30000142,1000035,1529)",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();

    // A never-referenced id: no SDE row, no market_location_names row.
    assert_eq!(
        repository
            .classify_location(workspace_id, 99_999)
            .await
            .unwrap(),
        iskworks_core::MarketLocationClassification::Unknown,
    );

    // A resolved structure with a captured type_id.
    sqlx::query(
        r#"
        INSERT INTO market_location_names
          (workspace_id,location_id,location_name,owner_id,solar_system_id,structure_type_id,resolved_by_connection_id,resolved_at,updated_at)
        VALUES ($1,1049588174021,'Perimeter - The Bar',1,30000142,35825,NULL,$2,$2)
        "#,
    )
    .bind(workspace_id.0)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        repository
            .classify_location(workspace_id, 1_049_588_174_021)
            .await
            .unwrap(),
        iskworks_core::MarketLocationClassification::Structure {
            solar_system_id: 30_000_142
        },
    );

    // A resolved location with no captured structure_type_id -- not
    // enough evidence to call it a Structure.
    sqlx::query(
        r#"
        INSERT INTO market_location_names
          (workspace_id,location_id,location_name,owner_id,solar_system_id,structure_type_id,resolved_by_connection_id,resolved_at,updated_at)
        VALUES ($1,1049588174099,'Unknown Type Structure',1,30000142,NULL,NULL,$2,$2)
        "#,
    )
    .bind(workspace_id.0)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        repository
            .classify_location(workspace_id, 1_049_588_174_099)
            .await
            .unwrap(),
        iskworks_core::MarketLocationClassification::Unknown,
    );

    // A real NPC station id -- even with a stale market_location_names
    // row present for the exact same id (shouldn't happen in practice,
    // but the SDE table must still win).
    sqlx::query(
        r#"
        INSERT INTO market_location_names
          (workspace_id,location_id,location_name,owner_id,solar_system_id,structure_type_id,resolved_by_connection_id,resolved_at,updated_at)
        VALUES ($1,60003760,'Stale Row',1,30000142,35825,NULL,$2,$2)
        "#,
    )
    .bind(workspace_id.0)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        repository
            .classify_location(workspace_id, 60_003_760)
            .await
            .unwrap(),
        iskworks_core::MarketLocationClassification::NpcStation,
    );
}

/// Order-book location display names (Market order-book presentation
/// fix): NPC stations resolve straight from the SDE with their solar
/// system's security status appended, player structures fall back to
/// `market_location_names`, and anything genuinely unresolvable is
/// simply absent from the result -- the route builds its own
/// `format!("Location {id}")` fallback for those, not this method.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn resolve_order_location_names_prefers_the_sde_and_falls_back_to_resolved_structures(
    pool: PgPool,
) {
    let (workspace_id, repository) = fixture(&pool).await;
    let now = crate::db_now();
    let import_id: Uuid = sqlx::query_scalar("SELECT id FROM sde_imports WHERE active=true")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO sde_regions (import_id,region_id,name_en) VALUES ($1,10000002,'The Forge')",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_solar_systems (import_id,solar_system_id,name_en,region_id,security_status) VALUES ($1,30000142,'Jita',10000002,0.9459)",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_npc_stations (import_id,station_id,name_en,solar_system_id,owner_corporation_id,station_type_id) VALUES ($1,60003760,'Jita IV - Moon 4 - Caldari Navy Assembly Plant',30000142,1000035,1529)",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO market_location_names
          (workspace_id,location_id,location_name,owner_id,solar_system_id,structure_type_id,resolved_by_connection_id,resolved_at,updated_at)
        VALUES ($1,1049588174021,'Perimeter - The Bar',1,30000142,35825,NULL,$2,$2)
        "#,
    )
    .bind(workspace_id.0)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let resolved = repository
        .resolve_order_location_names(workspace_id, &[60_003_760, 1_049_588_174_021, 999_999])
        .await
        .unwrap();

    // The NPC station resolves from the SDE with its security status
    // appended, rounded to one decimal with the trailing ".0" dropped.
    assert_eq!(
        resolved.get(&60_003_760).map(String::as_str),
        Some("Jita IV - Moon 4 - Caldari Navy Assembly Plant (0.9)")
    );
    // The player structure isn't in the SDE at all -- resolves via
    // `market_location_names` instead, with no security suffix (no
    // solar system security lookup is even attempted for it).
    assert_eq!(
        resolved.get(&1_049_588_174_021).map(String::as_str),
        Some("Perimeter - The Bar")
    );
    // Genuinely unresolvable -- absent from the result, not a fallback
    // string (that's the route's job, not this method's).
    assert!(!resolved.contains_key(&999_999));
}

/// `remember_market_access`/`market_access_connection`/
/// `clear_market_access` round-trip, and remembering is independent of
/// `resolved_by_connection_id` (name resolution) -- both can point at
/// different connections for the same structure.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn remember_and_clear_market_access_round_trips_independently_of_name_resolution(
    pool: PgPool,
) {
    let (workspace_id, repository) = fixture(&pool).await;
    let now = crate::db_now();
    let name_resolver_id = Uuid::new_v4();
    let market_resolver_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO eve_connections (id,workspace_id,owner_id,eve_character_id,character_name,status,granted_scopes,connected_at,updated_at) VALUES ($1,$2,(SELECT id FROM owners WHERE workspace_id=$2),9001,'Name Resolver','connected','{}',$3,$3),($4,$2,(SELECT id FROM owners WHERE workspace_id=$2),9002,'Market Resolver','connected','{}',$3,$3)",
    )
    .bind(name_resolver_id)
    .bind(workspace_id.0)
    .bind(now)
    .bind(market_resolver_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO market_location_names
          (workspace_id,location_id,location_name,owner_id,solar_system_id,structure_type_id,resolved_by_connection_id,resolved_at,updated_at)
        VALUES ($1,1049588174021,'Perimeter - The Bar',1,30000142,35825,$2,$3,$3)
        "#,
    )
    .bind(workspace_id.0)
    .bind(name_resolver_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    // Nothing remembered yet.
    assert_eq!(
        repository
            .market_access_connection(workspace_id, 1_049_588_174_021)
            .await
            .unwrap(),
        None,
    );

    repository
        .remember_market_access(
            workspace_id,
            1_049_588_174_021,
            ConnectedCharacterId(market_resolver_id),
            now,
        )
        .await
        .unwrap();
    assert_eq!(
        repository
            .market_access_connection(workspace_id, 1_049_588_174_021)
            .await
            .unwrap(),
        Some(ConnectedCharacterId(market_resolver_id)),
    );
    // The name-resolving connection is untouched -- these are separate
    // concerns even though this test happens to use the same location.
    let resolved_by: Uuid = sqlx::query_scalar(
        "SELECT resolved_by_connection_id FROM market_location_names WHERE workspace_id=$1 AND location_id=$2",
    )
    .bind(workspace_id.0)
    .bind(1_049_588_174_021_i64)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(resolved_by, name_resolver_id);

    repository
        .clear_market_access(workspace_id, 1_049_588_174_021, now)
        .await
        .unwrap();
    assert_eq!(
        repository
            .market_access_connection(workspace_id, 1_049_588_174_021)
            .await
            .unwrap(),
        None,
    );
}

/// `remember_market_access` is only ever valid for a location already
/// resolved into `market_location_names` -- an unresolved location_id
/// is a programming error, not a silent no-op.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn remember_market_access_fails_for_an_unresolved_location(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let now = crate::db_now();
    sqlx::query(
        "INSERT INTO eve_connections (id,workspace_id,owner_id,eve_character_id,character_name,status,granted_scopes,connected_at,updated_at) VALUES ($1,$2,(SELECT id FROM owners WHERE workspace_id=$2),9001,'Fixture','connected','{}',$3,$3)",
    )
    .bind(Uuid::new_v4())
    .bind(workspace_id.0)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    let connection_id: Uuid = sqlx::query_scalar("SELECT id FROM eve_connections LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();

    let result = repository
        .remember_market_access(
            workspace_id,
            999_999_999,
            ConnectedCharacterId(connection_id),
            now,
        )
        .await;
    assert!(matches!(result, Err(MarketError::Validation(_))));
}

/// Access state is derived from the joined connection's *current*
/// status, not stored directly -- covers all three states
/// `list_known_structures` can report.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn list_known_structures_derives_confirmed_expired_and_unknown_access_states(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let now = crate::db_now();
    let connected_id = Uuid::new_v4();
    let lapsed_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO eve_connections
          (id,workspace_id,owner_id,eve_character_id,character_name,status,granted_scopes,connected_at,updated_at)
        VALUES
          ($1,$3,(SELECT id FROM owners WHERE workspace_id=$3),9001,'Kira Vayne','connected','{}',$4,$4),
          ($2,$3,(SELECT id FROM owners WHERE workspace_id=$3),9002,'Drake Orin','needs_reconnection','{}',$4,$4)
        "#,
    )
    .bind(connected_id)
    .bind(lapsed_id)
    .bind(workspace_id.0)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO market_location_names
          (workspace_id,location_id,location_name,owner_id,solar_system_id,structure_type_id,resolved_by_connection_id,resolved_at,updated_at,market_access_connection_id,market_access_checked_at)
        VALUES
          ($1,1001,'Confirmed Structure',1,30000142,35832,NULL,$4,$4,$2,$4),
          ($1,1002,'Expired Structure',1,30000142,35832,NULL,$4,$4,$3,$4),
          ($1,1003,'Unverified Structure',1,30000142,35832,NULL,$4,$4,NULL,NULL)
        "#,
    )
    .bind(workspace_id.0)
    .bind(connected_id)
    .bind(lapsed_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let structures = repository
        .list_known_structures(workspace_id, "")
        .await
        .unwrap();
    assert_eq!(structures.len(), 3);

    let confirmed = structures.iter().find(|s| s.location_id == 1001).unwrap();
    assert_eq!(
        confirmed.access_state,
        iskworks_core::MarketAccessState::Confirmed
    );
    assert_eq!(
        confirmed.access_character_name.as_deref(),
        Some("Kira Vayne")
    );

    let expired = structures.iter().find(|s| s.location_id == 1002).unwrap();
    assert_eq!(
        expired.access_state,
        iskworks_core::MarketAccessState::Expired
    );
    assert_eq!(expired.access_character_name.as_deref(), Some("Drake Orin"));

    let unverified = structures.iter().find(|s| s.location_id == 1003).unwrap();
    assert_eq!(
        unverified.access_state,
        iskworks_core::MarketAccessState::Unknown
    );
    assert_eq!(unverified.access_character_name, None);
}

/// `list_known_structures`'s `query` filter (also used by global search):
/// matches the structure's own name
/// *or* its solar system's name, and never leaks another workspace's
/// structures even when that structure's name would otherwise match.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn list_known_structures_query_matches_name_or_system_and_stays_workspace_scoped(
    pool: PgPool,
) {
    let (workspace_id, repository) = fixture(&pool).await;
    let other_workspace_id = Uuid::new_v4();
    let other_owner_id = Uuid::new_v4();
    let now = crate::db_now();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Other Workspace',$2,$3,$3)",
    )
    .bind(other_workspace_id)
    .bind(other_owner_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Other Workspace',true,$3,$3)",
    )
    .bind(other_owner_id)
    .bind(other_workspace_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let import_id: Uuid = sqlx::query_scalar("SELECT id FROM sde_imports WHERE active=true")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO sde_solar_systems (import_id,solar_system_id,name_en,region_id) VALUES ($1,30000142,'Jita',10000002),($1,30000772,'C-J6MT',10000009)",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO market_location_names
          (workspace_id,location_id,location_name,owner_id,solar_system_id,structure_type_id,resolved_by_connection_id,resolved_at,updated_at)
        VALUES
          -- Own name matches "Jita" directly.
          ($1,2001,'Jita Trade Tower',1,30000142,35832,NULL,$3,$3),
          -- Own name has nothing to do with "C-J6MT" -- only findable
          -- via its solar system's name.
          ($1,2002,'Private Trade Hub',1,30000772,35832,NULL,$3,$3),
          -- Same name as the first, but a DIFFERENT workspace -- must
          -- never appear in workspace_id's own results.
          ($2,2003,'Jita Trade Tower',1,30000142,35832,NULL,$3,$3)
        "#,
    )
    .bind(workspace_id.0)
    .bind(other_workspace_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let by_own_name = repository
        .list_known_structures(workspace_id, "Jita")
        .await
        .unwrap();
    assert_eq!(by_own_name.len(), 1, "{by_own_name:?}");
    assert_eq!(by_own_name[0].location_id, 2001);

    let by_system_name = repository
        .list_known_structures(workspace_id, "C-J6MT")
        .await
        .unwrap();
    assert_eq!(by_system_name.len(), 1, "{by_system_name:?}");
    assert_eq!(by_system_name[0].location_id, 2002);

    // The other workspace's identically-named structure never leaks in,
    // regardless of query.
    assert!(by_own_name.iter().all(|s| s.location_id != 2003));

    let no_match = repository
        .list_known_structures(workspace_id, "Nonexistent Query")
        .await
        .unwrap();
    assert!(no_match.is_empty());
}
