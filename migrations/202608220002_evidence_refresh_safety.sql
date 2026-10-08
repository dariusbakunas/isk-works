ALTER TABLE market_source_coverage
  ADD COLUMN lease_expires_at timestamptz,
  ADD COLUMN priority_requested_at timestamptz;

UPDATE market_source_coverage
SET lease_expires_at = COALESCE(last_attempted_at, now())
WHERE refresh_state = 'refreshing';

ALTER TABLE market_source_coverage
  ADD CONSTRAINT market_source_coverage_refresh_lease_consistent
  CHECK (refresh_state <> 'refreshing' OR lease_expires_at IS NOT NULL);

ALTER TABLE industry_system_cost_index_registrations
  ADD COLUMN priority_requested_at timestamptz;

CREATE INDEX market_source_coverage_due_priority_idx
  ON market_source_coverage (priority_requested_at DESC NULLS LAST, next_refresh_at, type_id);

CREATE INDEX industry_system_cost_index_priority_due_idx
  ON industry_system_cost_index_registrations
  (priority_requested_at DESC NULLS LAST, next_refresh_at, solar_system_id);

CREATE TRIGGER industry_adjusted_price_observations_immutable
BEFORE UPDATE OR DELETE ON industry_adjusted_price_observations
FOR EACH ROW EXECUTE FUNCTION reject_market_evidence_mutation();
