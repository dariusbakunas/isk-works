use super::common::*;

/// `(observed_at, completed_at, etag, expires_at)` -- the immutable
/// `market_observation_batches` columns a 304 revalidation must not touch.
type BatchCacheColumns = (
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<String>,
    Option<DateTime<Utc>>,
);

/// Registers type 34 and drives one successful 200 refresh whose batch is
/// stamped `observed_at = observed_at`. Returns the completed batch id.
async fn seed_current_esi_book(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    source_id: PriceSourceId,
    location_id: i64,
    solar_system_id: i64,
    region_id: i64,
    observed_at: DateTime<Utc>,
) -> MarketObservationBatchId {
    let repository = PgMarketRepository::new(pool.clone());
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
    let claim = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            observed_at,
            observed_at + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    let batch_id = MarketObservationBatchId::new();
    repository
        .complete_esi_market_refresh(
            workspace_id,
            claim,
            observed_at + chrono::Duration::minutes(15),
            EsiMarketObservationBatch {
                id: batch_id,
                source_id,
                type_id: 34,
                type_name: "Tritanium".to_string(),
                region_id,
                solar_system_id,
                location_id,
                observed_at,
                etag: Some("seed-etag".to_string()),
                expires_at: Some(observed_at + chrono::Duration::minutes(5)),
                orders: vec![EsiMarketOrder::new(
                    101,
                    MarketOrderSide::Sell,
                    "4.2500",
                    100_000,
                    100_000,
                    1,
                    "station".to_string(),
                    observed_at,
                    90,
                    location_id,
                    solar_system_id,
                    Some(location_id),
                )
                .unwrap()],
            },
        )
        .await
        .unwrap();
    batch_id
}

async fn coverage_revalidated_at(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    source_id: PriceSourceId,
    type_id: i64,
) -> Option<DateTime<Utc>> {
    sqlx::query_scalar(
        "SELECT revalidated_at FROM market_source_coverage WHERE workspace_id=$1 AND price_source_id=$2 AND type_id=$3",
    )
    .bind(workspace_id.0)
    .bind(source_id.0)
    .bind(type_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn complete_esi_market_refresh_sets_revalidated_at_to_observed_at(pool: PgPool) {
    let (workspace_id, _repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    let t1 = crate::db_now() - chrono::Duration::hours(2);
    let batch_id = seed_current_esi_book(
        &pool,
        workspace_id,
        source_id,
        60_003_760,
        30_000_142,
        10_000_002,
        t1,
    )
    .await;

    let batch_observed_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT observed_at FROM market_observation_batches WHERE id=$1")
            .bind(batch_id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    let revalidated_at = coverage_revalidated_at(&pool, workspace_id, source_id, 34)
        .await
        .expect("a completed row must carry revalidated_at");

    assert_eq!(batch_observed_at, t1);
    assert_eq!(
        revalidated_at, t1,
        "200 sets revalidated_at = batch.observed_at exactly"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn revalidate_advances_only_revalidated_at(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    let t1 = crate::db_now() - chrono::Duration::hours(2);
    let batch_id = seed_current_esi_book(
        &pool,
        workspace_id,
        source_id,
        60_003_760,
        30_000_142,
        10_000_002,
        t1,
    )
    .await;

    let batch_before: BatchCacheColumns = sqlx::query_as(
        "SELECT observed_at,completed_at,etag,expires_at FROM market_observation_batches WHERE id=$1",
    )
    .bind(batch_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    let orders_before: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT id,remaining_volume FROM market_order_observations WHERE observation_batch_id=$1 ORDER BY id",
    )
    .bind(batch_id.0)
    .fetch_all(&pool)
    .await
    .unwrap();

    let retry_at = t1 + chrono::Duration::hours(1);
    let retry_claim = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            retry_at,
            retry_at + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    let wall_before = crate::db_now();
    assert!(repository
        .revalidate_esi_market_refresh(
            workspace_id,
            source_id,
            34,
            retry_claim,
            retry_at + chrono::Duration::minutes(15),
            None,
        )
        .await
        .unwrap());

    // batch.observed_at + last_completed_batch_id + batch/order rows unchanged.
    let batch_after: BatchCacheColumns = sqlx::query_as(
        "SELECT observed_at,completed_at,etag,expires_at FROM market_observation_batches WHERE id=$1",
    )
    .bind(batch_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(batch_after, batch_before);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM market_observation_batches WHERE workspace_id=$1"
        )
        .bind(workspace_id.0)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1,
    );
    let orders_after: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT id,remaining_volume FROM market_order_observations WHERE observation_batch_id=$1 ORDER BY id",
    )
    .bind(batch_id.0)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(orders_after, orders_before);
    let (batch_ptr, _lease, _priority): (Uuid, Option<DateTime<Utc>>, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT last_completed_batch_id,lease_expires_at,priority_requested_at FROM market_source_coverage WHERE workspace_id=$1 AND price_source_id=$2 AND type_id=34",
    ).bind(workspace_id.0).bind(source_id.0).fetch_one(&pool).await.unwrap();
    assert_eq!(batch_ptr, batch_id.0);

    // Only revalidated_at advanced.
    let revalidated_at = coverage_revalidated_at(&pool, workspace_id, source_id, 34)
        .await
        .unwrap();
    assert!(
        revalidated_at >= wall_before,
        "revalidated_at advanced to the 304 time"
    );
    assert!(revalidated_at > t1);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn fail_market_refresh_preserves_revalidated_at(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    let t1 = crate::db_now() - chrono::Duration::hours(2);
    seed_current_esi_book(
        &pool,
        workspace_id,
        source_id,
        60_003_760,
        30_000_142,
        10_000_002,
        t1,
    )
    .await;

    // 304 at T2.
    let t2_claim = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            t1 + chrono::Duration::hours(1),
            t1 + chrono::Duration::hours(1) + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .revalidate_esi_market_refresh(
            workspace_id,
            source_id,
            34,
            t2_claim,
            t1 + chrono::Duration::hours(1) + chrono::Duration::minutes(15),
            None,
        )
        .await
        .unwrap();
    let revalidated_after_304 = coverage_revalidated_at(&pool, workspace_id, source_id, 34)
        .await
        .unwrap();

    // Failure at T3.
    let t3 = t1 + chrono::Duration::hours(2);
    let t3_claim = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            t3,
            t3 + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(repository
        .fail_market_refresh(
            workspace_id,
            source_id,
            MarketRefreshFailure {
                type_id: 34,
                claim: t3_claim,
                attempted_at: t3,
                next_refresh_at: t3 + chrono::Duration::minutes(1),
                error_message: "temporary ESI failure".to_string(),
            },
        )
        .await
        .unwrap());

    let coverage = load_coverage(&pool, workspace_id, source_id).await.unwrap();
    assert_eq!(coverage[0].refresh_state, MarketRefreshState::Failed);
    assert!(coverage[0].last_error.is_some());
    assert_eq!(
        coverage_revalidated_at(&pool, workspace_id, source_id, 34).await,
        Some(revalidated_after_304),
        "a failed attempt must not erase the last successful confirmation",
    );
}

/// Coverage that keeps failing backs off exponentially (1, 2, 4... minutes,
/// capped) instead of hitting ESI every minute forever -- each failure spends
/// the per-IP error budget every tenant shares. A success resets it.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn repeated_failures_back_off_exponentially_until_a_success(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    let t0 = crate::db_now() - chrono::Duration::days(2);
    seed_current_esi_book(
        &pool,
        workspace_id,
        source_id,
        60_003_760,
        30_000_142,
        10_000_002,
        t0,
    )
    .await;

    let next_refresh_at = || async {
        sqlx::query_scalar::<_, DateTime<Utc>>(
            "SELECT next_refresh_at FROM market_source_coverage WHERE workspace_id=$1 AND price_source_id=$2 AND type_id=34",
        )
        .bind(workspace_id.0)
        .bind(source_id.0)
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    let fail_at = |attempted_at: DateTime<Utc>| {
        let repository = &repository;
        async move {
            let claim = repository
                .begin_market_refresh(
                    workspace_id,
                    source_id,
                    34,
                    attempted_at,
                    attempted_at + chrono::Duration::minutes(2),
                )
                .await
                .unwrap()
                .unwrap();
            assert!(repository
                .fail_market_refresh(
                    workspace_id,
                    source_id,
                    MarketRefreshFailure {
                        type_id: 34,
                        claim,
                        attempted_at,
                        next_refresh_at: attempted_at + chrono::Duration::minutes(1),
                        error_message: "ESI rejected the request".to_string(),
                    },
                )
                .await
                .unwrap());
        }
    };

    let mut attempted_at = t0 + chrono::Duration::hours(1);
    for expected_minutes in [1, 2, 4, 8] {
        fail_at(attempted_at).await;
        let next = next_refresh_at().await;
        assert_eq!(
            next - attempted_at,
            chrono::Duration::minutes(expected_minutes),
            "failure backing off {expected_minutes} min"
        );
        attempted_at = next;
    }

    // The backoff is capped.
    for _ in 0..10 {
        fail_at(attempted_at).await;
        attempted_at = next_refresh_at().await;
    }
    fail_at(attempted_at).await;
    assert_eq!(
        next_refresh_at().await - attempted_at,
        chrono::Duration::hours(6)
    );

    // A success (here a 304 revalidation) resets the streak.
    let claim = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            next_refresh_at().await,
            next_refresh_at().await + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    let revalidated_at = next_refresh_at().await;
    repository
        .revalidate_esi_market_refresh(
            workspace_id,
            source_id,
            34,
            claim,
            revalidated_at + chrono::Duration::minutes(15),
            None,
        )
        .await
        .unwrap();
    let after_success = revalidated_at + chrono::Duration::minutes(15);
    fail_at(after_success).await;
    assert_eq!(
        next_refresh_at().await - after_success,
        chrono::Duration::minutes(1)
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn complete_esi_market_refresh_sets_revalidated_at_for_structure_scope(pool: PgPool) {
    let (workspace_id, _repository) = fixture(&pool).await;
    // A player structure: large positive location_id, not an NPC station.
    let structure_location_id = 1_035_000_000_001_i64;
    let source_id = esi_source_at(
        &pool,
        workspace_id,
        structure_location_id,
        30_000_505,
        10_000_058,
    )
    .await;
    let t1 = crate::db_now() - chrono::Duration::hours(3);
    seed_current_esi_book(
        &pool,
        workspace_id,
        source_id,
        structure_location_id,
        30_000_505,
        10_000_058,
        t1,
    )
    .await;

    assert_eq!(
        coverage_revalidated_at(&pool, workspace_id, source_id, 34).await,
        Some(t1),
        "structure-market 200 completion flows through complete_esi_market_refresh and sets revalidated_at = observed_at",
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn prioritizing_a_structure_book_waits_for_esi_expires(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let structure_location_id = 1_035_000_000_001_i64;
    let source_id = esi_source_at(
        &pool,
        workspace_id,
        structure_location_id,
        30_000_505,
        10_000_058,
    )
    .await;
    // The seeded book's ESI cache expires 5 minutes after it was observed.
    let t0 = crate::db_now();
    seed_current_esi_book(
        &pool,
        workspace_id,
        source_id,
        structure_location_id,
        30_000_505,
        10_000_058,
        t0,
    )
    .await;

    let clicked = t0 + chrono::Duration::minutes(1);
    assert!(repository
        .prioritize_market_refresh(workspace_id, source_id, &[34], clicked)
        .await
        .unwrap());
    assert!(
        repository
            .begin_market_refresh(
                workspace_id,
                source_id,
                34,
                clicked,
                clicked + chrono::Duration::minutes(2)
            )
            .await
            .unwrap()
            .is_none(),
        "not fetchable before ESI's Expires"
    );
    let expires = t0 + chrono::Duration::minutes(5);
    assert!(repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            expires,
            expires + chrono::Duration::minutes(2)
        )
        .await
        .unwrap()
        .is_some());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn due_market_sources_lists_registered_work_once(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    repository
        .register_market_coverage(
            workspace_id,
            source_id,
            vec![
                MarketCoverageRegistration {
                    type_id: 34,
                    type_name: "Tritanium".to_string(),
                },
                MarketCoverageRegistration {
                    type_id: 35,
                    type_name: "Pyerite".to_string(),
                },
            ],
        )
        .await
        .unwrap();

    assert_eq!(
        repository
            .due_market_sources(crate::db_now(), 10)
            .await
            .unwrap(),
        vec![(workspace_id, source_id)]
    );
}

/// A second workspace, without the fixture's (globally unique) SDE import.
async fn second_workspace(pool: &PgPool) -> WorkspaceId {
    let workspace_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let now = crate::db_now();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Other Market Test',$2,$3,$3)",
    )
    .bind(workspace_id).bind(owner_id).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Other Market Test',true,$3,$3)",
    )
    .bind(owner_id).bind(workspace_id).bind(now).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    WorkspaceId(workspace_id)
}

fn tritanium() -> Vec<MarketCoverageRegistration> {
    vec![MarketCoverageRegistration {
        type_id: 34,
        type_name: "Tritanium".to_string(),
    }]
}

/// The worker takes a bounded batch of sources per pass. A workspace that
/// keeps bumping priority on many sources must not take every slot: sources
/// interleave across workspaces (each workspace's most urgent first), and a
/// priority bump only reorders that workspace's own sources.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn due_market_sources_interleave_workspaces(pool: PgPool) {
    let (busy, repository) = fixture(&pool).await;
    let quiet = second_workspace(&pool).await;
    let now = crate::db_now();

    let mut busy_sources = Vec::new();
    for (index, region_id) in [10_000_002_i64, 10_000_043, 10_000_030]
        .into_iter()
        .enumerate()
    {
        let source = esi_source_at(&pool, busy, 0, 0, region_id).await;
        repository
            .register_market_coverage(busy, source, tritanium())
            .await
            .unwrap();
        repository
            .prioritize_market_refresh(
                busy,
                source,
                &[34],
                now - chrono::Duration::seconds(i64::try_from(index).unwrap()),
            )
            .await
            .unwrap();
        busy_sources.push(source);
    }
    let quiet_source = esi_source(&pool, quiet).await;
    repository
        .register_market_coverage(quiet, quiet_source, tritanium())
        .await
        .unwrap();

    let due = repository
        .due_market_sources(now + chrono::Duration::seconds(1), 2)
        .await
        .unwrap();
    assert_eq!(due.len(), 2);
    assert!(due.contains(&(quiet, quiet_source)), "{due:?}");
    assert!(
        due.contains(&(busy, busy_sources[0])),
        "the busy workspace's most recently prioritized source goes first: {due:?}"
    );
}

/// Coverage nobody has needed for a week stops being refreshed: it keeps its
/// last prices (just staler), and wakes up the next time anything registers
/// it again. No error anywhere -- it simply isn't due.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn coverage_nobody_needed_for_a_week_goes_dormant_until_needed_again(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    repository
        .register_market_coverage(workspace_id, source_id, tritanium())
        .await
        .unwrap();
    sqlx::query(
        "UPDATE market_source_coverage SET last_needed_at = now() - interval '8 days' WHERE price_source_id = $1",
    )
    .bind(source_id.0)
    .execute(&pool)
    .await
    .unwrap();

    let now = crate::db_now();
    assert!(repository
        .due_market_sources(now, 10)
        .await
        .unwrap()
        .is_empty());
    assert!(repository
        .market_refresh_candidates(workspace_id, source_id, now, 10)
        .await
        .unwrap()
        .is_empty());

    // Needed again (any build/inventory/market read registers coverage).
    repository
        .register_market_coverage(workspace_id, source_id, tritanium())
        .await
        .unwrap();
    assert_eq!(
        repository.due_market_sources(now, 10).await.unwrap(),
        vec![(workspace_id, source_id)]
    );
    assert_eq!(
        repository
            .market_refresh_candidates(workspace_id, source_id, now, 10)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn manual_market_priority_is_scope_selective_and_precedes_routine_work(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    repository
        .register_market_coverage(
            workspace_id,
            source_id,
            vec![
                MarketCoverageRegistration {
                    type_id: 34,
                    type_name: "Tritanium".to_string(),
                },
                MarketCoverageRegistration {
                    type_id: 35,
                    type_name: "Pyerite".to_string(),
                },
                MarketCoverageRegistration {
                    type_id: 36,
                    type_name: "Mexallon".to_string(),
                },
            ],
        )
        .await
        .unwrap();
    let now = crate::db_now();
    assert!(repository
        .prioritize_market_refresh(workspace_id, source_id, &[36], now,)
        .await
        .unwrap());

    let due = repository
        .market_refresh_candidates(workspace_id, source_id, now, 1)
        .await
        .unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].type_id, 36);
    let prioritized_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM market_source_coverage WHERE price_source_id=$1 AND priority_requested_at IS NOT NULL",
    ).bind(source_id.0).fetch_one(&pool).await.unwrap();
    assert_eq!(prioritized_count, 1);
}

/// A large batch
/// registered and prioritized all at once (simulating a bulk
/// "request prices for this category" click) must not starve a
/// later, smaller prioritized request -- `candidates()`'s
/// `priority_requested_at DESC` ordering means the most recently
/// *requested* work always wins over an older backlog, even one that's
/// far larger.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_later_smaller_request_is_not_blocked_behind_an_earlier_large_prioritized_backlog(
    pool: PgPool,
) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;

    // A large "category" backlog: 200 items, registered and
    // prioritized together (as `register_and_refresh` does for
    // every call, including the bulk-category route).
    let backlog: Vec<MarketCoverageRegistration> = (1..=200)
        .map(|i| MarketCoverageRegistration {
            type_id: 500_000 + i,
            type_name: format!("Backlog Item {i}"),
        })
        .collect();
    let backlog_type_ids: Vec<i64> = backlog.iter().map(|item| item.type_id).collect();
    repository
        .register_market_coverage(workspace_id, source_id, backlog)
        .await
        .unwrap();
    let backlog_prioritized_at = crate::db_now();
    repository
        .prioritize_market_refresh(
            workspace_id,
            source_id,
            &backlog_type_ids,
            backlog_prioritized_at,
        )
        .await
        .unwrap();

    // A later, smaller request for one unrelated item, prioritized
    // strictly after the backlog.
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
    let later_prioritized_at = backlog_prioritized_at + chrono::Duration::seconds(1);
    repository
        .prioritize_market_refresh(workspace_id, source_id, &[34], later_prioritized_at)
        .await
        .unwrap();

    // A small pull must surface the later request first, not get lost
    // among the 200-item backlog it was registered strictly after.
    let due = repository
        .market_refresh_candidates(workspace_id, source_id, later_prioritized_at, 1)
        .await
        .unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].type_id, 34);
}

/// A registration larger than a single refresh pass's limit is
/// accepted in full -- the limit only bounds one `candidates()` call,
/// it never drops or loses the remainder. Simulates
/// `register_and_refresh` immediately dispatching a bounded
/// `item_batch_size` while the rest stays `due` for the worker's
/// normal polling loop to drain over subsequent ticks.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_registration_larger_than_the_batch_limit_leaves_the_remainder_due(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    let items: Vec<MarketCoverageRegistration> = (1..=250)
        .map(|i| MarketCoverageRegistration {
            type_id: 600_000 + i,
            type_name: format!("Category Item {i}"),
        })
        .collect();
    repository
        .register_market_coverage(workspace_id, source_id, items)
        .await
        .unwrap();

    // A bounded pull (mirrors `item_batch_size`) only ever returns that
    // many at once...
    let now = crate::db_now();
    let first_pass = repository
        .market_refresh_candidates(workspace_id, source_id, now, 100)
        .await
        .unwrap();
    assert_eq!(first_pass.len(), 100);

    // ...but every item is still there and pullable -- nothing was
    // dropped by the smaller limit, matching what a subsequent worker
    // tick would find.
    let everything = repository
        .market_refresh_candidates(workspace_id, source_id, now, 1_000)
        .await
        .unwrap();
    assert_eq!(everything.len(), 250);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn expired_market_refresh_lease_becomes_due_and_can_be_reclaimed(pool: PgPool) {
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
    let first_attempt = crate::db_now() - chrono::Duration::minutes(10);
    assert!(repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            first_attempt,
            first_attempt + chrono::Duration::minutes(2)
        )
        .await
        .unwrap()
        .is_some());

    let now = crate::db_now();
    let due = repository
        .market_refresh_candidates(workspace_id, source_id, now, 10)
        .await
        .unwrap();

    assert_eq!(due.len(), 1);
    assert_eq!(due[0].type_id, 34);
    assert!(repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            now,
            now + chrono::Duration::minutes(2)
        )
        .await
        .unwrap()
        .is_some());
}

/// `market_refresh_candidates_for_type_ids` returns exactly the
/// requested identities -- ignoring an unrelated, equally-due type
/// that a priority-ordered `LIMIT` (as in `market_refresh_candidates`)
/// could otherwise sweep in -- and, unlike that method, does not filter
/// on `next_refresh_at` at all: a type explicitly scheduled far in the
/// future by `prioritize_market_refresh` is still returned by identity.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn candidates_for_type_ids_is_identity_scoped_and_ignores_next_refresh_at(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    repository
        .register_market_coverage(
            workspace_id,
            source_id,
            vec![
                MarketCoverageRegistration {
                    type_id: 34,
                    type_name: "Tritanium".to_string(),
                },
                MarketCoverageRegistration {
                    type_id: 35,
                    type_name: "Pyerite".to_string(),
                },
            ],
        )
        .await
        .unwrap();
    let now = crate::db_now();

    // Only the requested identity comes back, never the other equally
    // "due" (never-fetched) type sitting in the same coverage table.
    let scoped = repository
        .market_refresh_candidates_for_type_ids(workspace_id, source_id, &[34], now)
        .await
        .unwrap();
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0].type_id, 34);

    // Explicitly schedule 34 far in the future -- the due-checked
    // `market_refresh_candidates` would exclude it...
    let far_future = now + chrono::Duration::hours(6);
    repository
        .prioritize_market_refresh(workspace_id, source_id, &[34], far_future)
        .await
        .unwrap();
    let due_checked = repository
        .market_refresh_candidates(workspace_id, source_id, now, 10)
        .await
        .unwrap();
    assert!(
        !due_checked.iter().any(|item| item.type_id == 34),
        "sanity check: the due-checked method excludes a future next_refresh_at"
    );

    // ...but the identity-scoped method still returns it by identity,
    // regardless of next_refresh_at.
    let identity_scoped = repository
        .market_refresh_candidates_for_type_ids(workspace_id, source_id, &[34], now)
        .await
        .unwrap();
    assert_eq!(identity_scoped.len(), 1);
    assert_eq!(identity_scoped[0].type_id, 34);
}

/// An identity requested while it's actively leased by another
/// in-flight refresh (unexpired) is excluded -- the same "don't
/// double-dispatch a fetch already in progress" guarantee
/// `market_refresh_candidates` provides, preserved here.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn candidates_for_type_ids_excludes_an_actively_leased_type(pool: PgPool) {
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
    let attempted_at = crate::db_now();
    repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            attempted_at,
            attempted_at + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .expect("lease should be granted");

    let while_leased = repository
        .market_refresh_candidates_for_type_ids(workspace_id, source_id, &[34], attempted_at)
        .await
        .unwrap();
    assert!(
        while_leased.is_empty(),
        "an actively-leased type must not be returned as a fresh candidate"
    );

    // Once the lease's `now` reference point is past its expiry, it's
    // eligible again.
    let after_expiry = attempted_at + chrono::Duration::minutes(3);
    let after_lease_expires = repository
        .market_refresh_candidates_for_type_ids(workspace_id, source_id, &[34], after_expiry)
        .await
        .unwrap();
    assert_eq!(after_lease_expires.len(), 1);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn expired_market_claim_cannot_fail_a_newer_claim(pool: PgPool) {
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
    let first = crate::db_now();
    let first_claim = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            first,
            first + chrono::Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    let second = first + chrono::Duration::minutes(2);
    let second_claim = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            second,
            second + chrono::Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();

    assert!(!repository
        .fail_market_refresh(
            workspace_id,
            source_id,
            MarketRefreshFailure {
                type_id: 34,
                claim: first_claim,
                attempted_at: first,
                next_refresh_at: first + chrono::Duration::minutes(1),
                error_message: "late failure".to_string(),
            },
        )
        .await
        .unwrap());
    assert!(repository
        .fail_market_refresh(
            workspace_id,
            source_id,
            MarketRefreshFailure {
                type_id: 34,
                claim: second_claim,
                attempted_at: second,
                next_refresh_at: second + chrono::Duration::minutes(1),
                error_message: "current failure".to_string(),
            },
        )
        .await
        .unwrap());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn esi_refresh_completion_is_atomic_and_failure_preserves_last_book(pool: PgPool) {
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
    let observed_at = crate::db_now();
    let claim = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            observed_at,
            observed_at + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    let batch_id = MarketObservationBatchId::new();
    repository
        .complete_esi_market_refresh(
            workspace_id,
            claim,
            observed_at + chrono::Duration::minutes(15),
            EsiMarketObservationBatch {
                id: batch_id,
                source_id,
                type_id: 34,
                type_name: "Tritanium".to_string(),
                region_id: 10_000_002,
                solar_system_id: 30_000_142,
                location_id: 60_003_760,
                observed_at,
                etag: Some("jita-etag".to_string()),
                expires_at: Some(observed_at + chrono::Duration::minutes(5)),
                orders: vec![
                    EsiMarketOrder::new(
                        101,
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
                    .unwrap(),
                    EsiMarketOrder::new(
                        102,
                        MarketOrderSide::Buy,
                        "4.0000",
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
            },
        )
        .await
        .unwrap();

    let coverage = load_coverage(&pool, workspace_id, source_id).await.unwrap();
    assert_eq!(coverage[0].refresh_state, MarketRefreshState::Current);
    assert_eq!(coverage[0].order_count, 2);
    assert_eq!(coverage[0].buy_order_count, 1);
    assert_eq!(coverage[0].sell_order_count, 1);
    assert!(coverage[0].next_refresh_at.unwrap() > observed_at);
    let book = repository
        .get_source_order_book(workspace_id, source_id, 34, 60_003_760, None)
        .await
        .unwrap();
    assert_eq!(book.observation_batch_id, batch_id);
    assert_eq!(book.import_batch_id, None);
    assert_eq!(book.lowest_sell, Some(Money::parse("4.2500").unwrap()));
    assert_eq!(book.highest_buy, Some(Money::parse("4.0000").unwrap()));

    let retry_at = coverage[0].next_refresh_at.unwrap() + chrono::Duration::seconds(1);
    let retry_claim = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            retry_at,
            retry_at + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .fail_market_refresh(
            workspace_id,
            source_id,
            MarketRefreshFailure {
                type_id: 34,
                claim: retry_claim,
                attempted_at: retry_at,
                next_refresh_at: retry_at + chrono::Duration::minutes(1),
                error_message: "temporary ESI failure".to_string(),
            },
        )
        .await
        .unwrap();
    let failed = load_coverage(&pool, workspace_id, source_id).await.unwrap();
    assert_eq!(failed[0].refresh_state, MarketRefreshState::Failed);
    assert_eq!(failed[0].observed_at, Some(observed_at));
    assert_eq!(failed[0].order_count, 2);
    assert_eq!(
        repository
            .get_source_order_book(workspace_id, source_id, 34, 60_003_760, None)
            .await
            .unwrap()
            .observation_batch_id,
        batch_id
    );

    let mutation = sqlx::query(
        "UPDATE market_order_observations SET remaining_volume=1 WHERE observation_batch_id=$1",
    )
    .bind(batch_id.0)
    .execute(&pool)
    .await;
    assert!(mutation.is_err());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn revalidate_esi_market_refresh_updates_only_coverage(pool: PgPool) {
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
    let observed_at = crate::db_now();
    let claim = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            observed_at,
            observed_at + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    let batch_id = MarketObservationBatchId::new();
    repository
        .complete_esi_market_refresh(
            workspace_id,
            claim,
            observed_at + chrono::Duration::minutes(15),
            EsiMarketObservationBatch {
                id: batch_id,
                source_id,
                type_id: 34,
                type_name: "Tritanium".to_string(),
                region_id: 10_000_002,
                solar_system_id: 30_000_142,
                location_id: 60_003_760,
                observed_at,
                etag: Some("jita-etag".to_string()),
                expires_at: Some(observed_at + chrono::Duration::minutes(5)),
                orders: vec![EsiMarketOrder::new(
                    101,
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
            },
        )
        .await
        .unwrap();

    // Snapshot everything the revalidation must NOT touch.
    let batch_before: BatchCacheColumns = sqlx::query_as(
        "SELECT observed_at,completed_at,etag,expires_at FROM market_observation_batches WHERE id=$1",
    )
    .bind(batch_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    let orders_before: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT id,remaining_volume FROM market_order_observations WHERE observation_batch_id=$1 ORDER BY id",
    )
    .bind(batch_id.0)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(orders_before.len(), 1);

    // A retry claim, then revalidate against it (ESI answered 304).
    let retry_at = observed_at + chrono::Duration::minutes(20);
    let retry_claim = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            retry_at,
            retry_at + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    let next_refresh_at = retry_at + chrono::Duration::minutes(15);
    let wall_before = crate::db_now();
    assert!(repository
        .revalidate_esi_market_refresh(
            workspace_id,
            source_id,
            34,
            retry_claim,
            next_refresh_at,
            None
        )
        .await
        .unwrap());

    // Coverage: refreshing -> current, error/lease/priority cleared,
    // next_refresh_at advanced, last_completed_batch_id unchanged.
    let coverage = load_coverage(&pool, workspace_id, source_id).await.unwrap();
    assert_eq!(coverage[0].refresh_state, MarketRefreshState::Current);
    assert_eq!(coverage[0].last_error, None);
    assert_eq!(coverage[0].observed_at, Some(observed_at));
    assert_eq!(coverage[0].order_count, 1);
    assert_eq!(coverage[0].next_refresh_at, Some(next_refresh_at));
    // `last_attempted_at` advanced to the revalidation instant (now), not
    // left at the claim value.
    let last_attempted_at = coverage[0].last_attempted_at.unwrap();
    assert!(last_attempted_at >= wall_before && last_attempted_at != retry_claim);

    let coverage_row: (Uuid, Option<DateTime<Utc>>, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT last_completed_batch_id,lease_expires_at,priority_requested_at FROM market_source_coverage WHERE workspace_id=$1 AND price_source_id=$2 AND type_id=34",
    )
    .bind(workspace_id.0)
    .bind(source_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        coverage_row.0, batch_id.0,
        "last_completed_batch_id unchanged"
    );
    assert_eq!(coverage_row.1, None, "lease cleared");
    assert_eq!(coverage_row.2, None, "priority cleared");

    // The batch and its order rows are byte-for-byte unchanged.
    let batch_after: BatchCacheColumns = sqlx::query_as(
        "SELECT observed_at,completed_at,etag,expires_at FROM market_observation_batches WHERE id=$1",
    )
    .bind(batch_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(batch_after, batch_before);
    let orders_after: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT id,remaining_volume FROM market_order_observations WHERE observation_batch_id=$1 ORDER BY id",
    )
    .bind(batch_id.0)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(orders_after, orders_before);
    let batch_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM market_observation_batches WHERE workspace_id=$1")
            .bind(workspace_id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(batch_count, 1, "no new batch row written");

    // A stale claim revalidates nothing and returns false.
    let stale_next = retry_at + chrono::Duration::hours(1);
    assert!(!repository
        .revalidate_esi_market_refresh(workspace_id, source_id, 34, claim, stale_next, None)
        .await
        .unwrap());
    let coverage = load_coverage(&pool, workspace_id, source_id).await.unwrap();
    assert_eq!(coverage[0].next_refresh_at, Some(next_refresh_at));
}
