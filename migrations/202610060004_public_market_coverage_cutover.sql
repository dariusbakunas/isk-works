-- Cutover to app-wide public market data
-- (docs/superpowers/specs/2026-10-05-global-public-market-data-design.md).
--
-- Public scopes -- region-wide (location_id = 0) and NPC stations in the
-- active SDE -- now read and register app-wide `public_market_coverage`
-- only. This moves their remaining per-workspace demand there (one row per
-- region and type, the latest demand winning) and deletes the per-workspace
-- rows; their old batches lose their last reference and the market GC
-- collects them. Structure coverage stays per workspace.
--
-- Rows the dual registration already created keep their own state and only
-- take the later demand time. New rows start missing, due and prioritized,
-- so the worker fills them first. Plain DML and idempotent: re-running it
-- finds nothing left to move.

INSERT INTO public_market_coverage (
  region_id, type_id, type_name, refresh_state, next_refresh_at,
  priority_requested_at, last_needed_at, created_at, updated_at
)
SELECT coverage.region_id, coverage.type_id, max(coverage.type_name), 'missing', now(),
       now(), max(coverage.last_needed_at), now(), now()
FROM market_source_coverage coverage
WHERE coverage.location_id = 0
   OR EXISTS (
     SELECT 1
     FROM sde_npc_stations station
     JOIN sde_imports import ON import.id = station.import_id AND import.active
     WHERE station.station_id = coverage.location_id
   )
GROUP BY coverage.region_id, coverage.type_id
ON CONFLICT (region_id, type_id) DO UPDATE SET
  last_needed_at = GREATEST(public_market_coverage.last_needed_at, EXCLUDED.last_needed_at);

DELETE FROM market_source_coverage coverage
WHERE coverage.location_id = 0
   OR EXISTS (
     SELECT 1
     FROM sde_npc_stations station
     JOIN sde_imports import ON import.id = station.import_id AND import.active
     WHERE station.station_id = coverage.location_id
   );
