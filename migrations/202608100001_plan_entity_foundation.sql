-- Build loses its lifecycle entirely -- it becomes a stateless, always-
-- editable configuration. Everything below is the commit/reservation/
-- production/completion machinery this retires from Build, on the way
-- to being rebuilt on the new `plans` table (this migration) and, in
-- later slices, `plan_material_reservations`/`plan_completions`/etc.
--
-- No in-flight Planned/InProduction/Completed builds exist in this
-- workspace as of this migration (verified: `SELECT status, count(*)
-- FROM builds GROUP BY status` -> draft: 4 only) -- everything dropped
-- here is genuinely legacy at this point, not data being discarded out
-- from under real work. `IF EXISTS ... CASCADE` throughout so the drop
-- order doesn't matter (several of these tables reference each other).
DROP TABLE IF EXISTS build_manufacturing_expenses CASCADE;
DROP TABLE IF EXISTS build_completions CASCADE;
DROP TABLE IF EXISTS build_material_consumptions CASCADE;
DROP TABLE IF EXISTS build_material_reservations CASCADE;
DROP TABLE IF EXISTS build_activity CASCADE;
DROP TABLE IF EXISTS build_effective_material_requirements CASCADE;
DROP TABLE IF EXISTS build_facility_snapshot_rigs CASCADE;
DROP TABLE IF EXISTS build_facility_snapshots CASCADE;
DROP TABLE IF EXISTS build_blueprint_snapshots CASCADE;
DROP TABLE IF EXISTS build_plans CASCADE;

ALTER TABLE builds DROP CONSTRAINT IF EXISTS builds_lifecycle_state_consistent;
ALTER TABLE builds DROP CONSTRAINT IF EXISTS builds_status_valid;
ALTER TABLE builds
  DROP COLUMN IF EXISTS status,
  DROP COLUMN IF EXISTS active_build_plan_id,
  -- Present since the very first builds migration but never covered by
  -- `builds_lifecycle_state_consistent` -- "when was this committed" is
  -- `plans.committed_at` now, not a Build concept.
  DROP COLUMN IF EXISTS planned_at,
  DROP COLUMN IF EXISTS production_started_at,
  DROP COLUMN IF EXISTS historical_material_cost,
  DROP COLUMN IF EXISTS historical_material_cost_quality,
  DROP COLUMN IF EXISTS production_plan_revision,
  DROP COLUMN IF EXISTS completed_at;

ALTER TABLE inventory_events DROP CONSTRAINT IF EXISTS inventory_events_kind_valid;
ALTER TABLE inventory_events DROP CONSTRAINT IF EXISTS inventory_events_build_output_link_consistent;
ALTER TABLE inventory_events
  DROP COLUMN IF EXISTS build_id,
  DROP COLUMN IF EXISTS build_completion_id,
  ADD CONSTRAINT inventory_events_kind_valid
    CHECK (event_kind IN ('opening_balance', 'purchase', 'consumption', 'reversal'));

-- `blueprint_observations` (real observed blueprint-asset data, used by
-- `BlueprintSelection::ObservedAsset` throughout the planner) is
-- untouched -- only `build_blueprint_snapshots` (the commit-time
-- capture, dropped above) referenced `build_plans`.

CREATE TABLE plans (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  source_build_id uuid NOT NULL REFERENCES builds(id) ON DELETE RESTRICT,
  source_build_revision bigint NOT NULL CHECK (source_build_revision > 0),
  display_name text NOT NULL,
  status text NOT NULL CHECK (status IN ('draft', 'committed', 'in_production', 'completed', 'archived')),
  revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
  price_snapshot_id uuid NOT NULL UNIQUE REFERENCES price_snapshots(id) ON DELETE CASCADE,
  recipe_fingerprint text NOT NULL,
  runs bigint NOT NULL CHECK (runs BETWEEN 1 AND 1000000),
  pricing_complete boolean NOT NULL,
  estimated_material_cost numeric(24, 4) NOT NULL CHECK (estimated_material_cost >= 0),
  expected_revenue numeric(24, 4) CHECK (expected_revenue >= 0),
  estimated_margin numeric(24, 4),
  missing_price_count integer NOT NULL CHECK (missing_price_count >= 0),
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  committed_at timestamptz,
  started_at timestamptz,
  completed_at timestamptz,
  archived_at timestamptz,
  CONSTRAINT plans_name_not_empty CHECK (length(btrim(display_name)) BETWEEN 1 AND 160),
  CONSTRAINT plans_lifecycle_timestamps_consistent CHECK (
    (status = 'draft' AND committed_at IS NULL AND started_at IS NULL AND completed_at IS NULL)
    OR (status = 'committed' AND committed_at IS NOT NULL AND started_at IS NULL AND completed_at IS NULL)
    OR (status = 'in_production' AND committed_at IS NOT NULL AND started_at IS NOT NULL AND completed_at IS NULL)
    OR (status = 'completed' AND committed_at IS NOT NULL AND started_at IS NOT NULL AND completed_at IS NOT NULL)
    OR (status = 'archived')
  )
);

CREATE INDEX plans_workspace_updated_idx ON plans (workspace_id, updated_at DESC);
CREATE INDEX plans_source_build_idx ON plans (source_build_id, revision DESC);

-- `plan_source_builds`, `plan_phases`, `plan_tasks` are Slice 2/3's
-- tables, not this one -- Slice 1's Plan is a bare snapshot only.
