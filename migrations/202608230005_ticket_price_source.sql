-- Freezes the "acquisition location" (the price source a ticket was priced
-- from at creation time) directly on tickets -- unlike the legacy
-- plan_tasks system, a ticket isn't 1:1 with an owning Plan whose own
-- price_snapshot it can be resolved through indirectly, so it has to carry
-- its own. Nullable and meaningful for any ticket kind (frozen execution
-- context), but only Acquisition tickets' value is checked for Acquisition
-- Run batching compatibility. See
-- docs/superpowers/specs/2026-08-21-build-order-board-design.md.

ALTER TABLE tickets
  ADD COLUMN price_source_id uuid REFERENCES price_sources(id) ON DELETE RESTRICT;
