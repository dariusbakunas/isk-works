use super::common::*;

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn superseding_a_batch_prunes_its_now_unreferenced_observations(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    repository
        .register_market_coverage(
            workspace_id,
            source_id,
            vec![MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }],
        )
        .await
        .unwrap();
    let count = |sql: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(sql)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };

    let t0 = crate::db_now();
    let claim_one = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            t0,
            t0 + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    let first = esi_batch_with_two_orders(source_id, 34, t0, "4.2500", "4.0000");
    let first_batch_id = first.id;
    repository
        .complete_esi_market_refresh(
            workspace_id,
            claim_one,
            t0 + chrono::Duration::minutes(15),
            first,
        )
        .await
        .unwrap();
    assert_eq!(
        count("SELECT count(*) FROM market_observation_batches").await,
        1
    );
    assert_eq!(
        count("SELECT count(*) FROM market_order_observations").await,
        2
    );

    let t1 = t0 + chrono::Duration::minutes(20);
    let claim_two = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            t1,
            t1 + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    let second = esi_batch_with_two_orders(source_id, 34, t1, "4.1000", "3.9000");
    let second_batch_id = second.id;
    repository
        .complete_esi_market_refresh(
            workspace_id,
            claim_two,
            t1 + chrono::Duration::minutes(15),
            second,
        )
        .await
        .unwrap();

    // The superseded batch and its observations are gone; only the
    // current batch's rows remain.
    assert_eq!(
        count("SELECT count(*) FROM market_observation_batches").await,
        1
    );
    assert_eq!(
        count("SELECT count(*) FROM market_order_observations").await,
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM market_observation_batches WHERE id = $1"
        )
        .bind(first_batch_id.0)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
    );

    // The order book still resolves -- to the new batch.
    let book = repository
        .get_source_order_book(workspace_id, source_id, 34, 60_003_760, None)
        .await
        .unwrap();
    assert_eq!(book.observation_batch_id, second_batch_id);
    assert_eq!(book.lowest_sell, Some(Money::parse("4.1000").unwrap()));
    assert_eq!(book.highest_buy, Some(Money::parse("3.9000").unwrap()));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn prune_orphaned_market_observations_clears_the_backlog_but_keeps_live_and_recent(
    pool: PgPool,
) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    repository
        .register_market_coverage(
            workspace_id,
            source_id,
            vec![MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }],
        )
        .await
        .unwrap();
    let now = crate::db_now();

    // The current batch for type 34 -- must never be pruned.
    let claim = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            now,
            now + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    let current = esi_batch_with_two_orders(source_id, 34, now, "4.2500", "4.0000");
    let current_batch_id = current.id;
    repository
        .complete_esi_market_refresh(
            workspace_id,
            claim,
            now + chrono::Duration::minutes(15),
            current,
        )
        .await
        .unwrap();

    // Backlog: three old orphaned batches (5 observations total).
    let old = now - chrono::Duration::hours(6);
    let orphan_a = insert_orphan_batch(&pool, workspace_id, source_id, old, 2).await;
    let orphan_b = insert_orphan_batch(&pool, workspace_id, source_id, old, 2).await;
    let orphan_c = insert_orphan_batch(&pool, workspace_id, source_id, old, 1).await;
    // An orphan that is too recent to touch (inside the grace window).
    let recent_orphan = insert_orphan_batch(
        &pool,
        workspace_id,
        source_id,
        now - chrono::Duration::minutes(5),
        1,
    )
    .await;

    // First pass: 1 orphan batch per chunk, 2 chunks -> clears 2 of the
    // 3 old orphan batches, reports it isn't finished.
    let partial = repository
        .prune_orphaned_market_observations(now - chrono::Duration::hours(1), 1, 2)
        .await
        .unwrap();
    assert_eq!(partial.chunks_run, 2);
    assert_eq!(partial.batches_deleted, 2);
    assert!(!partial.drained);

    // Second pass finishes the last orphan batch.
    let rest = repository
        .prune_orphaned_market_observations(now - chrono::Duration::hours(1), 500, 5_000)
        .await
        .unwrap();
    assert!(rest.drained);
    // 5 orphan observations and 3 orphan batches across the two passes.
    assert_eq!(partial.observations_deleted + rest.observations_deleted, 5);
    assert_eq!(partial.batches_deleted + rest.batches_deleted, 3);

    for gone in [orphan_a, orphan_b, orphan_c] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM market_observation_batches WHERE id = $1"
            )
            .bind(gone)
            .fetch_one(&pool)
            .await
            .unwrap(),
            0,
        );
    }
    // The current batch and the within-grace orphan survive.
    for kept in [current_batch_id.0, recent_orphan] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM market_observation_batches WHERE id = $1"
            )
            .bind(kept)
            .fetch_one(&pool)
            .await
            .unwrap(),
            1,
        );
    }

    // A no-op pass once the backlog is clear.
    let idle = repository
        .prune_orphaned_market_observations(now - chrono::Duration::hours(1), 20_000, 5_000)
        .await
        .unwrap();
    assert_eq!(idle.observations_deleted, 0);
    assert_eq!(idle.batches_deleted, 0);
}

fn esi_batch_with_two_orders(
    source_id: PriceSourceId,
    type_id: i64,
    observed_at: DateTime<Utc>,
    sell: &str,
    buy: &str,
) -> EsiMarketObservationBatch {
    EsiMarketObservationBatch {
        id: MarketObservationBatchId::new(),
        source_id,
        type_id,
        type_name: "Tritanium".to_string(),
        region_id: 10_000_002,
        solar_system_id: 30_000_142,
        location_id: 60_003_760,
        observed_at,
        etag: None,
        expires_at: None,
        orders: vec![
            EsiMarketOrder::new(
                101,
                MarketOrderSide::Sell,
                sell,
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
            .unwrap(),
            EsiMarketOrder::new(
                102,
                MarketOrderSide::Buy,
                buy,
                75_000,
                75_000,
                1,
                "region".to_string(),
                observed_at,
                90,
                60_003_760,
                30_000_142,
                Some(60_003_760),
            )
            .unwrap(),
        ],
    }
}

async fn insert_orphan_batch(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    source_id: PriceSourceId,
    observed_at: DateTime<Utc>,
    order_count: i64,
) -> Uuid {
    let batch_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO market_observation_batches (
          id,workspace_id,price_source_id,origin,status,type_id,captured_type_name,
          region_id,solar_system_id,location_id,observed_at,attempted_at,completed_at,etag,expires_at
        ) VALUES ($1,$2,$3,'esi_market_orders','completed',34,'Tritanium',
                  10000002,30000142,60003760,$4,$4,$4,NULL,NULL)
        "#,
    )
    .bind(batch_id)
    .bind(workspace_id.0)
    .bind(source_id.0)
    .bind(observed_at)
    .execute(pool)
    .await
    .unwrap();
    for order_id in 1..=order_count {
        sqlx::query(
            r#"
            INSERT INTO market_order_observations (
              id,workspace_id,observation_batch_id,source_kind,observed_at,imported_at,
              order_id,type_id,captured_type_name,order_side,price,remaining_volume,
              entered_volume,minimum_volume,order_range,issued_at,duration_days,
              location_id,solar_system_id,region_id,jumps,normalized_row_checksum
            ) VALUES ($1,$2,$3,'esi_market_orders',$4,$4,$5,34,'Tritanium','sell',
                      '4.5000',10,10,1,-1,$4,90,60003760,30000142,10000002,0,$6)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(workspace_id.0)
        .bind(batch_id)
        .bind(observed_at)
        .bind(order_id)
        .bind(format!("orphan:{batch_id}:{order_id}"))
        .execute(pool)
        .await
        .unwrap();
    }
    batch_id
}
