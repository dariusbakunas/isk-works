-- Persisted phase/task rows, computed via topological layering over the
-- Slice 2 merged plan graph (crate::compute_phases) at Plan generation/
-- regeneration time. No reservation or execution actions exist yet --
-- reused_quantity stays 0, status is only ever 'ready' (phase 0) or
-- 'blocked' (later phases), and the four cost columns stay NULL until a
-- later slice computes real per-task costs across the merged graph.
CREATE TABLE plan_phases (
  id uuid PRIMARY KEY,
  plan_id uuid NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
  sequence integer NOT NULL CHECK (sequence >= 0),
  label text NOT NULL,
  UNIQUE (plan_id, sequence)
);

CREATE TABLE plan_tasks (
  id uuid PRIMARY KEY,
  plan_id uuid NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
  phase_id uuid NOT NULL REFERENCES plan_phases(id) ON DELETE CASCADE,
  -- Nullable: an Acquire task whose demand is shared across two or more
  -- builds in the graph has no single "owning" build -- see
  -- `ComputedTask::source_build_id`'s doc comment.
  source_build_id uuid REFERENCES builds(id) ON DELETE RESTRICT,
  type_id bigint NOT NULL,
  captured_name text NOT NULL,
  task_kind text NOT NULL CHECK (task_kind IN ('acquire', 'produce')),
  required_quantity bigint NOT NULL CHECK (required_quantity > 0),
  reused_quantity bigint NOT NULL DEFAULT 0 CHECK (reused_quantity >= 0),
  fresh_quantity bigint NOT NULL CHECK (fresh_quantity >= 0),
  status text NOT NULL CHECK (status IN ('blocked', 'ready', 'in_progress', 'complete')),
  estimated_unit_cost numeric(24, 4),
  estimated_line_total numeric(24, 4),
  actual_unit_cost numeric(24, 4),
  actual_line_total numeric(24, 4),
  -- A type_id is merged into exactly one node/one phase per Plan, by
  -- construction (PlanGraphBuilder merges globally by type_id).
  UNIQUE (plan_id, type_id)
);

CREATE INDEX plan_tasks_plan_id_idx ON plan_tasks (plan_id);
CREATE INDEX plan_tasks_phase_id_idx ON plan_tasks (phase_id);
