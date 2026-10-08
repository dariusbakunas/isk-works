-- The legacy Plan/PlanTask execution model is superseded by the standalone
-- Build -> Order -> Ticket -> Acquisition Run system (now at full feature
-- parity, including Acquisition Run batching -- see PR B). All rows in
-- these seven tables are disposable development data; dropped outright,
-- unconditionally, with no backfill into Orders/Tickets and no compatibility
-- shim for old execution state.
--
-- `acquisition_runs`/`acquisition_run_items` and the `ticket_display_id_seq`/
-- `acquisition_run_display_id_seq` sequences are NOT touched -- they are
-- physically shared with the standalone `tickets` table (its
-- `acquisition_run_id` column FKs into the same `acquisition_runs` table),
-- confirmed via a dependency inspection (information_schema) showing no FK
-- from any table outside this list into any table in this list, and no FK
-- from any of these seven tables' own children reaching further out.
-- Dropped children-before-parents so a plain DROP TABLE (no CASCADE)
-- succeeds at every step.
DROP TABLE plan_material_reservations;
DROP TABLE plan_task_dependencies;
DROP TABLE plan_completions;
DROP TABLE plan_tasks;
DROP TABLE plan_phases;
DROP TABLE plan_source_builds;
DROP TABLE plans;
