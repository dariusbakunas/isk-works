-- Invention data from the SDE's blueprints.jsonl: the `invention` activity
-- on a Tech I blueprint (what an attempt consumes, the skills it needs, and
-- the Tech II blueprint copies it can yield with their base chance), the
-- `copying` time, and the decryptors that change an attempt's outcome. The
-- Invention page builds its quotes from these.
--
-- The existing sde_blueprint* tables keep meaning "manufacturing"; a source
-- blueprint here need not have a manufacturing row, so these reference
-- sde_types.
CREATE TABLE sde_blueprint_invention (
  import_id uuid NOT NULL,
  blueprint_type_id bigint NOT NULL,
  duration_seconds bigint,
  PRIMARY KEY (import_id, blueprint_type_id),
  FOREIGN KEY (import_id, blueprint_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE CASCADE,
  CONSTRAINT sde_blueprint_invention_duration_positive
    CHECK (duration_seconds IS NULL OR duration_seconds > 0)
);

CREATE TABLE sde_blueprint_invention_materials (
  import_id uuid NOT NULL,
  blueprint_type_id bigint NOT NULL,
  material_type_id bigint NOT NULL,
  quantity bigint NOT NULL,
  position integer NOT NULL,
  PRIMARY KEY (import_id, blueprint_type_id, material_type_id),
  FOREIGN KEY (import_id, blueprint_type_id)
    REFERENCES sde_blueprint_invention(import_id, blueprint_type_id)
    ON DELETE CASCADE,
  FOREIGN KEY (import_id, material_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE RESTRICT,
  CONSTRAINT sde_blueprint_invention_materials_quantity_positive CHECK (quantity > 0),
  CONSTRAINT sde_blueprint_invention_materials_position_non_negative CHECK (position >= 0)
);

-- One row per blueprint copy the source can yield; the player picks one per
-- job. base_runs is the runs on the invented copy before any decryptor.
CREATE TABLE sde_blueprint_invention_products (
  import_id uuid NOT NULL,
  blueprint_type_id bigint NOT NULL,
  product_blueprint_type_id bigint NOT NULL,
  base_runs bigint NOT NULL,
  probability numeric NOT NULL,
  position integer NOT NULL,
  PRIMARY KEY (import_id, blueprint_type_id, product_blueprint_type_id),
  FOREIGN KEY (import_id, blueprint_type_id)
    REFERENCES sde_blueprint_invention(import_id, blueprint_type_id)
    ON DELETE CASCADE,
  FOREIGN KEY (import_id, product_blueprint_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE RESTRICT,
  CONSTRAINT sde_blueprint_invention_products_runs_positive CHECK (base_runs > 0),
  CONSTRAINT sde_blueprint_invention_products_probability_range
    CHECK (probability > 0 AND probability <= 1),
  CONSTRAINT sde_blueprint_invention_products_position_non_negative CHECK (position >= 0)
);

-- Quotes look a recipe up from the blueprint being invented.
CREATE INDEX sde_blueprint_invention_products_product_idx
  ON sde_blueprint_invention_products (import_id, product_blueprint_type_id);

CREATE TABLE sde_blueprint_invention_skills (
  import_id uuid NOT NULL,
  blueprint_type_id bigint NOT NULL,
  skill_type_id bigint NOT NULL,
  level smallint NOT NULL CHECK (level BETWEEN 1 AND 5),
  PRIMARY KEY (import_id, blueprint_type_id, skill_type_id),
  FOREIGN KEY (import_id, blueprint_type_id)
    REFERENCES sde_blueprint_invention(import_id, blueprint_type_id)
    ON DELETE CASCADE,
  FOREIGN KEY (import_id, skill_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE RESTRICT
);

CREATE TABLE sde_blueprint_copying (
  import_id uuid NOT NULL,
  blueprint_type_id bigint NOT NULL,
  duration_seconds bigint NOT NULL CHECK (duration_seconds > 0),
  PRIMARY KEY (import_id, blueprint_type_id),
  FOREIGN KEY (import_id, blueprint_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE CASCADE
);

-- Group 1304 (Generic Decryptor) only. Other decryptor groups carry the
-- same dogma attributes with values the game does not use.
CREATE TABLE sde_decryptors (
  import_id uuid NOT NULL,
  type_id bigint NOT NULL,
  probability_multiplier numeric NOT NULL CHECK (probability_multiplier > 0),
  me_modifier integer NOT NULL,
  te_modifier integer NOT NULL,
  run_modifier integer NOT NULL,
  PRIMARY KEY (import_id, type_id),
  FOREIGN KEY (import_id, type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE CASCADE
);
