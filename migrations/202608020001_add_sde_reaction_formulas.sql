CREATE TABLE sde_reaction_formulas (
  import_id uuid NOT NULL,
  reaction_formula_type_id bigint NOT NULL,
  name_en text NOT NULL,
  duration_seconds bigint NULL,
  PRIMARY KEY (import_id, reaction_formula_type_id),
  FOREIGN KEY (import_id, reaction_formula_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE CASCADE,
  CONSTRAINT sde_reaction_formulas_duration_positive
    CHECK (duration_seconds IS NULL OR duration_seconds > 0)
);

CREATE INDEX sde_reaction_formulas_name_idx
  ON sde_reaction_formulas (import_id, lower(name_en) text_pattern_ops);

CREATE TABLE sde_reaction_formula_materials (
  import_id uuid NOT NULL,
  reaction_formula_type_id bigint NOT NULL,
  material_type_id bigint NOT NULL,
  quantity bigint NOT NULL,
  position integer NOT NULL,
  PRIMARY KEY (import_id, reaction_formula_type_id, material_type_id),
  FOREIGN KEY (import_id, reaction_formula_type_id)
    REFERENCES sde_reaction_formulas(import_id, reaction_formula_type_id)
    ON DELETE CASCADE,
  FOREIGN KEY (import_id, material_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE RESTRICT,
  CONSTRAINT sde_reaction_formula_materials_quantity_positive CHECK (quantity > 0),
  CONSTRAINT sde_reaction_formula_materials_position_non_negative CHECK (position >= 0)
);

CREATE TABLE sde_reaction_formula_products (
  import_id uuid NOT NULL,
  reaction_formula_type_id bigint NOT NULL,
  product_type_id bigint NOT NULL,
  quantity bigint NOT NULL,
  position integer NOT NULL,
  PRIMARY KEY (import_id, reaction_formula_type_id, product_type_id),
  FOREIGN KEY (import_id, reaction_formula_type_id)
    REFERENCES sde_reaction_formulas(import_id, reaction_formula_type_id)
    ON DELETE CASCADE,
  FOREIGN KEY (import_id, product_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE RESTRICT,
  CONSTRAINT sde_reaction_formula_products_quantity_positive CHECK (quantity > 0),
  CONSTRAINT sde_reaction_formula_products_position_non_negative CHECK (position >= 0)
);

CREATE INDEX sde_reaction_formula_products_product_idx
  ON sde_reaction_formula_products (import_id, product_type_id);

-- No structure-level reaction modifiers table: verified against real SDE
-- data that Refinery structures (Athanor/Tatara) carry no base
-- material/time/job-cost reaction discount at all -- the entire reaction
-- discount comes from reaction rigs. See sde_reaction_rig_modifiers below.
CREATE TABLE sde_reaction_rig_modifiers (
  import_id uuid NOT NULL REFERENCES sde_imports(id) ON DELETE CASCADE,
  type_id bigint NOT NULL,
  material_reduction_percent numeric NOT NULL,
  time_reduction_percent numeric NOT NULL,
  high_sec_multiplier numeric NOT NULL,
  low_sec_multiplier numeric NOT NULL,
  null_sec_multiplier numeric NOT NULL,
  compatible_structure_group_ids bigint[] NOT NULL DEFAULT '{}',
  rig_size bigint NULL,
  PRIMARY KEY (import_id, type_id)
);

ALTER TABLE sde_imports
  ADD COLUMN reaction_formula_count bigint NOT NULL DEFAULT 0,
  ADD COLUMN reaction_material_line_count bigint NOT NULL DEFAULT 0,
  ADD COLUMN reaction_product_line_count bigint NOT NULL DEFAULT 0,
  ADD COLUMN skipped_reaction_formula_count bigint NOT NULL DEFAULT 0,
  ADD CONSTRAINT sde_imports_reaction_counts_non_negative CHECK (
    reaction_formula_count >= 0
    AND reaction_material_line_count >= 0
    AND reaction_product_line_count >= 0
    AND skipped_reaction_formula_count >= 0
  );
