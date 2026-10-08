-- Legacy planner removal, final step
-- (docs/superpowers/specs/2026-10-04-legacy-planner-removal-design.md).
--
-- One planner remains: every Build belongs to a plan through
-- plan_root_build_id (a root owns itself) and sourcing lives on
-- production_dependencies. Drop the parent-linked representation, the
-- per-root planner authority, the SQL edge derivation, and the
-- reconciliation/cutover rollback state nothing reads any more.
--
-- Fails loudly, before changing anything, if any data still depends on the
-- legacy planner.

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM builds WHERE plan_root_build_id IS NULL) THEN
    RAISE EXCEPTION 'legacy planner removal: a Build has no plan root';
  END IF;
  IF EXISTS (SELECT 1 FROM builds WHERE parent_build_id IS NOT NULL) THEN
    RAISE EXCEPTION 'legacy planner removal: a Build still has a parent link';
  END IF;
  IF EXISTS (
    SELECT 1 FROM builds
    WHERE plan_root_build_id = id AND planner_authority <> 'canonical'
  ) THEN
    RAISE EXCEPTION 'legacy planner removal: a root plan is still on the legacy planner';
  END IF;
END;
$$;

DROP FUNCTION iskworks_sync_production_dependencies(uuid[]);
DROP FUNCTION iskworks_backfill_production_dependencies();
DROP FUNCTION iskworks_plan_root_of(uuid);
DROP FUNCTION iskworks_in_canonical_plan(uuid);

ALTER TABLE builds
  DROP CONSTRAINT builds_planner_authority_on_roots,
  DROP CONSTRAINT builds_parent_link_consistent;
DROP INDEX builds_parent_component_unique;
DROP INDEX builds_parent_build_id_idx;
ALTER TABLE builds
  DROP COLUMN parent_build_id,
  DROP COLUMN parent_component_type_id,
  DROP COLUMN planner_authority;

-- Cutovers stay as audit; their rollback state goes.
DROP INDEX canonical_planner_cutovers_live_unique;
ALTER TABLE canonical_planner_cutovers
  DROP COLUMN legacy_edges,
  DROP COLUMN reverted_at;

DROP TABLE producer_reconciliations;

ALTER TABLE builds ALTER COLUMN plan_root_build_id SET NOT NULL;
