-- At most one *active* ticket minted for an Order requirement.
--
-- `order_requirement_fulfillments` is deliberately many-to-many and keeps
-- the link rows of canceled tickets as history (see
-- 202608230003_order_ticket_fulfillments_and_allocations.sql), so a plain
-- UNIQUE (order_requirement_id) on it would break both demand splitting via
-- `link_ticket_to_requirement` and re-creating a ticket after canceling one.
-- A partial index can't filter on the *ticket's* status either (another
-- table, and statuses are reversible).
--
-- Instead, the "create ticket for requirement" path claims the requirement
-- here, in the same transaction that inserts the ticket and its
-- fulfillment link. The primary key is the uniqueness guarantee: two
-- concurrent creates for one requirement serialize on it, and the loser
-- gets the winner's ticket back instead of minting a second one. A claim
-- whose ticket has since been canceled is re-pointed at the new ticket
-- under a row lock; deleting the ticket (or the requirement) drops the
-- claim.
--
-- `ticket_id` is DEFERRABLE INITIALLY DEFERRED so the claim can be taken
-- *before* the ticket row exists (nothing is inserted into `tickets` -- no
-- display id consumed -- when the claim is already held).
CREATE TABLE order_requirement_ticket_claims (
  order_requirement_id uuid PRIMARY KEY
    REFERENCES order_requirements(id) ON DELETE CASCADE,
  ticket_id uuid NOT NULL
    REFERENCES tickets(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED,
  claimed_at timestamptz NOT NULL
);

CREATE INDEX order_requirement_ticket_claims_ticket_idx
  ON order_requirement_ticket_claims (ticket_id);

-- Backfill from existing links. Pre-existing duplicates (two active tickets
-- linked to one requirement, the very race this closes) are left untouched
-- -- no ticket or link is deleted -- and the claim deterministically goes
-- to the earliest active link (linked_at, then id), the same ticket the
-- API's idempotent path already returns. Requirements whose links are all
-- canceled get no claim (they are free for a new ticket).
INSERT INTO order_requirement_ticket_claims (order_requirement_id, ticket_id, claimed_at)
SELECT DISTINCT ON (f.order_requirement_id)
       f.order_requirement_id, f.ticket_id, f.linked_at
  FROM order_requirement_fulfillments f
  JOIN tickets t ON t.id = f.ticket_id
 WHERE t.status <> 'canceled'
 ORDER BY f.order_requirement_id, f.linked_at, f.id;
