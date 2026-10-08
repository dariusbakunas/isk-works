-- A build can be "linked" to a parent build: it exists to fulfill one of the
-- parent's sub-component demands (see docs/superpowers/specs/2026-08-04-linked-sub-component-builds-design.md).
-- Both columns are set together or not at all; at most one linked build
-- exists per (parent, component type) pair, so re-selecting "Build" on the
-- same component reuses the existing linked build instead of duplicating it.
ALTER TABLE builds
  ADD COLUMN parent_build_id uuid REFERENCES builds(id) ON DELETE CASCADE,
  ADD COLUMN parent_component_type_id bigint;

ALTER TABLE builds
  ADD CONSTRAINT builds_parent_link_consistent
    CHECK ((parent_build_id IS NULL) = (parent_component_type_id IS NULL));

CREATE UNIQUE INDEX builds_parent_component_unique
  ON builds (parent_build_id, parent_component_type_id)
  WHERE parent_build_id IS NOT NULL;

CREATE INDEX builds_parent_build_id_idx ON builds (parent_build_id);
