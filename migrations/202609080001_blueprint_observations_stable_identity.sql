-- Blueprint observations: one durable row per physical blueprint.
--
-- Until now `complete_blueprints` appended a fresh row (new `id`,
-- `observed_at = now()`) for every owned blueprint on every ESI sync and
-- only flagged the previous batch `is_current = false` -- never deleting
-- anything. The table grew unbounded (~80 rows per blueprint on this dev
-- DB), and every sync re-minted the `id` a Build's
-- `planning_input.blueprintSelection.observationId` had frozen, so the
-- item inspector could no longer match the selection against the
-- `is_current`-filtered picker list.
--
-- New model: one row per `(workspace_id, owner_id, eve_item_id)`, upserted
-- in place each sync, with rows for no-longer-reported blueprints deleted
-- (companion change in `PgEsiRepository::complete_blueprints`). `id` is
-- then stable for the life of the blueprint, and `is_current` is redundant.
--
-- This migration collapses the accumulated history to the newest row per
-- blueprint, re-points every affected Build's saved selection at that
-- surviving row, swaps the uniqueness key, and drops `is_current`.

-- ---------------------------------------------------------------------------
-- 1. Survivor = newest row per blueprint (same ordering the app will now
--    treat as canonical).
-- ---------------------------------------------------------------------------
CREATE TEMP TABLE _bp_survivor ON COMMIT DROP AS
SELECT DISTINCT ON (workspace_id, owner_id, eve_item_id)
       id AS survivor_id,
       workspace_id,
       owner_id,
       eve_item_id
FROM blueprint_observations
ORDER BY workspace_id, owner_id, eve_item_id, observed_at DESC, imported_at DESC;

-- ---------------------------------------------------------------------------
-- 2. Re-point Builds whose `blueprintSelection.observationId` names a row
--    that is about to be deleted, so the selection keeps resolving (and now
--    matches the picker list). Bump `revision` on those Builds so a client
--    holding a stale `expectedRevision` reloads rather than overwriting.
-- ---------------------------------------------------------------------------
UPDATE builds b
SET revision = b.revision + 1,
    updated_at = now()
FROM build_draft_planning d
JOIN blueprint_observations stale
  ON stale.id::text = d.planning_input #>> '{blueprintSelection,observationId}'
JOIN _bp_survivor s
  ON s.workspace_id = stale.workspace_id
 AND s.owner_id     = stale.owner_id
 AND s.eve_item_id  = stale.eve_item_id
WHERE d.build_id = b.id
  AND d.planning_input #>> '{blueprintSelection,mode}' = 'observedAsset'
  AND s.survivor_id <> stale.id;

UPDATE build_draft_planning d
SET planning_input = jsonb_set(
      d.planning_input,
      '{blueprintSelection,observationId}',
      to_jsonb(s.survivor_id::text))
FROM blueprint_observations stale
JOIN _bp_survivor s
  ON s.workspace_id = stale.workspace_id
 AND s.owner_id     = stale.owner_id
 AND s.eve_item_id  = stale.eve_item_id
WHERE stale.id::text = d.planning_input #>> '{blueprintSelection,observationId}'
  AND d.planning_input #>> '{blueprintSelection,mode}' = 'observedAsset'
  AND s.survivor_id <> stale.id;

-- ---------------------------------------------------------------------------
-- 3. Collapse the history: keep only the survivor per blueprint.
-- ---------------------------------------------------------------------------
DELETE FROM blueprint_observations b
USING _bp_survivor s
WHERE b.workspace_id = s.workspace_id
  AND b.owner_id     = s.owner_id
  AND b.eve_item_id  = s.eve_item_id
  AND b.id <> s.survivor_id;

-- ---------------------------------------------------------------------------
-- 4. New identity: (workspace_id, owner_id, eve_item_id). Drop the old
--    (…, observed_at) unique constraint by column set so a name-truncation
--    difference can't break the migration.
-- ---------------------------------------------------------------------------
DO $$
DECLARE
  cname text;
BEGIN
  SELECT c.conname INTO cname
  FROM pg_constraint c
  WHERE c.conrelid = 'blueprint_observations'::regclass
    AND c.contype = 'u'
    AND (
      SELECT array_agg(a.attname::text ORDER BY a.attname::text)
      FROM pg_attribute a
      WHERE a.attrelid = c.conrelid AND a.attnum = ANY (c.conkey)
    ) = ARRAY['eve_item_id', 'observed_at', 'owner_id', 'workspace_id'];
  IF cname IS NULL THEN
    RAISE EXCEPTION 'expected a UNIQUE(workspace_id, owner_id, eve_item_id, observed_at) on blueprint_observations';
  END IF;
  EXECUTE format('ALTER TABLE blueprint_observations DROP CONSTRAINT %I', cname);
END $$;

ALTER TABLE blueprint_observations
  ADD CONSTRAINT blueprint_observations_identity_key
  UNIQUE (workspace_id, owner_id, eve_item_id);

-- ---------------------------------------------------------------------------
-- 5. `is_current` is now always true. The plain
--    `blueprint_observations_compatible_idx` (workspace_id, owner_id,
--    blueprint_type_id, observed_at DESC) from the original migration still
--    covers the picker query.
-- ---------------------------------------------------------------------------
DROP INDEX IF EXISTS blueprint_observations_current_compatible_idx;
ALTER TABLE blueprint_observations DROP COLUMN is_current;
