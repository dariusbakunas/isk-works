-- `PgIndustryRepository::create_plan`/`replace_plan_snapshot` capture a
-- price snapshot for a Plan with purpose 'plan_generation', a value the
-- original price_snapshots_purpose_valid constraint (scoped to the old
-- build_planning-only workflow) never allowed for.
ALTER TABLE price_snapshots DROP CONSTRAINT price_snapshots_purpose_valid;
ALTER TABLE price_snapshots
  ADD CONSTRAINT price_snapshots_purpose_valid
    CHECK (purpose IN ('build_planning', 'plan_generation'));
