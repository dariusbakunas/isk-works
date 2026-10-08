-- Adds `InventoryEventKind::Adjustment` -- a quantity correction not tied
-- to a purchase, a Build/Order/Ticket consuming or producing material, or
-- an undo of a prior event. Same widening pattern as
-- 202608140002_allow_production_output_inventory_events.sql, which added
-- 'production_output' the same way.
ALTER TABLE inventory_events DROP CONSTRAINT inventory_events_kind_valid;
ALTER TABLE inventory_events
  ADD CONSTRAINT inventory_events_kind_valid
    CHECK (event_kind IN ('opening_balance', 'purchase', 'consumption', 'production_output', 'reversal', 'adjustment'));
