use super::common::*;

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn import_is_duplicate_safe_and_snapshot_retains_observation_links(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let service = MarketService::new(repository.clone());
    let mut current_upload = upload();
    current_upload.filename = "Tritanium.txt".to_string();
    current_upload.user_observed_at = Some(crate::db_now());
    let first = service
        .import(workspace_id, vec![current_upload.clone()])
        .await
        .unwrap();
    assert_eq!(first.imported_observations, 6);
    let duplicate = service
        .import(workspace_id, vec![current_upload])
        .await
        .unwrap();
    assert!(duplicate.batch.is_none());
    assert_eq!(duplicate.skipped_duplicate_files, 1);
    let observation_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM market_order_observations")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(observation_count, 6);

    let scope = iskworks_core::MarketScope {
        region_id: 10_000_009,
        location_id: Some(1_049_588_174_021),
    };
    let build_prices = PgIndustryRepository::new(pool.clone())
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
    assert_eq!(build_prices.len(), 1);
    assert_eq!(build_prices[0].price, Money::parse("3.9709").unwrap());
    assert!(build_prices[0].note.contains("requested 250000000"));
    // Staleness is proved separately, in
    // `derive_market_price_items_flags_stale_observations_using_the_orders_own_timestamp`
    // below: `market_order_observations` is append-only/immutable
    // (`reject_market_evidence_mutation`), so it can't be retroactively
    // aged via UPDATE; and `market_import_files.observed_at` alone is not
    // what freshness is computed from (it has to be the
    // orders' own timestamp, since `scoped_order_books` can merge
    // orders from multiple files/batches with no single file-level time
    // to trust for a region-wide scope).
}
