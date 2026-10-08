use super::common::*;

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn batched_esi_order_books_match_the_singular_lookup_per_type(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    let observed_at = crate::db_now();

    #[allow(clippy::too_many_arguments)]
    async fn complete_refresh(
        repository: &PgMarketRepository,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_id: i64,
        type_name: &str,
        observed_at: DateTime<Utc>,
        sell_price: &str,
        buy_price: &str,
    ) -> MarketObservationBatchId {
        repository
            .register_market_coverage(
                workspace_id,
                source_id,
                vec![MarketCoverageRegistration {
                    type_id,
                    type_name: type_name.to_string(),
                }],
            )
            .await
            .unwrap();
        let claim = repository
            .begin_market_refresh(
                workspace_id,
                source_id,
                type_id,
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
                    type_id,
                    type_name: type_name.to_string(),
                    region_id: 10_000_002,
                    solar_system_id: 30_000_142,
                    location_id: 60_003_760,
                    observed_at,
                    etag: None,
                    expires_at: Some(observed_at + chrono::Duration::minutes(5)),
                    orders: vec![
                        EsiMarketOrder::new(
                            type_id * 1000 + 1,
                            MarketOrderSide::Sell,
                            sell_price,
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
                            type_id * 1000 + 2,
                            MarketOrderSide::Buy,
                            buy_price,
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
        batch_id
    }

    let tritanium_batch = complete_refresh(
        &repository,
        workspace_id,
        source_id,
        34,
        "Tritanium",
        observed_at,
        "4.2500",
        "4.0000",
    )
    .await;
    let pyerite_batch = complete_refresh(
        &repository,
        workspace_id,
        source_id,
        35,
        "Pyerite",
        observed_at,
        "8.7500",
        "8.2500",
    )
    .await;

    // 99 is never registered for coverage -- it must be absent from the
    // batched result, exactly like `get_source_order_book` would return
    // `MarketError::OrdersUnavailable` for it on the singular path.
    let books = repository
        .get_source_order_books(workspace_id, source_id, &[34, 35, 99], 60_003_760, None)
        .await
        .unwrap();

    assert_eq!(books.len(), 2);
    let tritanium = books.get(&34).unwrap();
    assert_eq!(tritanium.observation_batch_id, tritanium_batch);
    assert_eq!(tritanium.lowest_sell, Some(Money::parse("4.2500").unwrap()));
    assert_eq!(tritanium.highest_buy, Some(Money::parse("4.0000").unwrap()));
    let pyerite = books.get(&35).unwrap();
    assert_eq!(pyerite.observation_batch_id, pyerite_batch);
    assert_eq!(pyerite.lowest_sell, Some(Money::parse("8.7500").unwrap()));
    assert_eq!(pyerite.highest_buy, Some(Money::parse("8.2500").unwrap()));
    assert!(!books.contains_key(&99));

    let singular_tritanium = repository
        .get_source_order_book(workspace_id, source_id, 34, 60_003_760, None)
        .await
        .unwrap();
    assert_eq!(singular_tritanium.lowest_sell, tritanium.lowest_sell);
    assert_eq!(singular_tritanium.highest_buy, tritanium.highest_buy);
    assert_eq!(
        singular_tritanium.observation_batch_id,
        tritanium.observation_batch_id
    );
    // Whole-book equality per type: the Inventory list's explicit-source
    // pricing (`MarketService::preview_prices`) relies on it.
    for (type_id, batched) in &books {
        assert_eq!(
            &repository
                .get_source_order_book(workspace_id, source_id, *type_id, 60_003_760, None)
                .await
                .unwrap(),
            batched
        );
    }
}

/// `scoped_order_books` is the temporary `MarketScope` -> `price_source_id`
/// shim. Proves: a location-scoped request only sees the configured
/// source's own orders; a region-wide request (`location_id: None`)
/// still includes that same source (region-wide must not be *stricter*
/// than a specific location); a type never registered for coverage is
/// simply absent, not an error; and a scope no configured source
/// matches at all returns nothing for every requested type.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn scoped_order_books_resolves_via_the_price_source_configs_shim(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    let observed_at = crate::db_now();

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
    repository
        .complete_esi_market_refresh(
            workspace_id,
            claim,
            observed_at + chrono::Duration::minutes(15),
            EsiMarketObservationBatch {
                id: MarketObservationBatchId::new(),
                source_id,
                type_id: 34,
                type_name: "Tritanium".to_string(),
                region_id: 10_000_002,
                solar_system_id: 30_000_142,
                location_id: 60_003_760,
                observed_at,
                etag: None,
                expires_at: Some(observed_at + chrono::Duration::minutes(5)),
                orders: vec![EsiMarketOrder::new(
                    341,
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

    // Correct location: Tritanium resolves.
    let at_jita = repository
        .scoped_order_books(
            workspace_id,
            iskworks_core::MarketScope {
                region_id: 10_000_002,
                location_id: Some(60_003_760),
            },
            &[34, 35],
        )
        .await
        .unwrap();
    assert_eq!(at_jita.get(&34).unwrap().len(), 1);
    assert_eq!(
        at_jita.get(&34).unwrap()[0].price,
        Money::parse("4.2500").unwrap()
    );
    // 35 was never registered/refreshed -- absent, not an error.
    assert!(!at_jita.contains_key(&35));

    // Region-wide (no location) must still include the Jita config.
    let region_wide = repository
        .scoped_order_books(
            workspace_id,
            iskworks_core::MarketScope {
                region_id: 10_000_002,
                location_id: None,
            },
            &[34],
        )
        .await
        .unwrap();
    assert_eq!(region_wide.get(&34).unwrap().len(), 1);

    // A different location within the same region: no configured
    // source there -- empty, not an error.
    let wrong_location = repository
        .scoped_order_books(
            workspace_id,
            iskworks_core::MarketScope {
                region_id: 10_000_002,
                location_id: Some(60_003_761),
            },
            &[34],
        )
        .await
        .unwrap();
    assert!(!wrong_location.contains_key(&34));

    // A region with no configured source at all: empty for every
    // requested type.
    let no_source_region = repository
        .scoped_order_books(
            workspace_id,
            iskworks_core::MarketScope {
                region_id: 10_000_009,
                location_id: None,
            },
            &[34],
        )
        .await
        .unwrap();
    assert!(no_source_region.is_empty());
}

/// Imported EVE client market exports are visible through
/// `scoped_order_books` at their own detected scope with no `PriceSource`
/// ever created -- and merge together with ESI data observed at the same
/// location (imports are additional market observations alongside ESI
/// data).
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn scoped_order_books_includes_import_data_with_no_price_source_and_merges_with_esi(
    pool: PgPool,
) {
    let (workspace_id, repository) = fixture(&pool).await;
    let service = MarketService::new(repository.clone());
    service.import(workspace_id, vec![upload()]).await.unwrap();

    // Concrete location: import data resolves with no PriceSource.
    let at_location = repository
        .scoped_order_books(
            workspace_id,
            iskworks_core::MarketScope {
                region_id: 10_000_009,
                location_id: Some(1_049_588_174_021),
            },
            &[34],
        )
        .await
        .unwrap();
    assert_eq!(at_location.get(&34).unwrap().len(), 6);

    // Region-wide: still discovered, via known_import_locations_in_scope.
    let region_wide = repository
        .scoped_order_books(
            workspace_id,
            iskworks_core::MarketScope {
                region_id: 10_000_009,
                location_id: None,
            },
            &[34],
        )
        .await
        .unwrap();
    assert_eq!(region_wide.get(&34).unwrap().len(), 6);

    // An ESI source at the exact same scope: its orders merge in
    // alongside the imported ones, not replace them.
    let esi_source_id = repository
        .ensure_esi_price_source_for_scope(
            workspace_id,
            iskworks_core::MarketScope {
                region_id: 10_000_009,
                location_id: Some(1_049_588_174_021),
            },
        )
        .await
        .unwrap();
    let observed_at = crate::db_now();
    repository
        .register_market_coverage(
            workspace_id,
            esi_source_id,
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
            esi_source_id,
            34,
            observed_at,
            observed_at + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_esi_market_refresh(
            workspace_id,
            claim,
            observed_at + chrono::Duration::minutes(15),
            EsiMarketObservationBatch {
                id: MarketObservationBatchId::new(),
                source_id: esi_source_id,
                type_id: 34,
                type_name: "Tritanium".to_string(),
                region_id: 10_000_009,
                solar_system_id: 30_000_772,
                location_id: 1_049_588_174_021,
                observed_at,
                etag: None,
                expires_at: Some(observed_at + chrono::Duration::minutes(5)),
                orders: vec![EsiMarketOrder::new(
                    999,
                    MarketOrderSide::Sell,
                    "3.9000",
                    50_000,
                    50_000,
                    1,
                    "station".to_string(),
                    observed_at,
                    90,
                    1_049_588_174_021,
                    30_000_772,
                    Some(1_049_588_174_021),
                )
                .unwrap()],
            },
        )
        .await
        .unwrap();

    let merged = repository
        .scoped_order_books(
            workspace_id,
            iskworks_core::MarketScope {
                region_id: 10_000_009,
                location_id: Some(1_049_588_174_021),
            },
            &[34],
        )
        .await
        .unwrap();
    assert_eq!(
        merged.get(&34).unwrap().len(),
        7,
        "6 imported orders + 1 ESI order, merged, not replaced"
    );
}

/// `derive_market_price_items` computes freshness from the orders' own
/// `observed_at` rather than a source config's, so it works uniformly
/// whether the merged book came from one import file, several, or ESI --
/// none of which necessarily share one file-level timestamp for a
/// region-wide scope. Import with an already-old timestamp (rather than
/// retroactively aging one via `UPDATE`, which `market_order_observations`'s
/// immutability trigger forbids) to prove it.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn derive_market_price_items_flags_stale_observations_using_the_orders_own_timestamp(
    pool: PgPool,
) {
    let (workspace_id, repository) = fixture(&pool).await;
    let service = MarketService::new(repository);
    let mut stale_upload = upload();
    stale_upload.user_observed_at = Some(crate::db_now() - chrono::Duration::hours(48));
    service
        .import(workspace_id, vec![stale_upload])
        .await
        .unwrap();

    let scope = iskworks_core::MarketScope {
        region_id: 10_000_009,
        location_id: Some(1_049_588_174_021),
    };
    let prices = PgIndustryRepository::new(pool.clone())
        .derive_market_price_items(
            workspace_id,
            scope,
            vec![MarketPriceRequest {
                type_id: 34,
                type_name: "Tritanium".to_string(),
                requested_quantity: 250_000_000,
                pricing_policy: MarketPricingPolicy::AcquireQuantityFromSellOrders,
            }],
            None,
        )
        .await
        .unwrap();
    assert_eq!(prices.len(), 1);
    assert!(prices[0]
        .note
        .contains(iskworks_core::STALE_MARKET_PROVENANCE));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn changed_order_state_is_appended_as_a_later_observation(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let service = MarketService::new(repository);
    let mut first = upload();
    first.filename = "Tritanium.csv".to_string();
    first.user_observed_at = Some(Utc.with_ymd_and_hms(2026, 7, 26, 19, 0, 0).unwrap());
    service
        .import(workspace_id, vec![first.clone()])
        .await
        .unwrap();
    let original = String::from_utf8(first.content.clone()).unwrap();
    let mut changed = first;
    changed.filename = "Tritanium-later.csv".to_string();
    changed.user_observed_at = Some(Utc.with_ymd_and_hms(2026, 7, 26, 20, 0, 0).unwrap());
    changed.content = original
        .replacen("226641769.0", "200000000.0", 1)
        .into_bytes();
    service.import(workspace_id, vec![changed]).await.unwrap();
    let versions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM market_order_observations WHERE order_id=7386855683",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(versions, 2);
}

/// Order-book provenance vs. effective freshness: after a 304 the book
/// keeps its physical `observed_at` (T1) and every order's `observed_at`
/// (T1), and additionally carries `revalidated_at = Some(T2)` so
/// `effective_observed_at()` returns T2.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn order_book_exposes_observed_and_revalidated(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    let t1 = crate::db_now() - chrono::Duration::hours(2);
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
    let batch_id = MarketObservationBatchId::new();
    repository
        .complete_esi_market_refresh(
            workspace_id,
            claim,
            t1 + chrono::Duration::minutes(15),
            EsiMarketObservationBatch {
                id: batch_id,
                source_id,
                type_id: 34,
                type_name: "Tritanium".to_string(),
                region_id: 10_000_002,
                solar_system_id: 30_000_142,
                location_id: 60_003_760,
                observed_at: t1,
                etag: Some("etag".to_string()),
                expires_at: None,
                orders: vec![EsiMarketOrder::new(
                    101,
                    MarketOrderSide::Sell,
                    "4.2500",
                    100_000,
                    100_000,
                    1,
                    "station".to_string(),
                    t1,
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

    for book in [
        repository
            .get_source_order_book(workspace_id, source_id, 34, 60_003_760, None)
            .await
            .unwrap(),
        repository
            .get_source_order_books(workspace_id, source_id, &[34], 60_003_760, None)
            .await
            .unwrap()
            .remove(&34)
            .unwrap(),
    ] {
        assert_eq!(book.observed_at, t1, "physical provenance stays T1");
        assert_eq!(
            book.orders[0].observed_at, t1,
            "per-order provenance stays T1"
        );
        let revalidated_at = book.revalidated_at.expect("revalidated after the 304");
        assert!(revalidated_at >= wall_before);
        assert_eq!(book.effective_observed_at(), revalidated_at);
    }
}
