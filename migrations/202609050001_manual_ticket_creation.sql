-- Canonical manual Ticket creation: a `generic` TicketKind (a standalone
-- organizational work item with no execution/recording contract) plus the
-- common fields every manually created Ticket needs regardless of kind --
-- free-form notes and an optional assignee. Epic membership already exists
-- (`tickets.order_id`, prior slice); this adds the rest.
--
-- `captured_name` already serves as a Ticket's human-readable title (it's
-- what every card/inspector already displays as the primary line) --
-- reused as-is rather than adding a second, overlapping title column. It
-- gains a non-empty check here, matching `orders.display_name`'s existing
-- convention, now that it's presented to users as an editable field rather
-- than only ever system-captured from an item/product name.

ALTER TABLE tickets
  ADD COLUMN notes text NOT NULL DEFAULT '',
  -- Disconnecting a character is a soft status transition
  -- (`eve_connections.status = 'disconnected'`), never a row delete -- but
  -- `ON DELETE SET NULL` is still the correct FK semantic for the same
  -- future-proofing reason as `tickets.order_id`: assignment is
  -- organizational metadata, a Ticket's own history must never depend on
  -- the assignee row surviving.
  ADD COLUMN assignee_character_id uuid REFERENCES eve_connections(id) ON DELETE SET NULL;

ALTER TABLE tickets
  ADD CONSTRAINT tickets_captured_name_not_empty CHECK (length(btrim(captured_name)) > 0);

-- Generic has no item/product and no output quantity -- both become
-- optional, gated by kind alongside the existing `source_build_id` rule.
ALTER TABLE tickets ALTER COLUMN type_id DROP NOT NULL;
ALTER TABLE tickets ALTER COLUMN quantity DROP NOT NULL;

ALTER TABLE tickets DROP CONSTRAINT tickets_type_id_check;
ALTER TABLE tickets ADD CONSTRAINT tickets_type_id_check CHECK (type_id IS NULL OR type_id > 0);
ALTER TABLE tickets DROP CONSTRAINT tickets_quantity_check;
ALTER TABLE tickets ADD CONSTRAINT tickets_quantity_check CHECK (quantity IS NULL OR quantity > 0);

ALTER TABLE tickets DROP CONSTRAINT tickets_kind_check;
ALTER TABLE tickets ADD CONSTRAINT tickets_kind_check
  CHECK (kind IN ('acquisition', 'manufacturing', 'reaction', 'generic'));

ALTER TABLE tickets DROP CONSTRAINT tickets_source_build_matches_kind;
ALTER TABLE tickets ADD CONSTRAINT tickets_source_build_matches_kind CHECK (
  (kind IN ('acquisition', 'generic') AND source_build_id IS NULL)
  OR (kind IN ('manufacturing', 'reaction') AND source_build_id IS NOT NULL)
);

ALTER TABLE tickets ADD CONSTRAINT tickets_type_id_required_unless_generic CHECK (
  (kind = 'generic' AND type_id IS NULL) OR (kind != 'generic' AND type_id IS NOT NULL)
);
ALTER TABLE tickets ADD CONSTRAINT tickets_quantity_required_unless_generic CHECK (
  (kind = 'generic' AND quantity IS NULL) OR (kind != 'generic' AND quantity IS NOT NULL)
);
