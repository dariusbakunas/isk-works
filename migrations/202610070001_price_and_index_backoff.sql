-- Consecutive failed refreshes of adjusted prices and system cost indices
-- (docs/security/2026-10-06-esi-usage-audit.md, finding 11). Each one
-- doubles the wait before the next attempt -- 1, 2, 4... minutes, capped at
-- 6 hours -- instead of retrying every minute for as long as ESI fails. A
-- success resets it.

ALTER TABLE industry_adjusted_price_refresh_state
  ADD COLUMN consecutive_failures integer NOT NULL DEFAULT 0
    CHECK (consecutive_failures >= 0);
ALTER TABLE industry_system_cost_index_registrations
  ADD COLUMN consecutive_failures integer NOT NULL DEFAULT 0
    CHECK (consecutive_failures >= 0);
