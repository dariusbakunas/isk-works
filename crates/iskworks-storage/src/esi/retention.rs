//! Retention for ESI observations that are re-fetched in full on every
//! refresh but only read at their latest: asset snapshots and adjusted
//! prices. Each sweep deletes in chunks, one transaction per chunk, so a
//! large backlog drains over several passes without a long lock or a WAL
//! spike.

use super::*;

/// Result of one retention sweep.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct EsiObservationPruneOutcome {
    /// Snapshots (with their observations and hierarchy) or observation rows.
    pub rows_deleted: u64,
    pub chunks_run: u32,
    /// `false` when `max_chunks` was reached with rows still to collect --
    /// the next pass continues from the oldest remaining.
    pub drained: bool,
}

impl PgEsiRepository {
    /// Delete asset snapshots nothing reads any more: every snapshot that is
    /// neither its connection's active one nor its newest (a newest failed
    /// attempt is what the sync status reports). `ON DELETE CASCADE` takes
    /// their observations and hierarchy rows with them. `complete_assets`
    /// already deletes what each sync supersedes; this is the backstop that
    /// clears the backlog left by earlier releases.
    pub async fn prune_superseded_asset_snapshots(
        &self,
        snapshots_per_chunk: i64,
        max_chunks: u32,
    ) -> Result<EsiObservationPruneOutcome, InventoryError> {
        self.prune_in_chunks(
            r#"
            WITH doomed AS (
              SELECT s.id
              FROM esi_asset_snapshots s
              WHERE NOT s.active
                AND EXISTS (
                  SELECT 1 FROM esi_asset_snapshots newer
                  WHERE newer.connection_id = s.connection_id
                    AND newer.observed_at > s.observed_at
                )
              ORDER BY s.observed_at
              LIMIT $1
            ),
            deleted AS (
              DELETE FROM esi_asset_snapshots s
              USING doomed
              WHERE s.id = doomed.id
              RETURNING 1
            )
            SELECT count(*) FROM deleted
            "#,
            snapshots_per_chunk,
            max_chunks,
        )
        .await
    }

    /// Delete adjusted-price observations superseded by a newer one for the
    /// same type. A type ESI stops listing keeps its last observation.
    pub async fn prune_superseded_adjusted_prices(
        &self,
        rows_per_chunk: i64,
        max_chunks: u32,
    ) -> Result<EsiObservationPruneOutcome, InventoryError> {
        self.prune_in_chunks(
            r#"
            WITH doomed AS (
              SELECT o.id
              FROM industry_adjusted_price_observations o
              WHERE EXISTS (
                SELECT 1 FROM industry_adjusted_price_observations newer
                WHERE newer.type_id = o.type_id
                  AND newer.observed_at > o.observed_at
              )
              LIMIT $1
            ),
            deleted AS (
              DELETE FROM industry_adjusted_price_observations o
              USING doomed
              WHERE o.id = doomed.id
              RETURNING 1
            )
            SELECT count(*) FROM deleted
            "#,
            rows_per_chunk,
            max_chunks,
        )
        .await
    }

    /// Run `chunk_sql` (which deletes up to `$1` rows and returns how many it
    /// deleted) until a chunk comes back short or `max_chunks` is reached.
    async fn prune_in_chunks(
        &self,
        chunk_sql: &str,
        per_chunk: i64,
        max_chunks: u32,
    ) -> Result<EsiObservationPruneOutcome, InventoryError> {
        let mut outcome = EsiObservationPruneOutcome::default();
        outcome.drained = loop {
            if outcome.chunks_run >= max_chunks {
                break false;
            }
            let deleted = sqlx::query_scalar::<_, i64>(chunk_sql)
                .bind(per_chunk)
                .fetch_one(&self.pool)
                .await
                .map_err(map_sqlx)?;
            outcome.chunks_run += 1;
            outcome.rows_deleted += u64::try_from(deleted).unwrap_or(0);
            if deleted < per_chunk {
                break true;
            }
        };
        Ok(outcome)
    }
}
