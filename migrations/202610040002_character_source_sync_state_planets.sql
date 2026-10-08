ALTER TABLE character_source_sync_state
  DROP CONSTRAINT character_source_sync_state_source_kind_check,
  ADD CONSTRAINT character_source_sync_state_source_kind_check CHECK (source_kind IN
    ('character_info', 'location', 'skills', 'wallet', 'industry_jobs', 'assets', 'wallet_transactions',
     'planets'));
