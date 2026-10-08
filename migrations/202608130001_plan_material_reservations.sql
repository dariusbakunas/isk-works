-- Fresh table, not a literal rename -- Slice 1 already dropped the old
-- build_material_reservations. One row per plan_task with
-- reused_quantity > 0, created atomically at commit time
-- (PgIndustryRepository::commit_plan).
CREATE TABLE plan_material_reservations (
  id uuid PRIMARY KEY,
  plan_id uuid NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
  plan_task_id uuid NOT NULL UNIQUE REFERENCES plan_tasks(id) ON DELETE CASCADE,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  type_id bigint NOT NULL,
  quantity bigint NOT NULL CHECK (quantity > 0),
  created_at timestamptz NOT NULL
);

-- Drives both commit's own "how much is already reserved elsewhere"
-- check and the future inventory -> reserving-plans reverse lookup the
-- design doc describes (not built this slice).
CREATE INDEX plan_material_reservations_type_idx
  ON plan_material_reservations (workspace_id, owner_id, type_id);
