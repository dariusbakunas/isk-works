-- Consecutive failed refreshes of a character source
-- (docs/security/2026-10-06-esi-usage-audit.md, finding 5). Each one doubles
-- the wait before the next attempt -- 5, 10, 20... minutes, capped at 6 hours
-- -- instead of retrying a source that keeps failing every 5 minutes
-- forever. A success resets it.

ALTER TABLE character_source_sync_state
  ADD COLUMN consecutive_failures integer NOT NULL DEFAULT 0
    CHECK (consecutive_failures >= 0);
