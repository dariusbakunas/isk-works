-- Evolves plan_tasks into the "Ticket" concept: a committed Plan's tasks get a
-- stable, human-readable display_id (ISK-####, assigned once at commit time --
-- see IndustryService::commit_plan) and can optionally belong to an
-- AcquisitionRun (ACQ-####), the first kind of Execution Batch. Tickets are
-- never deleted or renumbered once assigned; plan_tasks itself is unchanged
-- for still-Draft plans (display_id stays NULL, invisible to the Board).
CREATE SEQUENCE ticket_display_id_seq START 1000;
CREATE SEQUENCE acquisition_run_display_id_seq START 1;

CREATE TABLE acquisition_runs (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  display_id text NOT NULL UNIQUE,
  name text NOT NULL,
  -- Only kind implemented so far -- see the design doc for why this stays a
  -- CHECK constraint rather than a speculative enum of future kinds.
  kind text NOT NULL CHECK (kind = 'acquisition'),
  status text NOT NULL CHECK (status IN ('ready', 'in_progress', 'complete')),
  -- Shared acquisition-location key for every member ticket (their owning
  -- Plan's price_source_id) -- see decision 1 in the design doc.
  price_source_id uuid REFERENCES price_sources(id) ON DELETE RESTRICT,
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  started_at timestamptz,
  completed_at timestamptz,
  CONSTRAINT acquisition_runs_name_not_empty CHECK (length(btrim(name)) BETWEEN 1 AND 160),
  CONSTRAINT acquisition_runs_lifecycle_timestamps_consistent CHECK (
    (status = 'ready' AND started_at IS NULL AND completed_at IS NULL)
    OR (status = 'in_progress' AND started_at IS NOT NULL AND completed_at IS NULL)
    OR (status = 'complete' AND started_at IS NOT NULL AND completed_at IS NOT NULL)
  )
);

CREATE INDEX acquisition_runs_workspace_updated_idx ON acquisition_runs (workspace_id, updated_at DESC);

ALTER TABLE plan_tasks
  ADD COLUMN display_id text UNIQUE,
  ADD COLUMN acquisition_run_id uuid REFERENCES acquisition_runs (id) ON DELETE SET NULL,
  ADD COLUMN acquired_quantity bigint CHECK (acquired_quantity >= 0);

ALTER TABLE plan_tasks
  ADD CONSTRAINT plan_tasks_acquired_quantity_only_when_batched
    CHECK (acquired_quantity IS NULL OR acquisition_run_id IS NOT NULL);

CREATE INDEX plan_tasks_acquisition_run_id_idx ON plan_tasks (acquisition_run_id);

-- Backfill: plans committed before this migration never got a display_id
-- retroactively assigned (only commit_plan going forward does this).
UPDATE plan_tasks pt
SET display_id = 'ISK-' || nextval('ticket_display_id_seq')
FROM plans p
WHERE pt.plan_id = p.id
  AND p.status != 'draft'
  AND pt.display_id IS NULL;
