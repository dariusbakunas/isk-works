use super::common::*;

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn esi_coverage_registration_is_idempotent(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    let registration = MarketCoverageRegistration {
        type_id: 34,
        type_name: "Tritanium".to_string(),
    };

    repository
        .register_market_coverage(workspace_id, source_id, vec![registration.clone()])
        .await
        .unwrap();
    let rows = repository
        .register_market_coverage(workspace_id, source_id, vec![registration])
        .await
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].type_id, 34);
    assert_eq!(rows[0].type_name, "Tritanium");
    assert_eq!(rows[0].refresh_state, MarketRefreshState::Missing);
    assert!(rows[0].next_refresh_at.is_none());
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM market_source_coverage WHERE price_source_id=$1 AND type_id=34",
    )
    .bind(source_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}
