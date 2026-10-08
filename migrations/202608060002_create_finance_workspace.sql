CREATE TABLE esi_wallet_balances (
  id uuid PRIMARY KEY,
  connection_id uuid NOT NULL REFERENCES eve_connections(id) ON DELETE RESTRICT,
  sync_run_id uuid NOT NULL REFERENCES esi_sync_runs(id) ON DELETE RESTRICT,
  balance numeric(28, 4) NOT NULL CHECK (balance >= 0),
  observed_at timestamptz NOT NULL,
  source_checksum text NOT NULL CHECK (length(btrim(source_checksum)) > 0),
  UNIQUE (connection_id, sync_run_id)
);

CREATE INDEX esi_wallet_balances_connection_observed_idx
  ON esi_wallet_balances (connection_id, observed_at DESC);

CREATE TABLE finance_saved_filters (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  name text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 80),
  filter_payload jsonb NOT NULL,
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL
);

CREATE UNIQUE INDEX finance_saved_filters_workspace_name_idx
  ON finance_saved_filters (workspace_id, lower(name));
