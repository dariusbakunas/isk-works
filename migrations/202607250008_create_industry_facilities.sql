CREATE TABLE industry_facility_profiles (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  display_name text NOT NULL CHECK (length(btrim(display_name)) BETWEEN 1 AND 120),
  facility_kind text NOT NULL CHECK (facility_kind IN ('npc_station', 'upwell_structure', 'manual')),
  structure_id bigint CHECK (structure_id > 0),
  structure_type_id bigint CHECK (structure_type_id > 0),
  structure_type_name text NOT NULL DEFAULT '',
  solar_system_id bigint CHECK (solar_system_id > 0),
  solar_system_name text NOT NULL DEFAULT '',
  security_class text NOT NULL CHECK (security_class IN ('high_sec', 'low_sec', 'null_sec', 'wormhole', 'unknown')),
  material_reduction_percent numeric(9, 6) NOT NULL DEFAULT 0 CHECK (material_reduction_percent BETWEEN 0 AND 99.999999),
  time_reduction_percent numeric(9, 6) NOT NULL DEFAULT 0 CHECK (time_reduction_percent BETWEEN 0 AND 99.999999),
  facility_tax_percent numeric(9, 6) NOT NULL DEFAULT 0 CHECK (facility_tax_percent BETWEEN 0 AND 100),
  scc_surcharge_percent numeric(9, 6) NOT NULL DEFAULT 0 CHECK (scc_surcharge_percent BETWEEN 0 AND 100),
  alliance_surcharge_percent numeric(9, 6) NOT NULL DEFAULT 0 CHECK (alliance_surcharge_percent BETWEEN 0 AND 100),
  fixed_supplemental_cost numeric(24, 4) NOT NULL DEFAULT 0 CHECK (fixed_supplemental_cost >= 0),
  manual_system_cost_index numeric(12, 10) CHECK (manual_system_cost_index BETWEEN 0 AND 1),
  notes text NOT NULL DEFAULT '',
  archived_at timestamptz,
  revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  CHECK ((facility_kind = 'upwell_structure' AND structure_id IS NOT NULL) OR facility_kind <> 'upwell_structure')
);
CREATE INDEX industry_facility_profiles_workspace_idx
  ON industry_facility_profiles (workspace_id, archived_at, display_name);

CREATE TABLE industry_facility_profile_rigs (
  facility_profile_id uuid NOT NULL REFERENCES industry_facility_profiles(id) ON DELETE CASCADE,
  slot_number smallint NOT NULL CHECK (slot_number BETWEEN 1 AND 3),
  type_id bigint NOT NULL CHECK (type_id > 0),
  captured_name text NOT NULL CHECK (length(btrim(captured_name)) > 0),
  material_reduction_percent numeric(9, 6) NOT NULL DEFAULT 0 CHECK (material_reduction_percent BETWEEN 0 AND 99.999999),
  time_reduction_percent numeric(9, 6) NOT NULL DEFAULT 0 CHECK (time_reduction_percent BETWEEN 0 AND 99.999999),
  PRIMARY KEY (facility_profile_id, slot_number),
  UNIQUE (facility_profile_id, type_id)
);

CREATE TABLE industry_system_cost_index_observations (
  id uuid PRIMARY KEY,
  solar_system_id bigint NOT NULL CHECK (solar_system_id > 0),
  activity text NOT NULL CHECK (activity = 'manufacturing'),
  cost_index numeric(12, 10) NOT NULL CHECK (cost_index BETWEEN 0 AND 1),
  observed_at timestamptz NOT NULL,
  expires_at timestamptz,
  etag text,
  source_checksum text NOT NULL,
  source_url text NOT NULL,
  UNIQUE (solar_system_id, activity, observed_at, source_checksum)
);

CREATE TABLE industry_adjusted_price_observations (
  id uuid PRIMARY KEY,
  type_id bigint NOT NULL CHECK (type_id > 0),
  adjusted_price numeric(24, 4) NOT NULL CHECK (adjusted_price >= 0),
  observed_at timestamptz NOT NULL,
  expires_at timestamptz,
  etag text,
  source_checksum text NOT NULL,
  source_url text NOT NULL,
  UNIQUE (type_id, observed_at, source_checksum)
);

CREATE TABLE build_facility_snapshots (
  id uuid PRIMARY KEY,
  build_id uuid NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
  build_plan_id uuid NOT NULL UNIQUE REFERENCES build_plans(id) ON DELETE CASCADE,
  facility_profile_id uuid REFERENCES industry_facility_profiles(id) ON DELETE SET NULL,
  captured_profile_revision bigint CHECK (captured_profile_revision > 0),
  captured_name text NOT NULL,
  facility_kind text NOT NULL CHECK (facility_kind IN ('legacy', 'npc_station', 'upwell_structure', 'manual')),
  structure_id bigint,
  structure_type_id bigint,
  structure_type_name text NOT NULL DEFAULT '',
  solar_system_id bigint,
  solar_system_name text NOT NULL DEFAULT '',
  security_class text NOT NULL,
  blueprint_me smallint NOT NULL CHECK (blueprint_me BETWEEN 0 AND 10),
  blueprint_te smallint NOT NULL CHECK (blueprint_te BETWEEN 0 AND 20),
  material_reduction_percent numeric(9, 6) NOT NULL,
  time_reduction_percent numeric(9, 6) NOT NULL,
  facility_tax_percent numeric(9, 6) NOT NULL,
  scc_surcharge_percent numeric(9, 6) NOT NULL,
  alliance_surcharge_percent numeric(9, 6) NOT NULL,
  fixed_supplemental_cost numeric(24, 4) NOT NULL,
  system_cost_index numeric(12, 10),
  cost_index_source text NOT NULL CHECK (cost_index_source IN ('manual', 'esi', 'unavailable', 'legacy')),
  cost_index_observed_at timestamptz,
  planned_duration_seconds bigint CHECK (planned_duration_seconds > 0),
  estimated_item_value numeric(24, 4),
  planned_installation_cost numeric(24, 4),
  installation_cost_complete boolean NOT NULL,
  calculation_trace jsonb NOT NULL DEFAULT '{}'::jsonb,
  formula_version text NOT NULL,
  created_at timestamptz NOT NULL
);

CREATE TABLE build_facility_snapshot_rigs (
  facility_snapshot_id uuid NOT NULL REFERENCES build_facility_snapshots(id) ON DELETE CASCADE,
  slot_number smallint NOT NULL CHECK (slot_number BETWEEN 1 AND 3),
  type_id bigint NOT NULL CHECK (type_id > 0),
  captured_name text NOT NULL,
  material_reduction_percent numeric(9, 6) NOT NULL,
  time_reduction_percent numeric(9, 6) NOT NULL,
  PRIMARY KEY (facility_snapshot_id, slot_number)
);

CREATE TABLE build_effective_material_requirements (
  build_plan_id uuid NOT NULL REFERENCES build_plans(id) ON DELETE CASCADE,
  build_id uuid NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
  facility_snapshot_id uuid REFERENCES build_facility_snapshots(id) ON DELETE CASCADE,
  type_id bigint NOT NULL CHECK (type_id > 0),
  captured_name text NOT NULL,
  sort_order integer NOT NULL CHECK (sort_order >= 0),
  base_quantity_per_run bigint NOT NULL CHECK (base_quantity_per_run > 0),
  runs bigint NOT NULL CHECK (runs BETWEEN 1 AND 1000000),
  base_extended_quantity bigint NOT NULL CHECK (base_extended_quantity > 0),
  blueprint_me smallint NOT NULL CHECK (blueprint_me BETWEEN 0 AND 10),
  final_required_quantity bigint NOT NULL CHECK (final_required_quantity > 0),
  calculation_trace jsonb NOT NULL DEFAULT '{}'::jsonb,
  formula_version text NOT NULL,
  recipe_fingerprint text NOT NULL,
  PRIMARY KEY (build_plan_id, type_id),
  UNIQUE (build_plan_id, sort_order)
);

-- Compatibility capture: preserve the exact direct requirements used by all existing plans.
INSERT INTO build_effective_material_requirements (
  build_plan_id, build_id, type_id, captured_name, sort_order,
  base_quantity_per_run, runs, base_extended_quantity, blueprint_me,
  final_required_quantity, calculation_trace, formula_version, recipe_fingerprint
)
SELECT bp.id, bp.build_id, m.type_id, m.captured_name, m.sort_order,
  m.quantity_per_run, bp.runs, m.quantity_per_run * bp.runs, 0,
  m.quantity_per_run * bp.runs,
  jsonb_build_object('migration', 'legacy captured recipe x plan runs'),
  'legacy-direct-v1', bp.recipe_fingerprint
FROM build_plans bp
JOIN build_recipe_materials m ON m.build_id = bp.build_id;

ALTER TABLE build_material_reservations
  ADD COLUMN build_plan_id uuid REFERENCES build_plans(id) ON DELETE RESTRICT;
UPDATE build_material_reservations r
SET build_plan_id = b.active_build_plan_id
FROM builds b
WHERE b.id = r.build_id;
CREATE INDEX build_material_reservations_plan_idx
  ON build_material_reservations (build_plan_id, status);
