CREATE TABLE blueprint_observations (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id),
  owner_id uuid NOT NULL REFERENCES owners(id),
  connection_id uuid NULL REFERENCES eve_connections(id),
  eve_item_id bigint NOT NULL CHECK (eve_item_id > 0),
  blueprint_type_id bigint NOT NULL CHECK (blueprint_type_id > 0),
  captured_blueprint_name text NOT NULL,
  blueprint_kind text NOT NULL CHECK (blueprint_kind IN ('original','copy','unknown')),
  material_efficiency smallint NOT NULL CHECK (material_efficiency BETWEEN 0 AND 10),
  time_efficiency smallint NOT NULL CHECK (time_efficiency BETWEEN 0 AND 20),
  licensed_runs bigint NULL CHECK (licensed_runs > 0),
  location_id bigint NOT NULL CHECK (location_id > 0),
  location_flag text NOT NULL,
  captured_location_name text NULL,
  observed_at timestamptz NOT NULL,
  imported_at timestamptz NOT NULL DEFAULT now(),
  source_payload jsonb NOT NULL DEFAULT '{}'::jsonb,
  source_checksum text NOT NULL,
  UNIQUE (workspace_id, owner_id, eve_item_id, observed_at)
);

CREATE INDEX blueprint_observations_compatible_idx
  ON blueprint_observations (workspace_id, owner_id, blueprint_type_id, observed_at DESC);

CREATE TABLE build_blueprint_snapshots (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id),
  owner_id uuid NOT NULL REFERENCES owners(id),
  build_id uuid NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
  build_plan_id uuid NOT NULL UNIQUE REFERENCES build_plans(id) ON DELETE CASCADE,
  source_mode text NOT NULL CHECK (source_mode IN ('manual','observed_asset','legacy_migration')),
  blueprint_type_id bigint NOT NULL CHECK (blueprint_type_id > 0),
  captured_blueprint_name text NOT NULL,
  blueprint_kind text NOT NULL CHECK (blueprint_kind IN ('original','copy','unknown')),
  material_efficiency smallint NOT NULL CHECK (material_efficiency BETWEEN 0 AND 10),
  time_efficiency smallint NOT NULL CHECK (time_efficiency BETWEEN 0 AND 20),
  licensed_runs bigint NULL CHECK (licensed_runs > 0),
  requested_runs bigint NOT NULL CHECK (requested_runs > 0),
  source_observation_id uuid NULL REFERENCES blueprint_observations(id),
  source_eve_item_id bigint NULL CHECK (source_eve_item_id > 0),
  source_owner_id uuid NULL REFERENCES owners(id),
  captured_source_owner_name text NULL,
  source_location_id bigint NULL,
  captured_source_location_name text NULL,
  observed_at timestamptz NULL,
  imported_at timestamptz NULL,
  manual_notes text NULL,
  planned_duration_seconds bigint NULL CHECK (planned_duration_seconds > 0),
  formula_version text NOT NULL,
  captured_at timestamptz NOT NULL,
  CHECK (blueprint_kind <> 'copy' OR licensed_runs IS NULL OR requested_runs <= licensed_runs)
);

INSERT INTO build_blueprint_snapshots (
  id, workspace_id, owner_id, build_id, build_plan_id, source_mode,
  blueprint_type_id, captured_blueprint_name, blueprint_kind,
  material_efficiency, time_efficiency, licensed_runs, requested_runs,
  planned_duration_seconds, formula_version, captured_at
)
SELECT gen_random_uuid(), b.workspace_id, b.owner_id, bp.build_id, bp.id, 'legacy_migration',
       b.blueprint_type_id, b.blueprint_name, 'unknown',
       COALESCE(fs.blueprint_me, 0), COALESCE(fs.blueprint_te, 0), NULL, bp.runs,
       fs.planned_duration_seconds, 'legacy-blueprint-snapshot-v1', bp.created_at
FROM build_plans bp
JOIN builds b ON b.id = bp.build_id
LEFT JOIN build_facility_snapshots fs ON fs.build_plan_id = bp.id;

CREATE FUNCTION prevent_blueprint_snapshot_mutation() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'build blueprint snapshots are immutable';
END;
$$;

CREATE TRIGGER build_blueprint_snapshots_immutable
BEFORE UPDATE OR DELETE ON build_blueprint_snapshots
FOR EACH ROW EXECUTE FUNCTION prevent_blueprint_snapshot_mutation();
