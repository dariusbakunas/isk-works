-- Rig applicability is now resolved from the active SDE on every facility
-- profile load and never persisted. A stored value went stale whenever the
-- SDE was re-imported with different rig target filters (e.g. a profile
-- saved against an import predating the multi-filter fix kept a Hybrid-only
-- coverage for the all-reactions L-Set Reactor Efficiency rigs), and only a
-- manual re-save could refresh it.

ALTER TABLE industry_facility_profile_rigs DROP COLUMN applicability;
