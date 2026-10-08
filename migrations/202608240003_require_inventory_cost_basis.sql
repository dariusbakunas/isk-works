-- Every positive accounted inventory quantity must have a defined cost
-- basis: no more "unknown cost" state, no more known/unknown quantity
-- split. quantity + total_historical_cost is the whole model;
-- average_unit_cost is always derivable whenever quantity > 0.
--
-- Existing development data is disposable -- no backfill/compatibility
-- layer. inventory_event_sources/inventory_import_proposals FK-reference
-- inventory_events.id with ON DELETE RESTRICT, so they're cleared too.
TRUNCATE inventory_import_proposals, inventory_event_sources, inventory_events, inventory_balances;

ALTER TABLE inventory_balances DROP CONSTRAINT inventory_balances_quantities_consistent;
ALTER TABLE inventory_balances DROP COLUMN known_quantity;
ALTER TABLE inventory_balances DROP COLUMN unknown_quantity;

ALTER TABLE inventory_events DROP CONSTRAINT inventory_events_delta_consistent;
ALTER TABLE inventory_events DROP CONSTRAINT inventory_events_result_quantities_consistent;
ALTER TABLE inventory_events DROP CONSTRAINT inventory_events_average_consistent;
ALTER TABLE inventory_events DROP COLUMN known_quantity_delta;
ALTER TABLE inventory_events DROP COLUMN unknown_quantity_delta;
ALTER TABLE inventory_events DROP COLUMN resulting_known_quantity;
ALTER TABLE inventory_events DROP COLUMN resulting_unknown_quantity;

ALTER TABLE inventory_events
  ADD CONSTRAINT inventory_events_average_consistent
    CHECK (
      (resulting_quantity = 0 AND resulting_average_cost IS NULL)
      OR (resulting_quantity > 0 AND resulting_average_cost IS NOT NULL)
    );

ALTER TABLE inventory_events DROP CONSTRAINT inventory_events_quality_valid;
ALTER TABLE inventory_events
  ADD CONSTRAINT inventory_events_quality_valid
    CHECK (cost_quality IN ('known', 'estimated', 'zero_cost'));
