-- Finance can now record a Market Sell out of accounting Inventory, the
-- mirror of recording a Market Buy into it.
--
-- A sale is its own ledger event kind (not a 'consumption', which means "used
-- by a Build" and feeds material runway, nor an 'adjustment'): it removes the
-- sold quantity at the balance's weighted average cost. Its provenance row in
-- inventory_event_sources has accounting_effect_kind = 'sale', and shares the
-- purchase recording's lifecycle (reverted_at / reversal_event_id, at most one
-- active recording per wallet transaction and kind).

ALTER TABLE inventory_events DROP CONSTRAINT inventory_events_kind_valid;
ALTER TABLE inventory_events
  ADD CONSTRAINT inventory_events_kind_valid
    CHECK (event_kind IN ('opening_balance', 'purchase', 'consumption', 'production_output', 'reversal', 'adjustment', 'sale'));

ALTER TABLE inventory_event_sources
  DROP CONSTRAINT inventory_event_sources_accounting_effect_kind_check;
ALTER TABLE inventory_event_sources
  ADD CONSTRAINT inventory_event_sources_accounting_effect_kind_check
    CHECK (accounting_effect_kind IN ('purchase', 'sale'));
