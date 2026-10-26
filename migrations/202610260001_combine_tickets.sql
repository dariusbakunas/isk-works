-- Groundwork for combining production tickets across Epics.
--
-- A combined ticket executes several frozen operations: it holds every
-- member's `order_operation_ticket_claims` row. Its members are canceled
-- and point at it through `merged_into_ticket_id`, so splitting can restore
-- them exactly. Deleting the combined ticket frees them (they stay canceled).
ALTER TABLE tickets
  ADD COLUMN merged_into_ticket_id uuid REFERENCES tickets(id) ON DELETE SET NULL;

CREATE INDEX tickets_merged_into_idx
  ON tickets (merged_into_ticket_id)
  WHERE merged_into_ticket_id IS NOT NULL;

-- The Epic a prerequisite belongs to when it differs from its ticket's own
-- `order_id` -- set on a combined ticket's copies of its members'
-- prerequisites, which serve different Epics. NULL means the ticket's Epic.
ALTER TABLE ticket_prerequisites
  ADD COLUMN order_id uuid REFERENCES orders(id) ON DELETE CASCADE;
