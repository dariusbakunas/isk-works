ALTER TABLE industry_facility_profiles
  ADD COLUMN job_cost_reduction_percent numeric(9, 6) NOT NULL DEFAULT 0
  CHECK (job_cost_reduction_percent BETWEEN 0 AND 99.999999);

ALTER TABLE build_facility_snapshots
  ADD COLUMN job_cost_reduction_percent numeric(9, 6) NOT NULL DEFAULT 0
  CHECK (job_cost_reduction_percent BETWEEN 0 AND 99.999999);
