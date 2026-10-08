ALTER TABLE price_sources DROP CONSTRAINT price_sources_kind_valid;
ALTER TABLE price_sources
  ADD CONSTRAINT price_sources_kind_valid
  CHECK (source_kind IN ('manual', 'eve_client_market_export', 'esi_market_orders'));
CREATE UNIQUE INDEX price_sources_one_esi_market_per_workspace_idx
  ON price_sources (workspace_id) WHERE source_kind = 'esi_market_orders';

ALTER TABLE market_order_observations
  DROP CONSTRAINT market_order_observations_source_kind_check;
ALTER TABLE market_order_observations
  ADD CONSTRAINT market_order_observations_source_kind_check
  CHECK (source_kind IN ('eve_client_market_export', 'esi_market_orders'));

CREATE TABLE market_observation_batches (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  price_source_id uuid REFERENCES price_sources(id) ON DELETE CASCADE,
  origin text NOT NULL
    CHECK (origin IN ('eve_client_market_export', 'esi_market_orders')),
  status text NOT NULL CHECK (status IN ('refreshing', 'completed', 'failed')),
  type_id bigint NOT NULL CHECK (type_id > 0),
  captured_type_name text NOT NULL CHECK (length(btrim(captured_type_name)) > 0),
  region_id bigint NOT NULL CHECK (region_id > 0),
  solar_system_id bigint NOT NULL CHECK (solar_system_id > 0),
  location_id bigint NOT NULL CHECK (location_id > 0),
  observed_at timestamptz,
  attempted_at timestamptz NOT NULL,
  completed_at timestamptz,
  etag text,
  expires_at timestamptz,
  error_message text,
  CONSTRAINT market_observation_batch_status_consistent CHECK (
    (status = 'completed' AND observed_at IS NOT NULL AND completed_at IS NOT NULL
      AND error_message IS NULL)
    OR (status = 'refreshing' AND observed_at IS NULL AND completed_at IS NULL
      AND error_message IS NULL)
    OR (status = 'failed' AND observed_at IS NULL AND completed_at IS NOT NULL
      AND error_message IS NOT NULL)
  )
);
CREATE INDEX market_observation_batches_latest_idx
  ON market_observation_batches (
    workspace_id, price_source_id, type_id, location_id, observed_at DESC
  ) WHERE status = 'completed';

ALTER TABLE market_order_observations ADD COLUMN observation_batch_id uuid;

INSERT INTO market_observation_batches (
  id, workspace_id, price_source_id, origin, status, type_id,
  captured_type_name, region_id, solar_system_id, location_id,
  observed_at, attempted_at, completed_at
)
SELECT
  mif.id, mif.workspace_id, NULL, 'eve_client_market_export', 'completed',
  mif.type_id, mif.captured_type_name, mif.region_id, mif.solar_system_id,
  mif.location_id, mif.observed_at, mif.imported_at, mif.imported_at
FROM market_import_files mif;

DROP TRIGGER market_order_observations_immutable ON market_order_observations;

UPDATE market_order_observations observation
SET observation_batch_id = link.market_import_file_id
FROM market_import_file_observations link
WHERE link.market_order_observation_id = observation.id;

ALTER TABLE market_order_observations
  ALTER COLUMN observation_batch_id SET NOT NULL,
  ADD CONSTRAINT market_order_observations_batch_fk
    FOREIGN KEY (observation_batch_id)
    REFERENCES market_observation_batches(id) ON DELETE RESTRICT;
CREATE INDEX market_order_observations_batch_idx
  ON market_order_observations (observation_batch_id, order_side, price, order_id);
CREATE TRIGGER market_order_observations_immutable
BEFORE UPDATE OR DELETE ON market_order_observations
FOR EACH ROW EXECUTE FUNCTION reject_market_evidence_mutation();

CREATE TABLE market_source_coverage (
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  price_source_id uuid NOT NULL REFERENCES price_sources(id) ON DELETE CASCADE,
  type_id bigint NOT NULL CHECK (type_id > 0),
  type_name text NOT NULL CHECK (length(btrim(type_name)) > 0),
  refresh_state text NOT NULL
    CHECK (refresh_state IN ('missing', 'current', 'refreshing', 'failed')),
  last_attempted_at timestamptz,
  next_refresh_at timestamptz,
  last_completed_batch_id uuid
    REFERENCES market_observation_batches(id) ON DELETE RESTRICT,
  last_error text,
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  PRIMARY KEY (workspace_id, price_source_id, type_id),
  CONSTRAINT market_source_coverage_state_consistent CHECK (
    (refresh_state = 'missing' AND last_completed_batch_id IS NULL
      AND last_error IS NULL)
    OR (refresh_state = 'current' AND last_completed_batch_id IS NOT NULL
      AND last_error IS NULL)
    OR (refresh_state = 'refreshing' AND last_attempted_at IS NOT NULL)
    OR (refresh_state = 'failed' AND last_attempted_at IS NOT NULL
      AND last_error IS NOT NULL)
  )
);
CREATE INDEX market_source_coverage_due_idx
  ON market_source_coverage (workspace_id, price_source_id, next_refresh_at, type_id)
  WHERE refresh_state IN ('missing', 'current', 'failed');

CREATE TRIGGER market_observation_batches_immutable
BEFORE UPDATE OR DELETE ON market_observation_batches
FOR EACH ROW EXECUTE FUNCTION reject_market_evidence_mutation();
