-- Slice C2: whole-tree Epic planning snapshot. See the accompanying design
-- notes in `crates/iskworks-core/src/order/plan.rs`.
--
-- Today `order_requirements` freezes only the root Build's own single-level
-- material list (the design doc's "single-level dependency scope" decision),
-- and every descendant Build/Reaction ticket's own prerequisites are
-- computed lazily -- independently, per linked Build, live against
-- whatever inventory/prices exist *at the moment a user clicks to create
-- that specific ticket* -- rather than frozen once, together, from one
-- coherent whole-tree allocation at Epic-creation time.
--
-- This migration adds the persistence for a coherent whole-tree freeze:
--   * `order_plan_operations` -- one row per frozen production operation
--     (the root, and every active Build/Reaction descendant), addressed by
--     a stable `occurrence_key` (matches `graph_node_id`:
--     `root:<uuid>` / `build:<uuid>`) rather than `build_id` alone, since
--     the same Build could in principle occupy more than one occurrence.
--   * New columns on the EXISTING `order_requirements` (root-only until now)
--     so it becomes the one authoritative whole-tree requirement table
--     instead of introducing a second, competing one -- each row now
--     belongs to a specific `order_plan_operations` occurrence via
--     `operation_occurrence_key`, and a Build/Reaction row whose production
--     comes from another frozen operation names it via `child_occurrence_key`.
--   * The same evidence columns on `ticket_prerequisites`, so a generated
--     ticket's own frozen material need carries the identical audit trail.
--   * New columns on `tickets` so a generated production ticket can carry
--     its own occurrence identity, parent linkage, and the
--     material/installation/total cost split (Slice C1/C1.1's model)
--     instead of the prorated *consumed-by-parent* figure
--     `estimated_unit_cost`/`estimated_line_total` have always meant.
--   * `orders.planning_snapshot_version` distinguishes a legacy,
--     root-only-frozen Epic (`1`, the default -- every existing row) from a
--     whole-tree allocation-aware Epic (`2`, written by the new path).
--     Historical Epics are never rewritten or reinterpreted; they keep
--     reading back exactly as before (`operation_occurrence_key IS NULL`
--     reads as "this row belongs to the order's own root", same as today).
--
-- Every new column is nullable (or has a default that preserves existing
-- behavior) -- no backfill, no reinterpretation of historical rows.

ALTER TABLE orders
  ADD COLUMN planning_snapshot_version smallint NOT NULL DEFAULT 1
    CHECK (planning_snapshot_version IN (1, 2));

-- One row per frozen production operation in the whole tree (the root, and
-- every active -- i.e. not fully-covered-by-inventory -- Build/Reaction
-- descendant). A fully-covered Build/Reaction boundary is pruned exactly
-- as the live Graph/Materials projection prunes it: no row here, no ticket
-- -- only the owning `order_requirements` row (reused_quantity ==
-- required_quantity, fresh_quantity == 0).
CREATE TABLE order_plan_operations (
  id uuid PRIMARY KEY,
  order_id uuid NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  -- `root:<uuid>` / `build:<uuid>` -- matches `crate::build_graph`'s
  -- `graph_node_id` and `VerificationOperationInput::graph_node_id` exactly,
  -- so this row's identity is traceable straight back to the live
  -- projection it was frozen from.
  occurrence_key text NOT NULL,
  -- NULL for the root operation.
  parent_occurrence_key text,
  build_id uuid NOT NULL REFERENCES builds(id) ON DELETE RESTRICT,
  activity text NOT NULL CHECK (activity IN ('manufacturing', 'reaction')),
  -- The PROJECTED runs this plan required at freeze time -- never the
  -- child Build's own persisted `runs` (see `persisted_runs`).
  runs bigint NOT NULL CHECK (runs > 0),
  -- This operation's own Build.runs at freeze time -- informational only,
  -- exactly like the live Graph's `persistedRuns` (never re-derived later
  -- from the Build's current, possibly-since-changed, persisted runs).
  persisted_runs bigint NOT NULL CHECK (persisted_runs >= 0),
  product_type_id bigint NOT NULL CHECK (product_type_id > 0),
  product_name text NOT NULL,
  output_per_run bigint NOT NULL CHECK (output_per_run > 0),
  produced_quantity bigint NOT NULL CHECK (produced_quantity >= 0),
  blueprint_or_formula_type_id bigint NOT NULL,
  material_component_cost numeric(24, 4) CHECK (material_component_cost >= 0),
  own_installation_cost numeric(24, 4) CHECK (own_installation_cost >= 0),
  total_production_cost numeric(24, 4) CHECK (total_production_cost >= 0),
  -- `false` iff any of this operation's own boundaries, or a child
  -- operation it consumes, had an incomplete cost at freeze time -- never
  -- zero-substituted; the cost columns above are simply NULL in that case.
  complete boolean NOT NULL,
  -- Facility identity/revision, installation-cost breakdown (EIV, system
  -- cost index, job/tax/SCC/alliance percentages and amounts, formula
  -- version), ME/TE, recipe currency, and per-operation warnings -- the
  -- same "big bag of audit evidence" `tickets.execution_snapshot` already
  -- stores as jsonb, never queried by column, always read back whole.
  evidence jsonb NOT NULL,
  created_at timestamptz NOT NULL,
  UNIQUE (order_id, occurrence_key)
);

CREATE INDEX order_plan_operations_order_idx ON order_plan_operations (order_id);

-- order_requirements becomes the one authoritative WHOLE-TREE requirement
-- table (previously root-only). `operation_occurrence_key` places a row in
-- the tree; NULL means "the order's own root" (both a pre-existing
-- version-1 row, and forward-compatible if a caller ever omits it).
ALTER TABLE order_requirements
  ADD COLUMN operation_occurrence_key text,
  -- The `order_plan_operations` occurrence this Build/Reaction requirement's
  -- production comes from. NULL for a `Buy` row, a fully-covered row (the
  -- child subtree was pruned -- see `order_plan_operations`'s own doc), or
  -- an unresolved-but-Build-intended row with no linked child yet.
  ADD COLUMN child_occurrence_key text,
  -- The weighted-average unit basis actually used for `reused_quantity` at
  -- freeze time -- `reused_line_total / reused_quantity` is derivable, but
  -- freezing the unit figure directly means a later basis change (from an
  -- unrelated transaction) can never be read back into this row by
  -- accident.
  ADD COLUMN inventory_unit_basis numeric(24, 4) CHECK (inventory_unit_basis >= 0),
  -- Surplus-conserving child-production evidence (see
  -- `crate::build_cost`'s module doc): the child operation's own
  -- `produced_quantity`, the portion of it THIS requirement consumed, and
  -- the discrete-output remainder the child keeps -- planning evidence
  -- only, never added to any cost paid by this requirement or its parent.
  ADD COLUMN child_produced_quantity bigint CHECK (child_produced_quantity >= 0),
  ADD COLUMN child_consumed_quantity bigint CHECK (child_consumed_quantity >= 0),
  ADD COLUMN child_surplus_quantity bigint CHECK (child_surplus_quantity >= 0),
  ADD COLUMN child_surplus_retained_basis numeric(24, 4) CHECK (child_surplus_retained_basis >= 0),
  -- Fresh-price selection kind/note/region/location and staleness -- the
  -- same per-boundary provenance `VerificationBoundaryInput` already
  -- carries live, frozen verbatim.
  ADD COLUMN price_evidence jsonb;

-- The same type_id can now legitimately appear more than once per Epic (a
-- shared raw material demanded at two different tree depths) -- scope
-- uniqueness by operation, not just order + type.
ALTER TABLE order_requirements DROP CONSTRAINT order_requirements_order_id_type_id_key;
CREATE UNIQUE INDEX order_requirements_operation_type_idx
  ON order_requirements (order_id, operation_occurrence_key, type_id);

CREATE INDEX order_requirements_operation_idx
  ON order_requirements (operation_occurrence_key)
  WHERE operation_occurrence_key IS NOT NULL;

-- Identical evidence columns, one level down -- a generated ticket's own
-- frozen material need carries the same audit trail as the Epic requirement
-- it was generated from.
ALTER TABLE ticket_prerequisites
  ADD COLUMN operation_occurrence_key text,
  ADD COLUMN child_occurrence_key text,
  ADD COLUMN inventory_unit_basis numeric(24, 4) CHECK (inventory_unit_basis >= 0),
  ADD COLUMN child_produced_quantity bigint CHECK (child_produced_quantity >= 0),
  ADD COLUMN child_consumed_quantity bigint CHECK (child_consumed_quantity >= 0),
  ADD COLUMN child_surplus_quantity bigint CHECK (child_surplus_quantity >= 0),
  ADD COLUMN child_surplus_retained_basis numeric(24, 4) CHECK (child_surplus_retained_basis >= 0),
  ADD COLUMN price_evidence jsonb;

-- A generated production ticket's own occurrence identity, tree linkage,
-- and material/installation/total cost split. `estimated_unit_cost` /
-- `estimated_line_total` keep their existing meaning for every ticket kind
-- created before this migration and for Acquisition tickets going forward
-- (the fresh-shortage-only cost); a whole-tree-frozen Manufacturing/
-- Reaction ticket's own full job cost lives in the three new columns below
-- instead of being force-fit into those two.
ALTER TABLE tickets
  ADD COLUMN occurrence_key text,
  ADD COLUMN parent_ticket_id uuid REFERENCES tickets(id) ON DELETE SET NULL,
  ADD COLUMN produced_quantity bigint CHECK (produced_quantity >= 0),
  ADD COLUMN material_component_cost numeric(24, 4) CHECK (material_component_cost >= 0),
  ADD COLUMN own_installation_cost numeric(24, 4) CHECK (own_installation_cost >= 0),
  ADD COLUMN total_production_cost numeric(24, 4) CHECK (total_production_cost >= 0),
  ADD COLUMN plan_evidence jsonb;

CREATE UNIQUE INDEX tickets_order_occurrence_idx
  ON tickets (order_id, occurrence_key)
  WHERE order_id IS NOT NULL AND occurrence_key IS NOT NULL;

CREATE INDEX tickets_parent_ticket_idx ON tickets (parent_ticket_id) WHERE parent_ticket_id IS NOT NULL;
