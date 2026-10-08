-- Found during Slice 6's manual walkthrough: Slice 1's migration
-- (202608100001) rewrote inventory_events_kind_valid to only allow
-- ('opening_balance', 'purchase', 'consumption', 'reversal') -- at the
-- time nothing created a 'production_output' event (the old
-- production_output_posting function was dead code, later deleted
-- outright in Slice 4). POST /api/plans/:id/post-output is the first
-- feature to actually insert one, and hit the CHECK constraint.
ALTER TABLE inventory_events DROP CONSTRAINT inventory_events_kind_valid;
ALTER TABLE inventory_events
  ADD CONSTRAINT inventory_events_kind_valid
    CHECK (event_kind IN ('opening_balance', 'purchase', 'consumption', 'production_output', 'reversal'));
