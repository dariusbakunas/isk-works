-- The single source of truth for how much of a type has actually been
-- recorded as acquired in a Run, independent of any single ticket's own
-- demand -- unlike plan_tasks.acquired_quantity (which stays capped to
-- that ticket's own fresh_quantity for display/history), this can exceed
-- total demand: over-acquisition is valid, see the Acquisition Run
-- execution design note in PgIndustryRepository::complete_acquisition_run.
CREATE TABLE acquisition_run_items (
  id uuid PRIMARY KEY,
  acquisition_run_id uuid NOT NULL REFERENCES acquisition_runs (id) ON DELETE CASCADE,
  type_id bigint NOT NULL,
  acquired_quantity bigint NOT NULL DEFAULT 0 CHECK (acquired_quantity >= 0),
  updated_at timestamptz NOT NULL,
  UNIQUE (acquisition_run_id, type_id)
);
