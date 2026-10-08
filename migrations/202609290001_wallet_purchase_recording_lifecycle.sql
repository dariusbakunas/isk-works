-- Wallet-purchase recording lifecycle.
--
-- inventory_event_sources is the provenance row that ties one inventory
-- purchase event to the exact ESI wallet transaction (observation) it
-- recorded. Until now that row was permanent and unique per source record,
-- so a purchase whose inventory event was later reversed could never be
-- recorded again and the proposal kept claiming it was still accepted.
--
-- A source row is now one *recording*: it is immutable history, and its
-- lifecycle is `reverted_at` / `reversal_event_id`. At most one recording per
-- wallet transaction may be active; reverted recordings are kept forever and
-- a later re-record adds a new row for a new purchase event.

ALTER TABLE inventory_event_sources
  ADD COLUMN reverted_at timestamptz NULL,
  ADD COLUMN reversal_event_id uuid NULL REFERENCES inventory_events(id) ON DELETE RESTRICT;

-- Authoritative backfill, not a heuristic: a purchase event that already has a
-- ledger reversal (inventory_events.reverses_event_id) was reverted, whichever
-- path reversed it.
UPDATE inventory_event_sources s
SET reverted_at = r.recorded_at,
    reversal_event_id = r.id
FROM inventory_events r
WHERE r.reverses_event_id = s.inventory_event_id;

-- Proposals whose recording is reverted go back to pending so both surfaces
-- agree that the transaction is not currently in inventory.
UPDATE inventory_import_proposals p
SET status = 'pending',
    accepted_inventory_event_id = NULL,
    accepted_at = NULL,
    updated_at = now(),
    revision = p.revision + 1
FROM inventory_event_sources s
WHERE p.accepted_inventory_event_id = s.inventory_event_id
  AND s.reverted_at IS NOT NULL;

ALTER TABLE inventory_event_sources
  ADD CONSTRAINT inventory_event_sources_reversal_consistent
  CHECK ((reverted_at IS NULL) = (reversal_event_id IS NULL));

-- The permanent per-source-record uniqueness is what blocked re-recording. It
-- also keyed on the bare EVE transaction id, which is only unique per
-- connection; the real identity is the observation row.
ALTER TABLE inventory_event_sources
  DROP CONSTRAINT inventory_event_sources_workspace_id_owner_id_source_system_key;

-- Active-recording invariant: at most one ACTIVE accounting effect of a kind
-- per wallet transaction.
CREATE UNIQUE INDEX inventory_event_sources_active_observation_idx
  ON inventory_event_sources (observation_id, accounting_effect_kind)
  WHERE reverted_at IS NULL;

-- Finance joins every listed transaction to its latest recording.
CREATE INDEX inventory_event_sources_observation_history_idx
  ON inventory_event_sources (observation_id, accepted_at DESC);
