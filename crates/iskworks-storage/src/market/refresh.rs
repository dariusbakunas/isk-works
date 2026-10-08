//! Market refresh-lifecycle persistence: due/candidate selection, priority
//! and claim semantics, and the begin/complete/fail refresh transitions.
//! Queue ordering, locking, and the ESI completion transaction are unchanged.

use chrono::{DateTime, Utc};
use iskworks_core::{
    EsiMarketObservationBatch, MarketCoverageItem, MarketError, MarketOrderObservationId,
    MarketRefreshFailure, PriceSourceId, WorkspaceId,
};
use uuid::Uuid;

use super::convert::*;
use super::public_coverage::delete_unreferenced_batch;
use super::rows::*;
use super::PgMarketRepository;

/// Coverage nobody has needed for this long goes dormant: the worker stops
/// refreshing it (its last prices stay, just staler) until something
/// registers it again. Keeps the shared ESI budget on what's actually used.
pub(super) const COVERAGE_DORMANT_AFTER: std::time::Duration =
    std::time::Duration::from_secs(7 * 24 * 60 * 60);

impl PgMarketRepository {
    pub async fn due_market_sources(
        &self,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<(WorkspaceId, PriceSourceId)>, MarketError> {
        // Fair across workspaces: each workspace's sources are ranked by its
        // own urgency (a priority bump only reorders that workspace's
        // sources), then the batch takes every workspace's first-ranked
        // source before anyone's second, most overdue first within a round.
        // Dormant coverage (see `COVERAGE_DORMANT_AFTER`) isn't due at all.
        sqlx::query_as::<_, (Uuid, Uuid)>(
            r#"WITH due AS (
                 SELECT workspace_id,price_source_id,
                        max(priority_requested_at) priority_requested_at,
                        min(next_refresh_at) next_refresh_at
                 FROM market_source_coverage
                 WHERE (next_refresh_at IS NULL OR next_refresh_at <= $1)
                   AND (
                     refresh_state <> 'refreshing'
                     OR lease_expires_at <= $1
                   )
                   AND last_needed_at > $1 - $3
                 GROUP BY workspace_id,price_source_id
               ),
               ranked AS (
                 SELECT due.*,
                        row_number() OVER (
                          PARTITION BY workspace_id
                          ORDER BY priority_requested_at DESC NULLS LAST,
                                   next_refresh_at NULLS FIRST,price_source_id
                        ) workspace_turn
                 FROM due
               )
               SELECT workspace_id,price_source_id
               FROM ranked
               ORDER BY workspace_turn,next_refresh_at NULLS FIRST,
                        workspace_id,price_source_id
               LIMIT $2"#,
        )
        .bind(now)
        .bind(limit.max(0))
        .bind(COVERAGE_DORMANT_AFTER)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)
        .map(|rows| {
            rows.into_iter()
                .map(|(workspace_id, source_id)| {
                    (WorkspaceId(workspace_id), PriceSourceId(source_id))
                })
                .collect()
        })
    }

    pub(super) async fn market_refresh_candidates(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        now: DateTime<Utc>,
        limit: i64,
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
              AND (
                coverage.refresh_state <> 'refreshing'
                OR coverage.lease_expires_at <= $3
              )
              AND (
                coverage.next_refresh_at IS NULL
                OR coverage.next_refresh_at <= $3
              )
              AND coverage.last_needed_at > $3 - $5
            GROUP BY coverage.workspace_id,coverage.price_source_id,
                     coverage.type_id,coverage.type_name,coverage.refresh_state,
                     batch.observed_at,coverage.last_attempted_at,
                     coverage.next_refresh_at,coverage.last_error,batch.etag
            ORDER BY coverage.priority_requested_at DESC NULLS LAST,
                     coverage.next_refresh_at NULLS FIRST,coverage.type_id
            LIMIT $4
            "#,
        )
        .bind(workspace_id.0)
        .bind(source_id.0)
        .bind(now)
        .bind(limit.max(0))
        .bind(COVERAGE_DORMANT_AFTER)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        .into_iter()
        .map(CoverageRow::into_item)
        .collect()
    }

    pub(super) async fn market_refresh_candidates_for_type_ids(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        if type_ids.is_empty() {
            return Ok(Vec::new());
        }
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
              AND coverage.type_id = ANY($3)
              AND (
                coverage.refresh_state <> 'refreshing'
                OR coverage.lease_expires_at <= $4
              )
            GROUP BY coverage.workspace_id,coverage.price_source_id,
                     coverage.type_id,coverage.type_name,coverage.refresh_state,
                     batch.observed_at,coverage.last_attempted_at,
                     coverage.next_refresh_at,coverage.last_error,batch.etag
            "#,
        )
        .bind(workspace_id.0)
        .bind(source_id.0)
        .bind(type_ids)
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        .into_iter()
        .map(CoverageRow::into_item)
        .collect()
    }

    pub(super) async fn prioritize_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<bool, MarketError> {
        if type_ids.is_empty() {
            return Ok(false);
        }
        sqlx::query_scalar::<_, i32>(
            r#"UPDATE market_source_coverage
               SET next_refresh_at=GREATEST($3,refresh_not_before),priority_requested_at=$3,updated_at=$3
               WHERE workspace_id=$1 AND price_source_id=$2
                 AND type_id = ANY($4)
                 AND refresh_state <> 'refreshing'
               RETURNING 1"#,
        )
        .bind(workspace_id.0)
        .bind(source_id.0)
        .bind(now)
        .bind(type_ids)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.is_some())
        .map_err(map_sqlx)
    }

    pub(super) async fn begin_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_id: i64,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, MarketError> {
        sqlx::query_scalar::<_, DateTime<Utc>>(
            r#"
            UPDATE market_source_coverage
            SET refresh_state='refreshing',last_attempted_at=$4,lease_expires_at=$5,
                last_error=NULL,updated_at=$4
            WHERE workspace_id=$1 AND price_source_id=$2 AND type_id=$3
              AND (
                refresh_state <> 'refreshing'
                OR lease_expires_at <= $4
              )
              AND (next_refresh_at IS NULL OR next_refresh_at <= $4)
            RETURNING last_attempted_at
            "#,
        )
        .bind(workspace_id.0)
        .bind(source_id.0)
        .bind(type_id)
        .bind(attempted_at)
        .bind(lease_expires_at)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)
    }

    pub(super) async fn complete_esi_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        batch: EsiMarketObservationBatch,
    ) -> Result<bool, MarketError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        // Locks the coverage row for this refresh and hands back the batch it
        // is about to supersede -- `None` row means the claim isn't ours
        // (someone else completed or reset it), `Some(None)` is the first
        // refresh for this type.
        let claimed = sqlx::query_scalar::<_, Option<Uuid>>(
            "SELECT last_completed_batch_id FROM market_source_coverage WHERE workspace_id=$1 AND price_source_id=$2 AND type_id=$3 AND refresh_state='refreshing' AND last_attempted_at=$4 FOR UPDATE",
        )
        .bind(workspace_id.0).bind(batch.source_id.0).bind(batch.type_id).bind(claim)
        .fetch_optional(&mut *tx).await.map_err(map_sqlx)?;
        let Some(superseded_batch_id) = claimed else {
            tx.rollback().await.map_err(map_sqlx)?;
            return Ok(false);
        };
        let configured_location = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT config.location_id
            FROM market_price_source_configs config
            JOIN price_sources source ON source.id=config.price_source_id
            WHERE source.workspace_id=$1 AND source.id=$2
              AND source.source_kind='esi_market_orders'
            "#,
        )
        .bind(workspace_id.0)
        .bind(batch.source_id.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx)?
        .ok_or(MarketError::PriceSourceNotFound)?;
        if configured_location != batch.location_id {
            return Err(MarketError::InvalidEsiOrder(
                "observation batch location does not match its source".to_string(),
            ));
        }
        let completed_at = crate::db_now();
        sqlx::query(
            r#"
            INSERT INTO market_observation_batches (
              id,workspace_id,price_source_id,origin,status,type_id,captured_type_name,
              region_id,solar_system_id,location_id,observed_at,attempted_at,
              completed_at,etag,expires_at
            ) VALUES (
              $1,$2,$3,'esi_market_orders','completed',$4,$5,$6,$7,$8,$9,$9,
              $10,$11,$12
            )
            "#,
        )
        .bind(batch.id.0)
        .bind(workspace_id.0)
        .bind(batch.source_id.0)
        .bind(batch.type_id)
        .bind(&batch.type_name)
        .bind(batch.region_id)
        .bind(batch.solar_system_id)
        .bind(batch.location_id)
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
                  $1,$2,$3,'esi_market_orders',$4,$5,$6,$7,$8,$9,$10,$11,$12,
                  $13,$14,$15,$16,$17,$18,$19,0,$20
                )
                "#,
            )
            .bind(MarketOrderObservationId::new().0)
            .bind(workspace_id.0)
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
            UPDATE market_source_coverage
            SET refresh_state='current',last_attempted_at=$4,
                next_refresh_at=GREATEST($7,$9),refresh_not_before=$9,lease_expires_at=NULL,
                priority_requested_at=NULL,last_completed_batch_id=$5,last_error=NULL,
                revalidated_at=$8,consecutive_failures=0,updated_at=$4
            WHERE workspace_id=$1 AND price_source_id=$2 AND type_id=$3
              AND refresh_state='refreshing' AND last_attempted_at=$6
            "#,
        )
        .bind(workspace_id.0)
        .bind(batch.source_id.0)
        .bind(batch.type_id)
        .bind(completed_at)
        .bind(batch.id.0)
        .bind(claim)
        .bind(next_refresh_at)
        // A fresh 200 confirms the snapshot as of the moment it was
        // observed: `revalidated_at == observed_at` immediately after.
        .bind(batch.observed_at)
        // ESI's `Expires`: never refetched before it.
        .bind(batch.expires_at)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        // Garbage-collect the batch this refresh just superseded; a no-op for
        // anything still in use (see `delete_unreferenced_batch`).
        if let Some(superseded_batch_id) = superseded_batch_id {
            delete_unreferenced_batch(&mut tx, superseded_batch_id).await?;
        }
        tx.commit().await.map_err(map_sqlx)?;
        Ok(true)
    }

    pub(super) async fn fail_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        failure: MarketRefreshFailure,
    ) -> Result<bool, MarketError> {
        let MarketRefreshFailure {
            type_id,
            claim,
            attempted_at,
            next_refresh_at,
            error_message,
        } = failure;
        let error_message: String = error_message.chars().take(500).collect();
        // `next_refresh_at` is the caller's floor (it may carry a
        // server-directed wait); on top of that, consecutive failures back
        // off exponentially -- 1, 2, 4... minutes, capped at 6 hours -- so a
        // row that keeps failing stops spending the shared ESI error budget
        // every minute. The right-hand side sees the pre-update count. The
        // same wait becomes `refresh_not_before`, so prioritizing can't cut
        // it short.
        sqlx::query(
            r#"
            UPDATE market_source_coverage
            SET refresh_state='failed',last_attempted_at=$4,
                next_refresh_at=GREATEST(
                  $5,
                  $4 + LEAST(
                    interval '1 minute' * power(2, LEAST(consecutive_failures, 16)),
                    interval '6 hours'
                  )
                ),
                refresh_not_before=GREATEST(
                  $5,
                  $4 + LEAST(
                    interval '1 minute' * power(2, LEAST(consecutive_failures, 16)),
                    interval '6 hours'
                  )
                ),
                consecutive_failures=consecutive_failures + 1,
                lease_expires_at=NULL,priority_requested_at=NULL,last_error=$6,updated_at=$4
            WHERE workspace_id=$1 AND price_source_id=$2 AND type_id=$3
              AND refresh_state='refreshing' AND last_attempted_at=$7
            "#,
        )
        .bind(workspace_id.0)
        .bind(source_id.0)
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

    /// ESI answered a conditional refresh with `304 Not Modified`: the
    /// batch at `last_completed_batch_id` is still authoritative, so this
    /// writes **no** `market_observation_batches` / `market_order_observations`
    /// row and does **not** touch `last_completed_batch_id`. A single
    /// coverage UPDATE -- same claim guard as `complete_esi_market_refresh`
    /// (`refresh_state='refreshing' AND last_attempted_at=$claim`) -- moves
    /// the row back to `current`, clears `last_error`/lease/priority state
    /// and advances `last_attempted_at` (to the revalidation time) and
    /// `next_refresh_at`. No transaction: nothing else is written.
    pub(super) async fn revalidate_esi_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_id: i64,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        cache_expires_at: Option<DateTime<Utc>>,
    ) -> Result<bool, MarketError> {
        let revalidated_at = crate::db_now();
        sqlx::query(
            r#"
            UPDATE market_source_coverage
            SET refresh_state='current',last_attempted_at=$4,
                next_refresh_at=GREATEST($5,$7),refresh_not_before=$7,
                lease_expires_at=NULL,priority_requested_at=NULL,last_error=NULL,
                revalidated_at=$4,consecutive_failures=0,updated_at=$4
            WHERE workspace_id=$1 AND price_source_id=$2 AND type_id=$3
              AND refresh_state='refreshing' AND last_attempted_at=$6
            "#,
        )
        .bind(workspace_id.0)
        .bind(source_id.0)
        .bind(type_id)
        // The 304 confirms the existing snapshot as of *now*; `observed_at`
        // stays put so `effective = GREATEST(observed_at, revalidated_at)`
        // is this instant.
        .bind(revalidated_at)
        .bind(next_refresh_at)
        .bind(claim)
        .bind(cache_expires_at)
        .execute(&self.pool)
        .await
        .map(|result| result.rows_affected() == 1)
        .map_err(map_sqlx)
    }
}
