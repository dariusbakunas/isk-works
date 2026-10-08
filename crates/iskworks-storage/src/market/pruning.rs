//! Orphaned market-observation pruning. The chunked pruning transaction and
//! its limits are unchanged.

use chrono::{DateTime, Utc};
use iskworks_core::MarketError;

use super::convert::*;
use super::MarketObservationPruneOutcome;
use super::PgMarketRepository;

impl PgMarketRepository {
    /// Garbage-collect market observation batches that no coverage row
    /// (per-workspace or public) still points at and that nothing (import file / price snapshot) references.
    /// `complete_esi_market_refresh` already prunes the batch each refresh
    /// supersedes; this is the periodic backstop that also clears a
    /// pre-existing backlog and batches left behind by a type that stopped
    /// refreshing.
    ///
    /// Works **per batch**: each chunk picks up to `batches_per_chunk` old
    /// orphan batches and deletes their observations via the
    /// `observation_batch_id` index (an index range scan over a few thousand
    /// rows, not a filtered seq-scan of the whole 10M+ row table), then the
    /// now-empty batches -- one transaction per chunk. Runs up to
    /// `max_chunks` chunks before yielding; `drained` is true once every
    /// orphan older than `older_than` has been seen (or the only ones left
    /// are import/snapshot-pinned and can't be removed). Only batches whose
    /// `observed_at` is before `older_than` are touched, so an in-flight
    /// refresh is never raced.
    pub async fn prune_orphaned_market_observations(
        &self,
        older_than: DateTime<Utc>,
        batches_per_chunk: i64,
        max_chunks: u32,
    ) -> Result<MarketObservationPruneOutcome, MarketError> {
        let mut observations_deleted: u64 = 0;
        let mut batches_deleted: u64 = 0;
        let mut chunks_run: u32 = 0;
        let drained = loop {
            if chunks_run >= max_chunks {
                break false;
            }
            // Statement 1: take the oldest orphan batches and delete their
            // unreferenced observations (an index range scan on
            // `observation_batch_id`, not a filtered scan of the whole
            // table).
            let (candidates, obs): (i64, i64) = sqlx::query_as(
                r#"
                WITH orphan_batch AS (
                  SELECT b.id
                  FROM market_observation_batches b
                  WHERE b.observed_at < $1
                    AND NOT EXISTS (
                      SELECT 1 FROM market_source_coverage c
                      WHERE c.last_completed_batch_id = b.id
                    )
                    AND NOT EXISTS (
                      SELECT 1 FROM public_market_coverage p
                      WHERE p.last_completed_batch_id = b.id
                    )
                  ORDER BY b.observed_at
                  LIMIT $2
                ),
                deleted_obs AS (
                  DELETE FROM market_order_observations o
                  USING orphan_batch ob
                  WHERE o.observation_batch_id = ob.id
                    AND NOT EXISTS (
                      SELECT 1 FROM market_import_file_observations l
                      WHERE l.market_order_observation_id = o.id
                    )
                    AND NOT EXISTS (
                      SELECT 1 FROM market_price_snapshot_observations s
                      WHERE s.market_order_observation_id = o.id
                    )
                  RETURNING 1
                )
                SELECT
                  (SELECT count(*) FROM orphan_batch),
                  (SELECT count(*) FROM deleted_obs)
                "#,
            )
            .bind(older_than)
            .bind(batches_per_chunk)
            .fetch_one(&self.pool)
            .await
            .map_err(map_sqlx)?;
            // Statement 2 (fresh transaction, so it sees statement 1's
            // deletes): drop the now-empty orphan batches.
            let batches = sqlx::query_scalar::<_, i64>(
                r#"
                WITH deleted AS (
                  DELETE FROM market_observation_batches b
                  WHERE b.observed_at < $1
                    AND NOT EXISTS (
                      SELECT 1 FROM market_source_coverage c
                      WHERE c.last_completed_batch_id = b.id
                    )
                    AND NOT EXISTS (
                      SELECT 1 FROM public_market_coverage p
                      WHERE p.last_completed_batch_id = b.id
                    )
                    AND NOT EXISTS (
                      SELECT 1 FROM market_order_observations o
                      WHERE o.observation_batch_id = b.id
                    )
                  RETURNING 1
                )
                SELECT count(*) FROM deleted
                "#,
            )
            .bind(older_than)
            .fetch_one(&self.pool)
            .await
            .map_err(map_sqlx)?;
            chunks_run += 1;
            observations_deleted += u64::try_from(obs).unwrap_or(0);
            batches_deleted += u64::try_from(batches).unwrap_or(0);
            // Fewer orphan candidates than we asked for -> we've seen them
            // all. Or a full chunk that removed nothing at all -> everything
            // left is import/snapshot-pinned and can't be collected.
            if candidates < batches_per_chunk || (obs == 0 && batches == 0) {
                break true;
            }
        };
        Ok(MarketObservationPruneOutcome {
            observations_deleted,
            batches_deleted,
            chunks_run,
            drained,
        })
    }
}
