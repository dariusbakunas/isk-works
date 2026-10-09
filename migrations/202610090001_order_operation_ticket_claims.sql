-- At most one *active* ticket per frozen operation of an Epic.
--
-- Create Epic used to mint a ticket for every operation up front; tickets
-- for a step can now also be created later (after a delete or cancel, or
-- because Create Epic mints only the root). Same reasoning and mechanism
-- as `order_requirement_ticket_claims` (202610080001): a partial unique
-- index can't filter on the ticket's (reversible) status, so the create
-- path claims `(order_id, occurrence_key)` here in the same transaction as
-- the ticket insert. A concurrent loser gets the winner's ticket back; a
-- claim whose ticket was canceled is re-pointed at the new ticket under a
-- row lock; deleting the ticket (or the Epic) drops the claim.
--
-- `ticket_id` is DEFERRABLE INITIALLY DEFERRED so the claim can be taken
-- before the ticket row exists.
CREATE TABLE order_operation_ticket_claims (
  order_id uuid NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  occurrence_key text NOT NULL,
  ticket_id uuid NOT NULL
    REFERENCES tickets(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED,
  claimed_at timestamptz NOT NULL,
  PRIMARY KEY (order_id, occurrence_key)
);

CREATE INDEX order_operation_ticket_claims_ticket_idx
  ON order_operation_ticket_claims (ticket_id);

-- Backfill from existing version-3 Epics: the earliest non-canceled
-- production ticket per operation holds the claim. Duplicates (if any) are
-- left untouched; operations whose tickets are all canceled get no claim.
INSERT INTO order_operation_ticket_claims (order_id, occurrence_key, ticket_id, claimed_at)
SELECT DISTINCT ON (t.order_id, t.occurrence_key)
       t.order_id, t.occurrence_key, t.id, t.created_at
  FROM tickets t
  JOIN orders o ON o.id = t.order_id
 WHERE o.planning_snapshot_version >= 3
   AND t.occurrence_key IS NOT NULL
   AND t.kind IN ('manufacturing', 'reaction')
   AND t.status <> 'canceled'
 ORDER BY t.order_id, t.occurrence_key, t.created_at, t.id;

-- The claim now carries "one active ticket per operation". The old unique
-- index counted canceled tickets too, so a canceled step could never get a
-- new ticket; keep it as a plain lookup index. (As with requirement
-- claims, restoring a canceled ticket alongside a newer one is allowed --
-- the newer one holds the claim.)
DROP INDEX tickets_order_occurrence_idx;
CREATE INDEX tickets_order_occurrence_idx
  ON tickets (order_id, occurrence_key)
  WHERE order_id IS NOT NULL AND occurrence_key IS NOT NULL;
