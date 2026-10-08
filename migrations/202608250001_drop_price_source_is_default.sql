-- `is_default` on `price_sources` never drove any pricing resolution --
-- Build/Inventory pricing has resolved through `MarketScope`/
-- `Workspace.default_market_region_id`/`default_market_location_id` since
-- the Slice 8 redesign, and a build's manual Price Source fallback is
-- always explicitly selected (`manual_price_list_id`), never implied by
-- this flag. It only ever sorted the list and drove a "Default" badge --
-- pure UI dead weight, removed rather than kept as a no-op field.
DROP INDEX price_sources_one_default_per_workspace_idx;
ALTER TABLE price_sources DROP COLUMN is_default;
