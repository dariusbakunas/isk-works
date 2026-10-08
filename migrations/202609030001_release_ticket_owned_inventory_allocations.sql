-- Phase 2 of the ticket/Board simplification
-- (docs/superpowers/specs/2026-09-03-ticket-board-simplification-design.md):
-- a Ticket no longer owns inventory reservations. `create_ticket` stops
-- inserting `inventory_allocations` for a manufacturing/reaction ticket's
-- prerequisites, and `cancel_ticket` stops releasing them.
--
-- Any row still keyed to a `ticket_prerequisite_id` was written under the
-- old semantics. An *active* one (never released, never consumed) would
-- otherwise stay claimed forever -- no code path is left that frees it --
-- permanently understating available/reserved inventory for that type.
--
-- Free those active claims by marking them released: the existing
-- "claim abandoned before use, nothing was ever consumed" lifecycle state
-- (see order::InventoryAllocation). This posts NO inventory_events, touches
-- NO inventory_balances, and changes NO cost basis -- it only stops the
-- row from counting against `available = balance - active allocations`.
--
-- Rows already `consumed_at` (a real Consumption event was posted) are left
-- exactly as they are -- accounting history stays intact. Order-owned
-- allocations (`order_requirement_id IS NOT NULL`) are untouched: Order ->
-- Inventory reservation semantics are deliberately unchanged in this phase.
UPDATE inventory_allocations
SET released_at = now()
WHERE ticket_prerequisite_id IS NOT NULL
  AND released_at IS NULL
  AND consumed_at IS NULL;
