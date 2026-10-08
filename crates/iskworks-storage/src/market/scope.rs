//! Scope and price-source resolution: `MarketScope` <-> source mapping,
//! per-scope ESI price-source provisioning, and type-name/market-group
//! lookups. PriceSource semantics are unchanged.

use chrono::{DateTime, Utc};
use iskworks_core::{
    MarketError, MarketImportBatchId, MarketObservationBatchId, MarketPriceSource, MarketScope,
    MarketScopeEvidence, PriceSourceId, WorkspaceId,
};
use uuid::Uuid;

use super::convert::*;
use super::rows::*;
use super::PgMarketRepository;

impl PgMarketRepository {
    pub(super) async fn load_source(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<MarketPriceSource, MarketError> {
        sqlx::query_as::<_, MarketSourceRow>(
            r#"
            SELECT ps.id,ps.workspace_id,ps.display_name,ps.description,ps.source_kind,
                   ps.revision,ps.created_at,ps.updated_at,
                   CASE WHEN ps.source_kind='esi_market_orders' THEN (
                     SELECT count(*)
                     FROM market_source_coverage coverage
                     WHERE coverage.workspace_id=ps.workspace_id
                       AND coverage.price_source_id=ps.id
                   ) ELSE (
                     SELECT count(DISTINCT mif.type_id)
                     FROM market_import_files mif
                     WHERE mif.workspace_id=ps.workspace_id
                       AND mif.location_id=c.location_id
                       AND (
                         c.observation_mode='latest_compatible_import'
                         OR mif.batch_id=c.pinned_batch_id
                       )
                   ) END item_count,
                   c.location_id,c.solar_system_id,c.region_id,c.location_alias,
                   c.pricing_policy,c.coverage_policy,c.observation_mode,c.pinned_batch_id,
                   c.fresh_after_hours,c.stale_after_hours,c.archived_at,c.last_snapshot_at
            FROM price_sources ps
            JOIN market_price_source_configs c ON c.price_source_id=ps.id
            WHERE ps.workspace_id=$1 AND ps.id=$2
              AND ps.source_kind IN ('eve_client_market_export','esi_market_orders')
            "#,
        )
        .bind(workspace_id.0)
        .bind(source_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?
        .ok_or(MarketError::PriceSourceNotFound)?
        .into_source()
    }

    /// The market-evidence identity current for `scope` right now: the
    /// completed ESI observation batches its sources point at, the latest
    /// import batch for its locations, the newest `observed_at` across
    /// those, and a frozen `as_of` clock. Resolved once per scope at the
    /// start of a Build Graph request; every subsequent
    /// `derive_market_price_items` for that scope is then pinned to it.
    pub(crate) async fn resolve_scope_evidence(
        &self,
        workspace_id: WorkspaceId,
        scope: MarketScope,
    ) -> Result<MarketScopeEvidence, MarketError> {
        let as_of = crate::db_now();
        let public_region = self.public_region_for_scope(workspace_id, scope).await?;

        let observation_batch_ids: Vec<Uuid> = if let Some(region_id) = public_region {
            sqlx::query_scalar::<_, Uuid>(
                r#"
                SELECT coverage.last_completed_batch_id
                FROM public_market_coverage coverage
                JOIN market_observation_batches batch
                  ON batch.id=coverage.last_completed_batch_id
                WHERE coverage.region_id=$1 AND batch.status='completed'
                  AND batch.observed_at<=$2
                "#,
            )
            .bind(region_id)
            .bind(as_of)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx)?
        } else {
            sqlx::query_scalar::<_, Uuid>(
                r#"
            SELECT DISTINCT coverage.last_completed_batch_id
            FROM market_source_coverage coverage
            JOIN market_observation_batches batch
              ON batch.id=coverage.last_completed_batch_id
            WHERE coverage.workspace_id=$1 AND coverage.region_id=$2
              AND ($3::bigint IS NULL OR coverage.location_id=$3)
              AND coverage.last_completed_batch_id IS NOT NULL
              AND batch.status='completed' AND batch.observed_at<=$4
            "#,
            )
            .bind(workspace_id.0)
            .bind(scope.region_id)
            .bind(scope.location_id)
            .bind(as_of)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx)?
        };

        let import_batch_id: Option<Uuid> = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT batch_id
            FROM market_import_files
            WHERE workspace_id=$1 AND region_id=$2
              AND ($3::bigint IS NULL OR location_id=$3)
              AND observed_at<=$4
            ORDER BY observed_at DESC, imported_at DESC
            LIMIT 1
            "#,
        )
        .bind(workspace_id.0)
        .bind(scope.region_id)
        .bind(scope.location_id)
        .bind(as_of)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        // ESI evidence comes from the app-wide rows for a public scope
        // (`$5` is its region) and from the workspace's own coverage
        // otherwise; imports are always the workspace's own.
        let observed_at: Option<DateTime<Utc>> = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
            r#"
            SELECT max(observed_at) FROM (
              SELECT batch.observed_at
              FROM market_source_coverage coverage
              JOIN market_observation_batches batch
                ON batch.id=coverage.last_completed_batch_id
              WHERE $5::bigint IS NULL
                AND coverage.workspace_id=$1 AND coverage.region_id=$2
                AND ($3::bigint IS NULL OR coverage.location_id=$3)
                AND batch.observed_at<=$4
              UNION ALL
              SELECT batch.observed_at
              FROM public_market_coverage coverage
              JOIN market_observation_batches batch
                ON batch.id=coverage.last_completed_batch_id
              WHERE coverage.region_id=$5 AND batch.observed_at<=$4
              UNION ALL
              SELECT observed_at
              FROM market_import_files
              WHERE workspace_id=$1 AND region_id=$2
                AND ($3::bigint IS NULL OR location_id=$3)
                AND observed_at<=$4
            ) evidence
            "#,
        )
        .bind(workspace_id.0)
        .bind(scope.region_id)
        .bind(scope.location_id)
        .bind(as_of)
        .bind(public_region)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(MarketScopeEvidence {
            scope,
            observation_batch_ids: observation_batch_ids
                .into_iter()
                .map(MarketObservationBatchId)
                .collect(),
            import_batch_id: import_batch_id.map(MarketImportBatchId),
            observed_at,
            as_of,
        })
    }

    /// Every `esi_market_orders` `market_price_source_configs` row matching
    /// `scope` for this workspace -- the `MarketScope` -> `price_source_id`
    /// shim's one query, shared by `scoped_order_books` and
    /// `resolve_scope_price_sources`. ESI-kind sources only.
    pub(super) async fn resolve_scope_sources(
        &self,
        workspace_id: WorkspaceId,
        scope: iskworks_core::MarketScope,
    ) -> Result<Vec<ScopeSourceRow>, MarketError> {
        sqlx::query_as::<_, ScopeSourceRow>(
            r#"
            SELECT price_source_id, location_id
            FROM market_price_source_configs
            WHERE workspace_id=$1 AND region_id=$2
              AND ($3::bigint IS NULL OR location_id=$3)
              AND archived_at IS NULL AND source_kind='esi_market_orders'
            "#,
        )
        .bind(workspace_id.0)
        .bind(scope.region_id)
        .bind(scope.location_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)
    }

    /// Distinct locations within `region_id` that have at least one
    /// imported EVE client market export on file, for this workspace --
    /// `scoped_order_books`'s import-derived half of the merge.
    /// Import data was never tied to a `PriceSource`
    /// (`market_import_files`/`market_observation_batches` already carry
    /// their own real `region_id`/`location_id` per row, see `insert_file`),
    /// so this reads straight off the import tables instead of requiring
    /// one to be created first.
    pub(super) async fn known_import_locations_in_scope(
        &self,
        workspace_id: WorkspaceId,
        region_id: i64,
    ) -> Result<Vec<i64>, MarketError> {
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT DISTINCT location_id
            FROM market_import_files
            WHERE workspace_id=$1 AND region_id=$2
            "#,
        )
        .bind(workspace_id.0)
        .bind(region_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)
    }

    /// Best-effort display name + solar system for an `esi_market_orders`
    /// source auto-provisioned for `region_id`/`location_id` (by
    /// `ensure_esi_price_source_for_scope`). `location_id=0` is the
    /// region-wide sentinel. Every location offered by
    /// `list_market_region_locations` is already resolvable via one of
    /// these two lookups, so the "unknown" fallback is defensive, not an
    /// expected path.
    pub(super) async fn resolve_scope_display(
        &self,
        workspace_id: WorkspaceId,
        region_id: i64,
        location_id: i64,
    ) -> Result<(String, i64), MarketError> {
        let region_name = sqlx::query_scalar::<_, String>(
            r#"
            SELECT r.name_en
            FROM sde_imports import
            JOIN sde_regions r ON r.import_id=import.id
            WHERE import.active=true AND r.region_id=$1
            "#,
        )
        .bind(region_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?
        .unwrap_or_else(|| format!("Region {region_id}"));
        if location_id == 0 {
            return Ok((format!("ESI: {region_name} (all locations)"), 0));
        }
        let station = sqlx::query_as::<_, (String, i64)>(
            r#"
            SELECT station.name_en, system.solar_system_id
            FROM sde_imports import
            JOIN sde_npc_stations station
              ON station.import_id=import.id AND station.station_id=$1
            JOIN sde_solar_systems system
              ON system.import_id=import.id AND system.solar_system_id=station.solar_system_id
            WHERE import.active=true
            "#,
        )
        .bind(location_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;
        let (location_name, solar_system_id) = match station {
            Some(found) => found,
            None => sqlx::query_as::<_, (String, i64)>(
                r#"
                SELECT location_name, solar_system_id
                FROM market_location_names
                WHERE workspace_id=$1 AND location_id=$2
                "#,
            )
            .bind(workspace_id.0)
            .bind(location_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)?
            .unwrap_or_else(|| (format!("Location {location_id}"), 0)),
        };
        Ok((
            format!("ESI: {region_name} — {location_name}"),
            solar_system_id,
        ))
    }

    pub(super) async fn resolve_scope_price_sources(
        &self,
        workspace_id: WorkspaceId,
        scope: iskworks_core::MarketScope,
    ) -> Result<Vec<PriceSourceId>, MarketError> {
        Ok(self
            .resolve_scope_sources(workspace_id, scope)
            .await?
            .into_iter()
            .map(|source| PriceSourceId(source.price_source_id))
            .collect())
    }

    pub(super) async fn resolve_source_scope(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<iskworks_core::MarketScope, MarketError> {
        let row = sqlx::query_as::<_, (i64, i64)>(
            r#"
            SELECT region_id, location_id
            FROM market_price_source_configs
            WHERE workspace_id=$1 AND price_source_id=$2 AND source_kind='esi_market_orders'
            "#,
        )
        .bind(workspace_id.0)
        .bind(source_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?
        .ok_or(MarketError::PriceSourceNotFound)?;
        Ok(iskworks_core::MarketScope {
            region_id: row.0,
            location_id: if row.1 == 0 { None } else { Some(row.1) },
        })
    }

    pub(super) async fn ensure_esi_price_source_for_scope(
        &self,
        workspace_id: WorkspaceId,
        scope: iskworks_core::MarketScope,
    ) -> Result<PriceSourceId, MarketError> {
        let region_id = scope.region_id;
        let location_id = scope.location_id.unwrap_or(0);
        if let Some(existing) = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT price_source_id
            FROM market_price_source_configs
            WHERE workspace_id=$1 AND region_id=$2 AND location_id=$3
              AND source_kind='esi_market_orders'
            "#,
        )
        .bind(workspace_id.0)
        .bind(region_id)
        .bind(location_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?
        {
            return Ok(PriceSourceId(existing));
        }

        let (display_name, solar_system_id) = self
            .resolve_scope_display(workspace_id, region_id, location_id)
            .await?;
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        let id = PriceSourceId::new();
        let now = crate::db_now();
        sqlx::query(
            r#"
            INSERT INTO price_sources (
              id,workspace_id,display_name,description,source_kind,
              revision,created_at,updated_at
            ) VALUES ($1,$2,$3,'Public ESI market orders.','esi_market_orders',1,$4,$4)
            "#,
        )
        .bind(id.0)
        .bind(workspace_id.0)
        .bind(&display_name)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        let inserted = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO market_price_source_configs (
              price_source_id,workspace_id,location_id,solar_system_id,region_id,
              location_alias,source_kind,pricing_policy,coverage_policy,observation_mode,
              pinned_batch_id,fresh_after_hours,stale_after_hours
            ) VALUES (
              $1,$2,$3,$4,$5,'','esi_market_orders',
              'acquire_quantity_from_sell_orders','allow_partial_with_warning',
              'latest_compatible_import',NULL,1,24
            )
            ON CONFLICT (workspace_id,region_id,location_id) WHERE source_kind='esi_market_orders'
            DO NOTHING
            RETURNING price_source_id
            "#,
        )
        .bind(id.0)
        .bind(workspace_id.0)
        .bind(location_id)
        .bind(solar_system_id)
        .bind(region_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        if inserted.is_some() {
            tx.commit().await.map_err(map_sqlx)?;
            return Ok(id);
        }
        // Lost a create race against a concurrent request for the same
        // scope -- roll back our own price_sources insert (it would
        // otherwise be orphaned, ON CONFLICT DO NOTHING having skipped its
        // config row) and defer to whichever request won.
        tx.rollback().await.map_err(map_sqlx)?;
        let winner = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT price_source_id
            FROM market_price_source_configs
            WHERE workspace_id=$1 AND region_id=$2 AND location_id=$3
              AND source_kind='esi_market_orders'
            "#,
        )
        .bind(workspace_id.0)
        .bind(region_id)
        .bind(location_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(PriceSourceId(winner))
    }

    pub(super) async fn resolve_type_name(
        &self,
        type_id: i64,
    ) -> Result<Option<String>, MarketError> {
        sqlx::query_scalar(
            r#"
            SELECT t.name_en
            FROM sde_types t
            JOIN sde_imports i ON i.id=t.import_id AND i.active
            WHERE t.type_id=$1
            "#,
        )
        .bind(type_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)
    }

    pub(super) async fn resolve_type_market_group(
        &self,
        type_id: i64,
    ) -> Result<Option<i64>, MarketError> {
        sqlx::query_scalar::<_, Option<i64>>(
            r#"
            SELECT t.market_group_id
            FROM sde_types t
            JOIN sde_imports i ON i.id=t.import_id AND i.active
            WHERE t.type_id=$1
            "#,
        )
        .bind(type_id)
        .fetch_optional(&self.pool)
        .await
        .map(Option::flatten)
        .map_err(map_sqlx)
    }

    pub(super) async fn get_market_price_source(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<MarketPriceSource, MarketError> {
        self.load_source(workspace_id, source_id).await
    }
}
