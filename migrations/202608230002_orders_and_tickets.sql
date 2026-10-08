-- Build -> Order -> Board domain (see
-- docs/superpowers/specs/2026-08-21-build-order-board-design.md). Additive
-- alongside the existing plans/plan_tasks/tickets(-as-projection) system --
-- nothing here is read or written by the live app yet.

-- "Execute this version of this Build" -- an immutable snapshot, unlike
-- plans there is no status column and no Draft/Committed distinction: an
-- Order is created once, fully formed. Status is entirely derived (see
-- order::derive_order_status) from the four lifecycle timestamps below
-- plus its requirements' fulfillment state.
CREATE TABLE orders (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  source_build_id uuid NOT NULL REFERENCES builds(id) ON DELETE RESTRICT,
  source_build_revision bigint NOT NULL CHECK (source_build_revision > 0),
  display_name text NOT NULL,
  runs bigint NOT NULL CHECK (runs BETWEEN 1 AND 1000000),
  recipe_fingerprint text NOT NULL,
  price_snapshot_id uuid NOT NULL UNIQUE REFERENCES price_snapshots(id) ON DELETE CASCADE,
  estimated_material_cost numeric(24, 4) NOT NULL CHECK (estimated_material_cost >= 0),
  expected_revenue numeric(24, 4) CHECK (expected_revenue >= 0),
  estimated_margin numeric(24, 4),
  missing_price_count integer NOT NULL CHECK (missing_price_count >= 0),
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  -- Final assembly begun/done -- final assembly is not itself a Ticket, so
  -- these two manual timestamps are its whole lifecycle. completed_at
  -- requires started_at (can't complete an install that never began), and
  -- an Order can't be both completed and canceled.
  started_at timestamptz,
  completed_at timestamptz,
  canceled_at timestamptz,
  -- Orthogonal to workflow state -- see inventory_allocations below for
  -- why this must never gate anything allocation-related.
  archived_at timestamptz,
  CONSTRAINT orders_display_name_not_empty CHECK (length(btrim(display_name)) > 0),
  CONSTRAINT orders_completed_requires_started CHECK (completed_at IS NULL OR started_at IS NOT NULL),
  CONSTRAINT orders_not_completed_and_canceled CHECK (NOT (completed_at IS NOT NULL AND canceled_at IS NOT NULL))
);

CREATE INDEX orders_workspace_updated_idx ON orders (workspace_id, updated_at DESC);
CREATE INDEX orders_source_build_idx ON orders (source_build_id);

-- A frozen, single-level dependency of an Order's root Build (see the
-- design doc's "single-level dependency scope" decision) -- deliberately
-- carries no workflow status or ticket reference of its own; those live
-- on tickets, linked via order_requirement_fulfillments below, because a
-- requirement can exist with zero tickets (inventory-covered).
CREATE TABLE order_requirements (
  id uuid PRIMARY KEY,
  order_id uuid NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  type_id bigint NOT NULL CHECK (type_id > 0),
  captured_name text NOT NULL,
  kind text NOT NULL CHECK (kind IN ('buy', 'build', 'react')),
  required_quantity bigint NOT NULL CHECK (required_quantity > 0),
  -- Frozen inventory_allocations snapshot at creation time -- honest
  -- about what was available then, never re-checked afterward except
  -- through the allocation ledger's own release/consume transitions.
  reused_quantity bigint NOT NULL DEFAULT 0 CHECK (reused_quantity >= 0),
  fresh_quantity bigint NOT NULL CHECK (fresh_quantity >= 0),
  estimated_unit_cost numeric(24, 4) CHECK (estimated_unit_cost >= 0),
  estimated_line_total numeric(24, 4) CHECK (estimated_line_total >= 0),
  CONSTRAINT order_requirements_quantities_consistent
    CHECK (reused_quantity + fresh_quantity = required_quantity),
  UNIQUE (order_id, type_id)
);

CREATE INDEX order_requirements_order_idx ON order_requirements (order_id);

-- A standalone execution work item -- not tied 1:1 to any Order, linked to
-- whichever order_requirements/ticket_prerequisites row(s) it fulfills via
-- the join tables in the next migration. display_id reuses the existing
-- ticket_display_id_seq (already created for the old plan_tasks-as-Ticket
-- system) so the "ISK-####" id space stays one continuous sequence across
-- both systems during the migration period.
CREATE TABLE tickets (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  display_id text NOT NULL UNIQUE,
  kind text NOT NULL CHECK (kind IN ('acquisition', 'manufacturing', 'reaction')),
  type_id bigint NOT NULL CHECK (type_id > 0),
  captured_name text NOT NULL,
  quantity bigint NOT NULL CHECK (quantity > 0),
  -- Which recipe this ticket executes -- required for manufacturing/
  -- reaction (the linked build whose recipe this job assumes), never set
  -- for acquisition (nothing to manufacture).
  source_build_id uuid REFERENCES builds(id) ON DELETE RESTRICT,
  -- `blocked` is real: manufacturing/reaction tickets are creatable
  -- before their own prerequisites are met (see ticket_prerequisites).
  -- blocked/ready are system-derived (order::derive_ticket_blocked_ready),
  -- never manually toggled.
  status text NOT NULL CHECK (status IN ('blocked', 'ready', 'in_progress', 'complete', 'canceled')),
  estimated_unit_cost numeric(24, 4) CHECK (estimated_unit_cost >= 0),
  estimated_line_total numeric(24, 4) CHECK (estimated_line_total >= 0),
  actual_unit_cost numeric(24, 4) CHECK (actual_unit_cost >= 0),
  actual_line_total numeric(24, 4) CHECK (actual_line_total >= 0),
  acquisition_run_id uuid REFERENCES acquisition_runs(id) ON DELETE SET NULL,
  acquired_quantity bigint CHECK (acquired_quantity >= 0),
  execution_snapshot jsonb,
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  -- Orthogonal to status -- Board/list visibility filtering only.
  archived_at timestamptz,
  CONSTRAINT tickets_source_build_matches_kind CHECK (
    (kind = 'acquisition' AND source_build_id IS NULL)
    OR (kind IN ('manufacturing', 'reaction') AND source_build_id IS NOT NULL)
  ),
  CONSTRAINT tickets_acquired_quantity_only_for_acquisition
    CHECK (acquired_quantity IS NULL OR kind = 'acquisition')
);

CREATE INDEX tickets_workspace_updated_idx ON tickets (workspace_id, updated_at DESC);
CREATE INDEX tickets_acquisition_run_idx ON tickets (acquisition_run_id);

-- A manufacturing/reaction ticket's own frozen, single-level material
-- need -- the same shape as order_requirements, one level down, computed
-- at *ticket*-creation time from the linked build's own recipe. Only ever
-- populated for kind IN ('manufacturing', 'reaction'); acquisition
-- tickets have zero rows, which is exactly why they always start `ready`.
CREATE TABLE ticket_prerequisites (
  id uuid PRIMARY KEY,
  ticket_id uuid NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
  type_id bigint NOT NULL CHECK (type_id > 0),
  captured_name text NOT NULL,
  required_quantity bigint NOT NULL CHECK (required_quantity > 0),
  reused_quantity bigint NOT NULL DEFAULT 0 CHECK (reused_quantity >= 0),
  fresh_quantity bigint NOT NULL CHECK (fresh_quantity >= 0),
  estimated_unit_cost numeric(24, 4) CHECK (estimated_unit_cost >= 0),
  estimated_line_total numeric(24, 4) CHECK (estimated_line_total >= 0),
  CONSTRAINT ticket_prerequisites_quantities_consistent
    CHECK (reused_quantity + fresh_quantity = required_quantity),
  UNIQUE (ticket_id, type_id)
);

CREATE INDEX ticket_prerequisites_ticket_idx ON ticket_prerequisites (ticket_id);
