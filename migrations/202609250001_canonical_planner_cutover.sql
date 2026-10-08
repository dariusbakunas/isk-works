-- Canonical producers, slice 4: canonical dependency-graph planner cutover.
-- See docs/superpowers/specs/2026-09-22-production-dependency-read-model-design.md
-- ("Slice 4").
--
-- After this migration a root plan is calculated by exactly one planner,
-- chosen by persisted, explicit root authority -- never inferred from the
-- presence of production_dependencies rows, reconciliation decisions or the
-- absence of duplicates. Every existing root stays 'legacy'; a root becomes
-- 'canonical' only through the explicit, transactional cutover operation
-- (IndustryService::cutover_root_to_canonical_producers).

-- ---------------------------------------------------------------------------
-- Root planner authority
-- ---------------------------------------------------------------------------

ALTER TABLE builds
  ADD COLUMN planner_authority text NOT NULL DEFAULT 'legacy'
    CONSTRAINT builds_planner_authority_valid
    CHECK (planner_authority IN ('legacy', 'canonical'));

-- Only a root can be canonical-authoritative (a descendant producer belongs
-- to its root's plan and has no planner of its own).
ALTER TABLE builds
  ADD CONSTRAINT builds_planner_authority_on_roots CHECK (
    planner_authority = 'legacy'
    -- IS NOT DISTINCT FROM: a NULL plan root must fail the CHECK, not
    -- pass it as unknown.
    OR (parent_build_id IS NULL AND plan_root_build_id IS NOT DISTINCT FROM id)
  );

-- ---------------------------------------------------------------------------
-- Cutover audit + rollback evidence
-- ---------------------------------------------------------------------------

-- One row per cutover of one root. Holds everything needed to explain and
-- (during the rollback window) revert it: the plan state the readiness check
-- was run against, the reconciliation decisions that were applied, the edge
-- rows exactly as they were before the cutover, and every retarget.
CREATE TABLE canonical_planner_cutovers (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  plan_root_build_id uuid NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
  plan_state_token text NOT NULL,
  applied_decisions jsonb NOT NULL,
  legacy_edges jsonb NOT NULL,
  retargeted_edges jsonb NOT NULL,
  cut_over_at timestamptz NOT NULL,
  reverted_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now()
);

-- At most one live (non-reverted) cutover per root.
CREATE UNIQUE INDEX canonical_planner_cutovers_live_unique
  ON canonical_planner_cutovers (plan_root_build_id)
  WHERE reverted_at IS NULL;

-- ---------------------------------------------------------------------------
-- Retired (non-canonical duplicate) producers
-- ---------------------------------------------------------------------------

-- A non-canonical duplicate producer after cutover: kept (its Build row, its
-- legacy parent linkage and its own outgoing edges are rollback evidence),
-- but never referenced by a demand edge again and never a candidate when the
-- canonical write path resolves or creates the producer for a key.
CREATE TABLE retired_producers (
  producer_build_id uuid PRIMARY KEY REFERENCES builds(id) ON DELETE CASCADE,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  plan_root_build_id uuid NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
  -- The canonical producer that replaced it (audit only).
  canonical_producer_build_id uuid REFERENCES builds(id) ON DELETE SET NULL,
  cutover_id uuid NOT NULL REFERENCES canonical_planner_cutovers(id) ON DELETE CASCADE,
  retired_at timestamptz NOT NULL
);

CREATE INDEX retired_producers_plan_root_idx ON retired_producers (plan_root_build_id);

-- Defence in depth behind the write path: a demand edge never references a
-- retired producer, nor a producer of another root plan.
CREATE FUNCTION iskworks_check_production_dependency_producer() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.producer_build_id IS NOT NULL THEN
    IF EXISTS (SELECT 1 FROM retired_producers r WHERE r.producer_build_id = NEW.producer_build_id) THEN
      RAISE EXCEPTION 'production dependency % references retired producer %',
        NEW.id, NEW.producer_build_id
        USING ERRCODE = 'check_violation',
              CONSTRAINT = 'production_dependencies_producer_not_retired';
    END IF;
    IF NOT EXISTS (
      SELECT 1 FROM builds p
      WHERE p.id = NEW.producer_build_id
        AND p.plan_root_build_id IS NOT DISTINCT FROM NEW.plan_root_build_id
    ) THEN
      RAISE EXCEPTION 'production dependency % references producer % of another root plan',
        NEW.id, NEW.producer_build_id
        USING ERRCODE = 'check_violation',
              CONSTRAINT = 'production_dependencies_producer_same_plan';
    END IF;
  END IF;
  RETURN NEW;
END;
$$;

CREATE TRIGGER production_dependencies_producer_check
  BEFORE INSERT OR UPDATE OF producer_build_id, plan_root_build_id ON production_dependencies
  FOR EACH ROW EXECUTE FUNCTION iskworks_check_production_dependency_producer();

-- ---------------------------------------------------------------------------
-- Canonical plans are never re-derived from legacy parent links
-- ---------------------------------------------------------------------------

-- True when p_build belongs to a canonical-authoritative root plan.
CREATE FUNCTION iskworks_in_canonical_plan(p_build uuid) RETURNS boolean
LANGUAGE sql STABLE AS $$
  SELECT EXISTS (
    SELECT 1
    FROM builds b
    JOIN builds r ON r.id = b.plan_root_build_id
    WHERE b.id = p_build AND r.planner_authority = 'canonical'
  )
$$;

-- Same derivation as 202609230001, except that every Build of a
-- canonical-authoritative plan is skipped: there, production_dependencies
-- rows ARE the persisted sourcing and plan_root_build_id is assigned by the
-- canonical write path (a canonical producer may have no legacy parent).
CREATE OR REPLACE FUNCTION iskworks_sync_production_dependencies(p_builds uuid[]) RETURNS void
LANGUAGE plpgsql AS $$
DECLARE
  targets uuid[];
BEGIN
  SELECT coalesce(array_agg(DISTINCT id), '{}') INTO targets
  FROM (
    SELECT b.id FROM builds b WHERE b.id = ANY(p_builds)
    UNION
    SELECT b.parent_build_id FROM builds b
    WHERE b.id = ANY(p_builds) AND b.parent_build_id IS NOT NULL
  ) t(id)
  WHERE NOT iskworks_in_canonical_plan(t.id);

  UPDATE builds b
  SET plan_root_build_id = iskworks_plan_root_of(b.id)
  WHERE b.id = ANY(targets)
    AND b.plan_root_build_id IS DISTINCT FROM iskworks_plan_root_of(b.id);

  -- A legacy write may have just attached a Build to a canonical plan's
  -- legacy parent chain; never let that re-derive the canonical plan.
  SELECT coalesce(array_agg(t.id), '{}') INTO targets
  FROM unnest(targets) t(id)
  WHERE NOT iskworks_in_canonical_plan(t.id);

  DELETE FROM production_dependencies d
  WHERE d.consumer_build_id = ANY(targets)
    AND NOT EXISTS (
      SELECT 1
      FROM builds b
      JOIN build_recipe_materials m ON m.build_id = b.id
      WHERE b.id = d.consumer_build_id
        AND b.plan_root_build_id IS NOT NULL
        AND m.type_id = d.component_type_id
    );

  INSERT INTO production_dependencies AS d (
    workspace_id, plan_root_build_id, consumer_build_id, component_type_id,
    sourcing, method_kind, method_type_id, producer_build_id, fulfillment_scope
  )
  SELECT
    b.workspace_id,
    b.plan_root_build_id,
    b.id,
    m.type_id,
    CASE WHEN r.recipe IS NULL THEN 'buy' ELSE 'produce' END,
    r.recipe->>'mode',
    CASE r.recipe->>'mode'
      WHEN 'manufacturing' THEN (r.recipe->>'blueprintTypeId')::bigint
      WHEN 'reaction' THEN (r.recipe->>'reactionFormulaTypeId')::bigint
    END,
    CASE
      WHEN r.recipe IS NOT NULL
       AND child.recipe_kind = r.recipe->>'mode'
       AND CASE child.recipe_kind
             WHEN 'manufacturing' THEN child.blueprint_type_id
             ELSE child.reaction_formula_type_id
           END = CASE r.recipe->>'mode'
                   WHEN 'manufacturing' THEN (r.recipe->>'blueprintTypeId')::bigint
                   ELSE (r.recipe->>'reactionFormulaTypeId')::bigint
                 END
      THEN child.id
    END,
    CASE WHEN EXISTS (
      SELECT 1
      FROM jsonb_array_elements(coalesce(dp.planning_input->'fulfillmentScopes', '[]'::jsonb)) s(e)
      WHERE (s.e->>'typeId')::bigint = m.type_id AND s.e->>'scope' = 'full'
    ) THEN 'full' ELSE 'missing' END
  FROM builds b
  JOIN (
    SELECT DISTINCT build_id, type_id FROM build_recipe_materials WHERE build_id = ANY(targets)
  ) m ON m.build_id = b.id
  LEFT JOIN build_draft_planning dp ON dp.build_id = b.id
  LEFT JOIN LATERAL (
    SELECT x.e->'recipe' AS recipe
    FROM jsonb_array_elements(coalesce(dp.planning_input->'componentResolutions', '[]'::jsonb))
      WITH ORDINALITY x(e, ord)
    WHERE (x.e->>'typeId')::bigint = m.type_id
    ORDER BY x.ord DESC
    LIMIT 1
  ) r ON true
  LEFT JOIN builds child
    ON child.parent_build_id = b.id
   AND child.parent_component_type_id = m.type_id
   AND child.workspace_id = b.workspace_id
  WHERE b.id = ANY(targets) AND b.plan_root_build_id IS NOT NULL
  ON CONFLICT (consumer_build_id, component_type_id) DO UPDATE SET
    workspace_id = EXCLUDED.workspace_id,
    plan_root_build_id = EXCLUDED.plan_root_build_id,
    sourcing = EXCLUDED.sourcing,
    method_kind = EXCLUDED.method_kind,
    method_type_id = EXCLUDED.method_type_id,
    producer_build_id = EXCLUDED.producer_build_id,
    fulfillment_scope = EXCLUDED.fulfillment_scope,
    revision = d.revision + 1,
    updated_at = now()
  WHERE (d.workspace_id, d.plan_root_build_id, d.sourcing, d.method_kind,
         d.method_type_id, d.producer_build_id, d.fulfillment_scope)
    IS DISTINCT FROM
        (EXCLUDED.workspace_id, EXCLUDED.plan_root_build_id, EXCLUDED.sourcing,
         EXCLUDED.method_kind, EXCLUDED.method_type_id, EXCLUDED.producer_build_id,
         EXCLUDED.fulfillment_scope);
END;
$$;

-- Same repair as 202609230001, except canonical plans are left untouched: a
-- canonical producer created after cutover has no legacy parent, so the
-- legacy root derivation would otherwise make it a root of its own.
CREATE OR REPLACE FUNCTION iskworks_backfill_production_dependencies() RETURNS void
LANGUAGE plpgsql AS $$
BEGIN
  WITH RECURSIVE tree AS (
      SELECT id, workspace_id, id AS root FROM builds WHERE parent_build_id IS NULL
    UNION ALL
      SELECT child.id, child.workspace_id, tree.root
      FROM builds child
      JOIN tree ON child.parent_build_id = tree.id
      WHERE child.workspace_id = tree.workspace_id
  )
  CYCLE id SET is_cycle USING path
  UPDATE builds b
  SET plan_root_build_id = tree.root
  FROM tree
  WHERE b.id = tree.id AND NOT tree.is_cycle
    AND b.plan_root_build_id IS DISTINCT FROM tree.root
    AND NOT iskworks_in_canonical_plan(b.id);

  UPDATE builds b SET plan_root_build_id = NULL
  WHERE b.plan_root_build_id IS NOT NULL
    AND iskworks_plan_root_of(b.id) IS NULL
    AND NOT iskworks_in_canonical_plan(b.id);

  PERFORM iskworks_sync_production_dependencies(
    coalesce((SELECT array_agg(id ORDER BY id) FROM builds), '{}')
  );
END;
$$;
