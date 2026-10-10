use super::sync_runs::{checksum, finalize_run};
use super::*;
use async_trait::async_trait;

#[async_trait]
impl iskworks_core::AdjustedPriceRepository for PgEsiRepository {
    async fn latest_adjusted_prices(
        &self,
        type_ids: &[i64],
        _as_of: DateTime<Utc>,
    ) -> Result<std::collections::BTreeMap<i64, Decimal>, InventoryError> {
        let ids = type_ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(std::collections::BTreeMap::new());
        }
        let rows = sqlx::query_as::<_, (i64, Decimal)>(
            r#"
            SELECT DISTINCT ON (type_id) type_id, adjusted_price
            FROM industry_adjusted_price_observations
            WHERE type_id = ANY($1)
            ORDER BY type_id, observed_at DESC
        "#,
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(rows.into_iter().collect())
    }

    async fn latest_adjusted_price_observed_at(
        &self,
        type_ids: &[i64],
    ) -> Result<Option<DateTime<Utc>>, InventoryError> {
        let ids = type_ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        if ids.is_empty() {
            return Ok(None);
        }
        sqlx::query_scalar(
            r#"SELECT min(observed_at)
               FROM (
                 SELECT DISTINCT ON (type_id) type_id,observed_at
                 FROM industry_adjusted_price_observations
                 WHERE type_id = ANY($1)
                 ORDER BY type_id,observed_at DESC
               ) latest"#,
        )
        .bind(ids.into_iter().collect::<Vec<_>>())
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx)
    }

    async fn adjusted_price_refresh_overlay(
        &self,
    ) -> Result<iskworks_core::EvidenceRefreshOverlay, InventoryError> {
        let row = sqlx::query_as::<_, (String, Option<DateTime<Utc>>, Option<String>)>(
            "SELECT refresh_state,next_refresh_at,last_error FROM industry_adjusted_price_refresh_state WHERE singleton",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(row.map_or_else(
            iskworks_core::EvidenceRefreshOverlay::default,
            |(state, next, error)| iskworks_core::EvidenceRefreshOverlay {
                pending: state == "refreshing" || next.map_or(true, |due| due <= Utc::now()),
                last_error: error,
            },
        ))
    }

    async fn register_adjusted_price_refresh(
        &self,
        now: DateTime<Utc>,
    ) -> Result<(), InventoryError> {
        PgEsiRepository::register_adjusted_price_refresh(self, now).await
    }

    async fn register_system_cost_index(
        &self,
        solar_system_id: i64,
        now: DateTime<Utc>,
    ) -> Result<(), InventoryError> {
        PgEsiRepository::register_system_cost_index(self, solar_system_id, now).await
    }

    async fn latest_system_cost_index(
        &self,
        solar_system_id: i64,
    ) -> Result<Option<(Decimal, DateTime<Utc>)>, InventoryError> {
        PgEsiRepository::latest_system_cost_index(self, solar_system_id).await
    }

    async fn system_cost_index_refresh_overlay(
        &self,
        solar_system_id: i64,
    ) -> Result<iskworks_core::EvidenceRefreshOverlay, InventoryError> {
        let row = sqlx::query_as::<_, (String, Option<DateTime<Utc>>, Option<String>)>(
            "SELECT refresh_state,next_refresh_at,last_error FROM industry_system_cost_index_registrations WHERE solar_system_id=$1",
        )
        .bind(solar_system_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(row.map_or_else(
            iskworks_core::EvidenceRefreshOverlay::default,
            |(state, next, error)| iskworks_core::EvidenceRefreshOverlay {
                pending: state == "refreshing" || next.map_or(true, |due| due <= Utc::now()),
                last_error: error,
            },
        ))
    }

    async fn prioritize_adjusted_price_refresh(
        &self,
        now: DateTime<Utc>,
    ) -> Result<bool, InventoryError> {
        sqlx::query_scalar::<_, i32>(
            r#"UPDATE industry_adjusted_price_refresh_state
               SET next_refresh_at=GREATEST($1,refresh_not_before),updated_at=$1
               WHERE singleton AND refresh_state <> 'refreshing'
               RETURNING 1"#,
        )
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.is_some())
        .map_err(map_sqlx)
    }

    async fn prioritize_system_cost_index_refresh(
        &self,
        solar_system_id: i64,
        now: DateTime<Utc>,
    ) -> Result<bool, InventoryError> {
        sqlx::query_scalar::<_, i32>(
            r#"UPDATE industry_system_cost_index_registrations
               SET next_refresh_at=GREATEST($2,refresh_not_before),
                   priority_requested_at=$2,updated_at=$2
               WHERE solar_system_id=$1 AND refresh_state <> 'refreshing'
               RETURNING 1"#,
        )
        .bind(solar_system_id)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.is_some())
        .map_err(map_sqlx)
    }
}

impl PgEsiRepository {
    pub async fn register_adjusted_price_refresh(
        &self,
        now: DateTime<Utc>,
    ) -> Result<(), InventoryError> {
        sqlx::query(
            r#"INSERT INTO industry_adjusted_price_refresh_state
               (singleton,refresh_state,updated_at)
               VALUES (true,'missing',$1)
               ON CONFLICT (singleton) DO NOTHING"#,
        )
        .bind(now)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(map_sqlx)
    }

    pub async fn adjusted_price_refresh_due(
        &self,
        now: DateTime<Utc>,
    ) -> Result<bool, InventoryError> {
        sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                 SELECT 1 FROM industry_adjusted_price_refresh_state
                 WHERE (next_refresh_at IS NULL OR next_refresh_at <= $1)
                   AND (refresh_state <> 'refreshing' OR lease_expires_at <= $1)
               )"#,
        )
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx)
    }

    pub async fn begin_adjusted_price_refresh(
        &self,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, InventoryError> {
        sqlx::query_scalar::<_, DateTime<Utc>>(
            r#"UPDATE industry_adjusted_price_refresh_state
               SET refresh_state='refreshing',last_attempted_at=$1,
                   lease_expires_at=$2,last_error=NULL,updated_at=$1
               WHERE singleton
                 AND (refresh_state <> 'refreshing' OR lease_expires_at <= $1)
                 AND (next_refresh_at IS NULL OR next_refresh_at <= $1)
               RETURNING last_attempted_at"#,
        )
        .bind(attempted_at)
        .bind(lease_expires_at)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)
    }

    pub async fn complete_adjusted_price_refresh(
        &self,
        claim: DateTime<Utc>,
        prices: &[iskworks_esi::AdjustedPrice],
        observed_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        cache_expires_at: Option<DateTime<Utc>>,
        etag: Option<&str>,
    ) -> Result<bool, InventoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        let owns_claim = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM industry_adjusted_price_refresh_state WHERE singleton AND refresh_state='refreshing' AND last_attempted_at=$1 FOR UPDATE)",
        )
        .bind(claim)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        if !owns_claim {
            tx.rollback().await.map_err(map_sqlx)?;
            return Ok(false);
        }
        let checksum = etag.unwrap_or("esi-markets-prices");
        for price in prices {
            sqlx::query(
                r#"INSERT INTO industry_adjusted_price_observations
                   (id,type_id,adjusted_price,observed_at,expires_at,etag,
                    source_checksum,source_url)
                   VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
                   ON CONFLICT (type_id,observed_at,source_checksum) DO NOTHING"#,
            )
            .bind(Uuid::new_v4())
            .bind(price.type_id)
            .bind(price.adjusted_price)
            .bind(observed_at)
            .bind(next_refresh_at)
            .bind(etag)
            .bind(checksum)
            .bind("https://esi.evetech.net/latest/markets/prices/")
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;
        }
        sqlx::query(
            r#"UPDATE industry_adjusted_price_refresh_state
               SET refresh_state='current',observed_at=$1,
                   next_refresh_at=GREATEST($2,$4),refresh_not_before=$4,
                   consecutive_failures=0,
                   lease_expires_at=NULL,last_error=NULL,updated_at=$1
               WHERE singleton AND refresh_state='refreshing' AND last_attempted_at=$3"#,
        )
        .bind(observed_at)
        .bind(next_refresh_at)
        .bind(claim)
        .bind(cache_expires_at)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        tx.commit().await.map_err(map_sqlx)?;
        Ok(true)
    }

    pub async fn fail_adjusted_price_refresh(
        &self,
        claim: DateTime<Utc>,
        attempted_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        error_message: &str,
    ) -> Result<bool, InventoryError> {
        let error_message: String = error_message.chars().take(500).collect();
        sqlx::query(
            r#"UPDATE industry_adjusted_price_refresh_state
               SET refresh_state='failed',last_attempted_at=$1,
                   next_refresh_at=GREATEST(
                     $2,
                     $1 + LEAST(
                       interval '1 minute' * power(2, LEAST(consecutive_failures, 16)),
                       interval '6 hours'
                     )
                   ),
                   refresh_not_before=GREATEST(
                     $2,
                     $1 + LEAST(
                       interval '1 minute' * power(2, LEAST(consecutive_failures, 16)),
                       interval '6 hours'
                     )
                   ),
                   consecutive_failures=consecutive_failures + 1,
                   lease_expires_at=NULL,last_error=$3,updated_at=$1
               WHERE singleton AND refresh_state='refreshing' AND last_attempted_at=$4"#,
        )
        .bind(attempted_at)
        .bind(next_refresh_at)
        .bind(error_message)
        .bind(claim)
        .execute(&self.pool)
        .await
        .map(|result| result.rows_affected() == 1)
        .map_err(map_sqlx)
    }

    pub async fn register_system_cost_index(
        &self,
        solar_system_id: i64,
        now: DateTime<Utc>,
    ) -> Result<(), InventoryError> {
        if solar_system_id <= 0 {
            return Err(InventoryError::Validation(
                "solar system ID must be positive".to_string(),
            ));
        }
        sqlx::query(
            r#"INSERT INTO industry_system_cost_index_registrations
               (solar_system_id,refresh_state,created_at,updated_at)
               VALUES ($1,'missing',$2,$2)
               ON CONFLICT (solar_system_id) DO NOTHING"#,
        )
        .bind(solar_system_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(map_sqlx)
    }

    pub async fn system_cost_index_refresh_candidates(
        &self,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<i64>, InventoryError> {
        sqlx::query_scalar(
            r#"SELECT solar_system_id
               FROM industry_system_cost_index_registrations
               WHERE (next_refresh_at IS NULL OR next_refresh_at <= $1)
                 AND (refresh_state <> 'refreshing' OR lease_expires_at <= $1)
               ORDER BY priority_requested_at DESC NULLS LAST,
                        next_refresh_at NULLS FIRST,solar_system_id
               LIMIT $2"#,
        )
        .bind(now)
        .bind(limit.max(0))
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)
    }

    pub async fn begin_system_cost_index_refresh(
        &self,
        solar_system_id: i64,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, InventoryError> {
        sqlx::query_scalar::<_, DateTime<Utc>>(
            r#"UPDATE industry_system_cost_index_registrations
               SET refresh_state='refreshing',last_attempted_at=$2,
                   lease_expires_at=$3,last_error=NULL,updated_at=$2
               WHERE solar_system_id=$1
                 AND (refresh_state <> 'refreshing' OR lease_expires_at <= $2)
                 AND (next_refresh_at IS NULL OR next_refresh_at <= $2)
               RETURNING last_attempted_at"#,
        )
        .bind(solar_system_id)
        .bind(attempted_at)
        .bind(lease_expires_at)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn complete_system_cost_index_refresh(
        &self,
        solar_system_id: i64,
        claim: DateTime<Utc>,
        cost_index: Decimal,
        observed_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        cache_expires_at: Option<DateTime<Utc>>,
        etag: Option<&str>,
    ) -> Result<bool, InventoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        let owns_claim = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM industry_system_cost_index_registrations WHERE solar_system_id=$1 AND refresh_state='refreshing' AND last_attempted_at=$2 FOR UPDATE)",
        )
        .bind(solar_system_id)
        .bind(claim)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        if !owns_claim {
            tx.rollback().await.map_err(map_sqlx)?;
            return Ok(false);
        }
        let checksum = etag.unwrap_or("esi-industry-systems");
        sqlx::query(
            r#"INSERT INTO industry_system_cost_index_observations
               (id,solar_system_id,activity,cost_index,observed_at,expires_at,
                etag,source_checksum,source_url)
               VALUES ($1,$2,'manufacturing',$3,$4,$5,$6,$7,$8)
               ON CONFLICT (solar_system_id,activity,observed_at,source_checksum) DO NOTHING"#,
        )
        .bind(Uuid::new_v4())
        .bind(solar_system_id)
        .bind(cost_index)
        .bind(observed_at)
        .bind(next_refresh_at)
        .bind(etag)
        .bind(checksum)
        .bind("https://esi.evetech.net/latest/industry/systems/")
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        sqlx::query(
            r#"UPDATE industry_system_cost_index_registrations
               SET refresh_state='current',observed_at=$2,
                   next_refresh_at=GREATEST($3,$5),refresh_not_before=$5,
                   consecutive_failures=0,
                   priority_requested_at=NULL,
                   lease_expires_at=NULL,last_error=NULL,updated_at=$2
               WHERE solar_system_id=$1 AND refresh_state='refreshing' AND last_attempted_at=$4"#,
        )
        .bind(solar_system_id)
        .bind(observed_at)
        .bind(next_refresh_at)
        .bind(claim)
        .bind(cache_expires_at)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        tx.commit().await.map_err(map_sqlx)?;
        Ok(true)
    }

    pub async fn fail_system_cost_index_refresh(
        &self,
        solar_system_id: i64,
        claim: DateTime<Utc>,
        attempted_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        error_message: &str,
    ) -> Result<bool, InventoryError> {
        let error_message: String = error_message.chars().take(500).collect();
        sqlx::query(
            r#"UPDATE industry_system_cost_index_registrations
               SET refresh_state='failed',last_attempted_at=$2,
                   next_refresh_at=GREATEST(
                     $3,
                     $2 + LEAST(
                       interval '1 minute' * power(2, LEAST(consecutive_failures, 16)),
                       interval '6 hours'
                     )
                   ),
                   refresh_not_before=GREATEST(
                     $3,
                     $2 + LEAST(
                       interval '1 minute' * power(2, LEAST(consecutive_failures, 16)),
                       interval '6 hours'
                     )
                   ),
                   consecutive_failures=consecutive_failures + 1,
                   priority_requested_at=NULL,
                   lease_expires_at=NULL,last_error=$4,updated_at=$2
               WHERE solar_system_id=$1 AND refresh_state='refreshing' AND last_attempted_at=$5"#,
        )
        .bind(solar_system_id)
        .bind(attempted_at)
        .bind(next_refresh_at)
        .bind(error_message)
        .bind(claim)
        .execute(&self.pool)
        .await
        .map(|result| result.rows_affected() == 1)
        .map_err(map_sqlx)
    }

    pub async fn latest_system_cost_index(
        &self,
        solar_system_id: i64,
    ) -> Result<Option<(Decimal, DateTime<Utc>)>, InventoryError> {
        sqlx::query_as(
            r#"SELECT cost_index,observed_at
               FROM industry_system_cost_index_observations
               WHERE solar_system_id=$1 AND activity='manufacturing'
               ORDER BY observed_at DESC LIMIT 1"#,
        )
        .bind(solar_system_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)
    }

    pub async fn save_adjusted_prices(
        &self,
        prices: &[iskworks_esi::AdjustedPrice],
        observed_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        etag: Option<&str>,
    ) -> Result<(), InventoryError> {
        let source_checksum = etag.unwrap_or("esi-markets-prices");
        for chunk in prices.chunks(4_000) {
            let mut query = QueryBuilder::new(
                "INSERT INTO industry_adjusted_price_observations \
                 (id, type_id, adjusted_price, observed_at, expires_at, etag, source_checksum, source_url) ",
            );
            query.push_values(chunk, |mut row, price| {
                row.push_bind(Uuid::new_v4())
                    .push_bind(price.type_id)
                    .push_bind(price.adjusted_price)
                    .push_bind(observed_at)
                    .push_bind(expires_at)
                    .push_bind(etag)
                    .push_bind(source_checksum)
                    .push_bind("https://esi.evetech.net/latest/markets/prices/");
            });
            query
                .push(" ON CONFLICT (type_id, observed_at, source_checksum) DO NOTHING")
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sqlx)?;
        }
        Ok(())
    }

    pub async fn complete_assets(
        &self,
        run: &EsiSyncRun,
        pages: u32,
        observations: &[AssetObservation],
        etag: Option<&str>,
        skipped_blueprints: u64,
    ) -> Result<SyncCompletion, InventoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        let snapshot_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO esi_asset_snapshots (
              id, connection_id, sync_run_id, observed_at, status, page_count
            ) VALUES ($1,$2,$3,now(),'collecting',$4)
            "#,
        )
        .bind(snapshot_id)
        .bind(run.connection_id.0)
        .bind(run.id.0)
        .bind(i32::try_from(pages).map_err(|_| InventoryError::ArithmeticOverflow)?)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        for asset in observations {
            let payload = serde_json::to_value(asset).map_err(map_json)?;
            sqlx::query(
                r#"
                INSERT INTO esi_asset_observations (
                  snapshot_id, source_item_id, type_id, quantity, location_id,
                  location_type, location_flag, is_singleton, is_blueprint_copy,
                  raw_payload, source_checksum
                ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
                "#,
            )
            .bind(snapshot_id)
            .bind(asset.item_id)
            .bind(asset.type_id)
            .bind(asset.quantity)
            .bind(asset.location_id)
            .bind(&asset.location_type)
            .bind(&asset.location_flag)
            .bind(asset.is_singleton)
            .bind(asset.is_blueprint_copy)
            .bind(&payload)
            .bind(checksum(&payload))
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;
        }
        // Resolve the container hierarchy once, before the snapshot becomes
        // readable through `asset_browser_current`.
        sqlx::query("SELECT refresh_esi_asset_hierarchy($1)")
            .bind(snapshot_id)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;
        // Only the active snapshot is ever read, so the one this sync
        // supersedes and any earlier failed attempts go now (observations
        // and hierarchy cascade). Older leftovers are the retention sweep's.
        sqlx::query(
            "DELETE FROM esi_asset_snapshots WHERE connection_id = $1 AND id <> $2 AND (active OR status <> 'complete')",
        )
        .bind(run.connection_id.0)
        .bind(snapshot_id)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        sqlx::query(
            "UPDATE esi_asset_snapshots SET status = 'complete', active = true, completed_at = now(), row_count = $2 WHERE id = $1",
        )
        .bind(snapshot_id)
        .bind(i64::try_from(observations.len()).map_err(|_| InventoryError::ArithmeticOverflow)?)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        sqlx::query(
            r#"
            INSERT INTO esi_sync_checkpoints (connection_id, endpoint_kind, last_successful_sync_at, etag, page_count)
            VALUES ($1,'assets',now(),$2,$3)
            ON CONFLICT (connection_id, endpoint_kind) DO UPDATE SET
              last_successful_sync_at = now(), etag = EXCLUDED.etag, page_count = EXCLUDED.page_count
            "#,
        )
        .bind(run.connection_id.0)
        .bind(etag)
        .bind(i32::try_from(pages).map_err(|_| InventoryError::ArithmeticOverflow)?)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        let summary = match skipped_blueprints {
            0 => "Asset snapshot synchronized.".to_string(),
            1 => "Asset snapshot synchronized. 1 blueprint skipped: ESI reported an invalid ME/TE."
                .to_string(),
            n => format!(
                "Asset snapshot synchronized. {n} blueprints skipped: ESI reported an invalid ME/TE."
            ),
        };
        finalize_run(&mut tx, run.id, observations.len() as u64, 0, &summary).await?;
        if skipped_blueprints > 0 {
            sqlx::query("UPDATE esi_sync_runs SET skipped_count = $2 WHERE id = $1")
                .bind(run.id.0)
                .bind(
                    i64::try_from(skipped_blueprints)
                        .map_err(|_| InventoryError::ArithmeticOverflow)?,
                )
                .execute(&mut *tx)
                .await
                .map_err(map_sqlx)?;
        }
        tx.commit().await.map_err(map_sqlx)?;
        Ok(SyncCompletion {
            imported: observations.len() as u64,
            ..Default::default()
        })
    }

    /// One durable row per `(workspace_id, owner_id, eve_item_id)`: upsert
    /// every blueprint ESI reports and delete the rows for any this
    /// connection no longer reports (sold / moved / consumed). The row `id`
    /// is preserved across syncs, so a Build's frozen
    /// `blueprintSelection.observationId` keeps resolving; `observed_at`
    /// tracks the last confirming sync while `imported_at` stays the
    /// first-seen time. The full blueprint list is always present when this
    /// runs -- `EsiSyncService` fetches every page before calling it and
    /// propagates any page error first -- so an absent blueprint is a real
    /// disappearance, not a partial sync.
    ///
    /// `retained_item_ids` are blueprints ESI did report but the caller
    /// could not use (e.g. out-of-range ME/TE): their existing rows are kept
    /// as they were rather than deleted as gone.
    pub async fn complete_blueprints(
        &self,
        connection: &ConnectedCharacter,
        observations: &[iskworks_esi::BlueprintAssetObservation],
        retained_item_ids: &[i64],
    ) -> Result<(), InventoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        let observed_at = crate::db_now();

        let seen_item_ids: Vec<i64> = observations
            .iter()
            .map(|o| o.item_id)
            .chain(retained_item_ids.iter().copied())
            .collect();
        sqlx::query(
            r#"DELETE FROM blueprint_observations
               WHERE workspace_id=$1 AND owner_id=$2 AND connection_id=$3
                 AND eve_item_id <> ALL($4)"#,
        )
        .bind(connection.workspace_id.0)
        .bind(connection.owner_id.0)
        .bind(connection.id.0)
        .bind(&seen_item_ids)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        for observation in observations {
            let payload = serde_json::to_value(observation).map_err(map_json)?;
            let kind = match observation.quantity {
                -1 => "original",
                -2 => "copy",
                _ => "unknown",
            };
            let licensed_runs =
                (kind == "copy" && observation.runs > 0).then_some(observation.runs);
            sqlx::query(
                r#"INSERT INTO blueprint_observations (
                     id,workspace_id,owner_id,connection_id,eve_item_id,blueprint_type_id,
                     captured_blueprint_name,blueprint_kind,material_efficiency,time_efficiency,
                     licensed_runs,location_id,location_flag,captured_location_name,
                     observed_at,imported_at,source_payload,source_checksum
                   ) SELECT $1,$2,$3,$4,$5,$6,COALESCE(t.name_en,'Unknown EVE type '||$6::text),
                            $7,$8,$9,$10,$11,$12,mln.location_name,$13,$13,$14,$15
                     FROM (SELECT 1) x
                     LEFT JOIN sde_imports si ON si.active
                     LEFT JOIN sde_types t ON t.import_id=si.id AND t.type_id=$6
                     LEFT JOIN market_location_names mln ON mln.workspace_id=$2 AND mln.location_id=$11
                   ON CONFLICT (workspace_id,owner_id,eve_item_id) DO UPDATE SET
                     connection_id=EXCLUDED.connection_id,
                     blueprint_type_id=EXCLUDED.blueprint_type_id,
                     captured_blueprint_name=EXCLUDED.captured_blueprint_name,
                     blueprint_kind=EXCLUDED.blueprint_kind,
                     material_efficiency=EXCLUDED.material_efficiency,
                     time_efficiency=EXCLUDED.time_efficiency,
                     licensed_runs=EXCLUDED.licensed_runs,
                     location_id=EXCLUDED.location_id,
                     location_flag=EXCLUDED.location_flag,
                     captured_location_name=EXCLUDED.captured_location_name,
                     observed_at=EXCLUDED.observed_at,
                     source_payload=EXCLUDED.source_payload,
                     source_checksum=EXCLUDED.source_checksum"#,
            )
            .bind(Uuid::new_v4()).bind(connection.workspace_id.0).bind(connection.owner_id.0)
            .bind(connection.id.0).bind(observation.item_id).bind(observation.type_id)
            .bind(kind).bind(observation.material_efficiency).bind(observation.time_efficiency)
            .bind(licensed_runs).bind(observation.location_id).bind(&observation.location_flag)
            .bind(observed_at).bind(&payload).bind(checksum(&payload))
            .execute(&mut *tx).await.map_err(map_sqlx)?;
        }
        tx.commit().await.map_err(map_sqlx)
    }

    pub async fn cache_structure_names(
        &self,
        connection: &ConnectedCharacter,
        structures: &[StructureInformation],
    ) -> Result<(), InventoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        for structure in structures {
            sqlx::query(
                r#"INSERT INTO market_location_names (
                     workspace_id,location_id,location_name,owner_id,solar_system_id,
                     structure_type_id,resolved_by_connection_id,resolved_at,updated_at
                   ) VALUES ($1,$2,$3,$4,$5,$6,$7,now(),now())
                   ON CONFLICT (workspace_id,location_id) DO UPDATE SET
                     location_name=EXCLUDED.location_name,
                     owner_id=EXCLUDED.owner_id,
                     solar_system_id=EXCLUDED.solar_system_id,
                     structure_type_id=EXCLUDED.structure_type_id,
                     resolved_by_connection_id=EXCLUDED.resolved_by_connection_id,
                     resolved_at=EXCLUDED.resolved_at,
                     updated_at=now()"#,
            )
            .bind(connection.workspace_id.0)
            .bind(structure.structure_id)
            .bind(&structure.name)
            .bind(structure.owner_id)
            .bind(structure.solar_system_id)
            .bind(structure.type_id)
            .bind(connection.id.0)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;
        }
        tx.commit().await.map_err(map_sqlx)
    }

    /// Remembers that this character can't look these structures up (ESI
    /// answered 403 or 404) until `denied_until`.
    pub async fn record_structure_lookup_denials(
        &self,
        connection: &ConnectedCharacter,
        structure_ids: &[i64],
        denied_until: DateTime<Utc>,
    ) -> Result<(), InventoryError> {
        if structure_ids.is_empty() {
            return Ok(());
        }
        sqlx::query(
            r#"INSERT INTO esi_structure_lookup_denials (connection_id,structure_id,denied_until)
               SELECT $1, structure_id, $3 FROM unnest($2::bigint[]) AS ids(structure_id)
               ON CONFLICT (connection_id,structure_id) DO UPDATE SET
                 denied_until=EXCLUDED.denied_until"#,
        )
        .bind(connection.id.0)
        .bind(structure_ids)
        .bind(denied_until)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(map_sqlx)
    }

    /// Which of `structure_ids` this character should look up now: those
    /// without a workspace name resolved since `stale_before` (pass `None`
    /// to ignore names entirely) and not denied to this character as of
    /// `now`.
    pub async fn structures_to_look_up(
        &self,
        connection: &ConnectedCharacter,
        structure_ids: &[i64],
        stale_before: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<Vec<i64>, InventoryError> {
        if structure_ids.is_empty() {
            return Ok(Vec::new());
        }
        sqlx::query_scalar(
            r#"SELECT ids.structure_id
               FROM unnest($3::bigint[]) AS ids(structure_id)
               WHERE NOT EXISTS (
                   SELECT 1 FROM esi_structure_lookup_denials denial
                   WHERE denial.connection_id=$2 AND denial.structure_id=ids.structure_id
                     AND denial.denied_until > $5
                 )
                 AND ($4::timestamptz IS NULL OR NOT EXISTS (
                   SELECT 1 FROM market_location_names mln
                   WHERE mln.workspace_id=$1 AND mln.location_id=ids.structure_id
                     AND mln.resolved_at >= $4
                 ))
               ORDER BY ids.structure_id"#,
        )
        .bind(connection.workspace_id.0)
        .bind(connection.id.0)
        .bind(structure_ids)
        .bind(stale_before)
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)
    }

    pub async fn unresolved_structure_ids(
        &self,
        connection: &ConnectedCharacter,
    ) -> Result<Vec<i64>, InventoryError> {
        sqlx::query_scalar(
            r#"SELECT location_id
               FROM (
                 SELECT bo.location_id
                 FROM blueprint_observations bo
                 LEFT JOIN market_location_names mln
                   ON mln.workspace_id=bo.workspace_id AND mln.location_id=bo.location_id
                 WHERE bo.workspace_id=$1 AND bo.owner_id=$2
                   AND bo.location_id >= 1000000000000
                   AND mln.location_id IS NULL
                 UNION
                 SELECT asset.effective_location_id
                 FROM asset_browser_current asset
                 LEFT JOIN market_location_names mln
                   ON mln.workspace_id=asset.workspace_id
                  AND mln.location_id=asset.effective_location_id
                 WHERE asset.workspace_id=$1 AND asset.connection_id=$3
                   AND asset.effective_location_type='item'
                   AND asset.hierarchy_state='missing_parent'
                   AND asset.effective_location_id >= 1000000000000
                   AND mln.location_id IS NULL
               ) unresolved(location_id)
               ORDER BY location_id"#,
        )
        .bind(connection.workspace_id.0)
        .bind(connection.owner_id.0)
        .bind(connection.id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)
    }

    pub async fn mark_incomplete_asset_sync(
        &self,
        run: &EsiSyncRun,
        pages: u32,
        observations: &[AssetObservation],
        summary: &str,
    ) -> Result<(), InventoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        let snapshot_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO esi_asset_snapshots (id, connection_id, sync_run_id, observed_at, completed_at, status, page_count, row_count, active) VALUES ($1,$2,$3,now(),now(),'incomplete',$4,$5,false)",
        )
        .bind(snapshot_id)
        .bind(run.connection_id.0)
        .bind(run.id.0)
        .bind(i32::try_from(pages.max(1)).map_err(|_| InventoryError::ArithmeticOverflow)?)
        .bind(i64::try_from(observations.len()).map_err(|_| InventoryError::ArithmeticOverflow)?)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        sqlx::query("UPDATE esi_sync_runs SET status = 'failed', phase = 'Failed', completed_at = now(), error_count = 1, error_code = 'pagination_incomplete', summary = $2 WHERE id = $1")
            .bind(run.id.0)
            .bind(summary)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;
        tx.commit().await.map_err(map_sqlx)?;
        Ok(())
    }
}
