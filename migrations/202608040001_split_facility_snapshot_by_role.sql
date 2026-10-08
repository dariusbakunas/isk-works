-- A build_plan_id could only ever have one facility snapshot, because a
-- Build had exactly one facility slot. Dual facility selection lets a plan
-- carry both a manufacturing and a reaction facility snapshot at once (one
-- for the root job, keyed by which role it actually is), so the identity
-- moves from build_plan_id alone to (build_plan_id, captured_role).
ALTER TABLE build_facility_snapshots
  DROP CONSTRAINT build_facility_snapshots_build_plan_id_key;

ALTER TABLE build_facility_snapshots
  ADD CONSTRAINT build_facility_snapshots_build_plan_id_captured_role_key
    UNIQUE (build_plan_id, captured_role);
