ALTER TABLE blueprint_observations
  ADD COLUMN is_current boolean NOT NULL DEFAULT false;

WITH latest_observations AS (
  SELECT DISTINCT ON (workspace_id, owner_id, eve_item_id) id
  FROM blueprint_observations
  ORDER BY workspace_id, owner_id, eve_item_id, observed_at DESC, imported_at DESC
)
UPDATE blueprint_observations
SET is_current = true
WHERE id IN (SELECT id FROM latest_observations);

CREATE INDEX blueprint_observations_current_compatible_idx
  ON blueprint_observations (workspace_id, owner_id, blueprint_type_id)
  WHERE is_current;
