-- Slice 8 (market-centric pricing): PriceSnapshotLine gains its own market
-- scope, per design doc §3.6. Each line already carries item_role
-- (Material|Output), so per-role scope falls out naturally once
-- material_scope and output_scope can differ. Additive, nullable, no
-- backfill: historical rows simply have NULL, meaning "captured before
-- scope was tracked" -- acceptable since price_snapshot_items rows are
-- read-only audit records, never re-evaluated.

ALTER TABLE price_snapshot_items
  ADD COLUMN market_region_id bigint,
  ADD COLUMN market_location_id bigint;
