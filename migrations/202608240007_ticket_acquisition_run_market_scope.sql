-- Slice 9 (market-centric pricing): tickets/acquisition_runs gain their own
-- frozen market scope columns, per
-- docs/superpowers/specs/2026-08-23-market-centric-pricing-design.md §3.7/§4
-- Stage A. Deferred here from Slice 6's migration (which only covered
-- market_source_coverage) since the rollout plan's Slice 8 dependency
-- ordering means Ticket/AcquisitionRun scope-freezing only became possible
-- once PriceSnapshotLine itself carried a scope (Slice 8's
-- 202608240006_price_snapshot_line_market_scope.sql).
--
-- Nullable, no uniqueness constraint on these columns (unlike
-- market_source_coverage's scope index), so a plain NULL location_id is an
-- ordinary "unknown/region-wide" value here -- no 0-sentinel workaround
-- needed the way Slice 6 needed one.

ALTER TABLE tickets
  ADD COLUMN market_region_id bigint,
  ADD COLUMN market_location_id bigint;

ALTER TABLE acquisition_runs
  ADD COLUMN market_region_id bigint,
  ADD COLUMN market_location_id bigint;

-- Backfill from each row's already-frozen price_source_id, resolved through
-- market_price_source_configs. A manually-priced ticket/run's
-- price_source_id has no matching config row (manual price lists were
-- never configured there), so it simply stays NULL -- exactly the "scope
-- doesn't apply" case design doc §3.7 carves out for Manual Price List
-- provenance.
UPDATE tickets t
SET market_region_id = config.region_id,
    market_location_id = NULLIF(config.location_id, 0)
FROM market_price_source_configs config
WHERE config.price_source_id = t.price_source_id;

UPDATE acquisition_runs r
SET market_region_id = config.region_id,
    market_location_id = NULLIF(config.location_id, 0)
FROM market_price_source_configs config
WHERE config.price_source_id = r.price_source_id;
