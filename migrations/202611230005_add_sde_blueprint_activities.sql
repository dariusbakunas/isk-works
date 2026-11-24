-- Every industry activity of a blueprint or reaction formula, from the
-- SDE's blueprints.jsonl, in one shape: its time, what it consumes, what it
-- yields and the skills it needs. The Type Info window lists them the way
-- the game's blueprint info window does.
--
-- Only published types in the Blueprint category are kept: a blueprint the
-- game does not offer has no place in the app.
--
-- These tables are meant to become the one source of recipes. For now the
-- planner and the Invention page still read manufacturing, reaction and
-- invention recipes from their own tables (sde_blueprints*,
-- sde_reaction_formula*, sde_blueprint_invention*), which these repeat; they
-- also add what those leave out: material and time research, copying
-- materials, and each activity's skills.
CREATE TABLE sde_blueprint_activities (
  import_id uuid NOT NULL,
  blueprint_type_id bigint NOT NULL,
  activity text NOT NULL,
  duration_seconds bigint,
  PRIMARY KEY (import_id, blueprint_type_id, activity),
  FOREIGN KEY (import_id, blueprint_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE CASCADE,
  CONSTRAINT sde_blueprint_activities_activity_known CHECK (
    activity IN (
      'manufacturing', 'research_material', 'research_time', 'copying', 'invention', 'reaction'
    )
  ),
  CONSTRAINT sde_blueprint_activities_duration_positive
    CHECK (duration_seconds IS NULL OR duration_seconds > 0)
);

CREATE TABLE sde_blueprint_activity_materials (
  import_id uuid NOT NULL,
  blueprint_type_id bigint NOT NULL,
  activity text NOT NULL,
  material_type_id bigint NOT NULL,
  quantity bigint NOT NULL,
  position integer NOT NULL,
  PRIMARY KEY (import_id, blueprint_type_id, activity, material_type_id),
  FOREIGN KEY (import_id, blueprint_type_id, activity)
    REFERENCES sde_blueprint_activities(import_id, blueprint_type_id, activity)
    ON DELETE CASCADE,
  FOREIGN KEY (import_id, material_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE RESTRICT,
  CONSTRAINT sde_blueprint_activity_materials_quantity_positive CHECK (quantity > 0),
  CONSTRAINT sde_blueprint_activity_materials_position_non_negative CHECK (position >= 0)
);

-- For invention, quantity is the runs on the invented copy and probability
-- the base success chance; other activities leave probability NULL.
CREATE TABLE sde_blueprint_activity_products (
  import_id uuid NOT NULL,
  blueprint_type_id bigint NOT NULL,
  activity text NOT NULL,
  product_type_id bigint NOT NULL,
  quantity bigint NOT NULL,
  probability numeric,
  position integer NOT NULL,
  PRIMARY KEY (import_id, blueprint_type_id, activity, product_type_id),
  FOREIGN KEY (import_id, blueprint_type_id, activity)
    REFERENCES sde_blueprint_activities(import_id, blueprint_type_id, activity)
    ON DELETE CASCADE,
  FOREIGN KEY (import_id, product_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE RESTRICT,
  CONSTRAINT sde_blueprint_activity_products_quantity_positive CHECK (quantity > 0),
  CONSTRAINT sde_blueprint_activity_products_probability_range
    CHECK (probability IS NULL OR (probability > 0 AND probability <= 1)),
  CONSTRAINT sde_blueprint_activity_products_position_non_negative CHECK (position >= 0)
);

-- What makes or invents a type: the lookup recipe readers start from.
CREATE INDEX sde_blueprint_activity_products_product_idx
  ON sde_blueprint_activity_products (import_id, product_type_id, activity);

CREATE TABLE sde_blueprint_activity_skills (
  import_id uuid NOT NULL,
  blueprint_type_id bigint NOT NULL,
  activity text NOT NULL,
  skill_type_id bigint NOT NULL,
  level smallint NOT NULL CHECK (level BETWEEN 1 AND 5),
  position integer NOT NULL CHECK (position >= 0),
  PRIMARY KEY (import_id, blueprint_type_id, activity, skill_type_id),
  FOREIGN KEY (import_id, blueprint_type_id, activity)
    REFERENCES sde_blueprint_activities(import_id, blueprint_type_id, activity)
    ON DELETE CASCADE,
  FOREIGN KEY (import_id, skill_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE RESTRICT
);

-- maxProductionLimit: the most runs one job, and so one copy, can hold.
CREATE TABLE sde_blueprint_limits (
  import_id uuid NOT NULL,
  blueprint_type_id bigint NOT NULL,
  max_production_limit bigint NOT NULL CHECK (max_production_limit > 0),
  PRIMARY KEY (import_id, blueprint_type_id),
  FOREIGN KEY (import_id, blueprint_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE CASCADE
);
