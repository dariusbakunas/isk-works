-- Consecutive failed refreshes of one coverage row, so a row that keeps
-- failing backs off exponentially (see `fail_market_refresh`) instead of
-- hitting ESI every minute forever -- every failure spends the per-IP ESI
-- error budget all tenants share. Reset by any successful refresh.
ALTER TABLE market_source_coverage
  ADD COLUMN consecutive_failures integer NOT NULL DEFAULT 0
    CHECK (consecutive_failures >= 0);
