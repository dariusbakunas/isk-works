use super::common::*;

/// The regression case that motivated `scope_freshness`'s design: a
/// type whose most recent refresh *attempt* failed must still count as
/// "observed" if an earlier attempt succeeded -- `refresh_state='failed'`
/// doesn't erase `last_completed_batch_id`. Also proves an unregistered
/// scope is simply absent from the result map, and that two scopes
/// queried in one batched call don't cross-contaminate each other's
/// counts.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn scope_freshness_counts_observed_types_even_after_a_later_failed_refresh(pool: PgPool) {
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
    let observed_at = crate::db_now();

    // Type 34 (Tritanium): registered but never begun -- stays
    // "missing", contributes to tracked_type_count only.

    // Type 35 (Pyerite): begun and completed successfully once.
    let claim_35 = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            35,
            observed_at,
            observed_at + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_esi_market_refresh(
            workspace_id,
            claim_35,
            observed_at,
            freshness_test_batch(source_id, 35, "Pyerite", observed_at),
        )
        .await
        .unwrap();

    // Type 36 (Mexallon): completed successfully, then refreshed again
    // and that second attempt FAILED. Its refresh_state is now
    // 'failed', but the earlier successful batch must still count.
    let claim_36a = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            36,
            observed_at,
            observed_at + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_esi_market_refresh(
            workspace_id,
            claim_36a,
            observed_at,
            freshness_test_batch(source_id, 36, "Mexallon", observed_at),
        )
        .await
        .unwrap();
    let retry_at = observed_at + chrono::Duration::minutes(30);
    let claim_36b = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            36,
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
                type_id: 36,
                claim: claim_36b,
                attempted_at: retry_at,
                next_refresh_at: retry_at + chrono::Duration::minutes(5),
                error_message: "temporary ESI failure".to_string(),
            },
        )
        .await
        .unwrap();

    let freshness = repository
        .scope_freshness(
            workspace_id,
            &[(10_000_002, 60_003_760), (10_000_030, 60_004_588)],
        )
        .await
        .unwrap();

    let jita = freshness.get(&(10_000_002, 60_003_760)).copied().unwrap();
    assert_eq!(jita.tracked_type_count, 3);
    assert_eq!(
        jita.observed_type_count, 2,
        "34 never observed, 35 and 36 both were"
    );
    assert!(jita.most_recent_observed_at.is_some());

    // A scope with zero market_source_coverage rows at all is simply
    // absent -- never a fabricated zero-value entry.
    assert!(!freshness.contains_key(&(10_000_030, 60_004_588)));
}

/// `item_freshness` is `None` both when there's no coverage at all and
/// when the requested types were registered but never completed a
/// refresh -- a registered-but-never-observed row must never be
/// mistaken for "fresh."
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn item_freshness_returns_none_when_nothing_has_been_observed_yet(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;

    assert_eq!(
        repository
            .item_freshness(workspace_id, iskworks_core::DEFAULT_MARKET_SCOPE, &[34])
            .await
            .unwrap(),
        None,
        "no coverage rows exist at all yet"
    );

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

    assert_eq!(
        repository
            .item_freshness(workspace_id, iskworks_core::DEFAULT_MARKET_SCOPE, &[34])
            .await
            .unwrap(),
        None,
        "registered but never completed a refresh"
    );
}

/// Only the requested type_ids count toward the result -- a type
/// completed more recently than any requested type, but not itself
/// requested, must never leak into the returned timestamp. This is
/// what lets a poll scoped to exactly the visible page answer "has
/// anything I'm looking at changed" precisely.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn item_freshness_returns_the_max_among_only_the_requested_type_ids(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    repository
        .register_market_coverage(
            workspace_id,
            source_id,
            vec![
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
    let earlier = crate::db_now();
    let later = earlier + chrono::Duration::minutes(10);

    let claim_35 = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            35,
            earlier,
            earlier + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_esi_market_refresh(
            workspace_id,
            claim_35,
            earlier,
            freshness_test_batch(source_id, 35, "Pyerite", earlier),
        )
        .await
        .unwrap();

    let claim_36 = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            36,
            later,
            later + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_esi_market_refresh(
            workspace_id,
            claim_36,
            later,
            freshness_test_batch(source_id, 36, "Mexallon", later),
        )
        .await
        .unwrap();

    // Requesting only 35: the later completion for 36 must not leak in.
    assert_eq!(
        repository
            .item_freshness(workspace_id, iskworks_core::DEFAULT_MARKET_SCOPE, &[35])
            .await
            .unwrap(),
        Some(earlier)
    );

    // Requesting both: the max across the two.
    assert_eq!(
        repository
            .item_freshness(workspace_id, iskworks_core::DEFAULT_MARKET_SCOPE, &[35, 36])
            .await
            .unwrap(),
        Some(later)
    );
}

/// The same `type_id` observed under a different (region, location)
/// scope must never count toward this scope's freshness -- proves the
/// query filters on `market_source_coverage`'s own scope columns, not
/// just `type_id`.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn item_freshness_is_scoped_to_the_requested_region_and_location(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let jita_source_id = esi_source(&pool, workspace_id).await;
    let earlier = crate::db_now();
    repository
        .register_market_coverage(
            workspace_id,
            jita_source_id,
            vec![MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }],
        )
        .await
        .unwrap();
    let jita_claim = repository
        .begin_market_refresh(
            workspace_id,
            jita_source_id,
            34,
            earlier,
            earlier + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_esi_market_refresh(
            workspace_id,
            jita_claim,
            earlier,
            freshness_test_batch(jita_source_id, 34, "Tritanium", earlier),
        )
        .await
        .unwrap();

    // Same type_id, Rens scope, completed much later.
    let rens_scope = iskworks_core::MarketScope {
        region_id: 10_000_030,
        location_id: Some(60_004_588),
    };
    let rens_source_id = repository
        .ensure_esi_price_source_for_scope(workspace_id, rens_scope)
        .await
        .unwrap();
    let later = earlier + chrono::Duration::hours(1);
    repository
        .register_market_coverage(
            workspace_id,
            rens_source_id,
            vec![MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }],
        )
        .await
        .unwrap();
    let rens_claim = repository
        .begin_market_refresh(
            workspace_id,
            rens_source_id,
            34,
            later,
            later + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_esi_market_refresh(
            workspace_id,
            rens_claim,
            later,
            EsiMarketObservationBatch {
                id: MarketObservationBatchId::new(),
                source_id: rens_source_id,
                type_id: 34,
                type_name: "Tritanium".to_string(),
                region_id: rens_scope.region_id,
                solar_system_id: 30_002_510,
                location_id: rens_scope.location_id.unwrap(),
                observed_at: later,
                etag: None,
                expires_at: None,
                orders: Vec::new(),
            },
        )
        .await
        .unwrap();

    assert_eq!(
        repository
            .item_freshness(workspace_id, iskworks_core::DEFAULT_MARKET_SCOPE, &[34])
            .await
            .unwrap(),
        Some(earlier),
        "the later Rens-scoped completion must not leak into Jita's result"
    );
    assert_eq!(
        repository
            .item_freshness(workspace_id, rens_scope, &[34])
            .await
            .unwrap(),
        Some(later)
    );
}

fn freshness_test_batch(
    source_id: PriceSourceId,
    type_id: i64,
    type_name: &str,
    observed_at: DateTime<Utc>,
) -> EsiMarketObservationBatch {
    EsiMarketObservationBatch {
        id: MarketObservationBatchId::new(),
        source_id,
        type_id,
        type_name: type_name.to_string(),
        region_id: 10_000_002,
        solar_system_id: 30_000_142,
        location_id: 60_003_760,
        observed_at,
        etag: None,
        expires_at: None,
        orders: Vec::new(),
    }
}

/// Effective freshness: an old snapshot (T1) that ESI has since confirmed
/// unchanged via a 304 (T2) reads as fresh-as-of-T2 from both
/// `scope_freshness` and `item_freshness` -- neither still reports the
/// original T1 observation time.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn scope_freshness_and_item_freshness_use_effective_time(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    let t1 = crate::db_now() - chrono::Duration::hours(4);
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
            t1,
            t1 + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_esi_market_refresh(
            workspace_id,
            claim,
            t1 + chrono::Duration::minutes(15),
            freshness_test_batch(source_id, 34, "Tritanium", t1),
        )
        .await
        .unwrap();

    // 304 now.
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
    repository
        .revalidate_esi_market_refresh(
            workspace_id,
            source_id,
            34,
            retry_claim,
            retry_at + chrono::Duration::minutes(15),
            None,
        )
        .await
        .unwrap();

    let scope = iskworks_core::MarketScope {
        region_id: 10_000_002,
        location_id: Some(60_003_760),
    };
    let scope_fresh = repository
        .scope_freshness(workspace_id, &[(10_000_002, 60_003_760)])
        .await
        .unwrap();
    let most_recent = scope_fresh
        .get(&(10_000_002, 60_003_760))
        .unwrap()
        .most_recent_observed_at
        .unwrap();
    assert!(
        most_recent >= wall_before,
        "scope freshness advanced to the 304 time, not T1"
    );

    let item_fresh = repository
        .item_freshness(workspace_id, scope, &[34])
        .await
        .unwrap()
        .unwrap();
    assert!(
        item_fresh >= wall_before,
        "item freshness advanced to the 304 time, not T1"
    );
}
