-- The blueprint/facility/duration/cost a Produce or Final Assembly task
-- actually assumed, frozen at Plan generation time from the
-- BuildPlanRevision already computed for its owning build -- see
-- TaskExecutionSnapshot and the manufacturing-ticket-execution-sheet
-- design doc. NULL for Acquire tasks and for a Produce task whose owning
-- build's own calculate_build_snapshot failed.
ALTER TABLE plan_tasks ADD COLUMN execution_snapshot jsonb;
