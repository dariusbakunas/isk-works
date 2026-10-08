-- When something last needed this coverage row's prices: every build
-- preview, inventory valuation, opportunity scan, planetary view, or market
-- request re-registers the coverage it uses and bumps this. Rows nobody has
-- needed for a while go dormant -- the worker stops refreshing them, their
-- last prices stay (just staler) -- until the next registration wakes them,
-- so ESI budget follows actual use. Existing rows start as needed now.
ALTER TABLE market_source_coverage
  ADD COLUMN last_needed_at timestamptz NOT NULL DEFAULT now();
