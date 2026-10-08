-- Slice 6 (market-centric pricing): Stage A/B from
-- docs/superpowers/specs/2026-08-23-market-centric-pricing-design.md §4.
--
-- Stage A -- additive: market_source_coverage gains its own region_id/
-- location_id, denormalized off market_price_source_configs via the
-- existing price_source_id join, so coverage rows carry scope identity
-- directly instead of only through a PriceSource indirection.
--
-- NULL-safety decision (design doc §4/§10 item 2, resolved here): Postgres
-- unique indexes treat NULL as distinct per row, which would silently allow
-- duplicate "region-wide" rows for the same (workspace, region, type). A
-- concrete sentinel -- location_id = 0 -- is used instead of NULL to mean
-- "region-wide/all locations". 0 is never a real EVE location_id (station
-- and structure ids are large positive numbers), so it's an unambiguous,
-- ordinary NOT NULL column value rather than a special case at every call
-- site. The same sentinel is applied to market_observation_batches and
-- market_price_source_configs for the same reason -- a batch or a price
-- source's own configured scope can now also legitimately be region-wide.

ALTER TABLE market_source_coverage
  ADD COLUMN region_id bigint,
  ADD COLUMN location_id bigint;

UPDATE market_source_coverage coverage
SET region_id = config.region_id,
    location_id = config.location_id
FROM market_price_source_configs config
WHERE config.price_source_id = coverage.price_source_id;

ALTER TABLE market_source_coverage
  ALTER COLUMN region_id SET NOT NULL,
  ALTER COLUMN location_id SET NOT NULL,
  ADD CONSTRAINT market_source_coverage_region_id_positive CHECK (region_id > 0),
  ADD CONSTRAINT market_source_coverage_location_id_valid CHECK (location_id >= 0);

CREATE UNIQUE INDEX market_source_coverage_scope_idx
  ON market_source_coverage (workspace_id, region_id, location_id, type_id);

-- Stage B -- dual-write / relax the single-Jita-anchor constraint: only one
-- esi_market_orders PriceSource could ever exist per workspace
-- (price_sources_one_esi_market_per_workspace_idx), and the only command
-- that created one (create_jita_price_source) hardcoded Jita 4-4. That
-- meant "Request market data" (Slice 5) could never resolve a source for
-- any other scope no matter what market_esi.rs's fetch logic supported.
-- Widen the constraint to one esi_market_orders source per (region,
-- location) scope instead of per workspace. source_kind is denormalized
-- onto market_price_source_configs because a partial unique index's
-- predicate can't reach across to price_sources.source_kind.

ALTER TABLE market_price_source_configs ADD COLUMN source_kind text;

UPDATE market_price_source_configs config
SET source_kind = source.source_kind
FROM price_sources source
WHERE source.id = config.price_source_id;

ALTER TABLE market_price_source_configs
  ALTER COLUMN source_kind SET NOT NULL,
  ADD CONSTRAINT market_price_source_configs_source_kind_valid
    CHECK (source_kind IN ('eve_client_market_export', 'esi_market_orders')),
  DROP CONSTRAINT market_price_source_configs_location_id_check,
  ADD CONSTRAINT market_price_source_configs_location_id_check CHECK (location_id >= 0),
  DROP CONSTRAINT market_price_source_configs_solar_system_id_check,
  ADD CONSTRAINT market_price_source_configs_solar_system_id_check CHECK (solar_system_id >= 0);

DROP INDEX price_sources_one_esi_market_per_workspace_idx;

CREATE UNIQUE INDEX market_price_source_configs_one_esi_per_scope_idx
  ON market_price_source_configs (workspace_id, region_id, location_id)
  WHERE source_kind = 'esi_market_orders';

-- market_observation_batches: a region-wide fetch's batch spans many real
-- locations/systems (each order already carries its own true location_id/
-- solar_system_id in market_order_observations), so the batch-level values
-- use the same 0 sentinel rather than picking one arbitrary location.

ALTER TABLE market_observation_batches
  DROP CONSTRAINT market_observation_batches_location_id_check,
  ADD CONSTRAINT market_observation_batches_location_id_check CHECK (location_id >= 0),
  DROP CONSTRAINT market_observation_batches_solar_system_id_check,
  ADD CONSTRAINT market_observation_batches_solar_system_id_check CHECK (solar_system_id >= 0);
