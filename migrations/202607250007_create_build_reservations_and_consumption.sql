ALTER TABLE builds DROP CONSTRAINT builds_status_valid;
ALTER TABLE builds DROP CONSTRAINT builds_planned_state_consistent;
ALTER TABLE builds
  ADD COLUMN production_started_at timestamptz,
  ADD COLUMN historical_material_cost numeric(24, 4) CHECK (historical_material_cost >= 0),
  ADD COLUMN historical_material_cost_quality text
    CHECK (historical_material_cost_quality IN ('known', 'estimated', 'zero_cost', 'mixed')),
  ADD COLUMN production_plan_revision bigint CHECK (production_plan_revision > 0),
  ADD CONSTRAINT builds_status_valid
    CHECK (status IN ('draft', 'planned', 'in_production')),
  ADD CONSTRAINT builds_lifecycle_state_consistent CHECK (
    (status = 'draft' AND active_build_plan_id IS NULL
      AND production_started_at IS NULL AND historical_material_cost IS NULL
      AND historical_material_cost_quality IS NULL AND production_plan_revision IS NULL)
    OR (status = 'planned' AND active_build_plan_id IS NOT NULL
      AND production_started_at IS NULL AND historical_material_cost IS NULL
      AND historical_material_cost_quality IS NULL AND production_plan_revision IS NULL)
    OR (status = 'in_production' AND active_build_plan_id IS NOT NULL
      AND production_started_at IS NOT NULL AND historical_material_cost IS NOT NULL
      AND historical_material_cost_quality IS NOT NULL AND production_plan_revision IS NOT NULL)
  );

ALTER TABLE inventory_events DROP CONSTRAINT inventory_events_kind_valid;
ALTER TABLE inventory_events ADD CONSTRAINT inventory_events_kind_valid
  CHECK (event_kind IN ('opening_balance', 'purchase', 'consumption', 'reversal'));

CREATE TABLE build_material_reservations (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  build_id uuid NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
  type_id bigint NOT NULL CHECK (type_id > 0),
  captured_name text NOT NULL CHECK (length(btrim(captured_name)) > 0),
  sort_order integer NOT NULL CHECK (sort_order >= 0),
  required_quantity bigint NOT NULL CHECK (required_quantity > 0),
  reserved_quantity bigint NOT NULL CHECK (reserved_quantity > 0),
  status text NOT NULL CHECK (status IN ('active', 'released', 'consumed')),
  recipe_fingerprint text NOT NULL,
  build_runs bigint NOT NULL CHECK (build_runs BETWEEN 1 AND 1000000),
  revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  released_at timestamptz,
  consumed_at timestamptz,
  CHECK (reserved_quantity <= required_quantity),
  CHECK (
    (status = 'active' AND released_at IS NULL AND consumed_at IS NULL)
    OR (status = 'released' AND released_at IS NOT NULL AND consumed_at IS NULL)
    OR (status = 'consumed' AND released_at IS NULL AND consumed_at IS NOT NULL)
  )
);

CREATE UNIQUE INDEX build_material_reservations_one_active_idx
  ON build_material_reservations (build_id, type_id) WHERE status = 'active';
CREATE INDEX build_material_reservations_build_idx
  ON build_material_reservations (build_id, status, sort_order);
CREATE INDEX build_material_reservations_item_idx
  ON build_material_reservations (workspace_id, owner_id, type_id)
  WHERE status = 'active';

CREATE TABLE build_material_consumptions (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  build_id uuid NOT NULL REFERENCES builds(id) ON DELETE RESTRICT,
  reservation_id uuid NOT NULL UNIQUE REFERENCES build_material_reservations(id) ON DELETE RESTRICT,
  inventory_event_id uuid NOT NULL UNIQUE REFERENCES inventory_events(id) ON DELETE RESTRICT,
  type_id bigint NOT NULL CHECK (type_id > 0),
  captured_name text NOT NULL CHECK (length(btrim(captured_name)) > 0),
  sort_order integer NOT NULL CHECK (sort_order >= 0),
  required_quantity bigint NOT NULL CHECK (required_quantity > 0),
  consumed_quantity bigint NOT NULL CHECK (consumed_quantity > 0),
  average_unit_cost numeric(24, 4) NOT NULL CHECK (average_unit_cost >= 0),
  historical_total_cost numeric(24, 4) NOT NULL CHECK (historical_total_cost >= 0),
  cost_quality text NOT NULL CHECK (cost_quality IN ('known', 'estimated', 'zero_cost')),
  inventory_revision_before bigint NOT NULL CHECK (inventory_revision_before > 0),
  inventory_revision_after bigint NOT NULL CHECK (inventory_revision_after > inventory_revision_before),
  recipe_fingerprint text NOT NULL,
  build_runs bigint NOT NULL CHECK (build_runs BETWEEN 1 AND 1000000),
  consumed_at timestamptz NOT NULL,
  UNIQUE (build_id, type_id),
  CHECK (consumed_quantity = required_quantity)
);

CREATE INDEX build_material_consumptions_build_idx
  ON build_material_consumptions (build_id, sort_order);

CREATE TABLE build_activity (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  build_id uuid NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
  activity_kind text NOT NULL CHECK (activity_kind IN (
    'materials_reserved', 'reservations_refreshed', 'reservations_released',
    'returned_to_draft', 'production_started', 'materials_consumed'
  )),
  summary text NOT NULL CHECK (length(btrim(summary)) > 0),
  metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
  recorded_at timestamptz NOT NULL
);

CREATE INDEX build_activity_build_time_idx ON build_activity (build_id, recorded_at DESC);
