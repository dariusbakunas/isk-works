CREATE TABLE character_source_sync_state (
  connection_id uuid NOT NULL REFERENCES eve_connections(id) ON DELETE CASCADE,
  source_kind text NOT NULL CHECK (source_kind IN
    ('character_info', 'location', 'skills', 'wallet', 'industry_jobs')),
  refresh_state text NOT NULL DEFAULT 'missing' CHECK (refresh_state IN
    ('missing', 'current', 'refreshing', 'failed')),
  summary jsonb,
  observed_at timestamptz,
  last_attempted_at timestamptz,
  next_refresh_at timestamptz,
  lease_expires_at timestamptz,
  last_error text,
  PRIMARY KEY (connection_id, source_kind)
);

CREATE INDEX character_source_sync_state_due_idx
  ON character_source_sync_state (source_kind, next_refresh_at)
  WHERE refresh_state IN ('missing', 'current', 'failed');
