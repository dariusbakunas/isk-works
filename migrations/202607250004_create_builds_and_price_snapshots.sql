CREATE TABLE builds (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  display_name text NOT NULL,
  status text NOT NULL,
  blueprint_type_id bigint NOT NULL CHECK (blueprint_type_id > 0),
  blueprint_name text NOT NULL,
  product_type_id bigint NOT NULL CHECK (product_type_id > 0),
  product_name text NOT NULL,
  product_quantity_per_run bigint NOT NULL CHECK (product_quantity_per_run > 0),
  duration_seconds_per_run bigint CHECK (duration_seconds_per_run > 0),
  source_sde_dataset_id uuid NOT NULL REFERENCES sde_imports(id) ON DELETE RESTRICT,
  source_sde_version text NOT NULL,
  recipe_fingerprint text NOT NULL,
  runs bigint NOT NULL CHECK (runs BETWEEN 1 AND 1000000),
  notes text NOT NULL DEFAULT '',
  active_build_plan_id uuid,
  revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  planned_at timestamptz,
  CONSTRAINT builds_name_not_empty CHECK (length(btrim(display_name)) BETWEEN 1 AND 160),
  CONSTRAINT builds_status_valid CHECK (status IN ('draft', 'planned')),
  CONSTRAINT builds_planned_state_consistent CHECK (
    (status = 'draft' AND active_build_plan_id IS NULL)
    OR (status = 'planned' AND active_build_plan_id IS NOT NULL)
  )
);

CREATE INDEX builds_workspace_updated_idx ON builds (workspace_id, updated_at DESC);
CREATE INDEX builds_workspace_status_idx ON builds (workspace_id, status);

CREATE TABLE build_recipe_materials (
  build_id uuid NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
  type_id bigint NOT NULL CHECK (type_id > 0),
  captured_name text NOT NULL,
  quantity_per_run bigint NOT NULL CHECK (quantity_per_run > 0),
  sort_order integer NOT NULL CHECK (sort_order >= 0),
  PRIMARY KEY (build_id, type_id),
  UNIQUE (build_id, sort_order)
);

CREATE TABLE build_recipe_products (
  build_id uuid NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
  type_id bigint NOT NULL CHECK (type_id > 0),
  captured_name text NOT NULL,
  quantity_per_run bigint NOT NULL CHECK (quantity_per_run > 0),
  sort_order integer NOT NULL CHECK (sort_order >= 0),
  PRIMARY KEY (build_id, type_id),
  UNIQUE (build_id, sort_order)
);

CREATE TABLE price_sources (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  display_name text NOT NULL,
  description text NOT NULL DEFAULT '',
  source_kind text NOT NULL,
  is_default boolean NOT NULL DEFAULT false,
  revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  CONSTRAINT price_sources_name_not_empty CHECK (length(btrim(display_name)) BETWEEN 1 AND 120),
  CONSTRAINT price_sources_kind_valid CHECK (source_kind IN ('manual'))
);

CREATE UNIQUE INDEX price_sources_one_default_per_workspace_idx
  ON price_sources (workspace_id) WHERE is_default;
CREATE INDEX price_sources_workspace_updated_idx
  ON price_sources (workspace_id, updated_at DESC);

CREATE TABLE price_source_items (
  price_source_id uuid NOT NULL REFERENCES price_sources(id) ON DELETE CASCADE,
  type_id bigint NOT NULL CHECK (type_id > 0),
  captured_name text NOT NULL,
  price numeric(24, 4) NOT NULL CHECK (price >= 0),
  note text NOT NULL DEFAULT '',
  updated_at timestamptz NOT NULL,
  PRIMARY KEY (price_source_id, type_id)
);

CREATE TABLE price_snapshots (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  build_id uuid NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
  price_source_id uuid REFERENCES price_sources(id) ON DELETE SET NULL,
  captured_source_name text NOT NULL,
  captured_source_revision bigint NOT NULL CHECK (captured_source_revision > 0),
  purpose text NOT NULL,
  created_at timestamptz NOT NULL,
  CONSTRAINT price_snapshots_purpose_valid CHECK (purpose IN ('build_planning'))
);

CREATE INDEX price_snapshots_build_created_idx
  ON price_snapshots (build_id, created_at DESC);

CREATE TABLE price_snapshot_items (
  price_snapshot_id uuid NOT NULL REFERENCES price_snapshots(id) ON DELETE CASCADE,
  type_id bigint NOT NULL CHECK (type_id > 0),
  captured_name text NOT NULL,
  price numeric(24, 4) CHECK (price >= 0),
  missing boolean NOT NULL,
  source_note text NOT NULL DEFAULT '',
  sort_order integer NOT NULL CHECK (sort_order >= 0),
  PRIMARY KEY (price_snapshot_id, type_id),
  UNIQUE (price_snapshot_id, sort_order),
  CONSTRAINT price_snapshot_items_missing_consistent CHECK (
    (missing AND price IS NULL) OR (NOT missing AND price IS NOT NULL)
  )
);

CREATE TABLE build_plans (
  id uuid PRIMARY KEY,
  build_id uuid NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
  plan_revision bigint NOT NULL CHECK (plan_revision > 0),
  price_snapshot_id uuid NOT NULL UNIQUE REFERENCES price_snapshots(id) ON DELETE CASCADE,
  recipe_fingerprint text NOT NULL,
  runs bigint NOT NULL CHECK (runs BETWEEN 1 AND 1000000),
  pricing_complete boolean NOT NULL,
  estimated_material_cost numeric(24, 4) NOT NULL CHECK (estimated_material_cost >= 0),
  expected_revenue numeric(24, 4) CHECK (expected_revenue >= 0),
  estimated_margin numeric(24, 4),
  missing_price_count integer NOT NULL CHECK (missing_price_count >= 0),
  active boolean NOT NULL,
  created_at timestamptz NOT NULL,
  superseded_at timestamptz,
  UNIQUE (build_id, plan_revision),
  CONSTRAINT build_plans_active_consistent CHECK (
    (active AND superseded_at IS NULL) OR (NOT active AND superseded_at IS NOT NULL)
  )
);

CREATE UNIQUE INDEX build_plans_one_active_per_build_idx
  ON build_plans (build_id) WHERE active;
CREATE INDEX build_plans_build_revision_idx
  ON build_plans (build_id, plan_revision DESC);

ALTER TABLE builds
  ADD CONSTRAINT builds_active_plan_fk
  FOREIGN KEY (active_build_plan_id) REFERENCES build_plans(id) ON DELETE RESTRICT
  DEFERRABLE INITIALLY DEFERRED;
