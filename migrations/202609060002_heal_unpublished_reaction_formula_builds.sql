-- Heal linked Reaction Builds whose captured recipe points at an *unpublished*
-- reaction formula.
--
-- The SDE ships unpublished CCP test/debug reaction formulas that can share a
-- product type with the genuine formula and carry a nonsense output-per-run
-- (e.g. "Test Reaction Blueprint" 45732: 100 Rolled Tungsten Alloy + 100
-- Sulfuric Acid -> 20 Tungsten Carbide, vs the genuine 46207: + 5 Nitrogen
-- Fuel Block -> 10,000 Tungsten Carbide). Before the companion code fix,
-- `reaction_formula_for_product` had no `published` filter, so "Switch to
-- BUILD" on such a product resolved to the decoy and captured it onto the
-- linked Build -- producing wildly undersized linked builds (`making` far
-- below `required`) and, through the bad material list, cost explosions.
--
-- The code fix stops new bad captures but cannot repair Builds already
-- persisted with the decoy recipe. This migration re-points every affected
-- reaction Build (and every parent's `componentResolutions` entry that names
-- the decoy) at the genuine published formula for the same product, in the
-- currently active SDE import, and rewrites the captured recipe lines,
-- scalar recipe columns, fingerprint and -- for a linked child -- its run
-- count. It is a precise no-op on any database where nothing matches.

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ---------------------------------------------------------------------------
-- 1. The set of Builds to heal, resolved against the active SDE import.
-- ---------------------------------------------------------------------------
CREATE TEMP TABLE _heal_reaction_builds ON COMMIT DROP AS
WITH active AS (
    SELECT id AS import_id, source_version
    FROM sde_imports
    WHERE active
),
-- The genuine, published formula for each product: the lowest published
-- reaction-formula type id producing it, exactly the tie-break the fixed
-- `reaction_formula_for_product` query applies.
published_formula AS (
    SELECT p.product_type_id,
           min(p.reaction_formula_type_id) AS good_formula_type_id
    FROM active a
    JOIN sde_reaction_formula_products p ON p.import_id = a.import_id
    JOIN sde_types ft
      ON ft.import_id = a.import_id
     AND ft.type_id = p.reaction_formula_type_id
     AND ft.published = true
    GROUP BY p.product_type_id
)
SELECT b.id AS build_id,
       b.reaction_formula_type_id AS bad_formula_type_id,
       pf.good_formula_type_id,
       gf.name_en AS good_formula_name,
       gf.duration_seconds AS good_duration_seconds,
       gp.quantity AS good_output_per_run,
       a.import_id AS active_import_id,
       a.source_version AS active_source_version,
       b.parent_build_id,
       b.parent_component_type_id
FROM builds b
JOIN active a ON true
JOIN published_formula pf ON pf.product_type_id = b.product_type_id
JOIN sde_reaction_formulas gf
  ON gf.import_id = a.import_id
 AND gf.reaction_formula_type_id = pf.good_formula_type_id
JOIN sde_reaction_formula_products gp
  ON gp.import_id = a.import_id
 AND gp.reaction_formula_type_id = pf.good_formula_type_id
 AND gp.product_type_id = b.product_type_id
WHERE b.recipe_kind = 'reaction'
  AND b.reaction_formula_type_id IS DISTINCT FROM pf.good_formula_type_id
  -- Only heal a Build whose *current* formula is genuinely not a published
  -- formula in the active import (an unpublished decoy, or one dropped from
  -- the SDE entirely). A Build already on a different but valid published
  -- formula is left untouched.
  AND NOT EXISTS (
      SELECT 1
      FROM sde_types bft
      WHERE bft.import_id = a.import_id
        AND bft.type_id = b.reaction_formula_type_id
        AND bft.published = true
  );

-- ---------------------------------------------------------------------------
-- 2. Rewrite the captured recipe line tables from the genuine formula.
-- ---------------------------------------------------------------------------
DELETE FROM build_recipe_materials m
USING _heal_reaction_builds h
WHERE m.build_id = h.build_id;

INSERT INTO build_recipe_materials (build_id, type_id, captured_name, quantity_per_run, sort_order)
SELECT h.build_id,
       s.material_type_id,
       t.name_en,
       s.quantity,
       (row_number() OVER (PARTITION BY h.build_id
                           ORDER BY s.position, s.material_type_id) - 1)::int
FROM _heal_reaction_builds h
JOIN sde_reaction_formula_materials s
  ON s.import_id = h.active_import_id
 AND s.reaction_formula_type_id = h.good_formula_type_id
JOIN sde_types t
  ON t.import_id = h.active_import_id
 AND t.type_id = s.material_type_id;

DELETE FROM build_recipe_products p
USING _heal_reaction_builds h
WHERE p.build_id = h.build_id;

INSERT INTO build_recipe_products (build_id, type_id, captured_name, quantity_per_run, sort_order)
SELECT h.build_id,
       s.product_type_id,
       t.name_en,
       s.quantity,
       (row_number() OVER (PARTITION BY h.build_id
                           ORDER BY s.position, s.product_type_id) - 1)::int
FROM _heal_reaction_builds h
JOIN sde_reaction_formula_products s
  ON s.import_id = h.active_import_id
 AND s.reaction_formula_type_id = h.good_formula_type_id
JOIN sde_types t
  ON t.import_id = h.active_import_id
 AND t.type_id = s.product_type_id;

-- ---------------------------------------------------------------------------
-- 3. Re-point the Build's identity + scalar recipe columns.
-- ---------------------------------------------------------------------------
UPDATE builds b
SET reaction_formula_type_id = h.good_formula_type_id,
    reaction_formula_name = h.good_formula_name,
    duration_seconds_per_run = h.good_duration_seconds,
    product_quantity_per_run = h.good_output_per_run,
    source_sde_dataset_id = h.active_import_id,
    source_sde_version = h.active_source_version,
    revision = b.revision + 1,
    updated_at = now()
FROM _heal_reaction_builds h
WHERE b.id = h.build_id;

-- ---------------------------------------------------------------------------
-- 4. Resize a healed linked child to its parent's requirement.
--
-- The parent's exact ME/rig-adjusted requirement isn't reconstructable in
-- SQL, so size against the parent's *pre-reduction* base demand
-- (`parent runs * parent recipe qty/run`). That is an upper bound on the
-- real requirement, so `making >= required` holds; the app's
-- `resync_linked_descendants` tightens `runs` to the exact minimal figure on
-- the parent's next Draft save. Root reaction Builds (no parent) keep their
-- user-chosen `runs` -- the recipe fix alone corrects their `making`.
-- ---------------------------------------------------------------------------
UPDATE builds child
SET runs = LEAST(
        1000000,
        GREATEST(
            1,
            ceil(
                (parent.runs * pm.quantity_per_run)::numeric
                / NULLIF(h.good_output_per_run, 0)::numeric
            )::bigint
        )
    ),
    revision = child.revision + 1,
    updated_at = now()
FROM _heal_reaction_builds h
JOIN builds parent ON parent.id = h.parent_build_id
JOIN build_recipe_materials pm
  ON pm.build_id = parent.id
 AND pm.type_id = h.parent_component_type_id
WHERE child.id = h.build_id
  AND h.parent_build_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- 5. Recompute the recipe fingerprint, byte-for-byte matching
--    `CapturedReactionFormula::calculate_fingerprint` (SHA-256 of, in order:
--    the i64 formula type id BE, the u64 duration-per-run BE, the byte 'm'
--    then each material line's (i64 type id BE, u64 qty/run BE, u32 sort BE),
--    the byte 'p' then each product line the same way).
-- ---------------------------------------------------------------------------
UPDATE builds b
SET recipe_fingerprint = encode(
    digest(
        int8send(b.reaction_formula_type_id)
        || int8send(coalesce(b.duration_seconds_per_run, 0))
        || '\x6d'::bytea
        || coalesce(
            (SELECT string_agg(
                        int8send(m.type_id)
                        || int8send(m.quantity_per_run)
                        || substring(int8send(m.sort_order::bigint) FROM 5 FOR 4),
                        ''::bytea ORDER BY m.sort_order)
             FROM build_recipe_materials m
             WHERE m.build_id = b.id),
            ''::bytea)
        || '\x70'::bytea
        || coalesce(
            (SELECT string_agg(
                        int8send(p.type_id)
                        || int8send(p.quantity_per_run)
                        || substring(int8send(p.sort_order::bigint) FROM 5 FOR 4),
                        ''::bytea ORDER BY p.sort_order)
             FROM build_recipe_products p
             WHERE p.build_id = b.id),
            ''::bytea),
        'sha256'
    ),
    'hex'
)
FROM _heal_reaction_builds h
WHERE b.id = h.build_id;

-- ---------------------------------------------------------------------------
-- 6. Swap the decoy formula id out of every parent's stored
--    `componentResolutions` (the persisted Build/Buy intent that
--    `create_or_reuse_linked_build` / `resync_linked_descendants` read).
-- ---------------------------------------------------------------------------
UPDATE build_draft_planning d
SET planning_input = jsonb_set(
        d.planning_input,
        '{componentResolutions}',
        (
            SELECT coalesce(jsonb_agg(
                CASE
                    WHEN elem #>> '{recipe,mode}' = 'reaction'
                     AND (elem #>> '{recipe,reactionFormulaTypeId}') IS NOT NULL
                     AND EXISTS (
                         SELECT 1 FROM _heal_reaction_builds h
                         WHERE h.bad_formula_type_id
                               = (elem #>> '{recipe,reactionFormulaTypeId}')::bigint
                     )
                    THEN jsonb_set(
                        elem,
                        '{recipe,reactionFormulaTypeId}',
                        to_jsonb((
                            SELECT h.good_formula_type_id FROM _heal_reaction_builds h
                            WHERE h.bad_formula_type_id
                                  = (elem #>> '{recipe,reactionFormulaTypeId}')::bigint
                            LIMIT 1
                        ))
                    )
                    ELSE elem
                END
            ), '[]'::jsonb)
            FROM jsonb_array_elements(d.planning_input -> 'componentResolutions') elem
        )
    )
WHERE jsonb_typeof(d.planning_input -> 'componentResolutions') = 'array'
  AND EXISTS (
      SELECT 1
      FROM jsonb_array_elements(d.planning_input -> 'componentResolutions') elem
      JOIN _heal_reaction_builds h
        ON h.bad_formula_type_id = (elem #>> '{recipe,reactionFormulaTypeId}')::bigint
      WHERE elem #>> '{recipe,mode}' = 'reaction'
  );

-- Bump the parent Build revision wherever its resolutions were rewritten, so
-- a client holding a stale `expected_revision` reloads rather than silently
-- overwriting the heal on its next save.
UPDATE builds b
SET revision = b.revision + 1,
    updated_at = now()
FROM build_draft_planning d
WHERE d.build_id = b.id
  AND jsonb_typeof(d.planning_input -> 'componentResolutions') = 'array'
  AND EXISTS (
      SELECT 1
      FROM _heal_reaction_builds h
      WHERE h.parent_build_id = b.id
  );
