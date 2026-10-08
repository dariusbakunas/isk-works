-- Slice 10 follow-up (market-centric pricing): Inventory's default market
-- pricing moves from "the workspace's default price_source" to a
-- workspace-level default MarketScope. Nullable, no backfill -- NULL means
-- "unset, fall back to DEFAULT_MARKET_SCOPE (Jita 4-4)", the same
-- convention DraftPlanningInput's deserialization shim already uses
-- (Workspace::default_market_scope()). No settings UI writes these yet;
-- the columns exist so one can be added later without another migration.

ALTER TABLE workspaces
  ADD COLUMN default_market_region_id bigint,
  ADD COLUMN default_market_location_id bigint;
