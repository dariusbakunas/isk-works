-- Fresh table, not a rename -- Slice 1 already dropped build_completions/
-- build_manufacturing_expenses along with every other Build-lifecycle
-- table. One row per Plan, created atomically by
-- PgIndustryRepository::complete_plan once every plan_task is Complete.
-- total_actual_material_cost is SUM(plan_tasks.actual_line_total) --
-- materials only, no facility/installation fee tracking this slice (see
-- the plan doc's scope note).
CREATE TABLE plan_completions (
  id uuid PRIMARY KEY,
  plan_id uuid NOT NULL UNIQUE REFERENCES plans(id) ON DELETE CASCADE,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  output_type_id bigint NOT NULL,
  output_type_name text NOT NULL,
  output_quantity bigint NOT NULL CHECK (output_quantity > 0),
  total_actual_material_cost numeric(24, 4) NOT NULL CHECK (total_actual_material_cost >= 0),
  completed_at timestamptz NOT NULL,
  -- Set by the separate POST /api/plans/:id/post-output action -- NULL
  -- means the output has not been posted to inventory yet. Prevents
  -- posting the same completion twice.
  output_posted_at timestamptz
);

CREATE INDEX plan_completions_workspace_idx ON plan_completions (workspace_id, owner_id);
