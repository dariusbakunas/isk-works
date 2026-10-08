CREATE TABLE industry_adjusted_price_refresh_state (
  singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
  refresh_state text NOT NULL CHECK (refresh_state IN ('missing','current','refreshing','failed')),
  observed_at timestamptz,
  last_attempted_at timestamptz,
  next_refresh_at timestamptz,
  lease_expires_at timestamptz,
  last_error text,
  updated_at timestamptz NOT NULL,
  CONSTRAINT industry_adjusted_price_refresh_consistent CHECK (
    refresh_state <> 'refreshing' OR lease_expires_at IS NOT NULL
  )
);

CREATE TABLE industry_system_cost_index_registrations (
  solar_system_id bigint PRIMARY KEY CHECK (solar_system_id > 0),
  refresh_state text NOT NULL CHECK (refresh_state IN ('missing','current','refreshing','failed')),
  observed_at timestamptz,
  last_attempted_at timestamptz,
  next_refresh_at timestamptz,
  lease_expires_at timestamptz,
  last_error text,
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  CONSTRAINT industry_system_cost_index_refresh_consistent CHECK (
    refresh_state <> 'refreshing' OR lease_expires_at IS NOT NULL
  )
);

CREATE INDEX industry_system_cost_index_due_idx
  ON industry_system_cost_index_registrations (next_refresh_at, solar_system_id);

CREATE INDEX industry_system_cost_index_latest_idx
  ON industry_system_cost_index_observations (solar_system_id, observed_at DESC);

CREATE TRIGGER industry_system_cost_index_observations_immutable
BEFORE UPDATE OR DELETE ON industry_system_cost_index_observations
FOR EACH ROW EXECUTE FUNCTION reject_market_evidence_mutation();
