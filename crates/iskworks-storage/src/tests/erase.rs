use super::*;
use crate::erase::EXPLICIT_DELETES;
use crate::PgAdminRepository;
use iskworks_core::AdminUsersRepository;
use std::collections::BTreeSet;

async fn exec(pool: &PgPool, sql: String) {
    if let Err(error) = sqlx::query(&sql).execute(pool).await {
        panic!("seed failed: {error}\n--- sql ---\n{sql}");
    }
}

async fn provision(pool: &PgPool, character_id: i64, name: &str) -> (iskworks_core::User, Uuid) {
    let user = PgUserRepository::new(pool.clone())
        .claim_unclaimed_workspace_or_provision(
            EveIdentity {
                character_id,
                character_name: name.to_string(),
            },
            iskworks_core::InviteGrant::NotRequired,
        )
        .await
        .unwrap();
    let owner_id: Uuid = sqlx::query_scalar("SELECT owner_id FROM workspaces WHERE id = $1")
        .bind(user.workspace_id.0)
        .fetch_one(pool)
        .await
        .unwrap();
    (user, owner_id)
}

/// One row in (nearly) every workspace-scoped table, wired through every
/// RESTRICT chain: ledger events (purchase, reversal, ticket recording),
/// ESI observations, orders/tickets/builds, market snapshots.
async fn seed(pool: &PgPool, w: Uuid, o: Uuid, sde: Uuid, salt: i64) {
    let id = || Uuid::new_v4();
    let (conn, run, wtx, snap, ev_buy, ev_rev, ev_prod, rec) =
        (id(), id(), id(), id(), id(), id(), id(), id());
    let (build, build2, price_snap, order, req, arun, t1, t2, prereq) =
        (id(), id(), id(), id(), id(), id(), id(), id(), id());
    let (psrc, obatch, obs, ibatch, ifile, psnap) = (id(), id(), id(), id(), id(), id());
    let c = 900_000 + salt;

    // ESI.
    exec(pool, format!("INSERT INTO eve_connections (id, workspace_id, owner_id, eve_character_id, character_name, status, granted_scopes, connected_at, updated_at) VALUES ('{conn}','{w}','{o}',{c},'Alt {salt}','connected','{{}}',now(),now())")).await;
    exec(pool, format!("INSERT INTO eve_connection_tokens (connection_id, refresh_token_envelope, updated_at) VALUES ('{conn}','{{}}',now())")).await;
    exec(pool, format!("INSERT INTO eve_oauth_pending_authorizations (state_hash, workspace_id, owner_id, pkce_verifier_envelope, requested_scopes, return_path, created_at, expires_at) VALUES ('state{salt}','{w}','{o}','{{}}','{{}}','/settings/eve',now(),now() + interval '1 hour')")).await;
    exec(pool, format!("INSERT INTO esi_sync_runs (id, workspace_id, owner_id, connection_id, requested_kind, status, phase, started_at) VALUES ('{run}','{w}','{o}','{conn}','all_supported','succeeded','complete',now())")).await;
    exec(pool, format!("INSERT INTO esi_asset_snapshots (id, connection_id, sync_run_id, observed_at, status, page_count) VALUES ('{snap}','{conn}','{run}',now(),'complete',1)")).await;
    exec(pool, format!("INSERT INTO esi_wallet_balances (id, connection_id, sync_run_id, balance, observed_at, source_checksum) VALUES ('{}','{conn}','{run}',100,now(),'bal{salt}')", id())).await;
    exec(pool, format!("INSERT INTO esi_wallet_transactions (id, connection_id, source_transaction_id, first_sync_run_id, last_sync_run_id, type_id, quantity, unit_price, total_price, is_buy, is_personal, transacted_at, location_id, client_id, journal_ref_id, raw_payload, source_checksum, first_observed_at, last_observed_at) VALUES ('{wtx}','{conn}',{salt},'{run}','{run}',34,10,1,10,true,true,now(),60003760,1,1,'{{}}','wtx{salt}',now(),now())")).await;
    exec(pool, format!("INSERT INTO blueprint_observations (id, workspace_id, owner_id, eve_item_id, blueprint_type_id, captured_blueprint_name, blueprint_kind, material_efficiency, time_efficiency, location_id, location_flag, observed_at, source_checksum, licensed_runs) VALUES ('{}','{w}','{o}',{salt},691,'Rifter Blueprint','original',0,0,60003760,'Hangar',now(),'bp{salt}',1)", id())).await;

    // Tickets, recordings and the inventory ledger.
    exec(pool, format!("INSERT INTO price_sources (id, workspace_id, display_name, source_kind, created_at, updated_at) VALUES ('{psrc}','{w}','Prices {salt}','manual',now(),now())")).await;
    exec(pool, format!("INSERT INTO acquisition_runs (id, workspace_id, owner_id, display_id, name, kind, status, created_at, updated_at) VALUES ('{arun}','{w}','{o}','AR-{salt}','Run','acquisition','ready',now(),now())")).await;
    for (t, n) in [(t1, 1), (t2, 2)] {
        exec(pool, format!("INSERT INTO tickets (id, workspace_id, owner_id, display_id, kind, type_id, quantity, captured_name, status, price_source_id, acquisition_run_id, created_at, updated_at) VALUES ('{t}','{w}','{o}','T-{salt}-{n}','acquisition',34,5,'Tritanium','todo','{psrc}','{arun}',now(),now())")).await;
    }
    exec(pool, format!("INSERT INTO ticket_inventory_recordings (id, ticket_id, kind, idempotency_key, recorded_at, recorded_quantity) VALUES ('{rec}','{t1}','acquisition','{}',now(),5)", id())).await;
    exec(pool, format!("INSERT INTO inventory_events (id, workspace_id, owner_id, type_id, captured_name, event_kind, quantity_delta, total_cost_delta, cost_quality, effective_at, recorded_at, sequence, resulting_quantity, resulting_total_cost, resulting_revision, resulting_average_cost) VALUES ('{ev_buy}','{w}','{o}',34,'Tritanium','purchase',10,10,'known',now(),now(),1,10,10,1,1)")).await;
    exec(pool, format!("INSERT INTO inventory_events (id, workspace_id, owner_id, type_id, captured_name, event_kind, quantity_delta, total_cost_delta, cost_quality, effective_at, recorded_at, sequence, resulting_quantity, resulting_total_cost, resulting_revision, reverses_event_id) VALUES ('{ev_rev}','{w}','{o}',34,'Tritanium','reversal',-10,-10,'known',now(),now(),2,0,0,2,'{ev_buy}')")).await;
    exec(pool, format!("INSERT INTO inventory_events (id, workspace_id, owner_id, type_id, captured_name, event_kind, quantity_delta, total_cost_delta, cost_quality, effective_at, recorded_at, sequence, resulting_quantity, resulting_total_cost, resulting_revision, resulting_average_cost, ticket_inventory_recording_id) VALUES ('{ev_prod}','{w}','{o}',34,'Tritanium','purchase',5,5,'known',now(),now(),3,5,5,3,1,'{rec}')")).await;
    exec(pool, format!("INSERT INTO inventory_event_sources (inventory_event_id, workspace_id, owner_id, source_system, source_record_kind, source_record_id, accounting_effect_kind, connection_id, observation_id, sync_run_id, source_transaction_at, accepted_at) VALUES ('{ev_buy}','{w}','{o}','esi','wallet_transaction','wtx{salt}','purchase','{conn}','{wtx}','{run}',now(),now())")).await;
    exec(pool, format!("INSERT INTO inventory_balances (workspace_id, owner_id, type_id, captured_name, quantity, total_historical_cost, revision, last_activity_at) VALUES ('{w}','{o}',34,'Tritanium',5,5,1,now())")).await;
    exec(pool, format!("INSERT INTO inventory_reconciliation_exclusions (workspace_id, owner_id, type_id, eve_character_id, effective_location_id, created_at) VALUES ('{w}','{o}',34,{c},60003760,now())")).await;

    // Builds, orders, requirements and fulfilments.
    for b in [build, build2] {
        exec(pool, format!("INSERT INTO builds (id, workspace_id, owner_id, display_name, product_type_id, product_name, product_quantity_per_run, source_sde_dataset_id, source_sde_version, recipe_fingerprint, runs, created_at, updated_at, recipe_kind, blueprint_type_id, blueprint_name, duration_seconds_per_run, plan_root_build_id) VALUES ('{b}','{w}','{o}','Build','587','Rifter',1,'{sde}','v1','fp',1,now(),now(),'manufacturing',691,'Rifter Blueprint',600,'{b}')")).await;
    }
    exec(pool, format!("INSERT INTO production_dependencies (workspace_id, plan_root_build_id, consumer_build_id, component_type_id, sourcing) VALUES ('{w}','{build}','{build2}',34,'buy')")).await;
    exec(pool, format!("INSERT INTO price_snapshots (id, workspace_id, captured_source_name, captured_source_revision, purpose, created_at) VALUES ('{price_snap}','{w}','Prices',1,'build_planning',now())")).await;
    exec(pool, format!("INSERT INTO orders (id, workspace_id, owner_id, source_build_revision, display_name, runs, recipe_fingerprint, price_snapshot_id, estimated_material_cost, missing_price_count, created_at, updated_at, source_build_id) VALUES ('{order}','{w}','{o}',1,'Order',1,'fp','{price_snap}',0,0,now(),now(),'{build}')")).await;
    exec(pool, format!("INSERT INTO order_requirements (id, order_id, type_id, captured_name, kind, required_quantity, fresh_quantity) VALUES ('{req}','{order}',34,'Tritanium','buy',5,5)")).await;
    exec(pool, format!("INSERT INTO inventory_allocations (id, workspace_id, owner_id, type_id, quantity, created_at, order_requirement_id) VALUES ('{}','{w}','{o}',34,1,now(),'{req}')", id())).await;
    exec(pool, format!("INSERT INTO order_requirement_fulfillments (id, order_requirement_id, ticket_id, allocated_quantity, linked_at) VALUES ('{}','{req}','{t1}',5,now())", id())).await;
    exec(pool, format!("INSERT INTO ticket_prerequisites (id, ticket_id, type_id, captured_name, required_quantity, fresh_quantity, kind) VALUES ('{prereq}','{t1}',34,'Tritanium',5,5,'buy')")).await;
    exec(pool, format!("INSERT INTO ticket_prerequisite_fulfillments (id, ticket_prerequisite_id, fulfilling_ticket_id, allocated_quantity, linked_at) VALUES ('{}','{prereq}','{t2}',5,now())", id())).await;

    // Market data and price snapshots.
    exec(pool, format!("INSERT INTO market_observation_batches (id, workspace_id, origin, status, type_id, captured_type_name, region_id, solar_system_id, location_id, attempted_at, observed_at, completed_at) VALUES ('{obatch}','{w}','esi_market_orders','completed',34,'Tritanium',10000002,30000142,60003760,now(),now(),now())")).await;
    exec(pool, format!("INSERT INTO market_order_observations (id, workspace_id, source_kind, observed_at, imported_at, order_id, type_id, captured_type_name, order_side, price, remaining_volume, entered_volume, minimum_volume, order_range, issued_at, duration_days, location_id, solar_system_id, region_id, jumps, normalized_row_checksum, observation_batch_id) VALUES ('{obs}','{w}','esi_market_orders',now(),now(),{salt},34,'Tritanium','sell',5,10,10,1,0,now(),90,60003760,30000142,10000002,0,'row{salt}','{obatch}')")).await;
    exec(pool, format!("INSERT INTO market_import_batches (id, workspace_id, source_kind, status, observed_at_min, observed_at_max, imported_at, file_count, item_count, location_count, observation_count) VALUES ('{ibatch}','{w}','eve_client_market_export','succeeded',now(),now(),now(),1,1,1,1)")).await;
    exec(pool, format!("INSERT INTO market_import_files (id, batch_id, workspace_id, original_filename, sanitized_filename, file_checksum, normalized_checksum, file_size_bytes, observed_at, timestamp_source, type_id, captured_type_name, location_id, solar_system_id, region_id, row_count, buy_order_count, sell_order_count, status, imported_at) VALUES ('{ifile}','{ibatch}','{w}','f.csv','f.csv','fc{salt}','nc{salt}',10,now(),'filename',34,'Tritanium',60003760,30000142,10000002,1,0,1,'imported',now())")).await;
    exec(pool, format!("INSERT INTO market_import_file_observations (market_import_file_id, market_order_observation_id, workspace_id, source_row_number) VALUES ('{ifile}','{obs}','{w}',2)")).await;
    exec(pool, format!("INSERT INTO market_price_source_configs (price_source_id, workspace_id, location_id, solar_system_id, region_id, pricing_policy, coverage_policy, observation_mode, fresh_after_hours, stale_after_hours, source_kind, pinned_batch_id) VALUES ('{psrc}','{w}',60003760,30000142,10000002,'lowest_sell','require_full_coverage','pinned_import_batch',24,48,'eve_client_market_export','{ibatch}')")).await;
    exec(pool, format!("INSERT INTO market_source_coverage (workspace_id, price_source_id, type_id, type_name, refresh_state, created_at, updated_at, region_id, location_id) VALUES ('{w}','{psrc}',34,'Tritanium','missing',now(),now(),10000002,60003760)")).await;
    exec(pool, format!("INSERT INTO market_price_snapshots (id, workspace_id, price_source_id, captured_source_name, captured_source_revision, purpose, formula_version, oldest_observation_at, newest_observation_at, created_at) VALUES ('{psnap}','{w}','{psrc}','Prices',1,'build_planning','v1',now(),now(),now())")).await;
    exec(pool, format!("INSERT INTO market_price_snapshot_lines (market_price_snapshot_id, type_id, captured_type_name, pricing_policy, requested_quantity, calculated_total, covered_quantity, uncovered_quantity, fully_covered, order_count_used, available_volume, location_id, quality, calculation_trace) VALUES ('{psnap}',34,'Tritanium','lowest_sell',1,5,1,0,true,1,10,60003760,'direct','{{}}')")).await;
    exec(pool, format!("INSERT INTO market_price_snapshot_observations (market_price_snapshot_id, type_id, market_order_observation_id) VALUES ('{psnap}',34,'{obs}')")).await;
}

/// Every table the seed touches (plus cascaded children), for before/after counts.
const SEEDED_TABLES: &[&str] = &[
    "eve_connections",
    "eve_connection_tokens",
    "eve_oauth_pending_authorizations",
    "esi_sync_runs",
    "esi_asset_snapshots",
    "esi_wallet_balances",
    "esi_wallet_transactions",
    "blueprint_observations",
    "price_sources",
    "acquisition_runs",
    "tickets",
    "ticket_inventory_recordings",
    "inventory_events",
    "inventory_event_sources",
    "inventory_balances",
    "inventory_allocations",
    "inventory_reconciliation_exclusions",
    "builds",
    "production_dependencies",
    "price_snapshots",
    "orders",
    "order_requirements",
    "order_requirement_fulfillments",
    "ticket_prerequisites",
    "ticket_prerequisite_fulfillments",
    "market_observation_batches",
    "market_order_observations",
    "market_import_batches",
    "market_import_files",
    "market_import_file_observations",
    "market_price_source_configs",
    "market_source_coverage",
    "market_price_snapshots",
    "market_price_snapshot_lines",
    "market_price_snapshot_observations",
    "owners",
    "workspaces",
    "users",
];

async fn counts(pool: &PgPool) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    for table in SEEDED_TABLES {
        let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(pool)
            .await
            .unwrap();
        out.push((table.to_string(), n));
    }
    out
}

async fn sde_import(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    exec(pool, format!("INSERT INTO sde_imports (id, source_version, source_label, source_checksum, status, started_at) VALUES ('{id}','v1','test','sum','active',now())")).await;
    id
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_user_erases_their_whole_workspace_and_nothing_else(pool: PgPool) {
    let sde = sde_import(&pool).await;
    let (a, a_owner) = provision(&pool, 1, "Erased Pilot").await;
    let (b, b_owner) = provision(&pool, 2, "Kept Pilot").await;
    seed(&pool, a.workspace_id.0, a_owner, sde, 1).await;
    seed(&pool, b.workspace_id.0, b_owner, sde, 2).await;

    let before = counts(&pool).await;
    for (table, n) in &before {
        assert!(
            *n >= 2,
            "seed should populate {table} for both tenants (got {n})"
        );
    }

    let admin = PgAdminRepository::new(pool.clone());
    assert!(admin.delete_user(a.id).await.unwrap());
    assert!(!admin.delete_user(a.id).await.unwrap(), "already gone");

    // Tenant A's data is gone and tenant B's is untouched: every table
    // seeded identically for both now holds exactly half its rows.
    let after = counts(&pool).await;
    for ((table, was), (_, now)) in before.iter().zip(&after) {
        assert_eq!(*now * 2, *was, "{table}: before {was}, after {now}");
    }
    let a_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspaces WHERE id = $1")
        .bind(a.workspace_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(a_rows, 0);
    assert!(PgUserRepository::new(pool.clone())
        .find_by_character_id(2)
        .await
        .unwrap()
        .is_some());
    let sde_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sde_imports")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(sde_rows, 1, "shared reference data is never erased");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn erasing_a_workspace_leaves_app_wide_public_market_data(pool: PgPool) {
    let sde = sde_import(&pool).await;
    let (a, a_owner) = provision(&pool, 1, "Erased Pilot").await;
    seed(&pool, a.workspace_id.0, a_owner, sde, 1).await;
    let market = crate::PgMarketRepository::new(pool.clone());
    let now = chrono::Utc::now();
    market
        .register_public_market_demand(
            10_000_002,
            vec![iskworks_core::MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }],
            true,
            now,
        )
        .await
        .unwrap();
    let claim = market
        .begin_public_market_refresh(10_000_002, 34, now, now + Duration::minutes(2))
        .await
        .unwrap()
        .unwrap();
    let batch_id = iskworks_core::MarketObservationBatchId::new();
    assert!(market
        .complete_public_market_refresh(
            claim,
            now + Duration::minutes(15),
            iskworks_core::PublicMarketObservationBatch {
                id: batch_id,
                type_id: 34,
                type_name: "Tritanium".to_string(),
                region_id: 10_000_002,
                observed_at: now,
                etag: None,
                expires_at: None,
                orders: vec![],
            },
        )
        .await
        .unwrap());

    let admin = PgAdminRepository::new(pool.clone());
    assert!(admin.delete_user(a.id).await.unwrap());

    let (coverage, batches): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM public_market_coverage),
                    (SELECT count(*) FROM market_observation_batches WHERE id = $1)",
    )
    .bind(batch_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (coverage, batches),
        (1, 1),
        "app-wide market data is never erased"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_failed_erase_rolls_back_completely(pool: PgPool) {
    let sde = sde_import(&pool).await;
    let (a, a_owner) = provision(&pool, 1, "Pilot").await;
    seed(&pool, a.workspace_id.0, a_owner, sde, 1).await;
    // A row the teardown does not know about, pointing at the workspace
    // with a RESTRICT link, must abort the whole erase.
    exec(&pool, "CREATE TABLE zz_unknown_child (workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT)".to_string()).await;
    exec(
        &pool,
        format!(
            "INSERT INTO zz_unknown_child VALUES ('{}')",
            a.workspace_id.0
        ),
    )
    .await;

    let before = counts(&pool).await;
    let admin = PgAdminRepository::new(pool.clone());
    assert!(admin.delete_user(a.id).await.is_err());
    assert_eq!(counts(&pool).await, before, "nothing was erased");
    assert!(PgUserRepository::new(pool.clone())
        .find_by_character_id(1)
        .await
        .unwrap()
        .is_some());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn orphaned_workspaces_are_counted_and_erased_but_unclaimed_ones_are_not(pool: PgPool) {
    let sde = sde_import(&pool).await;
    let (kept, kept_owner) = provision(&pool, 1, "Active Pilot").await;
    let (gone, gone_owner) = provision(&pool, 2, "Orphaned Pilot").await;
    seed(&pool, kept.workspace_id.0, kept_owner, sde, 1).await;
    seed(&pool, gone.workspace_id.0, gone_owner, sde, 2).await;

    // An unclaimed legacy workspace (no user, claimed_at NULL) must be left alone.
    let legacy = Uuid::new_v4();
    let legacy_owner = Uuid::new_v4();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET CONSTRAINTS ALL DEFERRED")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces (id, display_name, owner_id, created_at, updated_at) VALUES ($1,'Legacy',$2,now(),now())")
        .bind(legacy).bind(legacy_owner).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO owners (id, workspace_id, owner_kind, display_name, created_at, updated_at) VALUES ($1,$2,'manual','Legacy',now(),now())")
        .bind(legacy_owner).bind(legacy).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();

    // Simulate the earlier account-only delete: remove just the user.
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(gone.id.0)
        .execute(&pool)
        .await
        .unwrap();

    let admin = PgAdminRepository::new(pool.clone());
    assert_eq!(admin.count_orphaned_workspaces().await.unwrap(), 1);
    assert_eq!(admin.erase_orphaned_workspaces().await.unwrap(), 1);
    assert_eq!(admin.count_orphaned_workspaces().await.unwrap(), 0);
    assert_eq!(admin.erase_orphaned_workspaces().await.unwrap(), 0);

    let workspaces: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM workspaces ORDER BY created_at")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(workspaces.len(), 2);
    assert!(workspaces.contains(&kept.workspace_id.0));
    assert!(workspaces.contains(&legacy));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn price_snapshots_stay_immutable_outside_an_erase(pool: PgPool) {
    let sde = sde_import(&pool).await;
    let (a, a_owner) = provision(&pool, 1, "Pilot").await;
    seed(&pool, a.workspace_id.0, a_owner, sde, 1).await;

    for sql in [
        "DELETE FROM market_price_snapshot_observations",
        "DELETE FROM market_price_snapshot_lines",
        "DELETE FROM market_price_snapshots",
        "UPDATE market_price_snapshots SET captured_source_name = 'x'",
    ] {
        let error = sqlx::query(sql)
            .execute(&pool)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("immutable"), "{sql}: {error}");
    }

    // Opting in only ever permits DELETE on the three snapshot tables.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('iskworks.erase_workspace', 'on', true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let error = sqlx::query("UPDATE market_price_snapshots SET captured_source_name = 'x'")
        .execute(&mut *tx)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("immutable"), "{error}");
}

// -- Schema guards: fail when a migration adds something erase doesn't know.

/// Tables that belong to a workspace: those with a workspace_id/owner_id
/// column, plus (transitively) anything with a foreign key into them.
/// App-wide tables that hold a foreign key into a workspace table but only
/// ever point at its app-wide rows, so they are not workspace data.
/// `public_market_coverage` references `market_observation_batches` rows
/// with `workspace_id IS NULL` only; erase never deletes those. (Were one
/// ever to point at a workspace batch, the RESTRICT would fail the erase
/// and roll it back, not lose data.)
const APP_WIDE_TABLES: &[&str] = &["public_market_coverage"];

async fn workspace_subtree(pool: &PgPool) -> BTreeSet<String> {
    let rows: Vec<String> = sqlx::query_scalar(
        r#"
            WITH RECURSIVE subtree(tbl) AS (
                SELECT DISTINCT table_name::text FROM information_schema.columns
                 WHERE table_schema = 'public'
                   AND column_name IN ('workspace_id', 'owner_id')
                   AND table_name NOT LIKE 'sde\_%'
                UNION
                SELECT conrelid::regclass::text FROM pg_constraint c
                  JOIN subtree s ON s.tbl = c.confrelid::regclass::text
                 WHERE c.contype = 'f'
                   AND conrelid::regclass::text <> ALL($1)
            )
            SELECT tbl FROM subtree
            "#,
    )
    .bind(APP_WIDE_TABLES)
    .fetch_all(pool)
    .await
    .unwrap();
    rows.into_iter().collect()
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn guard_every_restrict_link_into_the_workspace_is_explicitly_erased(pool: PgPool) {
    let subtree = workspace_subtree(&pool).await;
    let covered: BTreeSet<&str> = EXPLICIT_DELETES.iter().copied().collect();
    let blocking: Vec<(String, String, String)> = sqlx::query_as(
        r#"
            SELECT conrelid::regclass::text, confrelid::regclass::text, conname::text
              FROM pg_constraint
             WHERE contype = 'f' AND confdeltype IN ('a', 'r')
            "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let missing: Vec<String> = blocking
        .into_iter()
        .filter(|(child, parent, _)| subtree.contains(parent) && subtree.contains(child))
        .filter(|(child, _, _)| !covered.contains(child.as_str()))
        .map(|(child, parent, name)| format!("{child} -> {parent} ({name})"))
        .collect();
    assert!(
        missing.is_empty(),
        "new RESTRICT/NO ACTION links into workspace data; add them to erase.rs \
             (and EXPLICIT_DELETES): {missing:#?}"
    );
    for table in EXPLICIT_DELETES {
        assert!(
            matches!(*table, "workspaces" | "ticket_inventory_recordings")
                || subtree.contains(*table),
            "{table} is listed in EXPLICIT_DELETES but is not workspace data"
        );
    }
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn guard_no_unaccounted_uuid_columns_without_a_foreign_key(pool: PgPool) {
    // A uuid column with no FK can hold a workspace's data that no cascade
    // reaches. The known ones are handled in erase.rs by explicit lookup.
    let known: BTreeSet<&str> = [
        "ticket_inventory_recordings.ticket_id",
        "ticket_inventory_recordings.idempotency_key",
    ]
    .into_iter()
    .collect();
    let subtree = workspace_subtree(&pool).await;
    let loose: Vec<String> = sqlx::query_scalar(
        r#"
            SELECT c.table_name || '.' || c.column_name
              FROM information_schema.columns c
              JOIN information_schema.tables t
                ON t.table_name = c.table_name AND t.table_schema = 'public'
               AND t.table_type = 'BASE TABLE'
             WHERE c.table_schema = 'public' AND c.data_type = 'uuid'
               AND c.column_name <> 'id' AND c.table_name NOT LIKE 'sde\_%'
               AND NOT EXISTS (
                     SELECT 1 FROM pg_constraint k
                      WHERE k.contype = 'f' AND k.conrelid = c.table_name::regclass
                        AND (SELECT attnum FROM pg_attribute
                              WHERE attrelid = k.conrelid AND attname = c.column_name)
                            = ANY (k.conkey))
            "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let unexpected: Vec<&String> = loose
        .iter()
        .filter(|name| {
            let table = name.split('.').next().unwrap_or_default();
            subtree.contains(table) && !known.contains(name.as_str())
        })
        .collect();
    assert!(
        unexpected.is_empty(),
        "uuid columns without a foreign key in workspace data; make sure erase.rs \
             removes what they point at, then allow-list them here: {unexpected:#?}"
    );
}
