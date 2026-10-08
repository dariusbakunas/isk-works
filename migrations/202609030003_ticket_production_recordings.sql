-- Explicit inventory recording -- Slice 2: production (manufacturing /
-- reaction). Widens the Slice-1 `ticket_inventory_recordings` table rather
-- than adding another; the identity (`id`, `(ticket_id, idempotency_key)`)
-- and the acquisition row shape are preserved.
-- docs/superpowers/specs/2026-09-03-explicit-inventory-recording-design.md
--
-- Verified against the active SDE (0 of 4828 manufacturing blueprints and
-- 0 of 120 reaction formulas produce more than one product), so a single
-- output per recording is modelled inline. A future multi-output recipe
-- would add an `outputs[]` child table without disturbing these columns.

ALTER TABLE ticket_inventory_recordings
  ALTER COLUMN recorded_quantity DROP NOT NULL;

ALTER TABLE ticket_inventory_recordings
  DROP CONSTRAINT ticket_inventory_recordings_recorded_quantity_check,
  DROP CONSTRAINT ticket_inventory_recordings_kind_check;

ALTER TABLE ticket_inventory_recordings
  ADD COLUMN runs_completed bigint,
  -- Explicit actual installation cost attributable to THIS recording --
  -- never derived server-side from the plan. Stored for audit/provenance.
  ADD COLUMN installation_cost numeric(24, 4),
  ADD COLUMN output_type_id bigint,
  ADD COLUMN output_quantity bigint;

ALTER TABLE ticket_inventory_recordings
  ADD CONSTRAINT ticket_inventory_recordings_kind_valid
    CHECK (kind IN ('acquisition', 'production')),
  -- An acquisition row: `recorded_quantity` set (> 0), production columns
  -- all NULL. Unchanged from Slice 1.
  ADD CONSTRAINT ticket_inventory_recordings_acquisition_shape CHECK (
    kind <> 'acquisition' OR (
      recorded_quantity IS NOT NULL AND recorded_quantity > 0
      AND runs_completed IS NULL AND installation_cost IS NULL
      AND output_type_id IS NULL AND output_quantity IS NULL
    )
  ),
  -- A production row: `recorded_quantity` NULL; runs > 0; installation
  -- cost >= 0 (0 valid); one output identity; output quantity >= 0 (0 is a
  -- scrapped/failed job -- inputs consumed, nothing produced).
  ADD CONSTRAINT ticket_inventory_recordings_production_shape CHECK (
    kind <> 'production' OR (
      recorded_quantity IS NULL
      AND runs_completed IS NOT NULL AND runs_completed > 0
      AND installation_cost IS NOT NULL AND installation_cost >= 0
      AND output_type_id IS NOT NULL AND output_type_id > 0
      AND output_quantity IS NOT NULL AND output_quantity >= 0
    )
  );

-- `inventory_events.ticket_inventory_recording_id` (Slice 1) already
-- provides the 1:N link a production recording needs: its N Consumption
-- events and 1 ProductionOutput event all carry the same recording id.
-- No join table is added.
