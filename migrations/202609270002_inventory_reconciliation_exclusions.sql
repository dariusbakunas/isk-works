CREATE TABLE inventory_reconciliation_exclusions (
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  type_id bigint NOT NULL CHECK (type_id > 0),
  eve_character_id bigint NOT NULL CHECK (eve_character_id > 0),
  effective_location_id bigint NOT NULL CHECK (effective_location_id > 0),
  created_at timestamptz NOT NULL,
  PRIMARY KEY (workspace_id, owner_id, type_id, eve_character_id, effective_location_id)
);

CREATE INDEX inventory_reconciliation_exclusions_lookup_idx
  ON inventory_reconciliation_exclusions (workspace_id, owner_id, type_id);
