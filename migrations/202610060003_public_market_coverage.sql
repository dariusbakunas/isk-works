-- App-wide public market data
-- (docs/superpowers/specs/2026-10-05-global-public-market-data-design.md).
--
-- Order books from the public regional ESI endpoint (NPC stations and
-- region-wide scopes) are fetched once per (region, type) for every
-- workspace, instead of once per workspace. They are stored in the existing
-- observation tables with workspace_id = NULL and price_source_id = NULL;
-- `public_market_coverage` is their refresh lifecycle, mirroring
-- `market_source_coverage` column for column. Structure markets and imports
-- stay per workspace.

CREATE TABLE public_market_coverage (
  region_id bigint NOT NULL CHECK (region_id > 0),
  type_id bigint NOT NULL CHECK (type_id > 0),
  type_name text NOT NULL CHECK (length(btrim(type_name)) > 0),
  refresh_state text NOT NULL
    CHECK (refresh_state IN ('missing', 'current', 'refreshing', 'failed')),
  last_attempted_at timestamptz,
  next_refresh_at timestamptz,
  last_completed_batch_id uuid REFERENCES market_observation_batches(id) ON DELETE RESTRICT,
  last_error text,
  lease_expires_at timestamptz,
  priority_requested_at timestamptz,
  revalidated_at timestamptz,
  consecutive_failures integer NOT NULL DEFAULT 0 CHECK (consecutive_failures >= 0),
  -- Bumped by any workspace's demand; dormant once nobody has needed it
  -- for a week (`COVERAGE_DORMANT_AFTER`).
  last_needed_at timestamptz NOT NULL DEFAULT now(),
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  PRIMARY KEY (region_id, type_id),
  CONSTRAINT public_market_coverage_refresh_lease_consistent
    CHECK (refresh_state <> 'refreshing' OR lease_expires_at IS NOT NULL),
  CONSTRAINT public_market_coverage_revalidated_requires_batch
    CHECK (revalidated_at IS NULL OR last_completed_batch_id IS NOT NULL),
  CONSTRAINT public_market_coverage_state_consistent CHECK (
    (refresh_state = 'missing' AND last_completed_batch_id IS NULL AND last_error IS NULL)
    OR (refresh_state = 'current' AND last_completed_batch_id IS NOT NULL AND last_error IS NULL)
    OR (refresh_state = 'refreshing' AND last_attempted_at IS NOT NULL)
    OR (refresh_state = 'failed' AND last_attempted_at IS NOT NULL AND last_error IS NOT NULL)
  )
);

CREATE INDEX public_market_coverage_due_idx
  ON public_market_coverage (priority_requested_at DESC NULLS LAST, next_refresh_at, region_id, type_id)
  WHERE refresh_state IN ('missing', 'current', 'failed');
CREATE INDEX public_market_coverage_last_completed_batch_idx
  ON public_market_coverage (last_completed_batch_id);

-- A batch without a workspace is a public ESI batch with no price source.
-- Batches that have a workspace (imports, structure and legacy per-workspace
-- ESI books) keep exactly the rules they had.
ALTER TABLE market_observation_batches
  ALTER COLUMN workspace_id DROP NOT NULL,
  ADD CONSTRAINT market_observation_batches_public_scope CHECK (
    workspace_id IS NOT NULL
    OR (origin = 'esi_market_orders' AND price_source_id IS NULL)
  );

ALTER TABLE market_order_observations ALTER COLUMN workspace_id DROP NOT NULL;

-- An observation always shares its batch's scope (the same workspace, or
-- both public). A CHECK can't see the batch row, so this is a trigger.
CREATE FUNCTION market_order_observation_matches_batch_scope() RETURNS trigger AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM market_observation_batches b
    WHERE b.id = NEW.observation_batch_id
      AND b.workspace_id IS NOT DISTINCT FROM NEW.workspace_id
  ) THEN
    RAISE EXCEPTION 'market order observation scope does not match its batch'
      USING ERRCODE = 'check_violation';
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER market_order_observations_batch_scope
  BEFORE INSERT ON market_order_observations
  FOR EACH ROW EXECUTE FUNCTION market_order_observation_matches_batch_scope();

-- The workspace-led unique key and book indexes don't cover NULL rows; these
-- are their public twins.
CREATE UNIQUE INDEX market_order_observations_public_key
  ON market_order_observations (source_kind, observed_at, order_id, normalized_row_checksum)
  WHERE workspace_id IS NULL;
CREATE INDEX market_order_observations_public_book_idx
  ON market_order_observations (type_id, region_id, location_id, observed_at DESC, order_side, price)
  WHERE workspace_id IS NULL;
CREATE INDEX market_observation_batches_public_latest_idx
  ON market_observation_batches (region_id, type_id, observed_at DESC)
  WHERE status = 'completed' AND workspace_id IS NULL;
