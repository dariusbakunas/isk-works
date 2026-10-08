CREATE TABLE eve_connections (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  eve_character_id bigint NOT NULL CHECK (eve_character_id > 0),
  character_name text NOT NULL CHECK (length(btrim(character_name)) > 0),
  status text NOT NULL CHECK (status IN ('connected', 'needs_reconnection', 'missing_scope', 'temporarily_unavailable', 'disconnected')),
  granted_scopes text[] NOT NULL,
  access_token_expires_at timestamptz NULL,
  last_refreshed_at timestamptz NULL,
  last_error_code text NULL,
  last_error_message text NULL,
  connected_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  disconnected_at timestamptz NULL,
  revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
  UNIQUE (workspace_id, owner_id, eve_character_id)
);

CREATE UNIQUE INDEX eve_connections_one_active_owner_idx
  ON eve_connections (workspace_id, owner_id)
  WHERE disconnected_at IS NULL;

CREATE TABLE eve_connection_tokens (
  connection_id uuid PRIMARY KEY REFERENCES eve_connections(id) ON DELETE CASCADE,
  refresh_token_envelope jsonb NOT NULL,
  token_revision bigint NOT NULL DEFAULT 1 CHECK (token_revision > 0),
  updated_at timestamptz NOT NULL,
  last_refresh_error text NULL
);

CREATE TABLE eve_oauth_pending_authorizations (
  state_hash text PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  pkce_verifier_envelope jsonb NOT NULL,
  requested_scopes text[] NOT NULL,
  return_path text NOT NULL CHECK (return_path IN ('/settings/eve', '/settings/eve/callback')),
  created_at timestamptz NOT NULL,
  expires_at timestamptz NOT NULL,
  consumed_at timestamptz NULL
);

CREATE TABLE esi_sync_runs (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  connection_id uuid NOT NULL REFERENCES eve_connections(id) ON DELETE RESTRICT,
  requested_kind text NOT NULL CHECK (requested_kind IN ('assets', 'wallet_transactions', 'all_supported')),
  status text NOT NULL CHECK (status IN ('pending', 'running', 'succeeded', 'partially_succeeded', 'failed')),
  phase text NOT NULL,
  started_at timestamptz NOT NULL,
  completed_at timestamptz NULL,
  imported_count bigint NOT NULL DEFAULT 0,
  unchanged_count bigint NOT NULL DEFAULT 0,
  skipped_count bigint NOT NULL DEFAULT 0,
  proposal_count bigint NOT NULL DEFAULT 0,
  error_count bigint NOT NULL DEFAULT 0,
  error_code text NULL,
  summary text NOT NULL DEFAULT '',
  endpoint_results jsonb NOT NULL DEFAULT '[]'::jsonb,
  rate_limit_metadata jsonb NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX esi_sync_runs_connection_started_idx ON esi_sync_runs (connection_id, started_at DESC);

CREATE TABLE esi_sync_checkpoints (
  connection_id uuid NOT NULL REFERENCES eve_connections(id) ON DELETE CASCADE,
  endpoint_kind text NOT NULL CHECK (endpoint_kind IN ('assets', 'wallet_transactions')),
  last_successful_sync_at timestamptz NOT NULL,
  last_transaction_id bigint NULL,
  etag text NULL,
  expires_at timestamptz NULL,
  page_count integer NULL CHECK (page_count IS NULL OR page_count > 0),
  cursor_data jsonb NOT NULL DEFAULT '{}'::jsonb,
  PRIMARY KEY (connection_id, endpoint_kind)
);

CREATE TABLE esi_asset_snapshots (
  id uuid PRIMARY KEY,
  connection_id uuid NOT NULL REFERENCES eve_connections(id) ON DELETE RESTRICT,
  sync_run_id uuid NOT NULL UNIQUE REFERENCES esi_sync_runs(id) ON DELETE RESTRICT,
  observed_at timestamptz NOT NULL,
  completed_at timestamptz NULL,
  status text NOT NULL CHECK (status IN ('collecting', 'complete', 'incomplete')),
  page_count integer NOT NULL CHECK (page_count > 0),
  row_count bigint NOT NULL DEFAULT 0 CHECK (row_count >= 0),
  source_checksum text NULL,
  active boolean NOT NULL DEFAULT false,
  CONSTRAINT esi_asset_snapshots_active_complete CHECK (NOT active OR status = 'complete')
);

CREATE UNIQUE INDEX esi_asset_snapshots_one_active_idx
  ON esi_asset_snapshots (connection_id) WHERE active;

CREATE TABLE esi_asset_observations (
  snapshot_id uuid NOT NULL REFERENCES esi_asset_snapshots(id) ON DELETE CASCADE,
  source_item_id bigint NOT NULL,
  type_id bigint NOT NULL CHECK (type_id > 0),
  quantity bigint NOT NULL CHECK (quantity >= 0),
  location_id bigint NOT NULL,
  location_type text NOT NULL,
  location_flag text NOT NULL,
  is_singleton boolean NOT NULL,
  is_blueprint_copy boolean NULL,
  raw_payload jsonb NOT NULL,
  source_checksum text NOT NULL,
  PRIMARY KEY (snapshot_id, source_item_id)
);

CREATE INDEX esi_asset_observations_snapshot_type_idx ON esi_asset_observations (snapshot_id, type_id);
CREATE INDEX esi_asset_observations_location_idx ON esi_asset_observations (snapshot_id, location_id);

CREATE TABLE esi_wallet_transactions (
  id uuid PRIMARY KEY,
  connection_id uuid NOT NULL REFERENCES eve_connections(id) ON DELETE RESTRICT,
  source_transaction_id bigint NOT NULL,
  first_sync_run_id uuid NOT NULL REFERENCES esi_sync_runs(id) ON DELETE RESTRICT,
  last_sync_run_id uuid NOT NULL REFERENCES esi_sync_runs(id) ON DELETE RESTRICT,
  type_id bigint NOT NULL CHECK (type_id > 0),
  quantity bigint NOT NULL CHECK (quantity > 0),
  unit_price numeric(24, 4) NOT NULL CHECK (unit_price >= 0),
  total_price numeric(28, 4) NOT NULL CHECK (total_price >= 0),
  is_buy boolean NOT NULL,
  is_personal boolean NOT NULL,
  transacted_at timestamptz NOT NULL,
  location_id bigint NOT NULL,
  client_id bigint NOT NULL,
  journal_ref_id bigint NOT NULL,
  raw_payload jsonb NOT NULL,
  source_checksum text NOT NULL,
  first_observed_at timestamptz NOT NULL,
  last_observed_at timestamptz NOT NULL,
  UNIQUE (connection_id, source_transaction_id)
);

CREATE INDEX esi_wallet_transactions_connection_time_idx ON esi_wallet_transactions (connection_id, transacted_at DESC);

CREATE TABLE inventory_import_proposals (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  proposal_kind text NOT NULL CHECK (proposal_kind = 'wallet_purchase'),
  source_observation_id uuid NOT NULL REFERENCES esi_wallet_transactions(id) ON DELETE RESTRICT,
  status text NOT NULL CHECK (status IN ('pending', 'accepted', 'ignored', 'conflict', 'unsupported')),
  accepted_inventory_event_id uuid NULL REFERENCES inventory_events(id) ON DELETE RESTRICT,
  ignored_reason text NULL,
  unsupported_reason text NULL,
  accepted_at timestamptz NULL,
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
  UNIQUE (source_observation_id, proposal_kind),
  UNIQUE (accepted_inventory_event_id)
);

CREATE TABLE inventory_event_sources (
  inventory_event_id uuid PRIMARY KEY REFERENCES inventory_events(id) ON DELETE RESTRICT,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  source_system text NOT NULL CHECK (source_system = 'esi'),
  source_record_kind text NOT NULL CHECK (source_record_kind = 'wallet_transaction'),
  source_record_id text NOT NULL,
  accounting_effect_kind text NOT NULL CHECK (accounting_effect_kind = 'purchase'),
  connection_id uuid NOT NULL REFERENCES eve_connections(id) ON DELETE RESTRICT,
  observation_id uuid NOT NULL REFERENCES esi_wallet_transactions(id) ON DELETE RESTRICT,
  sync_run_id uuid NOT NULL REFERENCES esi_sync_runs(id) ON DELETE RESTRICT,
  source_transaction_at timestamptz NOT NULL,
  accepted_at timestamptz NOT NULL,
  UNIQUE (workspace_id, owner_id, source_system, source_record_kind, source_record_id, accounting_effect_kind)
);
