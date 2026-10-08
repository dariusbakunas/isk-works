-- Explicit inventory recording actions -- Slice 1: acquisition only.
-- docs/superpowers/specs/2026-09-03-explicit-inventory-recording-design.md
--
-- Product invariant: a ticket's workflow status never changes inventory;
-- inventory changes only through explicit, named, idempotent actions. This
-- table is the immutable provenance ledger for those actions -- one row
-- per "I actually acquired (Slice 2: / produced) this" event.
--
-- Deliberately shaped so Slice 2 widens it (kind 'production' + a few
-- production-only nullable columns) rather than replacing it: `id` and the
-- `(ticket_id, idempotency_key)` identity stay put.
CREATE TABLE ticket_inventory_recordings (
  id uuid PRIMARY KEY,
  ticket_id uuid NOT NULL REFERENCES tickets(id) ON DELETE RESTRICT,
  kind text NOT NULL CHECK (kind IN ('acquisition')),
  idempotency_key uuid NOT NULL,
  -- This recording's own amount -- never cumulative, never capped at the
  -- ticket's demand. Surplus acquisition is legitimate inventory, so
  -- `SUM(recorded_quantity)` may exceed `tickets.quantity`.
  recorded_quantity bigint NOT NULL CHECK (recorded_quantity > 0),
  -- Informational provenance only: inventory is not location-scoped, so
  -- this never affects which balance row moves.
  location_note text NOT NULL DEFAULT '',
  note text NOT NULL DEFAULT '',
  recorded_at timestamptz NOT NULL,
  -- Idempotency is DB-authoritative: the same (ticket, key) can never post
  -- inventory twice. This btree also serves the only other access pattern
  -- ("every recording of this ticket", a leftmost-prefix scan on
  -- ticket_id), so no separate ticket_id index is added.
  CONSTRAINT ticket_inventory_recordings_idempotent UNIQUE (ticket_id, idempotency_key)
);

-- Provenance link: the Purchase event a recording posts points back to the
-- recording. Nullable -- every pre-existing inventory event (ESI wallet
-- imports via inventory_event_sources, Order/legacy `/complete` postings,
-- adjustments, opening balances) stays valid with NULL here; this does not
-- replace those mechanisms. ON DELETE RESTRICT so an accounting event can
-- never be orphaned by removing its recording (recordings are immutable
-- and never deleted anyway).
ALTER TABLE inventory_events
  ADD COLUMN ticket_inventory_recording_id uuid
    REFERENCES ticket_inventory_recordings(id) ON DELETE RESTRICT;
