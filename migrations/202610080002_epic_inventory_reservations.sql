-- Epic inventory reservations: `inventory_allocations` becomes live again.
--
-- An Epic (Order) reserves stock for its requirements on creation, its
-- tickets' recorded output is reserved to the requirements it feeds, and a
-- recording consumes the consuming requirement's reservations first. Every
-- planning read counts free stock (`physical - active allocations`).
--
-- Rows written before this migration are history from the retired
-- reservation lifecycle (all released or consumed by 202609030001 /
-- 202609040001) and are tagged `legacy`; they carry no recording links, so
-- the recording-link checks below exempt them.
--
-- `source_recording_id` / `consumed_by_recording_id` are RESTRICT, like
-- every other link to `ticket_inventory_recordings`: recordings are ledger
-- history and are only deleted by workspace erase, which removes
-- allocations first.
ALTER TABLE inventory_allocations
  ADD COLUMN reason text NOT NULL DEFAULT 'legacy',
  ADD COLUMN source_recording_id uuid NULL
    REFERENCES ticket_inventory_recordings(id) ON DELETE RESTRICT,
  ADD COLUMN consumed_by_recording_id uuid NULL
    REFERENCES ticket_inventory_recordings(id) ON DELETE RESTRICT;

ALTER TABLE inventory_allocations
  ADD CONSTRAINT inventory_allocations_reason_check
    CHECK (reason IN ('legacy', 'epic_create', 'epic_top_up', 'recorded_output', 'manual')),
  -- Recorded output, and only recorded output, names the recording that
  -- produced the reserved stock.
  ADD CONSTRAINT inventory_allocations_source_recording_matches_reason
    CHECK ((reason = 'recorded_output') = (source_recording_id IS NOT NULL)),
  -- A non-legacy row is consumed by exactly one recording, recorded with it.
  ADD CONSTRAINT inventory_allocations_consumed_by_recording
    CHECK (
      reason = 'legacy'
      OR (consumed_at IS NULL) = (consumed_by_recording_id IS NULL)
    );

-- "What does this requirement still hold" (remaining need, release on
-- cancel/archive, consume-own-first).
CREATE INDEX inventory_allocations_active_requirement_idx
  ON inventory_allocations (order_requirement_id)
  WHERE released_at IS NULL AND consumed_at IS NULL;

-- Reversal: un-consume / release by recording.
CREATE INDEX inventory_allocations_source_recording_idx
  ON inventory_allocations (source_recording_id)
  WHERE source_recording_id IS NOT NULL;
CREATE INDEX inventory_allocations_consumed_by_recording_idx
  ON inventory_allocations (consumed_by_recording_id)
  WHERE consumed_by_recording_id IS NOT NULL;
