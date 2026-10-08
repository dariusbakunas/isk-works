-- Links an order_requirement to a ticket that (fully or partially)
-- fulfills it. Many-to-many by construction: multiple fulfillments can
-- point at the same requirement (splitting demand across tickets over
-- time) and multiple fulfillments -- from different Orders' requirements
-- entirely -- can point at the same ticket. That second case is the whole
-- mechanism behind "Required by N Orders"; this milestone doesn't build
-- the UI to create it, but nothing here forces 1:1.
--
-- Never deleted, even once the ticket is canceled -- fulfillment state is
-- always *derived* by filtering non-canceled links (see
-- order::derive_requirement_state), not by removing rows, so cancellation
-- history stays intact.
CREATE TABLE order_requirement_fulfillments (
  id uuid PRIMARY KEY,
  order_requirement_id uuid NOT NULL REFERENCES order_requirements(id) ON DELETE CASCADE,
  ticket_id uuid NOT NULL REFERENCES tickets(id) ON DELETE RESTRICT,
  allocated_quantity bigint NOT NULL CHECK (allocated_quantity > 0),
  linked_at timestamptz NOT NULL
);

CREATE INDEX order_requirement_fulfillments_requirement_idx
  ON order_requirement_fulfillments (order_requirement_id);
CREATE INDEX order_requirement_fulfillments_ticket_idx
  ON order_requirement_fulfillments (ticket_id);

-- Same shape/semantics as order_requirement_fulfillments, one level down
-- -- a ticket_prerequisite linked to another ticket that supplies it. Kept
-- as its own table rather than folded into a polymorphic one: it answers
-- a different reverse-lookup question ("which tickets feed this ticket")
-- and doesn't share a numeric pool with order_requirement_fulfillments
-- the way inventory_allocations below shares one between order_requirements
-- and ticket_prerequisites.
CREATE TABLE ticket_prerequisite_fulfillments (
  id uuid PRIMARY KEY,
  ticket_prerequisite_id uuid NOT NULL REFERENCES ticket_prerequisites(id) ON DELETE CASCADE,
  fulfilling_ticket_id uuid NOT NULL REFERENCES tickets(id) ON DELETE RESTRICT,
  allocated_quantity bigint NOT NULL CHECK (allocated_quantity > 0),
  linked_at timestamptz NOT NULL
);

CREATE INDEX ticket_prerequisite_fulfillments_prerequisite_idx
  ON ticket_prerequisite_fulfillments (ticket_prerequisite_id);
CREATE INDEX ticket_prerequisite_fulfillments_fulfilling_ticket_idx
  ON ticket_prerequisite_fulfillments (fulfilling_ticket_id);

-- A standing claim against physical inventory -- not a one-time check
-- like the old plan_material_reservations (locked only at Plan-commit,
-- keyed to a deep-tree node with no clean release point). Created (under
-- a real balance lock) when an order_requirement or ticket_prerequisite
-- freezes its reused_quantity at creation time; released on cancellation;
-- converted into a real 'consumption' inventory_events row on completion.
--
-- One shared table for both owner kinds, not two -- both draw from the
-- same finite physical-inventory pool, and splitting the ledger risks a
-- silent double-counting bug in the "available" aggregation. The
-- exclusive-or is enforced with num_nonnulls rather than a stringly-typed
-- owner_type/owner_id pair.
CREATE TABLE inventory_allocations (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  type_id bigint NOT NULL CHECK (type_id > 0),
  quantity bigint NOT NULL CHECK (quantity > 0),
  order_requirement_id uuid REFERENCES order_requirements(id) ON DELETE CASCADE,
  ticket_prerequisite_id uuid REFERENCES ticket_prerequisites(id) ON DELETE CASCADE,
  created_at timestamptz NOT NULL,
  -- Set on the owning Order/Ticket's cancellation -- frees the claim, no
  -- ledger event (nothing was ever actually consumed).
  released_at timestamptz,
  -- Set on the owning Order/Ticket's completion, in the same transaction
  -- as posting a real 'consumption' inventory_events row for `quantity`.
  consumed_at timestamptz,
  CONSTRAINT inventory_allocations_exactly_one_owner
    CHECK (num_nonnulls(order_requirement_id, ticket_prerequisite_id) = 1),
  CONSTRAINT inventory_allocations_not_released_and_consumed
    CHECK (NOT (released_at IS NOT NULL AND consumed_at IS NOT NULL))
);

-- Hot path: computing `available = balance - active allocations` on every
-- Order/Ticket creation and every live worksheet preview.
CREATE INDEX inventory_allocations_active_type_idx
  ON inventory_allocations (workspace_id, owner_id, type_id)
  WHERE released_at IS NULL AND consumed_at IS NULL;
