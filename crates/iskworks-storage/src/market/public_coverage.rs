//! App-wide public market data: the `public_market_coverage` refresh
//! lifecycle and the `workspace_id IS NULL` batches it completes. One row per
//! `(region, type)` serves every workspace and every station or region-wide
//! scope in that region. Mirrors the per-workspace lifecycle in `refresh.rs`
//! (states, lease/claim guard, 304 revalidation, exponential backoff) so the
//! two can't drift.

use chrono::{DateTime, Utc};
use iskworks_core::{
    MarketCoverageItem, MarketCoverageRegistration, MarketError, MarketOrderObservationId,
    PublicMarketCoverageWork, PublicMarketObservationBatch,
};
use uuid::Uuid;

use super::convert::*;
use super::refresh::COVERAGE_DORMANT_AFTER;
use super::rows::*;
use super::PgMarketRepository;

#[derive(sqlx::FromRow)]
struct PublicCoverageRow {
    region_id: i64,
    #[sqlx(flatten)]
    coverage: CoverageRow,
}

impl PgMarketRepository {
    /// Records that some workspace needs these types' public order books in
    /// `region_id`: inserts missing rows and keeps existing ones out of
    /// dormancy. `prioritize` (an explicit "request market data", or a view
    /// the user is waiting on) also puts them ahead of routine work, due now
    /// or once `refresh_not_before` (ESI's `Expires`, or a failure's backoff)
    /// has passed -- a click never refetches early.
    pub async fn register_public_market_demand(
        &self,
        region_id: i64,
        items: Vec<MarketCoverageRegistration>,
        prioritize: bool,
        now: DateTime<Utc>,
    ) -> Result<(), MarketError> {
        if region_id <= 0 {
            return Err(MarketError::InvalidEsiOrder(
                "public market region is invalid".to_string(),
            ));
        }
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        for item in items {
            if item.type_id <= 0 || item.type_name.trim().is_empty() {
                return Err(MarketError::InvalidEsiOrder(
                    "coverage item identity is invalid".to_string(),
                ));
            }
            sqlx::query(
                r#"
                INSERT INTO public_market_coverage (
                  region_id,type_id,type_name,refresh_state,created_at,updated_at,last_needed_at
                ) VALUES ($1,$2,$3,'missing',$4,$4,$4)
                ON CONFLICT (region_id,type_id) DO UPDATE SET
                  type_name=EXCLUDED.type_name,
                  updated_at=EXCLUDED.updated_at,
                  last_needed_at=GREATEST(public_market_coverage.last_needed_at,EXCLUDED.last_needed_at)
                "#,
            )
            .bind(region_id)
            .bind(item.type_id)
            .bind(item.type_name.trim())
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;
            if prioritize {
                sqlx::query(
                    r#"UPDATE public_market_coverage
                       SET next_refresh_at=GREATEST($3,refresh_not_before),priority_requested_at=$3,updated_at=$3
                       WHERE region_id=$1 AND type_id=$2 AND refresh_state <> 'refreshing'"#,
                )
                .bind(region_id)
                .bind(item.type_id)
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(map_sqlx)?;
            }
        }
        tx.commit().await.map_err(map_sqlx)
    }

    /// Puts existing rows for `type_ids` ahead of routine work, due no
    /// earlier than `refresh_not_before` (rows mid-refresh are left alone).
    /// `false` when none were updated.
    pub async fn prioritize_public_market_refresh(
        &self,
        region_id: i64,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<bool, MarketError> {
        if type_ids.is_empty() {
            return Ok(false);
        }
        sqlx::query(
            r#"UPDATE public_market_coverage
               SET next_refresh_at=GREATEST($3,refresh_not_before),priority_requested_at=$3,updated_at=$3
               WHERE region_id=$1 AND type_id = ANY($2) AND refresh_state <> 'refreshing'"#,
        )
        .bind(region_id)
        .bind(type_ids)
        .bind(now)
        .execute(&self.pool)
        .await
        .map(|result| result.rows_affected() > 0)
        .map_err(map_sqlx)
    }

    /// Due, unleased (or lease-expired), non-dormant rows: prioritized first,
    /// then most overdue. No per-workspace fairness is needed -- every row
    /// serves every workspace.
    pub async fn due_public_market_work(
        &self,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<PublicMarketCoverageWork>, MarketError> {
        sqlx::query_as::<_, PublicCoverageRow>(
            r#"
            SELECT coverage.region_id,coverage.type_id,coverage.type_name,coverage.refresh_state,
                   batch.observed_at,coverage.last_attempted_at,
                   coverage.next_refresh_at,coverage.last_error,batch.etag,coverage.revalidated_at,
                   count(observation.id) order_count,
                   count(observation.id) FILTER (WHERE observation.order_side='buy') buy_order_count,
                   count(observation.id) FILTER (WHERE observation.order_side='sell') sell_order_count
            FROM public_market_coverage coverage
            LEFT JOIN market_observation_batches batch
              ON batch.id=coverage.last_completed_batch_id
            LEFT JOIN market_order_observations observation
              ON observation.observation_batch_id=batch.id
            WHERE (coverage.next_refresh_at IS NULL OR coverage.next_refresh_at <= $1)
              AND (coverage.refresh_state <> 'refreshing' OR coverage.lease_expires_at <= $1)
              AND coverage.last_needed_at > $1 - $3
            GROUP BY coverage.region_id,coverage.type_id,coverage.type_name,
                     coverage.refresh_state,batch.observed_at,coverage.last_attempted_at,
                     coverage.next_refresh_at,coverage.last_error,batch.etag,
                     coverage.revalidated_at,coverage.priority_requested_at
            ORDER BY coverage.priority_requested_at DESC NULLS LAST,
                     coverage.next_refresh_at NULLS FIRST,coverage.region_id,coverage.type_id
            LIMIT $2
            "#,
        )
        .bind(now)
        .bind(limit.max(0))
        .bind(COVERAGE_DORMANT_AFTER)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        .into_iter()
        .map(|row| {
            Ok(PublicMarketCoverageWork {
                region_id: row.region_id,
                item: row.coverage.into_item()?,
            })
        })
        .collect()
    }

    /// The app-wide coverage rows for `type_ids` in `region_id`, whatever
    /// their state -- what a caller shows as "already current", "refreshing"
    /// or "failed" for its public scope.
    pub async fn public_market_coverage(
        &self,
        region_id: i64,
        type_ids: &[i64],
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        if type_ids.is_empty() {
            return Ok(Vec::new());
        }
        sqlx::query_as::<_, PublicCoverageRow>(
            r#"
            SELECT coverage.region_id,coverage.type_id,coverage.type_name,coverage.refresh_state,
                   batch.observed_at,coverage.last_attempted_at,
                   coverage.next_refresh_at,coverage.last_error,batch.etag,coverage.revalidated_at,
                   count(observation.id) order_count,
                   count(observation.id) FILTER (WHERE observation.order_side='buy') buy_order_count,
                   count(observation.id) FILTER (WHERE observation.order_side='sell') sell_order_count
            FROM public_market_coverage coverage
            LEFT JOIN market_observation_batches batch
              ON batch.id=coverage.last_completed_batch_id
            LEFT JOIN market_order_observations observation
              ON observation.observation_batch_id=batch.id
            WHERE coverage.region_id=$1 AND coverage.type_id = ANY($2)
            GROUP BY coverage.region_id,coverage.type_id,coverage.type_name,
                     coverage.refresh_state,batch.observed_at,coverage.last_attempted_at,
                     coverage.next_refresh_at,coverage.last_error,batch.etag,
                     coverage.revalidated_at
            ORDER BY coverage.type_id
            "#,
        )
        .bind(region_id)
        .bind(type_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        .into_iter()
        .map(|row| row.coverage.into_item())
        .collect()
    }

    /// Leases one row for a refresh. Returns the claim (its new
    /// `last_attempted_at`) that the completing call must present, or `None`
    /// when it isn't due or another lease is still live.
    pub async fn begin_public_market_refresh(
        &self,
        region_id: i64,
        type_id: i64,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, MarketError> {
        sqlx::query_scalar::<_, DateTime<Utc>>(
            r#"
            UPDATE public_market_coverage
            SET refresh_state='refreshing',last_attempted_at=$3,lease_expires_at=$4,
                last_error=NULL,updated_at=$3
            WHERE region_id=$1 AND type_id=$2
              AND (refresh_state <> 'refreshing' OR lease_expires_at <= $3)
              AND (next_refresh_at IS NULL OR next_refresh_at <= $3)
            RETURNING last_attempted_at
            "#,
        )
        .bind(region_id)
        .bind(type_id)
        .bind(attempted_at)
        .bind(lease_expires_at)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)
    }

    /// Stores a fetched regional book app-wide and moves the row to
    /// `current`. `false` when `claim` is no longer the row's lease (another
    /// refresh took over); nothing is written then. The superseded public
    /// batch is collected unless something still references it.
    pub async fn complete_public_market_refresh(
        &self,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        batch: PublicMarketObservationBatch,
    ) -> Result<bool, MarketError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        let claimed = sqlx::query_scalar::<_, Option<Uuid>>(
            r#"SELECT last_completed_batch_id FROM public_market_coverage
               WHERE region_id=$1 AND type_id=$2 AND refresh_state='refreshing'
                 AND last_attempted_at=$3
               FOR UPDATE"#,
        )
        .bind(batch.region_id)
        .bind(batch.type_id)
        .bind(claim)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        let Some(superseded_batch_id) = claimed else {
            tx.rollback().await.map_err(map_sqlx)?;
            return Ok(false);
        };
        let completed_at = crate::db_now();
        sqlx::query(
            r#"
            INSERT INTO market_observation_batches (
              id,workspace_id,price_source_id,origin,status,type_id,captured_type_name,
              region_id,solar_system_id,location_id,observed_at,attempted_at,
              completed_at,etag,expires_at
            ) VALUES (
              $1,NULL,NULL,'esi_market_orders','completed',$2,$3,$4,0,0,$5,$5,$6,$7,$8
            )
            "#,
        )
        .bind(batch.id.0)
        .bind(batch.type_id)
        .bind(&batch.type_name)
        .bind(batch.region_id)
        .bind(batch.observed_at)
        .bind(completed_at)
        .bind(&batch.etag)
        .bind(batch.expires_at)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        for order in batch.orders {
            let order_range = esi_order_range(&order.order_range)?;
            let checksum = format!("esi:{}:{}", batch.id.0, order.order_id);
            sqlx::query(
                r#"
                INSERT INTO market_order_observations (
                  id,workspace_id,observation_batch_id,source_kind,observed_at,
                  imported_at,order_id,type_id,captured_type_name,order_side,price,
                  remaining_volume,entered_volume,minimum_volume,order_range,
                  issued_at,duration_days,location_id,solar_system_id,region_id,
                  jumps,normalized_row_checksum
                ) VALUES (
                  $1,NULL,$2,'esi_market_orders',$3,$4,$5,$6,$7,$8,$9,$10,$11,
                  $12,$13,$14,$15,$16,$17,$18,0,$19
                )
                "#,
            )
            .bind(MarketOrderObservationId::new().0)
            .bind(batch.id.0)
            .bind(batch.observed_at)
            .bind(completed_at)
            .bind(order.order_id)
            .bind(batch.type_id)
            .bind(&batch.type_name)
            .bind(order_side_str(order.side))
            .bind(order.price.0)
            .bind(i64_from_u64(order.remaining_volume)?)
            .bind(i64_from_u64(order.entered_volume)?)
            .bind(i64_from_u64(order.minimum_volume)?)
            .bind(order_range)
            .bind(order.issued_at)
            .bind(i32::try_from(order.duration_days).map_err(|_| overflow())?)
            .bind(order.location_id)
            .bind(order.solar_system_id)
            .bind(batch.region_id)
            .bind(checksum)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;
        }
        sqlx::query(
            r#"
            UPDATE public_market_coverage
            SET refresh_state='current',last_attempted_at=$3,
                next_refresh_at=GREATEST($6,$8),refresh_not_before=$8,
                lease_expires_at=NULL,priority_requested_at=NULL,last_completed_batch_id=$4,
                last_error=NULL,revalidated_at=$7,consecutive_failures=0,updated_at=$3
            WHERE region_id=$1 AND type_id=$2
              AND refresh_state='refreshing' AND last_attempted_at=$5
            "#,
        )
        .bind(batch.region_id)
        .bind(batch.type_id)
        .bind(completed_at)
        .bind(batch.id.0)
        .bind(claim)
        .bind(next_refresh_at)
        // A fresh 200 confirms the book as of the moment it was observed.
        .bind(batch.observed_at)
        // ESI's `Expires`: never refetched before it.
        .bind(batch.expires_at)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        if let Some(superseded_batch_id) = superseded_batch_id {
            delete_unreferenced_batch(&mut tx, superseded_batch_id).await?;
        }
        tx.commit().await.map_err(map_sqlx)?;
        Ok(true)
    }

    /// ESI answered `304 Not Modified`: the current book stays authoritative
    /// and is confirmed as of now, until the 304's `cache_expires_at`. Same
    /// claim guard as completion.
    pub async fn revalidate_public_market_refresh(
        &self,
        region_id: i64,
        type_id: i64,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        cache_expires_at: Option<DateTime<Utc>>,
    ) -> Result<bool, MarketError> {
        let revalidated_at = crate::db_now();
        sqlx::query(
            r#"
            UPDATE public_market_coverage
            SET refresh_state='current',last_attempted_at=$3,
                next_refresh_at=GREATEST($4,$6),refresh_not_before=$6,
                lease_expires_at=NULL,priority_requested_at=NULL,last_error=NULL,
                revalidated_at=$3,consecutive_failures=0,updated_at=$3
            WHERE region_id=$1 AND type_id=$2
              AND refresh_state='refreshing' AND last_attempted_at=$5
            "#,
        )
        .bind(region_id)
        .bind(type_id)
        .bind(revalidated_at)
        .bind(next_refresh_at)
        .bind(claim)
        .bind(cache_expires_at)
        .execute(&self.pool)
        .await
        .map(|result| result.rows_affected() == 1)
        .map_err(map_sqlx)
    }

    /// A failed refresh: `next_refresh_at` is the caller's floor (it may carry
    /// a server-directed wait), and consecutive failures back off 1, 2, 4...
    /// minutes, capped at 6 hours, exactly as per-workspace coverage does.
    /// The same wait becomes `refresh_not_before`, so prioritizing can't cut
    /// it short.
    pub async fn fail_public_market_refresh(
        &self,
        region_id: i64,
        type_id: i64,
        claim: DateTime<Utc>,
        attempted_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        error_message: String,
    ) -> Result<bool, MarketError> {
        let error_message: String = error_message.chars().take(500).collect();
        sqlx::query(
            r#"
            UPDATE public_market_coverage
            SET refresh_state='failed',last_attempted_at=$3,
                next_refresh_at=GREATEST(
                  $4,
                  $3 + LEAST(
                    interval '1 minute' * power(2, LEAST(consecutive_failures, 16)),
                    interval '6 hours'
                  )
                ),
                refresh_not_before=GREATEST(
                  $4,
                  $3 + LEAST(
                    interval '1 minute' * power(2, LEAST(consecutive_failures, 16)),
                    interval '6 hours'
                  )
                ),
                consecutive_failures=consecutive_failures + 1,
                lease_expires_at=NULL,priority_requested_at=NULL,last_error=$5,updated_at=$3
            WHERE region_id=$1 AND type_id=$2
              AND refresh_state='refreshing' AND last_attempted_at=$6
            "#,
        )
        .bind(region_id)
        .bind(type_id)
        .bind(attempted_at)
        .bind(next_refresh_at)
        .bind(error_message)
        .bind(claim)
        .execute(&self.pool)
        .await
        .map(|result| result.rows_affected() == 1)
        .map_err(map_sqlx)
    }
}

/// Deletes a superseded batch and its observations unless anything still
/// reads them: a per-workspace or public coverage row's current batch, an
/// import-file link, or a price-snapshot line. Shared by both completion
/// paths so neither can collect what the other still uses.
pub(super) async fn delete_unreferenced_batch(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    batch_id: Uuid,
) -> Result<(), MarketError> {
    sqlx::query(
        r#"
        DELETE FROM market_order_observations o
        WHERE o.observation_batch_id = $1
          AND NOT EXISTS (SELECT 1 FROM market_source_coverage c WHERE c.last_completed_batch_id = $1)
          AND NOT EXISTS (SELECT 1 FROM public_market_coverage p WHERE p.last_completed_batch_id = $1)
          AND NOT EXISTS (
            SELECT 1 FROM market_import_file_observations l WHERE l.market_order_observation_id = o.id
          )
          AND NOT EXISTS (
            SELECT 1 FROM market_price_snapshot_observations s WHERE s.market_order_observation_id = o.id
          )
        "#,
    )
    .bind(batch_id)
    .execute(&mut **tx)
    .await
    .map_err(map_sqlx)?;
    sqlx::query(
        r#"
        DELETE FROM market_observation_batches b
        WHERE b.id = $1
          AND NOT EXISTS (SELECT 1 FROM market_source_coverage c WHERE c.last_completed_batch_id = b.id)
          AND NOT EXISTS (SELECT 1 FROM public_market_coverage p WHERE p.last_completed_batch_id = b.id)
          AND NOT EXISTS (SELECT 1 FROM market_order_observations o WHERE o.observation_batch_id = b.id)
        "#,
    )
    .bind(batch_id)
    .execute(&mut **tx)
    .await
    .map_err(map_sqlx)?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct PublicBookBatchRow {
    id: Uuid,
    type_id: i64,
    captured_type_name: String,
    region_id: i64,
    observed_at: DateTime<Utc>,
    revalidated_at: Option<DateTime<Utc>>,
}

impl PgMarketRepository {
    /// The region whose app-wide book serves `scope`, or `None` when it
    /// stays per workspace (`MarketScope::public_region`).
    pub(super) async fn public_region_for_scope(
        &self,
        workspace_id: iskworks_core::WorkspaceId,
        scope: iskworks_core::MarketScope,
    ) -> Result<Option<i64>, MarketError> {
        let classification = match scope.location_id {
            Some(location_id) => Some(self.classify_location(workspace_id, location_id).await?),
            None => None,
        };
        Ok(scope.public_region(classification))
    }

    /// For a workspace's ESI price source whose scope is public, its region
    /// and display alias; `None` for a structure (or unknown) source.
    pub(super) async fn public_source_region(
        &self,
        workspace_id: iskworks_core::WorkspaceId,
        source_id: iskworks_core::PriceSourceId,
    ) -> Result<Option<(i64, String)>, MarketError> {
        let Some((region_id, location_id, alias)) = sqlx::query_as::<_, (i64, i64, String)>(
            r#"SELECT region_id,location_id,location_alias FROM market_price_source_configs
               WHERE workspace_id=$1 AND price_source_id=$2"#,
        )
        .bind(workspace_id.0)
        .bind(source_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?
        else {
            return Ok(None);
        };
        let scope = iskworks_core::MarketScope {
            region_id,
            location_id: (location_id != 0).then_some(location_id),
        };
        Ok(self
            .public_region_for_scope(workspace_id, scope)
            .await?
            .map(|region_id| (region_id, alias)))
    }

    /// App-wide order books for `type_ids` in `region_id`: per type, the
    /// newest completed public batch at or before `observed_cutoff` (the
    /// newest overall when `None`), with its orders filtered to
    /// `location_id` unless it is `0` (region-wide). A type with no public
    /// book is absent from the map.
    pub(super) async fn public_order_books(
        &self,
        region_id: i64,
        location_id: i64,
        location_name: &str,
        type_ids: &[i64],
        observed_cutoff: Option<DateTime<Utc>>,
    ) -> Result<std::collections::BTreeMap<i64, iskworks_core::MarketOrderBook>, MarketError> {
        use iskworks_core::{MarketOrderBook, MarketOrderSide, MarketOrderView};
        use std::collections::BTreeMap;
        if type_ids.is_empty() {
            return Ok(BTreeMap::new());
        }
        let batches = sqlx::query_as::<_, PublicBookBatchRow>(
            r#"
            SELECT batch.id,batch.type_id,batch.captured_type_name,batch.region_id,
                   batch.observed_at,coverage.revalidated_at
            FROM public_market_coverage coverage
            JOIN LATERAL (
              SELECT b.id,b.type_id,b.captured_type_name,b.region_id,b.observed_at
              FROM market_observation_batches b
              WHERE b.workspace_id IS NULL AND b.region_id=coverage.region_id
                AND b.type_id=coverage.type_id AND b.status='completed'
                AND ($3::timestamptz IS NULL OR b.observed_at<=$3::timestamptz)
              ORDER BY b.observed_at DESC
              LIMIT 1
            ) batch ON true
            WHERE coverage.region_id=$1 AND coverage.type_id=ANY($2)
            "#,
        )
        .bind(region_id)
        .bind(type_ids)
        .bind(observed_cutoff)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;
        if batches.is_empty() {
            return Ok(BTreeMap::new());
        }
        let batch_ids: Vec<Uuid> = batches.iter().map(|batch| batch.id).collect();
        let mut orders_by_batch: BTreeMap<Uuid, Vec<MarketOrderView>> = BTreeMap::new();
        for row in sqlx::query_as::<_, BatchedEsiOrderRow>(
            r#"
            SELECT id,observation_batch_id,order_id,type_id,captured_type_name,order_side,
                   price,remaining_volume,entered_volume,minimum_volume,order_range,
                   issued_at,duration_days,observed_at,location_id,solar_system_id,region_id,jumps
            FROM market_order_observations
            WHERE workspace_id IS NULL AND observation_batch_id=ANY($1)
              AND ($2::bigint = 0 OR location_id=$2)
            ORDER BY observation_batch_id,
                     CASE WHEN order_side='sell' THEN 0 ELSE 1 END,
                     CASE WHEN order_side='sell' THEN price END ASC,
                     CASE WHEN order_side='buy' THEN price END DESC,
                     order_id
            "#,
        )
        .bind(&batch_ids)
        .bind(location_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        {
            let batch_id = row.observation_batch_id;
            orders_by_batch
                .entry(batch_id)
                .or_default()
                .push(row.into_order()?);
        }
        Ok(batches
            .into_iter()
            .map(|batch| {
                let mut orders = orders_by_batch.remove(&batch.id).unwrap_or_default();
                // A 304 confirms the whole regional snapshot, every order in it.
                for order in &mut orders {
                    order.revalidated_at = batch.revalidated_at;
                }
                let solar_system_id = if location_id == 0 {
                    0
                } else {
                    orders.first().map_or(0, |order| order.solar_system_id)
                };
                let buys: Vec<_> = orders
                    .iter()
                    .filter(|order| order.side == MarketOrderSide::Buy)
                    .collect();
                let sells: Vec<_> = orders
                    .iter()
                    .filter(|order| order.side == MarketOrderSide::Sell)
                    .collect();
                let book = MarketOrderBook {
                    type_id: batch.type_id,
                    type_name: batch.captured_type_name,
                    location_id,
                    location_name: if location_name.trim().is_empty() {
                        format!("Station {location_id}")
                    } else {
                        location_name.to_string()
                    },
                    solar_system_id,
                    region_id: batch.region_id,
                    observed_at: batch.observed_at,
                    revalidated_at: batch.revalidated_at,
                    observation_batch_id: iskworks_core::MarketObservationBatchId(batch.id),
                    import_batch_id: None,
                    imported_file_id: None,
                    buy_order_count: buys.len() as u64,
                    sell_order_count: sells.len() as u64,
                    total_buy_volume: buys.iter().map(|order| order.remaining_volume).sum(),
                    total_sell_volume: sells.iter().map(|order| order.remaining_volume).sum(),
                    lowest_sell: sells.iter().map(|order| order.price).min(),
                    highest_buy: buys.iter().map(|order| order.price).max(),
                    orders,
                };
                (batch.type_id, book)
            })
            .collect())
    }
}
