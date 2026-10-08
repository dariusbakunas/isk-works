-- Slice C2 hardening: a whole-tree-frozen (version-2) requirement/
-- prerequisite for a fully-covered Build/Reaction boundary (shortage 0,
-- the allocator pruned the subtree -- see `order::freeze`'s own module
-- doc) has no walked child operation to read a specific linked-build
-- identity from. `kind` is still `build`/`react` (the sourcing INTENT is
-- known and frozen), but `source_build_id` is legitimately `NULL` in that
-- one case -- unlike the pre-C2 (version-1) root-only freeze, which always
-- resolved `source_build_id` via a live `find_linked_build` lookup and so
-- never needed this.
--
-- The original 202608230004 constraint required `source_build_id IS NOT
-- NULL` whenever `kind IN ('build', 'react')`. Relax it to only enforce
-- the direction that still always holds (`buy` implies no source build);
-- a `build`/`react` row may or may not carry one now.

ALTER TABLE order_requirements
  DROP CONSTRAINT order_requirements_source_build_matches_kind,
  ADD CONSTRAINT order_requirements_source_build_matches_kind CHECK (
    kind != 'buy' OR source_build_id IS NULL
  );

ALTER TABLE ticket_prerequisites
  DROP CONSTRAINT ticket_prerequisites_source_build_matches_kind,
  ADD CONSTRAINT ticket_prerequisites_source_build_matches_kind CHECK (
    kind != 'buy' OR source_build_id IS NULL
  );
