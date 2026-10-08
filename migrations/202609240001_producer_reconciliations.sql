-- Canonical producers, slice 3: explicit producer reconciliation decisions.
-- See docs/superpowers/specs/2026-09-22-production-dependency-read-model-design.md
-- ("Slice 3").
--
-- One row = one explicit, user-made decision: "within this root plan, the
-- canonical producer for (output type, production method) is this Build,
-- with exactly this recursive configuration". It is DECISION STATE ONLY:
-- no planner, no edge derivation and no write path reads it yet. Legacy
-- duplicate producer Builds and their parent links are untouched until the
-- canonical-planner cutover applies the decisions.

CREATE TABLE producer_reconciliations (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  plan_root_build_id uuid NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
  -- CanonicalProducerKey
  output_type_id bigint NOT NULL CHECK (output_type_id > 0),
  method_kind text NOT NULL CHECK (method_kind IN ('manufacturing', 'reaction')),
  method_type_id bigint NOT NULL CHECK (method_type_id > 0),
  -- Deleting the chosen producer deletes the decision: the group is then
  -- simply unresolved again, never silently pointing at nothing.
  canonical_producer_build_id uuid NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
  -- How the group was classified when the decision was made.
  classification text NOT NULL CHECK (classification IN ('identical', 'divergent')),
  -- Audit evidence: the candidate set (build id, revision, consumers) and the
  -- chosen producer's complete recursive configuration at decision time.
  -- A decision whose evidence no longer matches the live plan is stale.
  candidates jsonb NOT NULL,
  canonical_configuration jsonb NOT NULL,
  revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
  decided_at timestamptz NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX producer_reconciliations_key_unique
  ON producer_reconciliations (plan_root_build_id, output_type_id, method_kind, method_type_id);
CREATE INDEX producer_reconciliations_canonical_idx
  ON producer_reconciliations (canonical_producer_build_id);
