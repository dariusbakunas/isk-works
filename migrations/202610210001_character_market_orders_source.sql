-- The `market_orders` character source syncs each connection's active
-- market orders (optional scope esi-markets.read_character_orders.v1).
ALTER TABLE character_source_sync_state
  DROP CONSTRAINT character_source_sync_state_source_kind_check,
  ADD CONSTRAINT character_source_sync_state_source_kind_check CHECK (source_kind IN
    ('character_info', 'location', 'skills', 'wallet', 'industry_jobs', 'assets', 'wallet_transactions',
     'planets', 'calendar', 'market_orders'));
