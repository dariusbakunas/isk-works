-- Separate Ticket workflow status from dependency/blocker state.
--
-- Before this slice, `tickets.status` mixed two concerns: a user-controlled
-- organizational lane (`in_progress`/`complete`/`canceled`) and a
-- system-derived dependency signal (`blocked` = has an unmet prerequisite,
-- `ready` = all prerequisites satisfied, written by
-- `order::initial_ticket_status` / `derive_ticket_blocked_ready` /
-- `recheck_dependent_tickets`). Dependency state is now purely derived at
-- read time (`order::derive_ticket_blockers`, returned as `blockedBy`);
-- workflow status is only ever what the user chose.
--
-- The four-state workflow vocabulary is `todo | in_progress | complete |
-- canceled`. Both legacy `blocked` and `ready` collapse to `todo` -- the
-- new "unstarted" lane. This is an ORGANIZATIONAL rewrite only: it is not
-- an attempt to infer where an old `blocked` ticket "should" land based on
-- its current prerequisites (that information now lives entirely in the
-- derived blocker list). It posts no inventory events, and does not touch
-- prerequisites, fulfillments, recordings, Epic membership, assignees, or
-- Build relationships.
-- Drop the old constraint first: it still forbids `todo`, so the collapse
-- UPDATE below would violate it on any database that actually has legacy
-- `blocked`/`ready` rows (CI misses this because its `tickets` table is
-- empty and the UPDATE matches nothing).
ALTER TABLE tickets DROP CONSTRAINT tickets_status_check;

UPDATE tickets
SET status = 'todo',
    updated_at = now()
WHERE status IN ('blocked', 'ready');

ALTER TABLE tickets ADD CONSTRAINT tickets_status_check
  CHECK (status IN ('todo', 'in_progress', 'complete', 'canceled'));
