-- The planner, the Blueprint library, the Invention page and Type Info now
-- read recipes from sde_blueprint_activities* alone. Imports made before
-- those tables existed (or before they kept unpublished blueprints, which
-- players can hold copies of) have recipes only in the old per-activity
-- tables, so copy across whatever is missing. Rows an import already has
-- are left alone. The old tables are dropped in a later migration.

INSERT INTO sde_blueprint_activities (import_id, blueprint_type_id, activity, duration_seconds)
SELECT import_id, blueprint_type_id, 'manufacturing', duration_seconds FROM sde_blueprints
UNION ALL
SELECT import_id, reaction_formula_type_id, 'reaction', duration_seconds FROM sde_reaction_formulas
UNION ALL
SELECT import_id, blueprint_type_id, 'invention', duration_seconds FROM sde_blueprint_invention
UNION ALL
SELECT import_id, blueprint_type_id, 'copying', duration_seconds FROM sde_blueprint_copying
ON CONFLICT DO NOTHING;

-- Lines are copied only for an activity that has none yet, so an activity
-- the import already wrote is never mixed with old rows.
INSERT INTO sde_blueprint_activity_materials
  (import_id, blueprint_type_id, activity, material_type_id, quantity, position)
SELECT line.import_id, line.blueprint_type_id, line.activity, line.material_type_id,
       line.quantity, line.position
FROM (
  SELECT import_id, blueprint_type_id, 'manufacturing' AS activity, material_type_id,
         quantity, position
  FROM sde_blueprint_materials
  UNION ALL
  SELECT import_id, reaction_formula_type_id, 'reaction', material_type_id, quantity, position
  FROM sde_reaction_formula_materials
  UNION ALL
  SELECT import_id, blueprint_type_id, 'invention', material_type_id, quantity, position
  FROM sde_blueprint_invention_materials
) line
WHERE NOT EXISTS (
  SELECT 1 FROM sde_blueprint_activity_materials existing
  WHERE existing.import_id = line.import_id
    AND existing.blueprint_type_id = line.blueprint_type_id
    AND existing.activity = line.activity
);

INSERT INTO sde_blueprint_activity_products
  (import_id, blueprint_type_id, activity, product_type_id, quantity, probability, position)
SELECT line.import_id, line.blueprint_type_id, line.activity, line.product_type_id,
       line.quantity, line.probability, line.position
FROM (
  SELECT import_id, blueprint_type_id, 'manufacturing' AS activity, product_type_id,
         quantity, NULL::numeric AS probability, position
  FROM sde_blueprint_products
  UNION ALL
  SELECT import_id, reaction_formula_type_id, 'reaction', product_type_id, quantity,
         NULL::numeric, position
  FROM sde_reaction_formula_products
  UNION ALL
  SELECT import_id, blueprint_type_id, 'invention', product_blueprint_type_id, base_runs,
         probability, position
  FROM sde_blueprint_invention_products
) line
WHERE NOT EXISTS (
  SELECT 1 FROM sde_blueprint_activity_products existing
  WHERE existing.import_id = line.import_id
    AND existing.blueprint_type_id = line.blueprint_type_id
    AND existing.activity = line.activity
);

INSERT INTO sde_blueprint_activity_skills
  (import_id, blueprint_type_id, activity, skill_type_id, level, position)
SELECT line.import_id, line.blueprint_type_id, 'invention', line.skill_type_id, line.level,
       (row_number() OVER (
          PARTITION BY line.import_id, line.blueprint_type_id ORDER BY line.skill_type_id
        ) - 1)::integer
FROM sde_blueprint_invention_skills line
WHERE NOT EXISTS (
  SELECT 1 FROM sde_blueprint_activity_skills existing
  WHERE existing.import_id = line.import_id
    AND existing.blueprint_type_id = line.blueprint_type_id
    AND existing.activity = 'invention'
);

-- "What is this used in?": recipes by the material they consume.
CREATE INDEX sde_blueprint_activity_materials_material_idx
  ON sde_blueprint_activity_materials (import_id, material_type_id, activity);
