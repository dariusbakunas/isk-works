//! Market coverage registration and state: the shared upsert body plus the
//! trait entry points and coverage-row reads.

use iskworks_core::{
    MarketCoverageItem, MarketCoverageRegistration, MarketError, PriceSourceId, WorkspaceId,
};
use sqlx::PgPool;

use super::convert::*;
use super::rows::*;
use super::PgMarketRepository;

impl PgMarketRepository {
    pub(super) async fn register_market_coverage(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        upsert_coverage_rows(&self.pool, workspace_id, source_id, items).await?;
        load_coverage(&self.pool, workspace_id, source_id).await
    }

    pub(super) async fn upsert_market_coverage(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<(), MarketError> {
        upsert_coverage_rows(&self.pool, workspace_id, source_id, items).await
    }
}
/// The upsert body shared by `register_market_coverage` (which reloads the
/// full coverage list afterward) and `upsert_market_coverage` (which
/// doesn't) -- kept as one function so the two can never drift apart on
/// what "registering coverage" actually writes.
async fn upsert_coverage_rows(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    source_id: PriceSourceId,
    items: Vec<MarketCoverageRegistration>,
) -> Result<(), MarketError> {
    let source_kind = sqlx::query_scalar::<_, String>(
        "SELECT source_kind FROM price_sources WHERE workspace_id=$1 AND id=$2",
    )
    .bind(workspace_id.0)
    .bind(source_id.0)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(MarketError::PriceSourceNotFound)?;
    if source_kind != "esi_market_orders" {
        return Err(MarketError::InvalidUpload(
            "Automatic coverage requires an ESI market source.".to_string(),
        ));
    }
    // The source's own scope, denormalized onto every coverage row it
    // owns -- a source's configured
    // region/location never changes after creation, so this is safe to
    // stamp once here rather than re-resolving on every subsequent
    // refresh/complete.
    let (region_id, location_id) = sqlx::query_as::<_, (i64, i64)>(
        "SELECT region_id, location_id FROM market_price_source_configs WHERE price_source_id=$1",
    )
    .bind(source_id.0)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    let mut tx = pool.begin().await.map_err(map_sqlx)?;
    let now = crate::db_now();
    for item in items {
        if item.type_id <= 0 || item.type_name.trim().is_empty() {
            return Err(MarketError::InvalidEsiOrder(
                "coverage item identity is invalid".to_string(),
            ));
        }
        sqlx::query(
            r#"
            INSERT INTO market_source_coverage (
              workspace_id,price_source_id,type_id,type_name,refresh_state,
              region_id,location_id,created_at,updated_at,last_needed_at
            ) VALUES ($1,$2,$3,$4,'missing',$6,$7,$5,$5,$5)
            ON CONFLICT (workspace_id,price_source_id,type_id) DO UPDATE SET
              type_name=EXCLUDED.type_name,
              region_id=EXCLUDED.region_id,
              location_id=EXCLUDED.location_id,
              updated_at=EXCLUDED.updated_at,
              last_needed_at=EXCLUDED.last_needed_at
            "#,
        )
        .bind(workspace_id.0)
        .bind(source_id.0)
        .bind(item.type_id)
        .bind(item.type_name.trim())
        .bind(now)
        .bind(region_id)
        .bind(location_id)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
    }
    tx.commit().await.map_err(map_sqlx)?;
    Ok(())
}
pub(super) async fn load_coverage(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    source_id: PriceSourceId,
) -> Result<Vec<MarketCoverageItem>, MarketError> {
    sqlx::query_as::<_, CoverageRow>(
        r#"
        SELECT coverage.type_id,coverage.type_name,coverage.refresh_state,
               batch.observed_at,coverage.last_attempted_at,
               coverage.next_refresh_at,coverage.last_error,batch.etag,coverage.revalidated_at,
               count(observation.id) order_count,
               count(observation.id) FILTER (
                 WHERE observation.order_side='buy'
               ) buy_order_count,
               count(observation.id) FILTER (
                 WHERE observation.order_side='sell'
               ) sell_order_count
        FROM market_source_coverage coverage
        LEFT JOIN market_observation_batches batch
          ON batch.id=coverage.last_completed_batch_id
        LEFT JOIN market_order_observations observation
          ON observation.observation_batch_id=batch.id
        WHERE coverage.workspace_id=$1 AND coverage.price_source_id=$2
        GROUP BY coverage.workspace_id,coverage.price_source_id,
                 coverage.type_id,coverage.type_name,coverage.refresh_state,
                 batch.observed_at,coverage.last_attempted_at,
                 coverage.next_refresh_at,coverage.last_error,batch.etag
        ORDER BY coverage.type_id
        "#,
    )
    .bind(workspace_id.0)
    .bind(source_id.0)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?
    .into_iter()
    .map(CoverageRow::into_item)
    .collect()
}
