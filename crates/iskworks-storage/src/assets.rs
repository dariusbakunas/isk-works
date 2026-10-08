use chrono::{DateTime, Utc};
use iskworks_core::{
    build_asset_hierarchy, AssetBlueprintSummary, AssetBrowserFacets, AssetBrowserFilters,
    AssetBrowserItem, AssetBrowserQuery, AssetBrowserSummary, AssetFilterOption, AssetLocationPage,
    AssetLocationSummary, AssetReconciliationSummary, AssetSyncState, FlatAssetCursor,
    FlatAssetPage, FlatAssetRow, InventoryError, ValidatedFlatAssetQuery, WorkspaceId,
};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct PgAssetBrowserRepository {
    pool: PgPool,
}

impl PgAssetBrowserRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn summary(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<AssetBrowserSummary, InventoryError> {
        let counts = sqlx::query_as::<_, SummaryRow>(
            r#"SELECT COUNT(DISTINCT effective_location_id)::bigint AS location_count,
                      COUNT(DISTINCT connection_id)::bigint AS character_count,
                      COUNT(*)::bigint AS stack_count,
                      COALESCE(SUM(quantity),0)::bigint AS total_quantity,
                      COALESCE(SUM(asset.quantity * type.packaged_volume_m3),0)::numeric(30,4)::text
                        AS total_packaged_volume,
                      MAX(observed_at) AS latest_observed_at,
                      COUNT(DISTINCT asset.effective_location_id)
                        FILTER (
                          WHERE asset.hierarchy_state <> 'resolved'
                            AND location.location_name IS NULL
                        )::bigint
                        AS unresolved_location_count
               FROM asset_browser_current asset
               LEFT JOIN sde_imports import ON import.active
               LEFT JOIN sde_types type
                 ON type.import_id=import.id AND type.type_id=asset.type_id
               LEFT JOIN market_location_names location
                 ON location.workspace_id=asset.workspace_id
                AND location.location_id=asset.effective_location_id
               WHERE asset.workspace_id=$1"#,
        )
        .bind(workspace_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(map_error)?;
        let sync_states = sqlx::query_as::<_, SyncStateRow>(
            r#"SELECT connection.id AS connection_id, connection.character_name,
                      connection.status AS connection_status,
                      snapshot.status AS snapshot_status,
                      snapshot.observed_at, snapshot.row_count
               FROM eve_connections connection
               LEFT JOIN LATERAL (
                 SELECT status,observed_at,row_count
                 FROM esi_asset_snapshots
                 WHERE connection_id=connection.id
                 ORDER BY observed_at DESC
                 LIMIT 1
               ) snapshot ON true
               WHERE connection.workspace_id=$1
               ORDER BY connection.character_name,connection.id"#,
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(SyncStateRow::into_domain)
        .collect();
        Ok(AssetBrowserSummary {
            location_count: counts.location_count,
            character_count: counts.character_count,
            stack_count: counts.stack_count,
            total_quantity: counts.total_quantity,
            total_packaged_volume: counts.total_packaged_volume,
            latest_observed_at: counts.latest_observed_at,
            unresolved_location_count: counts.unresolved_location_count,
            sync_states,
        })
    }

    pub async fn locations(
        &self,
        workspace_id: WorkspaceId,
        query: &AssetBrowserQuery,
    ) -> Result<Vec<AssetLocationSummary>, InventoryError> {
        let search = format!("%{}%", query.query.trim());
        let rows = sqlx::query_as::<_, LocationRow>(
            r#"WITH current_assets AS MATERIALIZED (
                 SELECT *
                 FROM asset_browser_current
                 WHERE workspace_id=$1
               ),
               enriched AS (
                 SELECT asset.*,
                        owner.display_name AS owner_name,
                        type.name_en AS type_name,
                        type.group_id,
                        type.group_name_en,
                        parent_type.name_en AS parent_type_name,
                        COALESCE(location.location_name,station.name_en,system.name_en,
                          CASE asset.effective_location_type
                            WHEN 'station' THEN 'Station '||asset.effective_location_id::text
                            WHEN 'solar_system' THEN 'Solar system '||asset.effective_location_id::text
                            WHEN 'item' THEN 'Unresolved container '||asset.effective_location_id::text
                            ELSE initcap(replace(asset.effective_location_type,'_',' '))||' '||asset.effective_location_id::text
                          END) AS location_name,
                        COALESCE(location.structure_type_id,station.station_type_id)
                          AS location_type_id,
                        system.name_en AS solar_system_name,
                        blueprint.eve_item_id IS NOT NULL AS is_blueprint,
                        child.present IS NOT NULL AS is_container
                 FROM current_assets asset
                 JOIN owners owner ON owner.id=asset.owner_id
                 LEFT JOIN sde_imports import ON import.active
                 LEFT JOIN sde_types type ON type.import_id=import.id AND type.type_id=asset.type_id
                 -- Look parent/child relationships up against the indexed base table
                 -- (snapshot_id,source_item_id pkey; snapshot_id,location_id idx) rather
                 -- than self-joining the current_assets CTE, whose row count the planner
                 -- can't estimate accurately — that mis-estimate made it pick a nested
                 -- loop over a full CTE scan per row, which is quadratic in asset count.
                 LEFT JOIN esi_asset_observations parent_obs
                   ON parent_obs.snapshot_id=asset.snapshot_id
                  AND parent_obs.source_item_id=asset.parent_item_id
                 LEFT JOIN sde_types parent_type
                   ON parent_type.import_id=import.id AND parent_type.type_id=parent_obs.type_id
                 LEFT JOIN LATERAL (
                   SELECT 1 AS present
                   FROM esi_asset_observations child_obs
                   WHERE child_obs.snapshot_id=asset.snapshot_id
                     AND child_obs.location_type='item'
                     AND child_obs.location_id=asset.source_item_id
                   LIMIT 1
                 ) child ON true
                 LEFT JOIN market_location_names location
                  ON location.workspace_id=asset.workspace_id
                  AND location.location_id=asset.effective_location_id
                 LEFT JOIN sde_npc_stations station
                   ON station.import_id=import.id
                  AND station.station_id=asset.effective_location_id
                 LEFT JOIN sde_solar_systems system
                   ON system.import_id=import.id
                  AND system.solar_system_id=COALESCE(location.solar_system_id,
                    station.solar_system_id,
                    CASE WHEN asset.effective_location_type='solar_system'
                         THEN asset.effective_location_id END)
                 LEFT JOIN LATERAL (
                   SELECT observation.eve_item_id
                   FROM blueprint_observations observation
                   WHERE observation.workspace_id=asset.workspace_id
                     AND observation.owner_id=asset.owner_id
                     AND observation.eve_item_id=asset.source_item_id
                     AND observation.blueprint_type_id=asset.type_id
                   ORDER BY observation.observed_at DESC
                   LIMIT 1
                 ) blueprint ON true
               ),
               filtered AS (
                 SELECT *,
                   CASE
                     WHEN is_blueprint THEN 'blueprint'
                     WHEN is_container THEN 'container'
                     WHEN lower(COALESCE(group_name_en,'')) LIKE '%mineral%' THEN 'material'
                     WHEN lower(COALESCE(group_name_en,'')) LIKE '%ship%'
                       OR lower(COALESCE(group_name_en,'')) IN ('frigate','destroyer','cruiser','battlecruiser','battleship')
                       THEN 'ship'
                     ELSE 'other'
                   END AS asset_kind
                 FROM enriched
               )
               SELECT effective_location_id AS location_id,
                      MIN(location_name) AS location_name,
                      MIN(effective_location_type) AS location_type,
                      MIN(location_type_id) AS location_type_id,
                      MIN(solar_system_name) AS solar_system_name,
                      COUNT(*)::bigint AS stack_count,
                      SUM(quantity)::bigint AS total_quantity,
                      COUNT(DISTINCT owner_id)::bigint AS owner_count,
                      COUNT(DISTINCT connection_id)::bigint AS character_count,
                      COUNT(*) FILTER (WHERE is_container)::bigint AS container_count,
                      COUNT(*) FILTER (WHERE is_blueprint)::bigint AS blueprint_count,
                      MAX(observed_at) AS latest_observed_at,
                      MIN(observed_at) AS oldest_observed_at,
                      COUNT(*) FILTER (
                        WHERE hierarchy_state <> 'resolved'
                          AND location_name LIKE 'Unresolved container %'
                      )::bigint
                        AS unresolved_child_count,
                      COUNT(*)::bigint AS match_count
               FROM filtered
               WHERE ($2='' OR type_name ILIKE $3 OR location_name ILIKE $3
                      OR character_name ILIKE $3 OR owner_name ILIKE $3
                      OR COALESCE(group_name_en,'') ILIKE $3
                      OR COALESCE(parent_type_name,'') ILIKE $3)
                 AND ($4::uuid IS NULL OR owner_id=$4)
                 AND ($5::bigint IS NULL OR eve_character_id=$5)
                 AND ($6::bigint IS NULL OR group_id=$6)
                 AND ($7::text IS NULL OR asset_kind=$7)
               GROUP BY effective_location_id
               ORDER BY MIN(location_name),effective_location_id
               LIMIT 100"#,
        )
        .bind(workspace_id.0)
        .bind(query.query.trim())
        .bind(search)
        .bind(query.owner_id)
        .bind(query.character_id)
        .bind(query.group_id)
        .bind(query.asset_kind.as_deref())
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?;
        Ok(rows.into_iter().map(LocationRow::into_domain).collect())
    }

    pub async fn filters(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<AssetBrowserFilters, InventoryError> {
        let owners = sqlx::query_as::<_, FilterRow>(
            r#"SELECT asset.owner_id::text AS value,owner.display_name AS label,
                      COUNT(*)::bigint AS count
               FROM asset_browser_current asset
               JOIN owners owner ON owner.id=asset.owner_id
               WHERE asset.workspace_id=$1
               GROUP BY asset.owner_id,owner.display_name
               ORDER BY owner.display_name"#,
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?;
        let characters = sqlx::query_as::<_, FilterRow>(
            r#"SELECT eve_character_id::text AS value,character_name AS label,
                      COUNT(*)::bigint AS count
               FROM asset_browser_current
               WHERE workspace_id=$1
               GROUP BY eve_character_id,character_name
               ORDER BY character_name"#,
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?;
        let groups = self.group_facets(workspace_id).await?;
        let locations = self.location_facets(workspace_id).await?;
        Ok(AssetBrowserFilters {
            owners: owners.into_iter().map(FilterRow::into_domain).collect(),
            characters: characters.into_iter().map(FilterRow::into_domain).collect(),
            groups,
            locations,
        })
    }

    async fn group_facets(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<AssetFilterOption>, InventoryError> {
        Ok(sqlx::query_as::<_, FilterRow>(
            r#"SELECT type.group_id::text AS value,type.group_name_en AS label,
                      COUNT(*)::bigint AS count
               FROM asset_browser_current asset
               JOIN sde_imports import ON import.active
               JOIN sde_types type ON type.import_id=import.id AND type.type_id=asset.type_id
               WHERE asset.workspace_id=$1 AND type.group_id IS NOT NULL
               GROUP BY type.group_id,type.group_name_en
               ORDER BY type.group_name_en"#,
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(FilterRow::into_domain)
        .collect())
    }

    /// Location labels and stack counts, named and ordered like
    /// `locations()` (same 100-location cap) without its per-row enrichment.
    async fn location_facets(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<AssetFilterOption>, InventoryError> {
        Ok(sqlx::query_as::<_, FilterRow>(
            r#"SELECT asset.effective_location_id::text AS value,
                      MIN(COALESCE(location.location_name,station.name_en,system.name_en,
                        CASE asset.effective_location_type
                          WHEN 'station' THEN 'Station '||asset.effective_location_id::text
                          WHEN 'solar_system' THEN 'Solar system '||asset.effective_location_id::text
                          WHEN 'item' THEN 'Unresolved container '||asset.effective_location_id::text
                          ELSE initcap(replace(asset.effective_location_type,'_',' '))||' '||asset.effective_location_id::text
                        END)) AS label,
                      COUNT(*)::bigint AS count
               FROM asset_browser_current asset
               LEFT JOIN sde_imports import ON import.active
               LEFT JOIN market_location_names location
                 ON location.workspace_id=asset.workspace_id
                AND location.location_id=asset.effective_location_id
               LEFT JOIN sde_npc_stations station
                 ON station.import_id=import.id
                AND station.station_id=asset.effective_location_id
               LEFT JOIN sde_solar_systems system
                 ON system.import_id=import.id
                AND system.solar_system_id=COALESCE(location.solar_system_id,
                  station.solar_system_id,
                  CASE WHEN asset.effective_location_type='solar_system'
                       THEN asset.effective_location_id END)
               WHERE asset.workspace_id=$1
               GROUP BY asset.effective_location_id
               ORDER BY label,asset.effective_location_id
               LIMIT 100"#,
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(FilterRow::into_domain)
        .collect())
    }

    async fn connection_facets(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<AssetFilterOption>, InventoryError> {
        Ok(sqlx::query_as::<_, FilterRow>(
            r#"SELECT connection_id::text AS value,character_name AS label,
                      COUNT(*)::bigint AS count
               FROM asset_browser_current
               WHERE workspace_id=$1
               GROUP BY connection_id,character_name
               ORDER BY character_name,connection_id"#,
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(FilterRow::into_domain)
        .collect())
    }

    pub async fn location(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
        query: &AssetBrowserQuery,
    ) -> Result<AssetLocationPage, InventoryError> {
        let locations = self.locations(workspace_id, query).await?;
        let location = locations
            .into_iter()
            .find(|location| location.location_id == location_id)
            .ok_or(InventoryError::ItemNotFound)?;
        let limit = query.limit.unwrap_or(200).clamp(1, 500);
        let offset = query.offset.unwrap_or(0);
        let search = format!("%{}%", query.query.trim());
        let rows = sqlx::query_as::<_, ItemRow>(
            r#"WITH current_assets AS MATERIALIZED (
                 SELECT *
                 FROM asset_browser_current
                 WHERE workspace_id=$1
               ),
               observed_totals AS (
                 SELECT owner_id,type_id,SUM(quantity)::bigint AS quantity
                 FROM current_assets
                 GROUP BY owner_id,type_id
               )
               SELECT asset.source_item_id AS eve_item_id,asset.type_id,
                      COALESCE(type.name_en,'Unknown EVE type '||asset.type_id::text) AS type_name,
                      asset.quantity,asset.owner_id,owner.display_name AS owner_name,
                      asset.connection_id,asset.eve_character_id AS character_id,
                      asset.character_name,asset.direct_location_id AS location_id,
                      asset.location_flag,asset.parent_item_id,type.group_id,
                      type.group_name_en AS group_name,asset.observed_at,
                      child.present IS NOT NULL AS is_container,
                      blueprint.blueprint_kind,
                      blueprint.material_efficiency::integer AS material_efficiency,
                      blueprint.time_efficiency::integer AS time_efficiency,
                      blueprint.licensed_runs::integer AS licensed_runs,
                      blueprint.observed_at AS blueprint_observed_at,
                      totals.quantity AS observed_owner_type_quantity,
                      COALESCE(balance.quantity,0)::bigint AS accounted_owner_type_quantity,
                      COUNT(*) OVER()::bigint AS total
               FROM current_assets asset
               JOIN owners owner ON owner.id=asset.owner_id
               LEFT JOIN sde_imports import ON import.active
               LEFT JOIN sde_types type ON type.import_id=import.id AND type.type_id=asset.type_id
               -- See locations(): look these up against the indexed base table instead
               -- of self-joining current_assets/asset_browser_current.
               LEFT JOIN esi_asset_observations parent_obs
                 ON parent_obs.snapshot_id=asset.snapshot_id
                AND parent_obs.source_item_id=asset.parent_item_id
               LEFT JOIN sde_types parent_type
                 ON parent_type.import_id=import.id AND parent_type.type_id=parent_obs.type_id
               LEFT JOIN observed_totals totals
                 ON totals.owner_id=asset.owner_id AND totals.type_id=asset.type_id
               LEFT JOIN inventory_balances balance
                 ON balance.workspace_id=asset.workspace_id
                AND balance.owner_id=asset.owner_id AND balance.type_id=asset.type_id
               LEFT JOIN LATERAL (
                 SELECT 1 AS present
                 FROM esi_asset_observations possible_child
                 WHERE possible_child.snapshot_id=asset.snapshot_id
                   AND possible_child.location_type='item'
                   AND possible_child.location_id=asset.source_item_id
                 LIMIT 1
               ) child ON true
               LEFT JOIN LATERAL (
                 SELECT observation.blueprint_kind,observation.material_efficiency,
                        observation.time_efficiency,observation.licensed_runs,
                        observation.observed_at
                 FROM blueprint_observations observation
                 WHERE observation.workspace_id=asset.workspace_id
                   AND observation.owner_id=asset.owner_id
                   AND observation.eve_item_id=asset.source_item_id
                   AND observation.blueprint_type_id=asset.type_id
                 ORDER BY observation.observed_at DESC
                 LIMIT 1
               ) blueprint ON true
               WHERE asset.workspace_id=$1 AND asset.effective_location_id=$2
                 AND ($3='' OR type.name_en ILIKE $4 OR asset.character_name ILIKE $4
                      OR owner.display_name ILIKE $4 OR COALESCE(type.group_name_en,'') ILIKE $4
                      OR COALESCE(parent_type.name_en,'') ILIKE $4)
                 AND ($5::uuid IS NULL OR asset.owner_id=$5)
                 AND ($6::bigint IS NULL OR asset.eve_character_id=$6)
                 AND ($7::bigint IS NULL OR type.group_id=$7)
                 AND ($8::text IS NULL OR
                   CASE
                     WHEN blueprint.blueprint_kind IS NOT NULL THEN 'blueprint'
                     WHEN child.present IS NOT NULL THEN 'container'
                     WHEN lower(COALESCE(type.group_name_en,'')) LIKE '%mineral%' THEN 'material'
                     WHEN lower(COALESCE(type.group_name_en,'')) LIKE '%ship%' THEN 'ship'
                     ELSE 'other'
                   END=$8)
               ORDER BY child.present IS NULL,type.name_en,asset.source_item_id
               LIMIT $9 OFFSET $10"#,
        )
        .bind(workspace_id.0)
        .bind(location_id)
        .bind(query.query.trim())
        .bind(search)
        .bind(query.owner_id)
        .bind(query.character_id)
        .bind(query.group_id)
        .bind(query.asset_kind.as_deref())
        .bind(i64::from(limit))
        .bind(i64::from(offset))
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?;
        let total = rows.first().map_or(0, |row| row.total);
        let items = rows
            .into_iter()
            .map(ItemRow::into_domain)
            .collect::<Vec<_>>();
        let hierarchy = build_asset_hierarchy(&items);
        Ok(AssetLocationPage {
            location,
            items,
            hierarchy,
            offset,
            limit,
            total,
            has_more: i64::from(offset) + i64::from(limit) < total,
        })
    }

    pub async fn flat_assets(
        &self,
        workspace_id: WorkspaceId,
        query: &ValidatedFlatAssetQuery,
    ) -> Result<FlatAssetPage, InventoryError> {
        let offset = query
            .cursor
            .as_ref()
            .and_then(|cursor| cursor.value.parse::<i64>().ok())
            .unwrap_or(0);
        let rows = self
            .flat_item_rows(
                workspace_id,
                query,
                Some(i64::from(query.limit) + 1),
                offset,
            )
            .await?;
        let total = rows.first().map_or(0, |row| row.total);
        let has_more = rows.len() > usize::from(query.limit);
        let mut rows = rows;
        if has_more {
            rows.pop();
        }
        let next_cursor = has_more.then(|| {
            FlatAssetCursor {
                sort: query.sort,
                value: (offset + i64::from(query.limit)).to_string(),
                connection_id: rows.last().map_or(Uuid::nil(), |row| row.connection_id),
                eve_item_id: rows.last().map_or(0, |row| row.eve_item_id),
            }
            .encode()
        });
        // Summary and facets are workspace-wide, so later pages don't
        // recompute them; the client keeps the first page's.
        let (summary, facets) = if query.cursor.is_none() {
            let (summary, characters, locations, groups) = tokio::try_join!(
                self.summary(workspace_id),
                self.connection_facets(workspace_id),
                self.location_facets(workspace_id),
                self.group_facets(workspace_id),
            )?;
            let facets = AssetBrowserFacets {
                characters,
                locations,
                groups,
                ..AssetBrowserFacets::default()
            };
            (Some(summary), Some(facets))
        } else {
            (None, None)
        };
        Ok(FlatAssetPage {
            rows: rows.into_iter().map(FlatItemRow::into_domain).collect(),
            total,
            next_cursor,
            summary,
            facets,
        })
    }

    /// Every row matching the query's filters and sort, ignoring its cursor
    /// and limit, in one query (for export).
    pub async fn all_flat_assets(
        &self,
        workspace_id: WorkspaceId,
        query: &ValidatedFlatAssetQuery,
    ) -> Result<Vec<FlatAssetRow>, InventoryError> {
        Ok(self
            .flat_item_rows(workspace_id, query, None, 0)
            .await?
            .into_iter()
            .map(FlatItemRow::into_domain)
            .collect())
    }

    /// `limit` `None` binds SQL `LIMIT NULL`, i.e. no limit.
    async fn flat_item_rows(
        &self,
        workspace_id: WorkspaceId,
        query: &ValidatedFlatAssetQuery,
        limit: Option<i64>,
        offset: i64,
    ) -> Result<Vec<FlatItemRow>, InventoryError> {
        let search = format!("%{}%", query.search);
        let asset_kinds = query
            .asset_kinds
            .iter()
            .map(|value| format!("{value:?}").to_lowercase())
            .collect::<Vec<_>>();
        let blueprint_kinds = query
            .blueprint_kinds
            .iter()
            .map(|value| format!("{value:?}").to_lowercase())
            .collect::<Vec<_>>();
        let reconciliation_states = query
            .reconciliation_states
            .iter()
            .map(|value| match value {
                iskworks_core::AssetReconciliationFilter::Matched => "accounted",
                iskworks_core::AssetReconciliationFilter::Difference => "difference",
                iskworks_core::AssetReconciliationFilter::NoAccountingRecord => "unaccounted",
            })
            .collect::<Vec<_>>();
        let sort = format!("{:?}", query.sort).to_lowercase();
        let order = format!("{:?}", query.order).to_lowercase();
        sqlx::query_as::<_, FlatItemRow>(
            r#"WITH current_assets AS MATERIALIZED (
                 SELECT * FROM asset_browser_current WHERE workspace_id=$1
               ),
               observed_totals AS (
                 SELECT owner_id,type_id,SUM(quantity)::bigint AS quantity
                 FROM current_assets GROUP BY owner_id,type_id
               ),
               enriched AS (
                 SELECT asset.source_item_id AS eve_item_id,asset.type_id,
                        COALESCE(type.name_en,'Unknown EVE type '||asset.type_id::text) AS type_name,
                        asset.quantity,type.packaged_volume_m3::text AS packaged_volume,
                        (type.packaged_volume_m3 * asset.quantity)::text AS total_packaged_volume,
                        asset.owner_id,owner.display_name AS owner_name,asset.connection_id,
                        asset.eve_character_id AS character_id,asset.character_name,
                        asset.effective_location_id AS location_id,
                        COALESCE(location.location_name,station.name_en,system.name_en) AS location_name,
                        asset.location_flag,asset.parent_item_id AS container_item_id,
                        parent_type.name_en AS container_name,type.group_id,
                        type.group_name_en AS group_name,asset.observed_at,
                        child.present IS NOT NULL AS is_container,
                        blueprint.blueprint_kind,
                        blueprint.material_efficiency::integer AS material_efficiency,
                        blueprint.time_efficiency::integer AS time_efficiency,
                        blueprint.licensed_runs::integer AS licensed_runs,
                        blueprint.observed_at AS blueprint_observed_at,
                        totals.quantity AS observed_owner_type_quantity,
                        COALESCE(balance.quantity,0)::bigint AS accounted_owner_type_quantity
                 FROM current_assets asset
                 JOIN owners owner ON owner.id=asset.owner_id
                 LEFT JOIN sde_imports import ON import.active
                 LEFT JOIN sde_types type ON type.import_id=import.id AND type.type_id=asset.type_id
                 LEFT JOIN esi_asset_observations parent_obs
                   ON parent_obs.snapshot_id=asset.snapshot_id
                  AND parent_obs.source_item_id=asset.parent_item_id
                 LEFT JOIN sde_types parent_type
                   ON parent_type.import_id=import.id AND parent_type.type_id=parent_obs.type_id
                 LEFT JOIN observed_totals totals
                   ON totals.owner_id=asset.owner_id AND totals.type_id=asset.type_id
                 LEFT JOIN inventory_balances balance
                   ON balance.workspace_id=asset.workspace_id
                  AND balance.owner_id=asset.owner_id AND balance.type_id=asset.type_id
                 LEFT JOIN LATERAL (
                   SELECT 1 AS present FROM esi_asset_observations possible_child
                   WHERE possible_child.snapshot_id=asset.snapshot_id
                     AND possible_child.location_type='item'
                     AND possible_child.location_id=asset.source_item_id LIMIT 1
                 ) child ON true
                 LEFT JOIN market_location_names location
                   ON location.workspace_id=asset.workspace_id
                  AND location.location_id=asset.effective_location_id
                 LEFT JOIN sde_npc_stations station
                   ON station.import_id=import.id AND station.station_id=asset.effective_location_id
                 LEFT JOIN sde_solar_systems system
                   ON system.import_id=import.id
                  AND system.solar_system_id=COALESCE(location.solar_system_id,station.solar_system_id,
                    CASE WHEN asset.effective_location_type='solar_system'
                         THEN asset.effective_location_id END)
                 LEFT JOIN LATERAL (
                   SELECT observation.blueprint_kind,observation.material_efficiency,
                          observation.time_efficiency,observation.licensed_runs,observation.observed_at
                   FROM blueprint_observations observation
                   WHERE observation.workspace_id=asset.workspace_id
                     AND observation.owner_id=asset.owner_id
                     AND observation.eve_item_id=asset.source_item_id
                     AND observation.blueprint_type_id=asset.type_id
                   ORDER BY observation.observed_at DESC LIMIT 1
                 ) blueprint ON true
               ),
               classified AS (
                 SELECT *,
                   CASE
                     WHEN blueprint_kind IS NOT NULL THEN 'blueprint'
                     WHEN is_container THEN 'container'
                     WHEN lower(COALESCE(group_name,'')) LIKE '%mineral%' THEN 'material'
                     WHEN lower(COALESCE(group_name,'')) LIKE '%ship%'
                       OR lower(COALESCE(group_name,'')) IN
                         ('frigate','destroyer','cruiser','battlecruiser','battleship') THEN 'ship'
                     ELSE 'other'
                   END AS asset_kind,
                   CASE
                     WHEN observed_owner_type_quantity=accounted_owner_type_quantity THEN 'accounted'
                     WHEN accounted_owner_type_quantity=0 THEN 'unaccounted'
                     ELSE 'difference'
                   END AS reconciliation_state
                 FROM enriched
               ),
               filtered AS (
                 SELECT *,COUNT(*) OVER()::bigint AS total
                 FROM classified
                 WHERE ($2='' OR COALESCE(type_name,'') ILIKE $3
                   OR character_name ILIKE $3 OR COALESCE(location_name,'') ILIKE $3
                   OR COALESCE(container_name,'') ILIKE $3 OR COALESCE(group_name,'') ILIKE $3)
                   AND (cardinality($4::uuid[])=0 OR connection_id=ANY($4))
                   AND (cardinality($5::bigint[])=0 OR location_id=ANY($5))
                   AND (cardinality($6::text[])=0 OR asset_kind=ANY($6))
                   AND (cardinality($7::bigint[])=0 OR group_id=ANY($7))
                   AND (cardinality($8::text[])=0 OR COALESCE(blueprint_kind,'unknown')=ANY($8))
                   AND (cardinality($9::text[])=0 OR reconciliation_state=ANY($9))
               )
               SELECT * FROM filtered
               ORDER BY
                 CASE WHEN $10='item' AND $11='asc' THEN lower(COALESCE(type_name,'')) END ASC,
                 CASE WHEN $10='item' AND $11='desc' THEN lower(COALESCE(type_name,'')) END DESC,
                 CASE WHEN $10='quantity' AND $11='asc' THEN quantity END ASC,
                 CASE WHEN $10='quantity' AND $11='desc' THEN quantity END DESC,
                 CASE WHEN $10='packagedvolume' AND $11='asc' THEN total_packaged_volume::numeric END ASC NULLS LAST,
                 CASE WHEN $10='packagedvolume' AND $11='desc' THEN total_packaged_volume::numeric END DESC NULLS LAST,
                 CASE WHEN $10='character' AND $11='asc' THEN lower(character_name) END ASC,
                 CASE WHEN $10='character' AND $11='desc' THEN lower(character_name) END DESC,
                 CASE WHEN $10='location' AND $11='asc' THEN lower(COALESCE(location_name,'')) END ASC,
                 CASE WHEN $10='location' AND $11='desc' THEN lower(COALESCE(location_name,'')) END DESC,
                 CASE WHEN $10='container' AND $11='asc' THEN lower(COALESCE(container_name,'')) END ASC,
                 CASE WHEN $10='container' AND $11='desc' THEN lower(COALESCE(container_name,'')) END DESC,
                 CASE WHEN $10='group' AND $11='asc' THEN lower(COALESCE(group_name,'')) END ASC,
                 CASE WHEN $10='group' AND $11='desc' THEN lower(COALESCE(group_name,'')) END DESC,
                 CASE WHEN $10='status' AND $11='asc' THEN reconciliation_state END ASC,
                 CASE WHEN $10='status' AND $11='desc' THEN reconciliation_state END DESC,
                 CASE WHEN $10='observed' AND $11='asc' THEN observed_at END ASC,
                 CASE WHEN $10='observed' AND $11='desc' THEN observed_at END DESC,
                 connection_id,eve_item_id
               LIMIT $12 OFFSET $13"#,
        )
        .bind(workspace_id.0)
        .bind(&query.search)
        .bind(search)
        .bind(&query.connection_ids)
        .bind(&query.location_ids)
        .bind(&asset_kinds)
        .bind(&query.group_ids)
        .bind(&blueprint_kinds)
        .bind(&reconciliation_states)
        .bind(sort)
        .bind(order)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)
    }
}

#[derive(sqlx::FromRow)]
struct SummaryRow {
    location_count: i64,
    character_count: i64,
    stack_count: i64,
    total_quantity: i64,
    total_packaged_volume: String,
    latest_observed_at: Option<DateTime<Utc>>,
    unresolved_location_count: i64,
}

#[derive(sqlx::FromRow)]
struct FlatItemRow {
    eve_item_id: i64,
    type_id: i64,
    type_name: Option<String>,
    quantity: i64,
    packaged_volume: Option<String>,
    total_packaged_volume: Option<String>,
    owner_id: Uuid,
    owner_name: String,
    connection_id: Uuid,
    character_id: i64,
    character_name: String,
    location_id: i64,
    location_name: Option<String>,
    location_flag: String,
    container_item_id: Option<i64>,
    container_name: Option<String>,
    group_id: Option<i64>,
    group_name: Option<String>,
    observed_at: DateTime<Utc>,
    is_container: bool,
    blueprint_kind: Option<String>,
    material_efficiency: Option<i32>,
    time_efficiency: Option<i32>,
    licensed_runs: Option<i32>,
    blueprint_observed_at: Option<DateTime<Utc>>,
    observed_owner_type_quantity: i64,
    accounted_owner_type_quantity: i64,
    asset_kind: String,
    reconciliation_state: String,
    total: i64,
}

impl FlatItemRow {
    fn into_domain(self) -> FlatAssetRow {
        let blueprint = self.blueprint_kind.map(|kind| AssetBlueprintSummary {
            kind,
            material_efficiency: self.material_efficiency.unwrap_or_default(),
            time_efficiency: self.time_efficiency.unwrap_or_default(),
            licensed_runs: self.licensed_runs,
            observed_at: self.blueprint_observed_at.unwrap_or(self.observed_at),
        });
        FlatAssetRow {
            eve_item_id: self.eve_item_id,
            type_id: self.type_id,
            type_name: self.type_name,
            quantity: self.quantity,
            packaged_volume: self.packaged_volume,
            total_packaged_volume: self.total_packaged_volume,
            owner_id: self.owner_id,
            owner_name: self.owner_name,
            connection_id: self.connection_id,
            character_id: self.character_id,
            character_name: self.character_name,
            location_id: self.location_id,
            location_name: self.location_name,
            location_flag: self.location_flag,
            container_item_id: self.container_item_id,
            container_name: self.container_name,
            group_id: self.group_id,
            group_name: self.group_name,
            asset_kind: self.asset_kind,
            observed_at: self.observed_at,
            blueprint,
            reconciliation: AssetReconciliationSummary {
                state: self.reconciliation_state,
                observed_owner_type_quantity: self.observed_owner_type_quantity,
                accounted_owner_type_quantity: self.accounted_owner_type_quantity,
                scope: "ownerType",
            },
            is_container: self.is_container,
        }
    }
}

#[derive(sqlx::FromRow)]
struct FilterRow {
    value: String,
    label: String,
    count: i64,
}

impl FilterRow {
    fn into_domain(self) -> AssetFilterOption {
        AssetFilterOption {
            value: self.value,
            label: self.label,
            count: self.count,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SyncStateRow {
    connection_id: Uuid,
    character_name: String,
    connection_status: String,
    snapshot_status: Option<String>,
    observed_at: Option<DateTime<Utc>>,
    row_count: Option<i64>,
}

impl SyncStateRow {
    fn into_domain(self) -> AssetSyncState {
        AssetSyncState {
            connection_id: self.connection_id,
            character_name: self.character_name,
            connection_status: self.connection_status,
            snapshot_status: self.snapshot_status,
            observed_at: self.observed_at,
            row_count: self.row_count,
        }
    }
}

#[derive(sqlx::FromRow)]
struct LocationRow {
    location_id: i64,
    location_name: String,
    location_type: String,
    location_type_id: Option<i64>,
    solar_system_name: Option<String>,
    stack_count: i64,
    total_quantity: i64,
    owner_count: i64,
    character_count: i64,
    container_count: i64,
    blueprint_count: i64,
    latest_observed_at: DateTime<Utc>,
    oldest_observed_at: DateTime<Utc>,
    unresolved_child_count: i64,
    match_count: i64,
}

impl LocationRow {
    fn into_domain(self) -> AssetLocationSummary {
        AssetLocationSummary {
            location_id: self.location_id,
            location_name: self.location_name,
            location_type: self.location_type,
            location_type_id: self.location_type_id,
            solar_system_name: self.solar_system_name,
            stack_count: self.stack_count,
            total_quantity: self.total_quantity,
            owner_count: self.owner_count,
            character_count: self.character_count,
            container_count: self.container_count,
            blueprint_count: self.blueprint_count,
            latest_observed_at: self.latest_observed_at,
            oldest_observed_at: self.oldest_observed_at,
            unresolved_child_count: self.unresolved_child_count,
            match_count: self.match_count,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ItemRow {
    eve_item_id: i64,
    type_id: i64,
    type_name: String,
    quantity: i64,
    owner_id: Uuid,
    owner_name: String,
    connection_id: Uuid,
    character_id: i64,
    character_name: String,
    location_id: i64,
    location_flag: String,
    parent_item_id: Option<i64>,
    group_id: Option<i64>,
    group_name: Option<String>,
    observed_at: DateTime<Utc>,
    is_container: bool,
    blueprint_kind: Option<String>,
    material_efficiency: Option<i32>,
    time_efficiency: Option<i32>,
    licensed_runs: Option<i32>,
    blueprint_observed_at: Option<DateTime<Utc>>,
    observed_owner_type_quantity: i64,
    accounted_owner_type_quantity: i64,
    total: i64,
}

impl ItemRow {
    fn into_domain(self) -> AssetBrowserItem {
        let blueprint = self.blueprint_kind.map(|kind| AssetBlueprintSummary {
            kind,
            material_efficiency: self.material_efficiency.unwrap_or_default(),
            time_efficiency: self.time_efficiency.unwrap_or_default(),
            licensed_runs: self.licensed_runs,
            observed_at: self.blueprint_observed_at.unwrap_or(self.observed_at),
        });
        let asset_kind = if blueprint.is_some() {
            "blueprint"
        } else if self.is_container {
            "container"
        } else if self
            .group_name
            .as_deref()
            .is_some_and(|name| name.to_lowercase().contains("mineral"))
        {
            "material"
        } else if self
            .group_name
            .as_deref()
            .is_some_and(|name| name.to_lowercase().contains("ship"))
        {
            "ship"
        } else {
            "other"
        };
        let state = match self
            .observed_owner_type_quantity
            .cmp(&self.accounted_owner_type_quantity)
        {
            std::cmp::Ordering::Equal => "accounted",
            std::cmp::Ordering::Greater if self.accounted_owner_type_quantity == 0 => "unaccounted",
            std::cmp::Ordering::Greater => "moreObserved",
            std::cmp::Ordering::Less => "moreAccounted",
        };
        AssetBrowserItem {
            eve_item_id: self.eve_item_id,
            type_id: self.type_id,
            type_name: self.type_name,
            quantity: self.quantity,
            owner_id: self.owner_id,
            owner_name: self.owner_name,
            connection_id: self.connection_id,
            character_id: self.character_id,
            character_name: self.character_name,
            location_id: self.location_id,
            location_flag: self.location_flag,
            parent_item_id: self.parent_item_id,
            group_id: self.group_id,
            group_name: self.group_name,
            asset_kind: asset_kind.to_string(),
            observed_at: self.observed_at,
            blueprint,
            reconciliation: AssetReconciliationSummary {
                state: state.to_string(),
                observed_owner_type_quantity: self.observed_owner_type_quantity,
                accounted_owner_type_quantity: self.accounted_owner_type_quantity,
                scope: "ownerType",
            },
            is_container: self.is_container,
        }
    }
}

fn map_error(error: sqlx::Error) -> InventoryError {
    InventoryError::Persistence(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use iskworks_core::{FlatAssetQuery, WorkspaceId};

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    async fn flat_assets_returns_an_empty_server_owned_page(pool: PgPool) {
        let workspace_id = WorkspaceId(Uuid::new_v4());
        let owner_id = Uuid::new_v4();
        let mut transaction = pool.begin().await.expect("transaction begins");
        sqlx::query(
            "INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at)
             VALUES ($1,'Assets test',$2,now(),now())",
        )
        .bind(workspace_id.0)
        .bind(owner_id)
        .execute(&mut *transaction)
        .await
        .expect("workspace inserted");
        sqlx::query(
            "INSERT INTO owners
               (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at)
             VALUES ($1,$2,'manual','Assets owner',true,now(),now())",
        )
        .bind(owner_id)
        .bind(workspace_id.0)
        .execute(&mut *transaction)
        .await
        .expect("owner inserted");
        transaction.commit().await.expect("fixtures committed");
        let query = FlatAssetQuery::default().validate().expect("valid query");

        let page = PgAssetBrowserRepository::new(pool)
            .flat_assets(workspace_id, &query)
            .await
            .expect("flat page loads");

        assert!(page.rows.is_empty());
        assert_eq!(page.total, 0);
        let summary = page.summary.expect("first page carries the summary");
        assert_eq!(summary.total_packaged_volume, "0.0000");
        assert_eq!(page.facets, Some(Default::default()));
        assert!(page.next_cursor.is_none());
    }

    /// One connection with a container at Jita holding Tritanium and an
    /// unknown type: three stacks.
    async fn seed_flat_assets(pool: &PgPool) -> (WorkspaceId, Uuid) {
        let workspace_id = WorkspaceId(Uuid::new_v4());
        let owner_id = Uuid::new_v4();
        let connection_id = Uuid::new_v4();
        let import_id = Uuid::new_v4();
        let sync_run_id = Uuid::new_v4();
        let snapshot_id = Uuid::new_v4();
        let now = crate::db_now();
        let mut tx = pool.begin().await.expect("transaction begins");
        sqlx::query("INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Flat assets',$2,$3,$3)")
            .bind(workspace_id.0).bind(owner_id).bind(now).execute(&mut *tx).await.expect("workspace");
        sqlx::query("INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Valka',true,$3,$3)")
            .bind(owner_id).bind(workspace_id.0).bind(now).execute(&mut *tx).await.expect("owner");
        sqlx::query("INSERT INTO eve_connections (id,workspace_id,owner_id,eve_character_id,character_name,status,granted_scopes,connected_at,updated_at) VALUES ($1,$2,$3,9001,'Valka','connected','{}',$4,$4)")
            .bind(connection_id).bind(workspace_id.0).bind(owner_id).bind(now).execute(&mut *tx).await.expect("connection");
        sqlx::query("INSERT INTO sde_imports (id,source_version,source_label,source_checksum,status,active,started_at,completed_at) VALUES ($1,'test','test','flat-assets','active',true,$2,$2)")
            .bind(import_id).bind(now).execute(&mut *tx).await.expect("import");
        sqlx::query("INSERT INTO sde_types (import_id,type_id,name_en,group_id,group_name_en,published,packaged_volume_m3) VALUES ($1,34,'Tritanium',18,'Mineral',true,0.01),($1,1001,'Station Container',12,'Cargo Container',true,10)")
            .bind(import_id).execute(&mut *tx).await.expect("types");
        sqlx::query("INSERT INTO esi_sync_runs (id,workspace_id,owner_id,connection_id,requested_kind,status,phase,started_at,completed_at,summary) VALUES ($1,$2,$3,$4,'assets','succeeded','complete',$5,$5,'test')")
            .bind(sync_run_id).bind(workspace_id.0).bind(owner_id).bind(connection_id).bind(now).execute(&mut *tx).await.expect("sync run");
        sqlx::query("INSERT INTO esi_asset_snapshots (id,connection_id,sync_run_id,observed_at,completed_at,status,page_count,row_count,active) VALUES ($1,$2,$3,$4,$4,'complete',1,3,true)")
            .bind(snapshot_id).bind(connection_id).bind(sync_run_id).bind(now).execute(&mut *tx).await.expect("snapshot");
        sqlx::query("INSERT INTO esi_asset_observations (snapshot_id,source_item_id,type_id,quantity,location_id,location_type,location_flag,is_singleton,raw_payload,source_checksum) VALUES ($1,5001,1001,1,60003760,'station','Hangar',true,'{}','container'),($1,5002,34,27075,5001,'item','Cargo',false,'{}','tritanium'),($1,5003,999999,1,5001,'item','Cargo',true,'{}','unknown')")
            .bind(snapshot_id).execute(&mut *tx).await.expect("observations");
        sqlx::query("SELECT refresh_esi_asset_hierarchy($1)")
            .bind(snapshot_id)
            .execute(&mut *tx)
            .await
            .expect("hierarchy");
        sqlx::query("INSERT INTO market_location_names (workspace_id,location_id,location_name,owner_id,solar_system_id,resolved_by_connection_id,resolved_at,updated_at) VALUES ($1,60003760,'Jita IV - Moon 4 - Caldari Navy Assembly Plant',1000125,30000142,$2,$3,$3)")
            .bind(workspace_id.0).bind(connection_id).bind(now).execute(&mut *tx).await.expect("location");
        tx.commit().await.expect("fixtures committed");
        (workspace_id, connection_id)
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    async fn flat_assets_projects_location_container_and_exact_volume(pool: PgPool) {
        let (workspace_id, connection_id) = seed_flat_assets(&pool).await;
        let query: FlatAssetQuery = serde_json::from_value(serde_json::json!({
            "search": "Station Container"
        }))
        .expect("query");
        let page = PgAssetBrowserRepository::new(pool)
            .flat_assets(workspace_id, &query.validate().expect("valid query"))
            .await
            .expect("page");

        assert_eq!(page.total, 3);
        let facets = page.facets.as_ref().expect("first page carries facets");
        assert_eq!(facets.characters.len(), 1);
        assert_eq!(facets.characters[0].value, connection_id.to_string());
        assert_eq!(facets.characters[0].label, "Valka");
        let tritanium = page
            .rows
            .iter()
            .find(|row| row.type_id == 34)
            .expect("tritanium row");
        assert_eq!(
            tritanium.container_name.as_deref(),
            Some("Station Container")
        );
        assert_eq!(
            tritanium.location_name.as_deref(),
            Some("Jita IV - Moon 4 - Caldari Navy Assembly Plant")
        );
        assert_eq!(tritanium.packaged_volume.as_deref(), Some("0.01"));
        assert_eq!(tritanium.total_packaged_volume.as_deref(), Some("270.75"));
        assert!(page.rows.iter().any(|row| {
            row.type_id == 999_999 && row.type_name.as_deref() == Some("Unknown EVE type 999999")
        }));
        assert_eq!(
            page.summary.expect("summary").total_packaged_volume,
            "280.7500"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    async fn only_the_first_flat_assets_page_carries_summary_and_facets(pool: PgPool) {
        let (workspace_id, _) = seed_flat_assets(&pool).await;
        let repository = PgAssetBrowserRepository::new(pool);
        let query: FlatAssetQuery =
            serde_json::from_value(serde_json::json!({ "limit": 2 })).expect("query");
        let mut query = query.validate().expect("valid query");

        let first = repository
            .flat_assets(workspace_id, &query)
            .await
            .expect("first page");
        let facets = first.facets.expect("first page carries facets");
        assert_eq!(first.summary.expect("first page summary").stack_count, 3);
        assert_eq!(
            facets
                .locations
                .iter()
                .map(|option| (option.value.as_str(), option.label.as_str(), option.count))
                .collect::<Vec<_>>(),
            vec![(
                "60003760",
                "Jita IV - Moon 4 - Caldari Navy Assembly Plant",
                3
            )]
        );

        query.cursor = Some(
            FlatAssetCursor::decode(first.next_cursor.as_deref().expect("more rows"))
                .expect("cursor"),
        );
        let second = repository
            .flat_assets(workspace_id, &query)
            .await
            .expect("second page");
        assert_eq!(second.rows.len(), 1);
        assert_eq!(second.total, 3);
        assert!(second.summary.is_none());
        assert!(second.facets.is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    async fn all_flat_assets_returns_every_matching_row_in_one_query(pool: PgPool) {
        let (workspace_id, _) = seed_flat_assets(&pool).await;
        let query: FlatAssetQuery = serde_json::from_value(serde_json::json!({
            "limit": 1, "sort": "quantity", "order": "desc"
        }))
        .expect("query");

        let rows = PgAssetBrowserRepository::new(pool)
            .all_flat_assets(workspace_id, &query.validate().expect("valid query"))
            .await
            .expect("rows");

        assert_eq!(
            rows.iter().map(|row| row.quantity).collect::<Vec<_>>(),
            vec![27_075, 1, 1]
        );
    }
}
