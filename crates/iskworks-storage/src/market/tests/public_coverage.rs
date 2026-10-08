//! App-wide public market data: `public_market_coverage` and the
//! `workspace_id IS NULL` observation batches it points at.

use super::common::*;

const FORGE: i64 = 10_000_002;

async fn insert_batch(
    pool: &PgPool,
    workspace_id: Option<Uuid>,
    price_source_id: Option<Uuid>,
    origin: &str,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    let now = crate::db_now();
    sqlx::query(
        r#"INSERT INTO market_observation_batches (
             id,workspace_id,price_source_id,origin,status,type_id,captured_type_name,
             region_id,solar_system_id,location_id,observed_at,attempted_at,completed_at
           ) VALUES ($1,$2,$3,$4,'completed',34,'Tritanium',$5,0,0,$6,$6,$6)"#,
    )
    .bind(id)
    .bind(workspace_id)
    .bind(price_source_id)
    .bind(origin)
    .bind(FORGE)
    .bind(now)
    .execute(pool)
    .await
    .map(|_| id)
}

async fn insert_observation(
    pool: &PgPool,
    workspace_id: Option<Uuid>,
    batch_id: Uuid,
) -> Result<(), sqlx::Error> {
    let now = crate::db_now();
    sqlx::query(
        r#"INSERT INTO market_order_observations (
             id,workspace_id,observation_batch_id,source_kind,observed_at,imported_at,
             order_id,type_id,captured_type_name,order_side,price,remaining_volume,
             entered_volume,minimum_volume,order_range,issued_at,duration_days,
             location_id,solar_system_id,region_id,jumps,normalized_row_checksum
           ) VALUES ($1,$2,$3,'esi_market_orders',$4,$4,1,34,'Tritanium','sell',4.25,
                     10,10,1,0,$4,90,60003760,30000142,$5,0,$6)"#,
    )
    .bind(Uuid::new_v4())
    .bind(workspace_id)
    .bind(batch_id)
    .bind(now)
    .bind(FORGE)
    .bind(format!("esi:{batch_id}:1"))
    .execute(pool)
    .await
    .map(|_| ())
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_public_batch_has_neither_workspace_nor_price_source(pool: PgPool) {
    let (workspace_id, _repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;

    let public = insert_batch(&pool, None, None, "esi_market_orders").await;
    assert!(public.is_ok(), "a public ESI batch is valid: {public:?}");
    insert_observation(&pool, None, public.unwrap())
        .await
        .expect("a public batch holds public observations");

    assert!(
        insert_batch(&pool, None, None, "eve_client_market_export")
            .await
            .is_err(),
        "imports always belong to a workspace"
    );
    assert!(
        insert_batch(&pool, None, Some(source_id.0), "esi_market_orders")
            .await
            .is_err(),
        "a batch with a price source belongs to that source's workspace"
    );
    let workspace_batch = insert_batch(
        &pool,
        Some(workspace_id.0),
        Some(source_id.0),
        "esi_market_orders",
    )
    .await
    .expect("structure-style workspace batches are unchanged");
    insert_observation(&pool, Some(workspace_id.0), workspace_batch)
        .await
        .expect("workspace observations are unchanged");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn an_observation_matches_its_batch_scope(pool: PgPool) {
    let (workspace_id, _repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    let public = insert_batch(&pool, None, None, "esi_market_orders")
        .await
        .unwrap();
    let workspace_batch = insert_batch(
        &pool,
        Some(workspace_id.0),
        Some(source_id.0),
        "esi_market_orders",
    )
    .await
    .unwrap();

    assert!(
        insert_observation(&pool, Some(workspace_id.0), public)
            .await
            .is_err(),
        "a workspace observation can't sit in a public batch"
    );
    assert!(
        insert_observation(&pool, None, workspace_batch)
            .await
            .is_err(),
        "a public observation can't sit in a workspace batch"
    );
}

// --- Lifecycle: register demand, due work, lease, complete/revalidate/fail ---

fn tritanium() -> Vec<MarketCoverageRegistration> {
    vec![MarketCoverageRegistration {
        type_id: 34,
        type_name: "Tritanium".to_string(),
    }]
}

fn public_batch(
    type_id: i64,
    observed_at: DateTime<Utc>,
    order_id: i64,
) -> PublicMarketObservationBatch {
    PublicMarketObservationBatch {
        id: MarketObservationBatchId::new(),
        type_id,
        type_name: "Tritanium".to_string(),
        region_id: FORGE,
        observed_at,
        etag: Some(format!("etag-{order_id}")),
        expires_at: Some(observed_at + chrono::Duration::minutes(5)),
        orders: vec![EsiMarketOrder::new(
            order_id,
            MarketOrderSide::Sell,
            "4.2500",
            100_000,
            100_000,
            1,
            "station".to_string(),
            observed_at,
            90,
            60_003_760,
            30_000_142,
            Some(60_003_760),
        )
        .unwrap()],
    }
}

async fn public_row_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM public_market_coverage")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Registers Tritanium, claims it and completes one 200 refresh observed at
/// `observed_at`. Returns `(repository, batch id)`.
async fn seed_public_book(
    pool: &PgPool,
    observed_at: DateTime<Utc>,
    order_id: i64,
) -> (PgMarketRepository, MarketObservationBatchId) {
    let repository = PgMarketRepository::new(pool.clone());
    repository
        .register_public_market_demand(FORGE, tritanium(), true, observed_at)
        .await
        .unwrap();
    let claim = repository
        .begin_public_market_refresh(
            FORGE,
            34,
            observed_at,
            observed_at + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .expect("a due row can be claimed");
    let batch = public_batch(34, observed_at, order_id);
    let batch_id = batch.id;
    assert!(repository
        .complete_public_market_refresh(claim, observed_at + chrono::Duration::minutes(15), batch)
        .await
        .unwrap());
    (repository, batch_id)
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn repeated_demand_shares_one_row_per_region_and_type(pool: PgPool) {
    let repository = PgMarketRepository::new(pool.clone());
    let t0 = crate::db_now() - chrono::Duration::days(3);
    let t1 = crate::db_now();
    repository
        .register_public_market_demand(FORGE, tritanium(), false, t0)
        .await
        .unwrap();
    repository
        .register_public_market_demand(FORGE, tritanium(), false, t1)
        .await
        .unwrap();

    assert_eq!(public_row_count(&pool).await, 1);
    let (state, last_needed_at): (String, DateTime<Utc>) = sqlx::query_as(
        "SELECT refresh_state,last_needed_at FROM public_market_coverage WHERE region_id=$1 AND type_id=34",
    )
    .bind(FORGE)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "missing");
    assert_eq!(
        last_needed_at, t1,
        "the latest demand keeps it out of dormancy"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn due_work_is_priority_first_then_oldest_and_skips_dormant_rows(pool: PgPool) {
    let repository = PgMarketRepository::new(pool.clone());
    let now = crate::db_now();
    let item = |type_id: i64, name: &str| MarketCoverageRegistration {
        type_id,
        type_name: name.to_string(),
    };
    repository
        .register_public_market_demand(
            FORGE,
            vec![item(34, "Tritanium"), item(35, "Pyerite")],
            false,
            now,
        )
        .await
        .unwrap();
    repository
        .register_public_market_demand(FORGE, vec![item(36, "Mexallon")], true, now)
        .await
        .unwrap();
    repository
        .register_public_market_demand(
            FORGE,
            vec![item(37, "Isogen")],
            false,
            now - chrono::Duration::days(8),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE public_market_coverage SET next_refresh_at=$1 WHERE type_id=35")
        .bind(now - chrono::Duration::hours(1))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE public_market_coverage SET next_refresh_at=$1 WHERE type_id=34")
        .bind(now - chrono::Duration::minutes(5))
        .execute(&pool)
        .await
        .unwrap();

    let due: Vec<(i64, i64)> = repository
        .due_public_market_work(now, 10)
        .await
        .unwrap()
        .into_iter()
        .map(|work| (work.region_id, work.item.type_id))
        .collect();

    assert_eq!(
        due,
        vec![(FORGE, 36), (FORGE, 35), (FORGE, 34)],
        "prioritized first, then most overdue; Isogen is dormant"
    );
    assert_eq!(
        repository
            .due_public_market_work(now, 1)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_lease_is_exclusive_until_it_expires_and_a_stale_claim_cannot_complete(pool: PgPool) {
    let repository = PgMarketRepository::new(pool.clone());
    let now = crate::db_now();
    repository
        .register_public_market_demand(FORGE, tritanium(), true, now)
        .await
        .unwrap();
    let first = repository
        .begin_public_market_refresh(FORGE, 34, now, now + chrono::Duration::minutes(2))
        .await
        .unwrap()
        .expect("claimable");
    assert!(
        repository
            .begin_public_market_refresh(
                FORGE,
                34,
                now + chrono::Duration::seconds(30),
                now + chrono::Duration::minutes(3)
            )
            .await
            .unwrap()
            .is_none(),
        "a live lease blocks a second claim"
    );
    assert!(repository
        .due_public_market_work(now, 10)
        .await
        .unwrap()
        .is_empty());

    let later = now + chrono::Duration::minutes(5);
    let second = repository
        .begin_public_market_refresh(FORGE, 34, later, later + chrono::Duration::minutes(2))
        .await
        .unwrap()
        .expect("an expired lease is reclaimable");
    assert!(
        !repository
            .complete_public_market_refresh(first, later, public_batch(34, now, 1))
            .await
            .unwrap(),
        "the stale claim can't complete"
    );
    assert!(repository
        .complete_public_market_refresh(
            second,
            later + chrono::Duration::minutes(15),
            public_batch(34, later, 2)
        )
        .await
        .unwrap());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn completing_stores_a_public_book_and_collects_the_one_it_supersedes(pool: PgPool) {
    let t1 = crate::db_now() - chrono::Duration::hours(1);
    let (repository, first_batch) = seed_public_book(&pool, t1, 1).await;
    let (workspace_id, price_source_id, state, revalidated_at): (
        Option<Uuid>,
        Option<Uuid>,
        String,
        Option<DateTime<Utc>>,
    ) = sqlx::query_as(
        r#"SELECT b.workspace_id,b.price_source_id,c.refresh_state,c.revalidated_at
               FROM public_market_coverage c
               JOIN market_observation_batches b ON b.id=c.last_completed_batch_id
               WHERE c.region_id=$1 AND c.type_id=34"#,
    )
    .bind(FORGE)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (workspace_id, price_source_id),
        (None, None),
        "stored app-wide"
    );
    assert_eq!(state, "current");
    assert_eq!(revalidated_at, Some(t1));
    let observations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM market_order_observations WHERE observation_batch_id=$1 AND workspace_id IS NULL",
    )
    .bind(first_batch.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(observations, 1);

    let t2 = crate::db_now();
    sqlx::query("UPDATE public_market_coverage SET next_refresh_at=$1")
        .bind(t2)
        .execute(&pool)
        .await
        .unwrap();
    let claim = repository
        .begin_public_market_refresh(FORGE, 34, t2, t2 + chrono::Duration::minutes(2))
        .await
        .unwrap()
        .unwrap();
    assert!(repository
        .complete_public_market_refresh(
            claim,
            t2 + chrono::Duration::minutes(15),
            public_batch(34, t2, 2)
        )
        .await
        .unwrap());
    let old_left: i64 =
        sqlx::query_scalar("SELECT count(*) FROM market_observation_batches WHERE id=$1")
            .bind(first_batch.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(old_left, 0, "the superseded public batch is collected");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_304_keeps_the_book_and_failures_back_off(pool: PgPool) {
    let t1 = crate::db_now() - chrono::Duration::hours(1);
    let (repository, batch_id) = seed_public_book(&pool, t1, 1).await;
    let now = crate::db_now();
    sqlx::query("UPDATE public_market_coverage SET next_refresh_at=$1")
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

    let claim = repository
        .begin_public_market_refresh(FORGE, 34, now, now + chrono::Duration::minutes(2))
        .await
        .unwrap()
        .unwrap();
    assert!(repository
        .revalidate_public_market_refresh(
            FORGE,
            34,
            claim,
            now + chrono::Duration::minutes(15),
            None
        )
        .await
        .unwrap());
    let (state, last_batch, revalidated_at): (String, Option<Uuid>, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT refresh_state,last_completed_batch_id,revalidated_at FROM public_market_coverage",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "current");
    assert_eq!(last_batch, Some(batch_id.0), "a 304 keeps the book");
    assert!(revalidated_at.unwrap() > t1);

    let later = now + chrono::Duration::minutes(20);
    for attempt in 0..2 {
        sqlx::query("UPDATE public_market_coverage SET next_refresh_at=$1")
            .bind(later)
            .execute(&pool)
            .await
            .unwrap();
        let claim = repository
            .begin_public_market_refresh(FORGE, 34, later, later + chrono::Duration::minutes(2))
            .await
            .unwrap()
            .unwrap();
        assert!(repository
            .fail_public_market_refresh(FORGE, 34, claim, later, later, format!("boom {attempt}"))
            .await
            .unwrap());
    }
    let (state, failures, next_refresh_at): (String, i32, DateTime<Utc>) = sqlx::query_as(
        "SELECT refresh_state,consecutive_failures,next_refresh_at FROM public_market_coverage",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "failed");
    assert_eq!(failures, 2);
    assert_eq!(
        next_refresh_at,
        later + chrono::Duration::minutes(2),
        "second failure waits 2 minutes"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn pruning_keeps_the_current_public_book_and_collects_it_once_superseded(pool: PgPool) {
    let old = crate::db_now() - chrono::Duration::days(2);
    let (repository, current) = seed_public_book(&pool, old, 1).await;
    let batch_exists = |id: Uuid| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM market_observation_batches WHERE id=$1",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap()
                == 1
        }
    };

    repository
        .prune_orphaned_market_observations(crate::db_now() - chrono::Duration::hours(1), 500, 10)
        .await
        .unwrap();
    assert!(
        batch_exists(current.0).await,
        "an old but current public book is not an orphan"
    );

    // A later book replaces it.
    sqlx::query("UPDATE public_market_coverage SET next_refresh_at=$1")
        .bind(crate::db_now())
        .execute(&pool)
        .await
        .unwrap();
    let now = crate::db_now();
    let claim = repository
        .begin_public_market_refresh(FORGE, 34, now, now + chrono::Duration::minutes(2))
        .await
        .unwrap()
        .unwrap();
    assert!(repository
        .complete_public_market_refresh(
            claim,
            now + chrono::Duration::minutes(15),
            public_batch(34, now, 2)
        )
        .await
        .unwrap());
    assert!(
        !batch_exists(current.0).await,
        "completion collects the public book it supersedes"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn prioritizing_makes_existing_rows_due_now_and_leaves_leased_ones(pool: PgPool) {
    let repository = PgMarketRepository::new(pool.clone());
    let now = crate::db_now();
    let item = |type_id: i64, name: &str| MarketCoverageRegistration {
        type_id,
        type_name: name.to_string(),
    };
    repository
        .register_public_market_demand(
            FORGE,
            vec![item(34, "Tritanium"), item(35, "Pyerite")],
            false,
            now,
        )
        .await
        .unwrap();
    sqlx::query("UPDATE public_market_coverage SET next_refresh_at=$1")
        .bind(now + chrono::Duration::minutes(10))
        .execute(&pool)
        .await
        .unwrap();
    assert!(repository
        .due_public_market_work(now, 10)
        .await
        .unwrap()
        .is_empty());

    assert!(repository
        .prioritize_public_market_refresh(FORGE, &[34, 999], now)
        .await
        .unwrap());

    let due: Vec<i64> = repository
        .due_public_market_work(now, 10)
        .await
        .unwrap()
        .into_iter()
        .map(|work| work.item.type_id)
        .collect();
    assert_eq!(
        due,
        vec![34],
        "only the existing, prioritized row; unknown ids are ignored"
    );
    assert!(
        !repository
            .prioritize_public_market_refresh(FORGE, &[999], now)
            .await
            .unwrap(),
        "nothing to prioritize"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn prioritizing_never_refetches_before_esi_expires_or_a_failure_backs_off(pool: PgPool) {
    // ESI's book expires at t0 + 5 minutes (`public_batch`).
    let t0 = crate::db_now();
    let (repository, _) = seed_public_book(&pool, t0, 1).await;
    let expires = t0 + chrono::Duration::minutes(5);
    let next_refresh_at = || async {
        sqlx::query_scalar::<_, DateTime<Utc>>("SELECT next_refresh_at FROM public_market_coverage")
            .fetch_one(&pool)
            .await
            .unwrap()
    };

    let clicked = t0 + chrono::Duration::minutes(1);
    assert!(repository
        .prioritize_public_market_refresh(FORGE, &[34], clicked)
        .await
        .unwrap());
    assert_eq!(
        next_refresh_at().await,
        expires,
        "first in line, once ESI's cache expires"
    );
    repository
        .register_public_market_demand(FORGE, tritanium(), true, clicked)
        .await
        .unwrap();
    assert_eq!(
        next_refresh_at().await,
        expires,
        "demand prioritizes the same way"
    );
    assert!(repository
        .due_public_market_work(clicked, 10)
        .await
        .unwrap()
        .is_empty());

    // A 304 carries ESI's new expiry.
    let claim = repository
        .begin_public_market_refresh(FORGE, 34, expires, expires + chrono::Duration::minutes(2))
        .await
        .unwrap()
        .unwrap();
    let revalidated_expires = expires + chrono::Duration::minutes(5);
    assert!(repository
        .revalidate_public_market_refresh(
            FORGE,
            34,
            claim,
            expires + chrono::Duration::minutes(15),
            Some(revalidated_expires),
        )
        .await
        .unwrap());
    assert!(repository
        .prioritize_public_market_refresh(FORGE, &[34], expires)
        .await
        .unwrap());
    assert_eq!(next_refresh_at().await, revalidated_expires);

    // A failure's backoff (here a server-directed 10-minute wait) holds too.
    let failed_at = revalidated_expires;
    let claim = repository
        .begin_public_market_refresh(
            FORGE,
            34,
            failed_at,
            failed_at + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    let retry_after = failed_at + chrono::Duration::minutes(10);
    assert!(repository
        .fail_public_market_refresh(FORGE, 34, claim, failed_at, retry_after, "429".to_string())
        .await
        .unwrap());
    assert!(repository
        .prioritize_public_market_refresh(FORGE, &[34], failed_at)
        .await
        .unwrap());
    assert_eq!(next_refresh_at().await, retry_after);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_refresh_is_never_scheduled_before_esi_expires(pool: PgPool) {
    let t0 = crate::db_now();
    let repository = PgMarketRepository::new(pool.clone());
    repository
        .register_public_market_demand(FORGE, tritanium(), true, t0)
        .await
        .unwrap();
    let claim = repository
        .begin_public_market_refresh(FORGE, 34, t0, t0 + chrono::Duration::minutes(2))
        .await
        .unwrap()
        .unwrap();
    let mut batch = public_batch(34, t0, 1);
    let expires = t0 + chrono::Duration::minutes(30);
    batch.expires_at = Some(expires);
    assert!(repository
        .complete_public_market_refresh(claim, t0 + chrono::Duration::minutes(15), batch)
        .await
        .unwrap());

    let next: DateTime<Utc> =
        sqlx::query_scalar("SELECT next_refresh_at FROM public_market_coverage")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(next, expires);
}

// --- Reads: public scopes read the app-wide book ---

const JITA_44: i64 = 60_003_760;
const JITA_OTHER: i64 = 60_003_761;

/// Marks Jita 4-4 as an NPC station in the fixture's active SDE, so it
/// classifies as a public scope.
async fn jita_is_an_npc_station(pool: &PgPool) {
    for sql in [
        "INSERT INTO sde_regions (import_id,region_id,name_en) SELECT id,10000002,'The Forge' FROM sde_imports WHERE active",
        "INSERT INTO sde_solar_systems (import_id,solar_system_id,name_en,region_id) SELECT id,30000142,'Jita',10000002 FROM sde_imports WHERE active",
        "INSERT INTO sde_npc_stations (import_id,station_id,name_en,solar_system_id,owner_corporation_id,station_type_id) SELECT id,60003760,'Jita IV - Moon 4 - Caldari Navy Assembly Plant',30000142,1000035,1529 FROM sde_imports WHERE active",
    ] {
        sqlx::query(sql).execute(pool).await.unwrap();
    }
}

/// A public Forge Tritanium book with one order at Jita 4-4 and one at
/// another station in the region.
async fn seed_two_station_public_book(pool: &PgPool) -> MarketObservationBatchId {
    let repository = PgMarketRepository::new(pool.clone());
    let now = crate::db_now();
    repository
        .register_public_market_demand(FORGE, tritanium(), true, now)
        .await
        .unwrap();
    let claim = repository
        .begin_public_market_refresh(FORGE, 34, now, now + chrono::Duration::minutes(2))
        .await
        .unwrap()
        .unwrap();
    let order = |order_id: i64, location_id: i64| {
        EsiMarketOrder::new(
            order_id,
            MarketOrderSide::Sell,
            "4.2500",
            100,
            100,
            1,
            "station".to_string(),
            now,
            90,
            location_id,
            30_000_142,
            None,
        )
        .unwrap()
    };
    let batch = PublicMarketObservationBatch {
        id: MarketObservationBatchId::new(),
        type_id: 34,
        type_name: "Tritanium".to_string(),
        region_id: FORGE,
        observed_at: now,
        etag: None,
        expires_at: None,
        orders: vec![order(1, JITA_44), order(2, JITA_OTHER)],
    };
    let batch_id = batch.id;
    assert!(repository
        .complete_public_market_refresh(claim, now + chrono::Duration::minutes(15), batch)
        .await
        .unwrap());
    batch_id
}

fn order_ids(
    books: &std::collections::BTreeMap<i64, Vec<iskworks_core::MarketOrderView>>,
) -> Vec<i64> {
    let mut ids: Vec<i64> = books
        .get(&34)
        .map(|orders| orders.iter().map(|order| order.order_id).collect())
        .unwrap_or_default();
    ids.sort_unstable();
    ids
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_fresh_workspace_reads_the_public_book_for_stations_and_region_wide_scopes(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    jita_is_an_npc_station(&pool).await;
    seed_two_station_public_book(&pool).await;

    let station = repository
        .scoped_order_books(
            workspace_id,
            iskworks_core::MarketScope {
                region_id: FORGE,
                location_id: Some(JITA_44),
            },
            &[34],
        )
        .await
        .unwrap();
    assert_eq!(
        order_ids(&station),
        vec![1],
        "a station scope reads its own orders from the regional book, with no price source of its own"
    );

    let region_wide = repository
        .scoped_order_books(
            workspace_id,
            iskworks_core::MarketScope {
                region_id: FORGE,
                location_id: None,
            },
            &[34],
        )
        .await
        .unwrap();
    assert_eq!(order_ids(&region_wide), vec![1, 2]);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_public_price_source_reads_the_app_wide_book(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    jita_is_an_npc_station(&pool).await;
    let batch_id = seed_two_station_public_book(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;

    let books = repository
        .get_source_order_books(workspace_id, source_id, &[34], JITA_44, None)
        .await
        .unwrap();
    let book = books
        .get(&34)
        .expect("the public book serves the workspace's Jita source");
    assert_eq!(book.observation_batch_id, batch_id);
    assert_eq!(book.location_id, JITA_44);
    assert_eq!(
        book.orders
            .iter()
            .map(|order| order.order_id)
            .collect::<Vec<_>>(),
        vec![1],
        "filtered to the source's station"
    );

    let single = repository
        .get_source_order_book(workspace_id, source_id, 34, JITA_44, None)
        .await
        .unwrap();
    assert_eq!(single.observation_batch_id, batch_id);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_region_wide_read_adds_the_workspaces_structure_books_without_double_counting(
    pool: PgPool,
) {
    let (workspace_id, repository) = fixture(&pool).await;
    jita_is_an_npc_station(&pool).await;
    seed_two_station_public_book(&pool).await;
    // The workspace's own Jita source (public: served by the regional book,
    // not merged again) and a structure source in the same region.
    esi_source(&pool, workspace_id).await;
    let structure = esi_source_at(&pool, workspace_id, 1_035_466_617_946, 30_000_142, FORGE).await;
    let now = crate::db_now();
    repository
        .register_market_coverage(workspace_id, structure, tritanium())
        .await
        .unwrap();
    let claim = repository
        .begin_market_refresh(
            workspace_id,
            structure,
            34,
            now,
            now + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(repository
        .complete_esi_market_refresh(
            workspace_id,
            claim,
            now + chrono::Duration::minutes(15),
            iskworks_core::EsiMarketObservationBatch {
                id: MarketObservationBatchId::new(),
                source_id: structure,
                type_id: 34,
                type_name: "Tritanium".to_string(),
                region_id: FORGE,
                solar_system_id: 30_000_142,
                location_id: 1_035_466_617_946,
                observed_at: now,
                etag: None,
                expires_at: None,
                orders: vec![EsiMarketOrder::new(
                    77,
                    MarketOrderSide::Sell,
                    "4.1000",
                    10,
                    10,
                    1,
                    "station".to_string(),
                    now,
                    90,
                    1_035_466_617_946,
                    30_000_142,
                    Some(1_035_466_617_946),
                )
                .unwrap()],
            },
        )
        .await
        .unwrap());

    let region_wide = repository
        .scoped_order_books(
            workspace_id,
            iskworks_core::MarketScope {
                region_id: FORGE,
                location_id: None,
            },
            &[34],
        )
        .await
        .unwrap();
    assert_eq!(
        order_ids(&region_wide),
        vec![1, 2, 77],
        "the public regional book once, plus the workspace's structure book"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn freshness_and_evidence_for_a_public_scope_come_from_the_app_wide_book(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    jita_is_an_npc_station(&pool).await;
    let batch_id = seed_two_station_public_book(&pool).await;
    let observed_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT observed_at FROM market_observation_batches WHERE id=$1")
            .bind(batch_id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    let jita = iskworks_core::MarketScope {
        region_id: FORGE,
        location_id: Some(JITA_44),
    };

    assert_eq!(
        repository
            .item_freshness(workspace_id, jita, &[34])
            .await
            .unwrap(),
        Some(observed_at)
    );
    let scopes = repository
        .scope_freshness(workspace_id, &[(FORGE, JITA_44), (FORGE, 0)])
        .await
        .unwrap();
    for key in [(FORGE, JITA_44), (FORGE, 0)] {
        let freshness = scopes.get(&key).expect("public scope freshness");
        assert_eq!(
            (freshness.tracked_type_count, freshness.observed_type_count),
            (1, 1),
            "{key:?}"
        );
        assert_eq!(freshness.most_recent_observed_at, Some(observed_at));
    }

    let evidence = repository
        .resolve_scope_evidence(workspace_id, jita)
        .await
        .unwrap();
    assert_eq!(evidence.observation_batch_ids, vec![batch_id]);
    assert_eq!(evidence.observed_at, Some(observed_at));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn public_coverage_reports_each_requested_rows_state(pool: PgPool) {
    let (repository, _batch) = seed_public_book(&pool, crate::db_now(), 1).await;
    repository
        .register_public_market_demand(
            FORGE,
            vec![MarketCoverageRegistration {
                type_id: 35,
                type_name: "Pyerite".to_string(),
            }],
            false,
            crate::db_now(),
        )
        .await
        .unwrap();

    let coverage = repository
        .public_market_coverage(FORGE, &[34, 35, 999])
        .await
        .unwrap();

    assert_eq!(
        coverage
            .iter()
            .map(|item| (item.type_id, item.refresh_state, item.order_count))
            .collect::<Vec<_>>(),
        vec![
            (34, MarketRefreshState::Current, 1),
            (35, MarketRefreshState::Missing, 0)
        ]
    );
}

// --- Cutover migration (202610060004) ---

const CUTOVER_SQL: &str =
    include_str!("../../../../../migrations/202610060004_public_market_coverage_cutover.sql");

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn the_cutover_moves_public_demand_app_wide_and_keeps_structures(pool: PgPool) {
    let (workspace_a, repository) = fixture(&pool).await;
    jita_is_an_npc_station(&pool).await;
    let structure_location = 1_035_466_617_946;
    let jita = esi_source(&pool, workspace_a).await;
    let region_wide = esi_source_at(&pool, workspace_a, 0, 0, 10_000_043).await;
    let structure = esi_source_at(&pool, workspace_a, structure_location, 30_000_142, FORGE).await;
    let item = |type_id: i64, name: &str| MarketCoverageRegistration {
        type_id,
        type_name: name.to_string(),
    };
    for (source, items) in [
        (jita, vec![item(34, "Tritanium"), item(35, "Pyerite")]),
        (region_wide, vec![item(34, "Tritanium")]),
        (structure, vec![item(34, "Tritanium")]),
    ] {
        repository
            .register_market_coverage(workspace_a, source, items)
            .await
            .unwrap();
    }
    // Pyerite was already filled app-wide by the dual registration.
    let earlier = crate::db_now() - chrono::Duration::days(1);
    repository
        .register_public_market_demand(FORGE, vec![item(35, "Pyerite")], false, earlier)
        .await
        .unwrap();
    sqlx::query("UPDATE market_source_coverage SET last_needed_at=$1 WHERE type_id=35")
        .bind(crate::db_now())
        .execute(&pool)
        .await
        .unwrap();

    sqlx::raw_sql(CUTOVER_SQL).execute(&pool).await.unwrap();

    let rows: Vec<(i64, i64, String, bool)> = sqlx::query_as(
        r#"SELECT region_id,type_id,refresh_state,priority_requested_at IS NOT NULL
           FROM public_market_coverage ORDER BY region_id,type_id"#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            (FORGE, 34, "missing".to_string(), true),
            (FORGE, 35, "missing".to_string(), false),
            (10_000_043, 34, "missing".to_string(), true),
        ],
        "one row per public (region, type); an existing row keeps its own state and priority"
    );
    let pyerite_needed: DateTime<Utc> = sqlx::query_scalar(
        "SELECT last_needed_at FROM public_market_coverage WHERE region_id=$1 AND type_id=35",
    )
    .bind(FORGE)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(pyerite_needed > earlier, "the latest demand carries over");

    let remaining: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT location_id,type_id FROM market_source_coverage ORDER BY location_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        remaining,
        vec![(structure_location, 34)],
        "only structure coverage stays per workspace"
    );

    sqlx::raw_sql(CUTOVER_SQL).execute(&pool).await.unwrap();
    assert_eq!(public_row_count(&pool).await, 3, "re-running is a no-op");
}
