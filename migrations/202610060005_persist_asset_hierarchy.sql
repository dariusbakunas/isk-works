-- Resolve each asset snapshot's container hierarchy once, when it completes,
-- instead of on every read. `asset_browser_current` used to run this walk as
-- a recursive CTE, which Postgres cannot filter by workspace before
-- recursing: every read walked every active snapshot in the app.
CREATE TABLE esi_asset_hierarchy (
  snapshot_id uuid NOT NULL,
  source_item_id bigint NOT NULL,
  effective_location_id bigint NOT NULL,
  effective_location_type text NOT NULL,
  parent_item_id bigint NULL,
  hierarchy_depth integer NOT NULL,
  hierarchy_state text NOT NULL
    CHECK (hierarchy_state IN ('resolved','missing_parent','cycle')),
  PRIMARY KEY (snapshot_id, source_item_id),
  FOREIGN KEY (snapshot_id, source_item_id)
    REFERENCES esi_asset_observations (snapshot_id, source_item_id) ON DELETE CASCADE
);

-- The one definition of the walk: follow `location_type='item'` parents
-- within the snapshot to a terminal location, with a cycle guard and a depth
-- cap of 32; the deepest (or cycle) row wins. Call it before a snapshot
-- becomes readable (active and complete).
CREATE FUNCTION refresh_esi_asset_hierarchy(p_snapshot_id uuid) RETURNS void
LANGUAGE sql AS $$
DELETE FROM esi_asset_hierarchy WHERE snapshot_id = p_snapshot_id;
INSERT INTO esi_asset_hierarchy (
  snapshot_id, source_item_id, effective_location_id, effective_location_type,
  parent_item_id, hierarchy_depth, hierarchy_state
)
WITH RECURSIVE walk AS (
  SELECT asset.source_item_id,
         asset.location_id AS direct_location_id,
         asset.location_type AS direct_location_type,
         asset.location_id AS current_location_id,
         asset.location_type AS current_location_type,
         ARRAY[asset.source_item_id]::bigint[] AS path,
         0 AS depth,
         false AS cycle
  FROM esi_asset_observations asset
  WHERE asset.snapshot_id = p_snapshot_id
  UNION ALL
  SELECT walk.source_item_id,
         walk.direct_location_id,
         walk.direct_location_type,
         parent.location_id,
         parent.location_type,
         walk.path || parent.source_item_id,
         walk.depth + 1,
         parent.source_item_id = ANY(walk.path)
  FROM walk
  JOIN esi_asset_observations parent
    ON parent.snapshot_id = p_snapshot_id
   AND parent.source_item_id = walk.current_location_id
  WHERE walk.current_location_type = 'item'
    AND NOT walk.cycle
    AND walk.depth < 32
)
SELECT DISTINCT ON (source_item_id)
       p_snapshot_id,
       source_item_id,
       current_location_id,
       current_location_type,
       CASE
         WHEN direct_location_type = 'item' AND direct_location_id <> current_location_id
           THEN direct_location_id
         ELSE NULL
       END,
       depth,
       CASE
         WHEN cycle THEN 'cycle'
         WHEN current_location_type = 'item' THEN 'missing_parent'
         ELSE 'resolved'
       END
FROM walk
ORDER BY source_item_id, cycle DESC, depth DESC;
$$;

-- Only active complete snapshots are ever read; superseded ones never become
-- active again.
SELECT refresh_esi_asset_hierarchy(id)
FROM esi_asset_snapshots
WHERE active AND status = 'complete';

CREATE OR REPLACE VIEW asset_browser_current AS
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
       asset.location_id AS direct_location_id,
       asset.location_type AS direct_location_type,
       asset.location_flag,
       asset.is_singleton,
       asset.is_blueprint_copy,
       hierarchy.effective_location_id,
       hierarchy.effective_location_type,
       hierarchy.parent_item_id,
       hierarchy.hierarchy_depth,
       hierarchy.hierarchy_state
FROM eve_connections connection
JOIN esi_asset_snapshots snapshot
  ON snapshot.connection_id = connection.id
 AND snapshot.active
 AND snapshot.status = 'complete'
JOIN esi_asset_observations asset ON asset.snapshot_id = snapshot.id
JOIN esi_asset_hierarchy hierarchy
  ON hierarchy.snapshot_id = asset.snapshot_id
 AND hierarchy.source_item_id = asset.source_item_id;
