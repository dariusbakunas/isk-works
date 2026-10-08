CREATE TABLE market_location_names (
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  location_id bigint NOT NULL CHECK (location_id > 0),
  location_name text NOT NULL CHECK (length(btrim(location_name)) > 0),
  owner_id bigint NOT NULL CHECK (owner_id > 0),
  solar_system_id bigint NOT NULL CHECK (solar_system_id > 0),
  structure_type_id bigint,
  resolved_by_connection_id uuid REFERENCES eve_connections(id) ON DELETE SET NULL,
  resolved_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  PRIMARY KEY (workspace_id, location_id)
);

CREATE INDEX market_location_names_workspace_name_idx
  ON market_location_names (workspace_id, location_name);
