-- Canonical producers, slice 2: persistent ProductionDependency edges and
-- root-plan identity. See
-- docs/superpowers/specs/2026-09-22-production-dependency-read-model-design.md.
--
-- TRANSITIONAL: the legacy parent-owned representation
-- (builds.parent_build_id / parent_component_type_id +
-- build_draft_planning.planning_input.componentResolutions/fulfillmentScopes)
-- stays authoritative for every planner read. The rows below are derived
-- from it by iskworks_sync_production_dependencies(), which the backfill,
-- every application write path (same transaction), and any post-rollback
-- repair all call -- one derivation, never two.

-- ---------------------------------------------------------------------------
-- Root-plan identity
-- ---------------------------------------------------------------------------

-- A root Build owns itself; every descendant producer owns its root. NULL
-- only for rows no root reaches (corrupt parent cycles) or rows written by
-- an older binary before the next repair; nothing reads it as authoritative
-- yet, so NULL is safe.
ALTER TABLE builds
  ADD COLUMN plan_root_build_id uuid REFERENCES builds(id) ON DELETE CASCADE;

-- Deliberately no CHECK tying plan_root_build_id to parent_build_id while
-- the legacy columns stay authoritative: the transitional column must never
-- make a write (or a data repair) the legacy schema accepts fail. It is kept
-- consistent by iskworks_sync_production_dependencies() instead; the
-- invariant becomes a constraint when parent_build_id is retired.

CREATE INDEX builds_plan_root_build_id_idx ON builds (plan_root_build_id);

-- ---------------------------------------------------------------------------
-- Demand edges
-- ---------------------------------------------------------------------------

-- One row per requirement (captured recipe material) of every rooted Build:
-- the consumer's Buy vs Produce decision, its fulfillment scope, and --
-- for Produce -- the producer Build satisfying it. Producer configuration
-- (facility, blueprint, ME/TE, descendant sourcing) is never copied here;
-- it stays on the producer Build.
CREATE TABLE production_dependencies (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  plan_root_build_id uuid NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
  consumer_build_id uuid NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
  component_type_id bigint NOT NULL CHECK (component_type_id > 0),
  sourcing text NOT NULL CHECK (sourcing IN ('buy', 'produce')),
  -- The consumer's requested method (Produce only).
  method_kind text CHECK (method_kind IN ('manufacturing', 'reaction')),
  method_type_id bigint CHECK (method_type_id > 0),
  -- NO ACTION (checked at statement end), not RESTRICT: a whole-plan
  -- cascade that removes both an edge and its producer in one statement is
  -- fine; deleting a producer while a surviving edge references it is not.
  producer_build_id uuid REFERENCES builds(id) ON DELETE NO ACTION,
  fulfillment_scope text NOT NULL DEFAULT 'missing'
    CHECK (fulfillment_scope IN ('missing', 'full')),
  -- Bumped whenever the derived row changes. Change tracking only:
  -- concurrency is enforced by the consumer Build's own revision, which
  -- every write that re-derives these rows has already checked.
  revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT production_dependencies_method_matches_sourcing CHECK (
    (sourcing = 'buy' AND method_kind IS NULL AND method_type_id IS NULL)
    OR (sourcing = 'produce' AND method_kind IS NOT NULL AND method_type_id IS NOT NULL)
  ),
  CONSTRAINT production_dependencies_producer_requires_produce CHECK (
    sourcing = 'produce' OR producer_build_id IS NULL
  ),
  CONSTRAINT production_dependencies_not_self_produced CHECK (
    producer_build_id IS DISTINCT FROM consumer_build_id
  )
);

CREATE UNIQUE INDEX production_dependencies_edge_unique
  ON production_dependencies (consumer_build_id, component_type_id);
CREATE INDEX production_dependencies_producer_idx
  ON production_dependencies (producer_build_id)
  WHERE producer_build_id IS NOT NULL;
CREATE INDEX production_dependencies_plan_root_idx
  ON production_dependencies (plan_root_build_id);

-- ---------------------------------------------------------------------------
-- Derivation from the legacy representation
-- ---------------------------------------------------------------------------

-- The root a Build belongs to, walking parent_build_id upward within one
-- workspace. NULL when the chain never reaches a root (a corrupt cycle).
CREATE FUNCTION iskworks_plan_root_of(p_build uuid) RETURNS uuid
LANGUAGE sql STABLE AS $$
  WITH RECURSIVE chain AS (
      SELECT id, parent_build_id, workspace_id FROM builds WHERE id = p_build
    UNION ALL
      SELECT parent.id, parent.parent_build_id, parent.workspace_id
      FROM builds parent
      JOIN chain ON parent.id = chain.parent_build_id
      WHERE parent.workspace_id = chain.workspace_id
  )
  CYCLE id SET is_cycle USING path
  SELECT id FROM chain WHERE parent_build_id IS NULL AND NOT is_cycle LIMIT 1
$$;

-- Re-derive plan_root_build_id and every outgoing demand edge of p_builds
-- **and of their legacy parents** (a Build's creation, recipe change, or
-- deletion changes its parent's edge for that slot) from the legacy
-- representation. Idempotent; a no-op when nothing changed (no revision
-- bump). Semantics mirror
-- iskworks_core::production_dependency::RootPlanDependencyGraph::from_legacy_parent_links
-- with one deliberate difference: a slot child whose recipe does not match
-- the consumer's requested method is NOT recorded as the edge's producer
-- (the legacy stale-recipe reuse bug is not carried into the new model).
CREATE FUNCTION iskworks_sync_production_dependencies(p_builds uuid[]) RETURNS void
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
  ) t(id);

  UPDATE builds b
  SET plan_root_build_id = iskworks_plan_root_of(b.id)
  WHERE b.id = ANY(targets)
    AND b.plan_root_build_id IS DISTINCT FROM iskworks_plan_root_of(b.id);

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
  -- Last resolution for the type wins, as in the Rust adapter's map.
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

-- Whole-database (re-)derivation: roots first (set-based, deterministic),
-- then every rooted Build's edges. Idempotent and safe to re-run -- the
-- documented repair after running an older binary against this schema.
CREATE FUNCTION iskworks_backfill_production_dependencies() RETURNS void
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
    AND b.plan_root_build_id IS DISTINCT FROM tree.root;

  -- Builds no root reaches (corrupt cycles) are not part of any plan.
  UPDATE builds b SET plan_root_build_id = NULL
  WHERE b.plan_root_build_id IS NOT NULL
    AND iskworks_plan_root_of(b.id) IS NULL;

  PERFORM iskworks_sync_production_dependencies(
    coalesce((SELECT array_agg(id ORDER BY id) FROM builds), '{}')
  );
END;
$$;

SELECT iskworks_backfill_production_dependencies();
