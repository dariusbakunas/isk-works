ALTER TABLE builds DROP CONSTRAINT builds_status_valid;
ALTER TABLE builds DROP CONSTRAINT builds_lifecycle_state_consistent;
ALTER TABLE builds
  ADD COLUMN completed_at timestamptz,
  ADD CONSTRAINT builds_status_valid
    CHECK (status IN ('draft', 'planned', 'in_production', 'completed')),
  ADD CONSTRAINT builds_lifecycle_state_consistent CHECK (
    (status = 'draft' AND active_build_plan_id IS NULL
      AND production_started_at IS NULL AND completed_at IS NULL
      AND historical_material_cost IS NULL
      AND historical_material_cost_quality IS NULL AND production_plan_revision IS NULL)
    OR (status = 'planned' AND active_build_plan_id IS NOT NULL
      AND production_started_at IS NULL AND completed_at IS NULL
      AND historical_material_cost IS NULL
      AND historical_material_cost_quality IS NULL AND production_plan_revision IS NULL)
    OR (status = 'in_production' AND active_build_plan_id IS NOT NULL
      AND production_started_at IS NOT NULL AND completed_at IS NULL
      AND historical_material_cost IS NOT NULL
      AND historical_material_cost_quality IS NOT NULL AND production_plan_revision IS NOT NULL)
    OR (status = 'completed' AND active_build_plan_id IS NOT NULL
      AND production_started_at IS NOT NULL AND completed_at IS NOT NULL
      AND historical_material_cost IS NOT NULL
      AND historical_material_cost_quality IS NOT NULL AND production_plan_revision IS NOT NULL)
  );

ALTER TABLE inventory_events DROP CONSTRAINT inventory_events_kind_valid;
ALTER TABLE inventory_events
  ADD COLUMN build_id uuid REFERENCES builds(id) ON DELETE RESTRICT,
  ADD COLUMN build_completion_id uuid,
  ADD CONSTRAINT inventory_events_kind_valid
    CHECK (event_kind IN ('opening_balance', 'purchase', 'consumption', 'production_output', 'reversal')),
  ADD CONSTRAINT inventory_events_build_output_link_consistent CHECK (
    (event_kind = 'production_output' AND build_id IS NOT NULL AND build_completion_id IS NOT NULL)
    OR (event_kind <> 'production_output' AND build_completion_id IS NULL)
  );

CREATE TABLE build_completions (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  build_id uuid NOT NULL UNIQUE REFERENCES builds(id) ON DELETE RESTRICT,
  build_plan_id uuid NOT NULL REFERENCES build_plans(id) ON DELETE RESTRICT,
  plan_revision bigint NOT NULL CHECK (plan_revision > 0),
  facility_snapshot_id uuid NOT NULL REFERENCES build_facility_snapshots(id) ON DELETE RESTRICT,
  recipe_fingerprint text NOT NULL,
  product_type_id bigint NOT NULL CHECK (product_type_id > 0),
  captured_product_name text NOT NULL CHECK (length(btrim(captured_product_name)) > 0),
  product_quantity_per_run bigint NOT NULL CHECK (product_quantity_per_run > 0),
  runs bigint NOT NULL CHECK (runs BETWEEN 1 AND 1000000),
  expected_output_quantity bigint NOT NULL CHECK (expected_output_quantity > 0),
  actual_output_quantity bigint NOT NULL CHECK (actual_output_quantity > 0),
  historical_material_cost numeric(24, 4) NOT NULL CHECK (historical_material_cost >= 0),
  actual_installation_fee_state text NOT NULL
    CHECK (actual_installation_fee_state IN ('recorded', 'explicit_zero')),
  actual_installation_fee numeric(24, 4) NOT NULL CHECK (actual_installation_fee >= 0),
  capitalized_additional_expense_total numeric(24, 4) NOT NULL
    CHECK (capitalized_additional_expense_total >= 0),
  non_capitalized_expense_total numeric(24, 4) NOT NULL
    CHECK (non_capitalized_expense_total >= 0),
  actual_total_build_cost numeric(24, 4) NOT NULL CHECK (actual_total_build_cost >= 0),
  manufactured_unit_cost numeric(24, 4) NOT NULL CHECK (manufactured_unit_cost >= 0),
  cost_quality text NOT NULL
    CHECK (cost_quality IN ('known', 'estimated', 'zero_cost', 'mixed')),
  output_inventory_event_id uuid NOT NULL UNIQUE REFERENCES inventory_events(id)
    ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
  inventory_revision_before bigint NOT NULL CHECK (inventory_revision_before >= 0),
  inventory_revision_after bigint NOT NULL
    CHECK (inventory_revision_after = inventory_revision_before + 1),
  formula_version text NOT NULL,
  notes text NOT NULL DEFAULT '',
  completed_at timestamptz NOT NULL
);

ALTER TABLE inventory_events
  ADD CONSTRAINT inventory_events_completion_fk
  FOREIGN KEY (build_completion_id) REFERENCES build_completions(id)
  ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED;

CREATE UNIQUE INDEX inventory_events_one_output_per_completion_idx
  ON inventory_events (build_completion_id)
  WHERE event_kind = 'production_output';
CREATE UNIQUE INDEX inventory_events_one_output_per_build_idx
  ON inventory_events (build_id)
  WHERE event_kind = 'production_output';

CREATE TABLE build_manufacturing_expenses (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  build_id uuid NOT NULL REFERENCES builds(id) ON DELETE RESTRICT,
  completion_id uuid NOT NULL REFERENCES build_completions(id) ON DELETE RESTRICT,
  category text NOT NULL CHECK (category IN (
    'installation_fee', 'facility_surcharge', 'fuel', 'logistics',
    'manual_adjustment', 'other'
  )),
  description text NOT NULL CHECK (length(btrim(description)) BETWEEN 1 AND 240),
  amount numeric(24, 4) NOT NULL CHECK (amount >= 0),
  capitalized boolean NOT NULL,
  cost_quality text NOT NULL CHECK (cost_quality IN ('known', 'estimated', 'zero_cost')),
  source text NOT NULL CHECK (length(btrim(source)) > 0),
  source_reference text,
  recorded_at timestamptz NOT NULL
);
CREATE INDEX build_manufacturing_expenses_build_idx
  ON build_manufacturing_expenses (build_id, recorded_at, id);

ALTER TABLE build_activity DROP CONSTRAINT build_activity_activity_kind_check;
ALTER TABLE build_activity ADD CONSTRAINT build_activity_activity_kind_check
  CHECK (activity_kind IN (
    'materials_reserved', 'reservations_refreshed', 'reservations_released',
    'returned_to_draft', 'production_started', 'materials_consumed',
    'production_completed', 'manufactured_output_added', 'actual_cost_recorded'
  ));

CREATE FUNCTION reject_completed_accounting_update() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'completed Build accounting records are immutable';
END;
$$;

CREATE TRIGGER build_completions_immutable
BEFORE UPDATE ON build_completions
FOR EACH ROW EXECUTE FUNCTION reject_completed_accounting_update();

CREATE TRIGGER build_manufacturing_expenses_immutable
BEFORE UPDATE ON build_manufacturing_expenses
FOR EACH ROW EXECUTE FUNCTION reject_completed_accounting_update();
