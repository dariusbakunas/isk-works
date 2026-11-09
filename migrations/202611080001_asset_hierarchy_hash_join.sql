-- `refresh_esi_asset_hierarchy` runs on a snapshot inserted earlier in the
-- same transaction, so the planner has no statistics for its snapshot_id and
-- estimates ~1 row. With that estimate it looks parents up through
-- `(snapshot_id, location_id)` and filters `source_item_id` per row: the walk
-- turns quadratic (6.7k assets: 15M rows filtered, 5-9 s instead of ~0.1 s).
--
-- Forbidding nested loops inside the function makes each recursive step hash
-- the snapshot's rows once, which is cheap at any size and independent of the
-- row estimate. The walk itself is unchanged.
ALTER FUNCTION refresh_esi_asset_hierarchy(uuid) SET enable_nestloop = off;
