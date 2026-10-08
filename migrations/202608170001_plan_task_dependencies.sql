-- Real, material-level dependency edges between a Plan's own tasks --
-- replaces execution-phase adjacency as the source of Ticket.blocked_by/
-- unblocks and the Blocked -> Ready completion cascade (previously "is the
-- whole previous phase complete", now "are this task's actual inputs
-- complete"). Populated by IndustryService::capture_plan_graph_state via
-- replace_plan_phases, from ComputedTask.depends_on. Purely additive: no
-- existing column/table changes, and deleting a Plan's tasks (via the
-- existing plan_phases -> plan_tasks cascade) cascades here too.
CREATE TABLE plan_task_dependencies (
  id uuid PRIMARY KEY,
  plan_id uuid NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
  task_id uuid NOT NULL REFERENCES plan_tasks(id) ON DELETE CASCADE,
  depends_on_task_id uuid NOT NULL REFERENCES plan_tasks(id) ON DELETE CASCADE,
  quantity bigint NOT NULL CHECK (quantity > 0),
  UNIQUE (task_id, depends_on_task_id)
);

CREATE INDEX plan_task_dependencies_task_idx ON plan_task_dependencies (task_id);
CREATE INDEX plan_task_dependencies_depends_on_idx ON plan_task_dependencies (depends_on_task_id);
