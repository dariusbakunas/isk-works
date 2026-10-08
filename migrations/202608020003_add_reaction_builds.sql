-- Reaction Builds: a Build's recipe is either a manufacturing blueprint or a
-- reaction formula. Every Build that existed before reactions were modeled
-- was a manufacturing Build, so the default preserves current behavior
-- exactly -- same pattern as 202608020002_add_facility_profile_role.sql.
ALTER TABLE builds
  ADD COLUMN recipe_kind text NOT NULL DEFAULT 'manufacturing'
    CHECK (recipe_kind IN ('manufacturing', 'reaction')),
  ADD COLUMN reaction_formula_type_id bigint
    CHECK (reaction_formula_type_id > 0),
  ADD COLUMN reaction_formula_name text;

ALTER TABLE builds
  ALTER COLUMN recipe_kind DROP DEFAULT,
  ALTER COLUMN blueprint_type_id DROP NOT NULL,
  ALTER COLUMN blueprint_name DROP NOT NULL;

-- Exactly one identity pair is populated, and it matches recipe_kind.
ALTER TABLE builds
  ADD CONSTRAINT builds_recipe_identity_matches_kind CHECK (
    (recipe_kind = 'manufacturing'
       AND blueprint_type_id IS NOT NULL AND blueprint_name IS NOT NULL
       AND reaction_formula_type_id IS NULL AND reaction_formula_name IS NULL)
    OR (recipe_kind = 'reaction'
       AND reaction_formula_type_id IS NOT NULL AND reaction_formula_name IS NOT NULL
       AND blueprint_type_id IS NULL AND blueprint_name IS NULL)
  );

-- Reaction plans have no blueprint ME/TE at all. NULL, never 0 -- a 0 would
-- let callers silently treat a reaction line as ME-adjusted.
ALTER TABLE build_facility_snapshots
  ALTER COLUMN blueprint_me DROP NOT NULL,
  ALTER COLUMN blueprint_te DROP NOT NULL;

ALTER TABLE build_effective_material_requirements
  ALTER COLUMN blueprint_me DROP NOT NULL;

-- Which activity the captured facility profile was for; without this a
-- reloaded reaction plan would report a Manufacturing-role profile.
ALTER TABLE build_facility_snapshots
  ADD COLUMN captured_role text NOT NULL DEFAULT 'manufacturing'
    CHECK (captured_role IN ('manufacturing', 'reaction'));

ALTER TABLE build_facility_snapshots
  ALTER COLUMN captured_role DROP DEFAULT;
