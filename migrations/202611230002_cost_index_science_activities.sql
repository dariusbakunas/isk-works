-- ESI reports a cost index per industry activity. Invention and copying job
-- fees use their own index, so those are now stored next to the
-- manufacturing one. A CHECK change needs no row rewrite.
ALTER TABLE industry_system_cost_index_observations
  DROP CONSTRAINT industry_system_cost_index_observations_activity_check;

ALTER TABLE industry_system_cost_index_observations
  ADD CONSTRAINT industry_system_cost_index_observations_activity_check
  CHECK (activity IN ('manufacturing', 'invention', 'copying'));

-- "Latest index for a system and activity" is now the read.
DROP INDEX industry_system_cost_index_latest_idx;
CREATE INDEX industry_system_cost_index_latest_idx
  ON industry_system_cost_index_observations (solar_system_id, activity, observed_at DESC);
