-- "What is this used in?": recipes by the material they consume, for the
-- Type Info window's used-in count and list.
CREATE INDEX sde_blueprint_materials_material_idx
  ON sde_blueprint_materials (import_id, material_type_id);

CREATE INDEX sde_reaction_formula_materials_material_idx
  ON sde_reaction_formula_materials (import_id, material_type_id);
