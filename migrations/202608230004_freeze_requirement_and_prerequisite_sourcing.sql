-- Freezes the Buy/Build/React sourcing decision (and, for Build/React, the
-- specific linked Build selected) on order_requirements/ticket_prerequisites
-- at creation time, so a later ticket-creation action reads a committed
-- decision instead of re-resolving it against the current, possibly-changed
-- Build graph. See docs/superpowers/specs/2026-08-21-build-order-board-design.md.

ALTER TABLE order_requirements
  ADD COLUMN source_build_id uuid REFERENCES builds(id) ON DELETE RESTRICT,
  ADD CONSTRAINT order_requirements_source_build_matches_kind CHECK (
    (kind = 'buy' AND source_build_id IS NULL)
    OR (kind IN ('build', 'react') AND source_build_id IS NOT NULL)
  );

-- ticket_prerequisites has no `kind` column yet (unlike order_requirements) --
-- today it can't distinguish Buy/Build/React at all, which is also why no
-- recursive "create a ticket for this prerequisite" action exists yet.
ALTER TABLE ticket_prerequisites
  ADD COLUMN kind text NOT NULL CHECK (kind IN ('buy', 'build', 'react')),
  ADD COLUMN source_build_id uuid REFERENCES builds(id) ON DELETE RESTRICT,
  ADD CONSTRAINT ticket_prerequisites_source_build_matches_kind CHECK (
    (kind = 'buy' AND source_build_id IS NULL)
    OR (kind IN ('build', 'react') AND source_build_id IS NOT NULL)
  );
