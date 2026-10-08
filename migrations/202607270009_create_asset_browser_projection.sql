CREATE INDEX esi_asset_observations_snapshot_item_location_idx
  ON esi_asset_observations (snapshot_id, source_item_id, location_id);

CREATE INDEX esi_asset_snapshots_active_connection_observed_idx
  ON esi_asset_snapshots (connection_id, observed_at DESC)
  WHERE active AND status = 'complete';

CREATE VIEW asset_browser_current AS
WITH RECURSIVE current_assets AS (
  SELECT connection.workspace_id,
         connection.owner_id,
         connection.id AS connection_id,
         connection.eve_character_id,
         connection.character_name,
         snapshot.id AS snapshot_id,
         snapshot.observed_at,
         asset.source_item_id,
         asset.type_id,
         asset.quantity,
         asset.location_id,
         asset.location_type,
         asset.location_flag,
         asset.is_singleton,
         asset.is_blueprint_copy
  FROM eve_connections connection
  JOIN esi_asset_snapshots snapshot
    ON snapshot.connection_id=connection.id
   AND snapshot.active
   AND snapshot.status='complete'
  JOIN esi_asset_observations asset ON asset.snapshot_id=snapshot.id
),
walk AS (
  SELECT asset.workspace_id,
         asset.owner_id,
         asset.connection_id,
         asset.eve_character_id,
         asset.character_name,
         asset.snapshot_id,
         asset.observed_at,
         asset.source_item_id,
         asset.type_id,
         asset.quantity,
         asset.location_id AS direct_location_id,
         asset.location_type AS direct_location_type,
         asset.location_flag,
         asset.is_singleton,
         asset.is_blueprint_copy,
         asset.location_id AS current_location_id,
         asset.location_type AS current_location_type,
         ARRAY[asset.source_item_id]::bigint[] AS path,
         0 AS depth,
         false AS cycle
  FROM current_assets asset
  UNION ALL
  SELECT walk.workspace_id,
         walk.owner_id,
         walk.connection_id,
         walk.eve_character_id,
         walk.character_name,
         walk.snapshot_id,
         walk.observed_at,
         walk.source_item_id,
         walk.type_id,
         walk.quantity,
         walk.direct_location_id,
         walk.direct_location_type,
         walk.location_flag,
         walk.is_singleton,
         walk.is_blueprint_copy,
         parent.location_id,
         parent.location_type,
         walk.path || parent.source_item_id,
         walk.depth + 1,
         parent.source_item_id = ANY(walk.path)
  FROM walk
  JOIN current_assets parent
    ON parent.snapshot_id=walk.snapshot_id
   AND parent.source_item_id=walk.current_location_id
  WHERE walk.current_location_type='item'
    AND NOT walk.cycle
    AND walk.depth < 32
),
terminal AS (
  SELECT DISTINCT ON (workspace_id,connection_id,source_item_id)
         *,
         CASE
           WHEN cycle THEN 'cycle'
           WHEN current_location_type='item' THEN 'missing_parent'
           ELSE 'resolved'
         END AS hierarchy_state
  FROM walk
  ORDER BY workspace_id,connection_id,source_item_id,cycle DESC,depth DESC
)
SELECT workspace_id,
       owner_id,
       connection_id,
       eve_character_id,
       character_name,
       snapshot_id,
       observed_at,
       source_item_id,
       type_id,
       quantity,
       direct_location_id,
       direct_location_type,
       location_flag,
       is_singleton,
       is_blueprint_copy,
       current_location_id AS effective_location_id,
       current_location_type AS effective_location_type,
       CASE WHEN direct_location_type='item' THEN direct_location_id ELSE NULL END AS parent_item_id,
       depth AS hierarchy_depth,
       hierarchy_state
FROM terminal;
