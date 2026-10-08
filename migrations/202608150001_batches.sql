-- Fresh tables -- Batch is a new entity, nothing to rename from the old
-- Build lifecycle.
CREATE TABLE batches (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  name text NOT NULL,
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  CONSTRAINT batches_name_not_empty CHECK (length(btrim(name)) BETWEEN 1 AND 160)
);

CREATE INDEX batches_workspace_updated_idx ON batches (workspace_id, updated_at DESC);

-- A Plan belongs to at most one Batch (plan_id UNIQUE) -- see the Slice 7
-- plan doc's scope note for why this is a deliberate simplification, not
-- an oversight.
CREATE TABLE batch_plans (
  batch_id uuid NOT NULL REFERENCES batches(id) ON DELETE CASCADE,
  plan_id uuid NOT NULL UNIQUE REFERENCES plans(id) ON DELETE RESTRICT,
  added_at timestamptz NOT NULL,
  PRIMARY KEY (batch_id, plan_id)
);
