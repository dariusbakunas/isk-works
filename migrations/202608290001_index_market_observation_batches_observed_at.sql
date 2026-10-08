-- The orphaned-observation GC sweep
-- (`PgMarketRepository::prune_orphaned_market_observations`) scans
-- `market_observation_batches` ordered by `observed_at` to pick the oldest
-- orphan batches a chunk at a time. Without this index each chunk is a
-- seq-scan + sort of the whole batch table.

CREATE INDEX IF NOT EXISTS market_observation_batches_observed_at_idx
  ON market_observation_batches (observed_at);
