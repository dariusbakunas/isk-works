ALTER TABLE price_sources DROP CONSTRAINT price_sources_kind_valid;
ALTER TABLE price_sources
  ADD CONSTRAINT price_sources_kind_valid
  CHECK (source_kind IN ('manual', 'eve_client_market_export'));

CREATE TABLE market_import_batches (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  source_kind text NOT NULL CHECK (source_kind = 'eve_client_market_export'),
  status text NOT NULL CHECK (status IN ('succeeded', 'partially_succeeded')),
  observed_at_min timestamptz NOT NULL,
  observed_at_max timestamptz NOT NULL,
  imported_at timestamptz NOT NULL,
  file_count integer NOT NULL CHECK (file_count > 0),
  item_count integer NOT NULL CHECK (item_count > 0),
  location_count integer NOT NULL CHECK (location_count > 0),
  observation_count bigint NOT NULL CHECK (observation_count > 0),
  skipped_duplicate_file_count integer NOT NULL DEFAULT 0 CHECK (skipped_duplicate_file_count >= 0),
  warnings_json jsonb NOT NULL DEFAULT '[]'::jsonb,
  CONSTRAINT market_import_batches_observation_range
    CHECK (observed_at_min <= observed_at_max)
);
CREATE INDEX market_import_batches_workspace_imported_idx
  ON market_import_batches (workspace_id, imported_at DESC);

CREATE TABLE market_import_files (
  id uuid PRIMARY KEY,
  batch_id uuid NOT NULL REFERENCES market_import_batches(id) ON DELETE RESTRICT,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  original_filename text NOT NULL,
  sanitized_filename text NOT NULL,
  file_checksum text NOT NULL,
  normalized_checksum text NOT NULL,
  file_size_bytes bigint NOT NULL CHECK (file_size_bytes > 0),
  observed_at timestamptz NOT NULL,
  timestamp_source text NOT NULL
    CHECK (timestamp_source IN ('filename', 'user_supplied', 'import_time')),
  type_id bigint NOT NULL CHECK (type_id > 0),
  captured_type_name text NOT NULL CHECK (length(btrim(captured_type_name)) > 0),
  location_id bigint NOT NULL CHECK (location_id > 0),
  captured_location_name text,
  solar_system_id bigint NOT NULL CHECK (solar_system_id > 0),
  captured_solar_system_name text,
  region_id bigint NOT NULL CHECK (region_id > 0),
  captured_region_name text,
  row_count bigint NOT NULL CHECK (row_count > 0),
  buy_order_count bigint NOT NULL CHECK (buy_order_count >= 0),
  sell_order_count bigint NOT NULL CHECK (sell_order_count >= 0),
  status text NOT NULL CHECK (status = 'imported'),
  imported_at timestamptz NOT NULL,
  UNIQUE (workspace_id, file_checksum),
  UNIQUE (workspace_id, normalized_checksum, observed_at)
);
CREATE INDEX market_import_files_batch_idx ON market_import_files (batch_id);
CREATE INDEX market_import_files_workspace_type_location_observed_idx
  ON market_import_files (workspace_id, type_id, location_id, observed_at DESC);

CREATE TABLE market_order_observations (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  source_kind text NOT NULL CHECK (source_kind = 'eve_client_market_export'),
  observed_at timestamptz NOT NULL,
  imported_at timestamptz NOT NULL,
  order_id bigint NOT NULL CHECK (order_id > 0),
  type_id bigint NOT NULL CHECK (type_id > 0),
  captured_type_name text NOT NULL CHECK (length(btrim(captured_type_name)) > 0),
  order_side text NOT NULL CHECK (order_side IN ('buy', 'sell')),
  price numeric(24, 4) NOT NULL CHECK (price > 0),
  remaining_volume bigint NOT NULL CHECK (remaining_volume >= 0),
  entered_volume bigint NOT NULL CHECK (entered_volume >= 0),
  minimum_volume bigint NOT NULL CHECK (minimum_volume > 0),
  order_range integer NOT NULL,
  issued_at timestamptz NOT NULL,
  duration_days integer NOT NULL CHECK (duration_days > 0),
  location_id bigint NOT NULL CHECK (location_id > 0),
  captured_location_name text,
  solar_system_id bigint NOT NULL CHECK (solar_system_id > 0),
  captured_solar_system_name text,
  region_id bigint NOT NULL CHECK (region_id > 0),
  captured_region_name text,
  jumps integer NOT NULL CHECK (jumps >= 0),
  normalized_row_checksum text NOT NULL,
  UNIQUE (workspace_id, source_kind, observed_at, order_id, normalized_row_checksum)
);
CREATE INDEX market_order_observations_book_idx
  ON market_order_observations
  (workspace_id, type_id, location_id, observed_at DESC, order_side, price);
CREATE INDEX market_order_observations_order_history_idx
  ON market_order_observations (workspace_id, order_id, observed_at DESC);

CREATE TABLE market_import_file_observations (
  market_import_file_id uuid NOT NULL REFERENCES market_import_files(id) ON DELETE RESTRICT,
  market_order_observation_id uuid NOT NULL REFERENCES market_order_observations(id) ON DELETE RESTRICT,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  source_row_number integer NOT NULL CHECK (source_row_number >= 2),
  PRIMARY KEY (market_import_file_id, market_order_observation_id),
  UNIQUE (market_import_file_id, source_row_number)
);
CREATE INDEX market_import_file_observations_observation_idx
  ON market_import_file_observations (market_order_observation_id);

CREATE TABLE market_price_source_configs (
  price_source_id uuid PRIMARY KEY REFERENCES price_sources(id) ON DELETE CASCADE,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  location_id bigint NOT NULL CHECK (location_id > 0),
  solar_system_id bigint NOT NULL CHECK (solar_system_id > 0),
  region_id bigint NOT NULL CHECK (region_id > 0),
  location_alias text NOT NULL DEFAULT '',
  pricing_policy text NOT NULL CHECK (pricing_policy IN (
    'lowest_sell', 'highest_buy',
    'acquire_quantity_from_sell_orders', 'liquidate_quantity_into_buy_orders'
  )),
  coverage_policy text NOT NULL CHECK (coverage_policy IN (
    'require_full_coverage', 'allow_partial_with_warning'
  )),
  observation_mode text NOT NULL CHECK (observation_mode IN (
    'latest_compatible_import', 'pinned_import_batch'
  )),
  pinned_batch_id uuid REFERENCES market_import_batches(id) ON DELETE RESTRICT,
  fresh_after_hours integer NOT NULL CHECK (fresh_after_hours > 0),
  stale_after_hours integer NOT NULL CHECK (stale_after_hours > fresh_after_hours),
  archived_at timestamptz,
  last_snapshot_at timestamptz,
  CONSTRAINT market_price_source_mode_consistent CHECK (
    (observation_mode = 'latest_compatible_import' AND pinned_batch_id IS NULL)
    OR (observation_mode = 'pinned_import_batch' AND pinned_batch_id IS NOT NULL)
  )
);
CREATE INDEX market_price_source_configs_workspace_location_idx
  ON market_price_source_configs (workspace_id, location_id);

CREATE TABLE market_price_snapshots (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  price_source_id uuid NOT NULL REFERENCES price_sources(id) ON DELETE RESTRICT,
  captured_source_name text NOT NULL,
  captured_source_revision bigint NOT NULL CHECK (captured_source_revision > 0),
  purpose text NOT NULL CHECK (purpose IN ('source_refresh', 'quantity_preview', 'build_planning')),
  formula_version text NOT NULL,
  oldest_observation_at timestamptz NOT NULL,
  newest_observation_at timestamptz NOT NULL,
  created_at timestamptz NOT NULL,
  warnings_json jsonb NOT NULL DEFAULT '[]'::jsonb,
  CONSTRAINT market_price_snapshots_observation_range
    CHECK (oldest_observation_at <= newest_observation_at)
);
CREATE INDEX market_price_snapshots_source_created_idx
  ON market_price_snapshots (price_source_id, created_at DESC);

CREATE TABLE market_price_snapshot_lines (
  market_price_snapshot_id uuid NOT NULL REFERENCES market_price_snapshots(id) ON DELETE RESTRICT,
  type_id bigint NOT NULL CHECK (type_id > 0),
  captured_type_name text NOT NULL,
  pricing_policy text NOT NULL,
  requested_quantity bigint NOT NULL CHECK (requested_quantity > 0),
  calculated_unit_price numeric(24, 4),
  calculated_total numeric(28, 4) NOT NULL CHECK (calculated_total >= 0),
  best_price numeric(24, 4),
  marginal_price numeric(24, 4),
  covered_quantity bigint NOT NULL CHECK (covered_quantity >= 0),
  uncovered_quantity bigint NOT NULL CHECK (uncovered_quantity >= 0),
  fully_covered boolean NOT NULL,
  order_count_used bigint NOT NULL CHECK (order_count_used >= 0),
  available_volume bigint NOT NULL CHECK (available_volume >= 0),
  location_id bigint NOT NULL CHECK (location_id > 0),
  quality text NOT NULL CHECK (quality IN (
    'direct', 'stale', 'insufficient_volume', 'unavailable'
  )),
  warnings_json jsonb NOT NULL DEFAULT '[]'::jsonb,
  calculation_trace jsonb NOT NULL,
  PRIMARY KEY (market_price_snapshot_id, type_id)
);

CREATE TABLE market_price_snapshot_observations (
  market_price_snapshot_id uuid NOT NULL,
  type_id bigint NOT NULL,
  market_order_observation_id uuid NOT NULL REFERENCES market_order_observations(id) ON DELETE RESTRICT,
  PRIMARY KEY (market_price_snapshot_id, type_id, market_order_observation_id),
  FOREIGN KEY (market_price_snapshot_id, type_id)
    REFERENCES market_price_snapshot_lines(market_price_snapshot_id, type_id)
    ON DELETE RESTRICT
);

CREATE FUNCTION reject_market_evidence_mutation() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'market observations and snapshots are immutable';
END;
$$;

CREATE TRIGGER market_order_observations_immutable
BEFORE UPDATE OR DELETE ON market_order_observations
FOR EACH ROW EXECUTE FUNCTION reject_market_evidence_mutation();

CREATE TRIGGER market_price_snapshots_immutable
BEFORE UPDATE OR DELETE ON market_price_snapshots
FOR EACH ROW EXECUTE FUNCTION reject_market_evidence_mutation();

CREATE TRIGGER market_price_snapshot_lines_immutable
BEFORE UPDATE OR DELETE ON market_price_snapshot_lines
FOR EACH ROW EXECUTE FUNCTION reject_market_evidence_mutation();

CREATE TRIGGER market_price_snapshot_observations_immutable
BEFORE UPDATE OR DELETE ON market_price_snapshot_observations
FOR EACH ROW EXECUTE FUNCTION reject_market_evidence_mutation();
