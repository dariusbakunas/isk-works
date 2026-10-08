-- Explicit organizational Epic membership on Ticket -- backend/storage
-- still says Order (the full Order -> Epic rename is a later slice).
-- Answers *only* "which Order/Epic organizationally contains this
-- Ticket?", never
-- inventory/execution/provenance -- `order_requirement_fulfillments`
-- keeps meaning "this ticket contributes to/fulfills this frozen
-- requirement" (a different, still-live concept: `link_ticket_to_requirement`
-- can fulfill another Order's requirement with a ticket that belongs
-- elsewhere, or nowhere).
--
-- Nullable by design: a standalone ticket (no Epic) is a first-class,
-- valid state -- the foundation for the upcoming manual "Create Ticket"
-- flow. `ON DELETE SET NULL` because Epic membership is organizational --
-- deleting/removing an Order must never imply deleting a Ticket's own
-- execution/accounting history. (Orders currently have no hard-delete
-- route at all -- only archive/restore -- so this is chosen for future
-- safety, not because it is reachable today.)
ALTER TABLE tickets ADD COLUMN order_id uuid REFERENCES orders(id) ON DELETE SET NULL;

-- Matches the repository's actual query pattern
-- (`list_tickets_for_order`: `WHERE workspace_id = ? AND order_id = ?`).
CREATE INDEX tickets_workspace_order_idx ON tickets (workspace_id, order_id);

-- ─── Backfill (unambiguous cases only -- NULL is a legitimate, expected
-- outcome for the rest; see the migration/backfill report) ────────────

-- 1. Requirement-linked tickets: a ticket reached through exactly one
-- Order's requirement fulfillment(s) unambiguously belongs to that Order.
-- Canceled fulfillment links still count -- they record which Order this
-- ticket was originally *created for*, which is exactly what `order_id`
-- means; only when a ticket has fulfillment links pointing at more than
-- one *distinct* Order (the "shared/reused ticket" mechanism
-- `link_ticket_to_requirement` allows) is membership genuinely ambiguous,
-- and it is deliberately left NULL rather than guessed.
-- `min()`/`max()` have no built-in aggregate for `uuid` in PostgreSQL --
-- cast through `text` to pick (arbitrarily, but only ever among a set of
-- exactly one distinct value, so it doesn't matter which) a single
-- representative `order_id`.
UPDATE tickets t
SET order_id = unambiguous.order_id
FROM (
  SELECT
    orf.ticket_id,
    min(oreq.order_id::text)::uuid AS order_id,
    count(DISTINCT oreq.order_id) AS distinct_orders
  FROM order_requirement_fulfillments orf
  JOIN order_requirements oreq ON oreq.id = orf.order_requirement_id
  GROUP BY orf.ticket_id
) unambiguous
WHERE t.id = unambiguous.ticket_id
  AND unambiguous.distinct_orders = 1;

-- 2. Root Manufacturing/Reaction tickets with no requirement-fulfillment
-- link at all (the root ticket is never itself linked through
-- order_requirement_fulfillments -- it fulfills nothing, it *is* the root
-- production): backfilled only when exactly one Order shares this
-- ticket's `source_build_id`. Two Orders created from the same Build
-- before this slice's root-ticket-per-Epic fix (see the accompanying
-- code change) may have shared one root ticket -- that history is
-- genuinely ambiguous and is intentionally left NULL rather than
-- arbitrarily assigned to one of the candidate Orders.
UPDATE tickets t
SET order_id = unambiguous.order_id
FROM (
  SELECT
    ticket.id AS ticket_id,
    min(candidate_order.id::text)::uuid AS order_id,
    count(DISTINCT candidate_order.id) AS distinct_orders
  FROM tickets ticket
  JOIN orders candidate_order ON candidate_order.source_build_id = ticket.source_build_id
  WHERE ticket.kind IN ('manufacturing', 'reaction')
    AND ticket.order_id IS NULL
  GROUP BY ticket.id
) unambiguous
WHERE t.id = unambiguous.ticket_id
  AND unambiguous.distinct_orders = 1;

-- Every other ticket (standalone Acquisition tickets never linked to a
-- requirement, non-root Manufacturing/Reaction tickets with no
-- fulfillment link, and every genuinely ambiguous case above) keeps
-- `order_id IS NULL` -- a legitimate, expected state, not a migration
-- gap.
