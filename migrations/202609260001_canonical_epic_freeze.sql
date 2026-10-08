-- Canonical producers slice 5: Epic freeze for canonical operations that
-- serve more than one demand requirement.
--
-- A canonical root (`builds.planner_authority = 'canonical'`) plans one
-- ProductionOperation per producer Build, and that one operation may serve
-- several consuming requirements (fan-in). A version-3 Epic freezes it
-- exactly once (one `order_plan_operations` row, one production ticket) and
-- freezes each served requirement separately (one `order_requirements` row
-- per demand edge, all naming the same `child_occurrence_key`). The
-- operation dependency DAG is exactly the set of requirement rows
-- `(child_occurrence_key -> operation_occurrence_key)`; no separate edge
-- table is needed.
--
-- Everything here is additive and nullable: version-1/2 Epics are never
-- rewritten and keep reading back exactly as before (new columns NULL).
--
--   * `planning_snapshot_version = 3` marks a canonical whole-tree freeze.
--     `order_plan_operations.parent_occurrence_key` / `tickets.parent_ticket_id`
--     are set only when an operation has exactly one consuming operation;
--     a fan-in operation keeps them NULL instead of naming an arbitrary
--     consumer. The root is the operation whose key starts with `root:`.
--   * `child_consumed_cost` freezes this requirement's own share of the
--     producer's total cost; `dependency_id` names the demand edge
--     (`pd:<production_dependencies.id>` canonical, `dep:<consumer>:<type>`
--     legacy). Conservation per producer operation:
--     total_production_cost = sum(child_consumed_cost) + surplus_retained_basis.
--   * Operation-level `consumed_quantity` / `surplus_quantity` /
--     `surplus_retained_basis` freeze the operation's aggregate physical
--     consumption and surplus once, instead of only on one arbitrary
--     "surplus owner" requirement row.

ALTER TABLE orders
  DROP CONSTRAINT orders_planning_snapshot_version_check;
ALTER TABLE orders
  ADD CONSTRAINT orders_planning_snapshot_version_check
    CHECK (planning_snapshot_version IN (1, 2, 3));

ALTER TABLE order_plan_operations
  ADD COLUMN consumed_quantity bigint CHECK (consumed_quantity >= 0),
  ADD COLUMN surplus_quantity bigint CHECK (surplus_quantity >= 0),
  ADD COLUMN surplus_retained_basis numeric(24, 4);

ALTER TABLE order_requirements
  ADD COLUMN child_consumed_cost numeric(24, 4),
  ADD COLUMN dependency_id text;

ALTER TABLE ticket_prerequisites
  ADD COLUMN child_consumed_cost numeric(24, 4),
  ADD COLUMN dependency_id text;
