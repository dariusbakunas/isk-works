-- EVE's `industryModifierSources` lists a SEPARATE entry per covered
-- `filterID` for any rig whose bonus reaches more than one target-filter
-- scope (e.g. "Standup L-Set Reactor Efficiency I", type 46496, carries
-- reaction filters 16 + 17 + 18 = Hybrid + Biochemical + Composite = every
-- reaction). The importer previously kept only the first id, so such a rig
-- was modelled as covering a single sub-class and wrongly reported
-- non-applicable (and silently dropped its bonus) for the others.
--
-- Widen the resolved SDE representation from one filter id per activity to
-- the full set. Applicability stays "the produced item's category or group
-- is listed by ANY of the rig's filters" -- the union of the referenced
-- filters' `(category_ids, group_ids)`. Material and time are still resolved
-- independently. An empty set keeps the pre-feature meaning: no filter data
-- in the SDE, treated as unrestricted.

ALTER TABLE sde_structure_rig_modifiers
  ADD COLUMN material_filter_ids bigint[] NOT NULL DEFAULT '{}',
  ADD COLUMN time_filter_ids bigint[] NOT NULL DEFAULT '{}';

UPDATE sde_structure_rig_modifiers SET
  material_filter_ids = CASE WHEN material_filter_id IS NULL
                             THEN '{}'::bigint[] ELSE ARRAY[material_filter_id] END,
  time_filter_ids     = CASE WHEN time_filter_id IS NULL
                             THEN '{}'::bigint[] ELSE ARRAY[time_filter_id] END;

ALTER TABLE sde_structure_rig_modifiers
  DROP COLUMN material_filter_id,
  DROP COLUMN time_filter_id;

ALTER TABLE sde_reaction_rig_modifiers
  ADD COLUMN material_filter_ids bigint[] NOT NULL DEFAULT '{}',
  ADD COLUMN time_filter_ids bigint[] NOT NULL DEFAULT '{}';

UPDATE sde_reaction_rig_modifiers SET
  material_filter_ids = CASE WHEN material_filter_id IS NULL
                             THEN '{}'::bigint[] ELSE ARRAY[material_filter_id] END,
  time_filter_ids     = CASE WHEN time_filter_id IS NULL
                             THEN '{}'::bigint[] ELSE ARRAY[time_filter_id] END;

ALTER TABLE sde_reaction_rig_modifiers
  DROP COLUMN material_filter_id,
  DROP COLUMN time_filter_id;

-- Existing persisted facility-profile rigs may hold the pre-fix,
-- single-filter resolution baked into their `applicability` JSON (resolved
-- once at save time, immune to SDE re-imports by design). Reset every
-- already-resolved row to the unresolved sentinel so
-- `facility::load_profile`'s `rig_select_query` re-heals it from the
-- corrected SDE data on the next read. Rows already at the sentinel are
-- untouched; no other facility-profile state is affected.
--
-- NOTE: this only produces the corrected union once a post-fix SDE has been
-- re-imported. Until then healing yields the union of the (still
-- single-element) migrated arrays -- identical to the previous behaviour, so
-- there is no intermediate regression. See the migration note in
-- docs/superpowers/plans for the required deploy sequence
-- (migrate -> re-import SDE -> profiles self-heal on read).
UPDATE industry_facility_profile_rigs
  SET applicability = '{"material":"unrestricted","time":"unrestricted"}'::jsonb
  WHERE applicability IS DISTINCT FROM '{"material":"unrestricted","time":"unrestricted"}'::jsonb;
