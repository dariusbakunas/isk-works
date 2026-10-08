-- Which connected character (if any) has confirmed docking access AND the
-- esi-markets.structure_markets.v1 scope for a resolved player structure --
-- a separate concern from `resolved_by_connection_id` above, which only
-- means "who resolved this structure's *name*" (a different ESI scope,
-- possibly a different character). Nullable: cleared (ON DELETE SET NULL,
-- matching resolved_by_connection_id's existing behavior) if the
-- remembered character disconnects, or explicitly by the refresh worker on
-- a later 403 -- either way the next refresh simply re-resolves from
-- scratch.
ALTER TABLE market_location_names
  ADD COLUMN market_access_connection_id uuid REFERENCES eve_connections(id) ON DELETE SET NULL,
  ADD COLUMN market_access_checked_at timestamptz;
