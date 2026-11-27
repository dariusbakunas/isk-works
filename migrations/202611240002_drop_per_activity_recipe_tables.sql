-- Every recipe now lives in sde_blueprint_activities and its line tables
-- (202611240001 copied what older imports only had here). Nothing reads or
-- writes these any more. sde_decryptors stays: it is not a recipe.
DROP TABLE sde_blueprint_materials;
DROP TABLE sde_blueprint_products;
DROP TABLE sde_blueprints;

DROP TABLE sde_reaction_formula_materials;
DROP TABLE sde_reaction_formula_products;
DROP TABLE sde_reaction_formulas;

DROP TABLE sde_blueprint_invention_materials;
DROP TABLE sde_blueprint_invention_products;
DROP TABLE sde_blueprint_invention_skills;
DROP TABLE sde_blueprint_invention;

DROP TABLE sde_blueprint_copying;
