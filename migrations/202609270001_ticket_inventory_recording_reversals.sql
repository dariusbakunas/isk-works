-- A ticket recording remains immutable accounting evidence. Reversal adds
-- lifecycle state only; original quantities, costs, provenance, and time stay
-- unchanged, while compensating inventory events carry the ledger correction.
ALTER TABLE ticket_inventory_recordings
  ADD COLUMN reverted_at timestamptz;
