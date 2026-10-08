-- The earliest moment a refresh row may be fetched from ESI again
-- (docs/security/2026-10-06-esi-usage-audit.md, finding 2).
--
-- After a success it is ESI's `Expires`; after a failure it is the retry
-- floor (exponential backoff or a server-directed `Retry-After`). A
-- prioritized refresh ("request market data", a view the user is waiting
-- on) moves to the front of the queue but never becomes due before it:
-- refetching a resource before its `Expires` circumvents ESI's cache, which
-- CCP treats as abuse. NULL means no bound (never fetched).

ALTER TABLE public_market_coverage ADD COLUMN refresh_not_before timestamptz;
ALTER TABLE market_source_coverage ADD COLUMN refresh_not_before timestamptz;
ALTER TABLE industry_adjusted_price_refresh_state ADD COLUMN refresh_not_before timestamptz;
ALTER TABLE industry_system_cost_index_registrations ADD COLUMN refresh_not_before timestamptz;
