-- `market_source_coverage.revalidated_at`: the most recent time ESI
-- positively confirmed (via a conditional `If-None-Match` request that
-- returned `304 Not Modified`) that the snapshot at
-- `last_completed_batch_id` was still current -- WITHOUT fetching a new
-- snapshot. `market_observation_batches.observed_at` remains the immutable
-- provenance of when the order rows were physically fetched; this column
-- is the "still valid as of" companion. Effective market freshness is
-- `GREATEST(batch.observed_at, COALESCE(coverage.revalidated_at, batch.observed_at))`.
--
-- A successful 200 sets `revalidated_at = batch.observed_at` (so
-- `observed_at == revalidated_at` immediately after a fresh fetch); a
-- successful 304 advances only `revalidated_at`; a failed refresh leaves
-- it untouched (a failed attempt never erases the last confirmation).

ALTER TABLE market_source_coverage ADD COLUMN revalidated_at timestamptz NULL;

COMMENT ON COLUMN market_source_coverage.revalidated_at IS
  'Most recent time ESI confirmed the snapshot at last_completed_batch_id is still current (200 sets it = batch.observed_at; 304 advances it; failure leaves it). Provenance stays on market_observation_batches.observed_at.';

-- Existing completed coverage rows have never been revalidated by a 304,
-- so their last confirmation is exactly their original observation.
UPDATE market_source_coverage coverage
SET revalidated_at = batch.observed_at
FROM market_observation_batches batch
WHERE batch.id = coverage.last_completed_batch_id;

-- A revalidation only makes sense for a row that has a completed batch to
-- have revalidated. (Cannot cross-reference batch.observed_at from a table
-- CHECK; the `revalidated_at >= observed_at` invariant is enforced at
-- write time in complete_esi_market_refresh / revalidate_esi_market_refresh
-- and defended in reads via GREATEST.)
ALTER TABLE market_source_coverage
  ADD CONSTRAINT market_source_coverage_revalidated_requires_batch
  CHECK (revalidated_at IS NULL OR last_completed_batch_id IS NOT NULL);
