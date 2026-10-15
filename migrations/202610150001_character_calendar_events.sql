-- Upcoming in-game EVE calendar events, one row per connection that can see
-- the event (responses are per character). The `calendar` character source
-- replaces a connection's whole set on each successful sync, so cancelled
-- or deleted events disappear. `text` is raw player-authored markup.
ALTER TABLE character_source_sync_state
  DROP CONSTRAINT character_source_sync_state_source_kind_check,
  ADD CONSTRAINT character_source_sync_state_source_kind_check CHECK (source_kind IN
    ('character_info', 'location', 'skills', 'wallet', 'industry_jobs', 'assets', 'wallet_transactions',
     'planets', 'calendar'));

CREATE TABLE character_calendar_events (
    connection_id uuid NOT NULL REFERENCES eve_connections(id) ON DELETE CASCADE,
    event_id bigint NOT NULL,
    event_date timestamptz NOT NULL,
    title text NOT NULL,
    importance bigint NOT NULL,
    response text NOT NULL
      CHECK (response IN ('accepted', 'declined', 'tentative', 'not_responded')),
    duration_minutes bigint NOT NULL CHECK (duration_minutes >= 0),
    owner_id bigint,
    owner_name text,
    owner_type text,
    text text NOT NULL,
    observed_at timestamptz NOT NULL,
    PRIMARY KEY (connection_id, event_id)
);

CREATE INDEX character_calendar_events_event_date_idx ON character_calendar_events (event_date);
