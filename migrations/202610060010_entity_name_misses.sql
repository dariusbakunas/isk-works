-- IDs `/universe/names` could not name
-- (docs/security/2026-10-06-esi-usage-audit.md, finding 4). ESI rejects a
-- whole batch with 404 when any one ID is invalid, so wallet name enrichment
-- used to send the same failing batch on every sync. A miss is not asked
-- about again for a week.

CREATE TABLE eve_entity_name_misses (
  entity_id bigint PRIMARY KEY CHECK (entity_id > 0),
  checked_at timestamptz NOT NULL
);
