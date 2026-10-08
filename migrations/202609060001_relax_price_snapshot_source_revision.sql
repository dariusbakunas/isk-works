-- A market-scope-priced Build (the default since market-centric pricing,
-- migration 202608230002) has no manual Price Source behind its plan, so
-- `calculate_candidate_plan` records `price_source_revision = 0` /
-- `price_source_id = NULL` for the snapshot (see
-- crates/iskworks-core/src/industry/plan_calculation.rs). Freezing that
-- snapshot when creating an Epic (`POST /api/builds/:id/orders`) then hit
-- the original `captured_source_revision > 0` check from migration
-- 202607250004 and failed with a persistence error -- market-priced
-- Builds could never create an Epic.
--
-- Relax the check to allow 0, which is the well-defined "no manual source
-- revision" value. A stricter `price_source_id IS NULL <=> revision = 0`
-- form is deliberately avoided: `price_snapshots.price_source_id` is
-- `ON DELETE SET NULL`, so deleting a Price Override would retroactively
-- flip an existing manual snapshot to `(NULL, revision > 0)` and the
-- delete itself would fail the check.
--
-- `market_price_snapshots` (migration 202607250011) keeps its own
-- `> 0` check unchanged: its `price_source_id` is `NOT NULL` and it only
-- snapshots real market price sources, which always carry a revision.

ALTER TABLE price_snapshots
  DROP CONSTRAINT price_snapshots_captured_source_revision_check;

ALTER TABLE price_snapshots
  ADD CONSTRAINT price_snapshots_captured_source_revision_check
    CHECK (captured_source_revision >= 0);
