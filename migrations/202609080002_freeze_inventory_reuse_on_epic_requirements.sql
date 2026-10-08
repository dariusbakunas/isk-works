-- Inventory-aware Epic snapshotting (Model B). See
-- docs/superpowers/specs/2026-09-03-ticket-board-simplification-design.md and
-- the design investigation it grew from.
--
-- `create_order` now nets every `Missing`-scoped requirement against the same
-- live inventory coverage the Build worksheet uses, and freezes that intent
-- onto the Epic. A frozen `reused_quantity` means "at Epic-creation time this
-- Epic intended to use this much existing stock" -- it is NOT a reservation:
-- no `inventory_allocations` row, no `inventory_events` row, no balance/cost
-- change. Two Epics may plan against the same physical stock; execution
-- reconciliation is a deliberate follow-up.
--
-- Two new frozen columns per requirement/prerequisite:
--   * `fulfillment_scope` -- the scope the row was sourced under, frozen so a
--     later read never has to consult the (mutable) source Build's current
--     `fulfillment_scopes`.
--   * `reused_line_total` -- the expected cost of just the reused (inventory)
--     portion, at the inventory weighted-average cost at creation time. The
--     single authoritative frozen figure for the split: the fresh portion's
--     own cost is `estimated_line_total - reused_line_total`, and the reused
--     unit cost is `reused_line_total / reused_quantity` -- both derived, so
--     there is no second representation to drift.
--
-- `reused_quantity` / `fresh_quantity` already exist (they were kept through
-- the inventory-neutral lifecycle change). This migration stops them being
-- hard-coded to `0` / `required_quantity` for new Epics.
--
-- Historical rows: `fulfillment_scope` defaults to 'full', which -- together
-- with their existing stored `reused_quantity = 0`, `fresh_quantity =
-- required_quantity` -- exactly preserves their effective "sourced the whole
-- requirement fresh" behavior. No historical Epic is re-interpreted against
-- current inventory; there is no inventory read in this migration.

ALTER TABLE order_requirements
  ADD COLUMN fulfillment_scope text NOT NULL DEFAULT 'full'
    CHECK (fulfillment_scope IN ('missing', 'full')),
  ADD COLUMN reused_line_total numeric(24, 4)
    CHECK (reused_line_total IS NULL OR reused_line_total >= 0);

ALTER TABLE ticket_prerequisites
  ADD COLUMN fulfillment_scope text NOT NULL DEFAULT 'full'
    CHECK (fulfillment_scope IN ('missing', 'full')),
  ADD COLUMN reused_line_total numeric(24, 4)
    CHECK (reused_line_total IS NULL OR reused_line_total >= 0);
