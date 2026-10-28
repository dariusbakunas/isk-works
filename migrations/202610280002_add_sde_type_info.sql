-- What the Type Info window shows: descriptions and physical properties
-- (types.jsonl), published dogma attributes with their units and per-type
-- values (dogmaAttributes / dogmaUnits / dogmaAttributeCategories /
-- typeDogma), fixed reprocessing outputs (typeMaterials) and ship bonuses
-- (typeBonus). Text keeps CCP's markup; the API turns it into segments.

-- One row per imported type that has any of these fields.
CREATE TABLE sde_type_details (
  import_id uuid NOT NULL,
  type_id bigint NOT NULL,
  description_en text NULL,
  -- The assembled volume; sde_types.packaged_volume_m3 is the packaged one.
  volume_m3 numeric NULL CHECK (volume_m3 IS NULL OR volume_m3 >= 0),
  mass_kg numeric NULL CHECK (mass_kg IS NULL OR mass_kg >= 0),
  capacity_m3 numeric NULL CHECK (capacity_m3 IS NULL OR capacity_m3 >= 0),
  portion_size integer NULL CHECK (portion_size IS NULL OR portion_size > 0),
  -- No FK: the parent (e.g. a T1 hull) may not be imported.
  variation_parent_type_id bigint NULL,
  PRIMARY KEY (import_id, type_id),
  FOREIGN KEY (import_id, type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE CASCADE
);

CREATE INDEX sde_type_details_variation_parent_idx
  ON sde_type_details (import_id, variation_parent_type_id)
  WHERE variation_parent_type_id IS NOT NULL;

CREATE TABLE sde_dogma_units (
  import_id uuid NOT NULL REFERENCES sde_imports(id) ON DELETE CASCADE,
  unit_id bigint NOT NULL,
  name text NOT NULL,
  display_name text NULL,
  PRIMARY KEY (import_id, unit_id)
);

-- Published attributes with an English display name only.
CREATE TABLE sde_dogma_attributes (
  import_id uuid NOT NULL REFERENCES sde_imports(id) ON DELETE CASCADE,
  attribute_id bigint NOT NULL,
  name text NOT NULL,
  display_name_en text NOT NULL,
  unit_id bigint NULL,
  category_id bigint NULL,
  category_name text NULL,
  display_when_zero boolean NOT NULL,
  high_is_good boolean NOT NULL,
  PRIMARY KEY (import_id, attribute_id),
  FOREIGN KEY (import_id, unit_id)
    REFERENCES sde_dogma_units(import_id, unit_id)
);

CREATE TABLE sde_type_attributes (
  import_id uuid NOT NULL,
  type_id bigint NOT NULL,
  attribute_id bigint NOT NULL,
  value numeric NOT NULL,
  PRIMARY KEY (import_id, type_id, attribute_id),
  FOREIGN KEY (import_id, type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE CASCADE,
  FOREIGN KEY (import_id, attribute_id)
    REFERENCES sde_dogma_attributes(import_id, attribute_id)
    ON DELETE CASCADE
);

-- "Used with": modules whose charge groups (or charges whose launcher
-- group) hold a given value.
CREATE INDEX sde_type_attributes_value_idx
  ON sde_type_attributes (import_id, attribute_id, value);

CREATE TABLE sde_type_materials (
  import_id uuid NOT NULL,
  type_id bigint NOT NULL,
  material_type_id bigint NOT NULL,
  quantity bigint NOT NULL CHECK (quantity > 0),
  PRIMARY KEY (import_id, type_id, material_type_id),
  FOREIGN KEY (import_id, type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE CASCADE,
  FOREIGN KEY (import_id, material_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE CASCADE
);

CREATE INDEX sde_type_materials_material_idx
  ON sde_type_materials (import_id, material_type_id);

-- ordinal runs across a type's skill, then role, then misc bonuses.
CREATE TABLE sde_type_bonuses (
  import_id uuid NOT NULL,
  type_id bigint NOT NULL,
  ordinal integer NOT NULL CHECK (ordinal >= 1),
  source_kind text NOT NULL CHECK (source_kind IN ('skill', 'role', 'misc')),
  -- No FK: a bonus skill may be unpublished and not imported.
  skill_type_id bigint NULL,
  bonus numeric NULL,
  unit_id bigint NULL,
  text_en text NOT NULL,
  importance integer NULL,
  is_positive boolean NULL,
  PRIMARY KEY (import_id, type_id, ordinal),
  FOREIGN KEY (import_id, type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE CASCADE,
  FOREIGN KEY (import_id, unit_id)
    REFERENCES sde_dogma_units(import_id, unit_id),
  CONSTRAINT sde_type_bonuses_skill_source CHECK (
    (source_kind = 'skill') = (skill_type_id IS NOT NULL)
  )
);
