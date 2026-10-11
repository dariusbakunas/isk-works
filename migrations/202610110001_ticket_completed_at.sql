-- When a ticket last entered the Complete lane, so the Board can order that
-- lane by completion rather than by display id.
--
-- Set on the transition into `complete` (a repeated `complete` write keeps
-- the original time) and cleared on any transition out of it; NULL for
-- every other status.
--
-- Backfill: tickets were never stamped on completion, so an existing
-- Complete ticket takes its `updated_at`. That is exact unless the ticket
-- was edited or archived after completing, in which case it is later than
-- the real completion.
ALTER TABLE tickets ADD COLUMN completed_at timestamptz;

UPDATE tickets SET completed_at = updated_at WHERE status = 'complete';
