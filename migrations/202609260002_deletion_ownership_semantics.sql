-- Planning/workflow deletion must not erase posted inventory/accounting facts.
-- Build links below are live while the Build exists, but historical provenance
-- after an Epic/Ticket freezes its own snapshot.  Deleting the source clears
-- navigation without deleting the snapshot.

-- A recording keeps the original Ticket UUID forever.  It intentionally is
-- not a live FK: the Ticket is workflow state and may be hard-deleted, while
-- the recording and its RESTRICT-linked inventory_events remain durable.
ALTER TABLE ticket_inventory_recordings
  DROP CONSTRAINT ticket_inventory_recordings_ticket_id_fkey;
COMMENT ON COLUMN ticket_inventory_recordings.ticket_id IS
  'Historical source Ticket UUID. NOT a live foreign key; the Ticket may have been deleted.';

ALTER TABLE orders ALTER COLUMN source_build_id DROP NOT NULL;
ALTER TABLE orders DROP CONSTRAINT orders_source_build_id_fkey;
ALTER TABLE orders ADD CONSTRAINT orders_source_build_id_fkey
  FOREIGN KEY (source_build_id) REFERENCES builds(id) ON DELETE SET NULL;

ALTER TABLE order_plan_operations ALTER COLUMN build_id DROP NOT NULL;
ALTER TABLE order_plan_operations DROP CONSTRAINT order_plan_operations_build_id_fkey;
ALTER TABLE order_plan_operations ADD CONSTRAINT order_plan_operations_build_id_fkey
  FOREIGN KEY (build_id) REFERENCES builds(id) ON DELETE SET NULL;

ALTER TABLE order_requirements DROP CONSTRAINT order_requirements_source_build_id_fkey;
ALTER TABLE order_requirements ADD CONSTRAINT order_requirements_source_build_id_fkey
  FOREIGN KEY (source_build_id) REFERENCES builds(id) ON DELETE SET NULL;

ALTER TABLE tickets DROP CONSTRAINT tickets_source_build_id_fkey;
ALTER TABLE tickets ADD CONSTRAINT tickets_source_build_id_fkey
  FOREIGN KEY (source_build_id) REFERENCES builds(id) ON DELETE SET NULL;

ALTER TABLE ticket_prerequisites DROP CONSTRAINT ticket_prerequisites_source_build_id_fkey;
ALTER TABLE ticket_prerequisites ADD CONSTRAINT ticket_prerequisites_source_build_id_fkey
  FOREIGN KEY (source_build_id) REFERENCES builds(id) ON DELETE SET NULL;

-- A production Ticket is a captured execution snapshot.  A live source Build
-- is required when it is created by application code, but not for the rest of
-- the Ticket's lifetime.  Acquisition/generic Tickets still may never point at
-- a production Build.
ALTER TABLE tickets DROP CONSTRAINT tickets_source_build_matches_kind;
ALTER TABLE tickets ADD CONSTRAINT tickets_source_build_matches_kind CHECK (
  (kind IN ('acquisition', 'generic') AND source_build_id IS NULL)
  OR kind IN ('manufacturing', 'reaction')
);

-- Frozen Epic economics may be the final owner of a price snapshot after its
-- source Build is deleted.  Repository teardown removes only unreferenced
-- Build-only snapshots and Epic deletion removes its own retained snapshot.
ALTER TABLE price_snapshots ALTER COLUMN build_id DROP NOT NULL;
ALTER TABLE price_snapshots DROP CONSTRAINT price_snapshots_build_id_fkey;
ALTER TABLE price_snapshots ADD CONSTRAINT price_snapshots_build_id_fkey
  FOREIGN KEY (build_id) REFERENCES builds(id) ON DELETE SET NULL;
