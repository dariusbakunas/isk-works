use super::common::*;

/// `resolve_scope_price_sources` is the standalone id-only half of the
/// same shim `scoped_order_books` uses -- "Request market data" needs
/// exactly this (which sources to call `register_and_refresh` against),
/// not full order books.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn resolve_scope_price_sources_finds_the_configured_source_and_nothing_else(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;

    let at_jita = repository
        .resolve_scope_price_sources(
            workspace_id,
            iskworks_core::MarketScope {
                region_id: 10_000_002,
                location_id: Some(60_003_760),
            },
        )
        .await
        .unwrap();
    assert_eq!(at_jita, vec![source_id]);

    let region_wide = repository
        .resolve_scope_price_sources(
            workspace_id,
            iskworks_core::MarketScope {
                region_id: 10_000_002,
                location_id: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(region_wide, vec![source_id]);

    let wrong_location = repository
        .resolve_scope_price_sources(
            workspace_id,
            iskworks_core::MarketScope {
                region_id: 10_000_002,
                location_id: Some(60_003_761),
            },
        )
        .await
        .unwrap();
    assert!(wrong_location.is_empty());

    let unconfigured_region = repository
        .resolve_scope_price_sources(
            workspace_id,
            iskworks_core::MarketScope {
                region_id: 10_000_009,
                location_id: None,
            },
        )
        .await
        .unwrap();
    assert!(unconfigured_region.is_empty());
}

/// `ensure_esi_price_source_for_scope` -- proves a second `esi_market_
/// orders` source can exist per workspace (one per scope), that it's
/// idempotent per scope, and that coverage registered against it dual-
/// writes the `region_id`/`location_id` columns.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn ensure_esi_price_source_for_scope_provisions_per_scope_not_per_workspace(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let jita_id = esi_source(&pool, workspace_id).await;
    let rens_scope = iskworks_core::MarketScope {
        region_id: 10_000_030,
        location_id: Some(60_004_588),
    };

    // A second scope gets its own source -- uniqueness is per scope, not
    // per workspace.
    let rens_id = repository
        .ensure_esi_price_source_for_scope(workspace_id, rens_scope)
        .await
        .unwrap();
    assert_ne!(jita_id, rens_id);

    // Idempotent: asking for the same scope again returns the same id.
    let rens_id_again = repository
        .ensure_esi_price_source_for_scope(workspace_id, rens_scope)
        .await
        .unwrap();
    assert_eq!(rens_id, rens_id_again);

    // Asking for Jita's own scope resolves the pre-existing fixture
    // source rather than creating a third one.
    let jita_again = repository
        .ensure_esi_price_source_for_scope(
            workspace_id,
            iskworks_core::MarketScope {
                region_id: 10_000_002,
                location_id: Some(60_003_760),
            },
        )
        .await
        .unwrap();
    assert_eq!(jita_again, jita_id);

    let resolved_scope = repository
        .resolve_source_scope(workspace_id, rens_id)
        .await
        .unwrap();
    assert_eq!(resolved_scope, rens_scope);

    repository
        .register_market_coverage(
            workspace_id,
            rens_id,
            vec![MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }],
        )
        .await
        .unwrap();
    let (region_id, location_id): (i64, i64) = sqlx::query_as(
        "SELECT region_id,location_id FROM market_source_coverage WHERE workspace_id=$1 AND price_source_id=$2 AND type_id=34",
    )
    .bind(workspace_id.0)
    .bind(rens_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(region_id, 10_000_030);
    assert_eq!(location_id, 60_004_588);
}

/// A region-wide scope (`location_id: None`) provisions a source using
/// the `0` sentinel rather than `NULL`, so it
/// behaves as an ordinary, distinct scope identity instead of silently
/// colliding with -- or being indistinguishable from -- any other scope.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn ensure_esi_price_source_for_scope_handles_region_wide_scope(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let region_wide = iskworks_core::MarketScope {
        region_id: 10_000_030,
        location_id: None,
    };

    let source_id = repository
        .ensure_esi_price_source_for_scope(workspace_id, region_wide)
        .await
        .unwrap();
    let resolved_scope = repository
        .resolve_source_scope(workspace_id, source_id)
        .await
        .unwrap();
    assert_eq!(resolved_scope, region_wide);

    let (location_id,): (i64,) = sqlx::query_as(
        "SELECT location_id FROM market_price_source_configs WHERE price_source_id=$1",
    )
    .bind(source_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(location_id, 0);

    // A concrete-location scope in the same region is a distinct source.
    let concrete_id = repository
        .ensure_esi_price_source_for_scope(
            workspace_id,
            iskworks_core::MarketScope {
                region_id: 10_000_030,
                location_id: Some(60_004_588),
            },
        )
        .await
        .unwrap();
    assert_ne!(source_id, concrete_id);
}
