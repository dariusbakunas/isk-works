//! EVE-client market-import persistence: imported files, import batches,
//! and the commit transaction. Import idempotency and provenance are
//! unchanged.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use iskworks_core::{
    ImportedMarketFileId, MarketError, MarketImportBatch, MarketImportBatchId,
    MarketOrderObservationId, MarketOrderSide, ResolvedMarketExport, WorkspaceId,
};
use serde_json::json;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::convert::*;
use super::rows::*;
use super::PgMarketRepository;

impl PgMarketRepository {
    pub(super) async fn load_batch(
        &self,
        workspace_id: WorkspaceId,
        batch_id: MarketImportBatchId,
    ) -> Result<MarketImportBatch, MarketError> {
        let row = sqlx::query_as::<_, BatchRow>(
            r#"
            SELECT id,workspace_id,observed_at_min,observed_at_max,imported_at,
                   file_count,item_count,location_count,observation_count,
                   skipped_duplicate_file_count,warnings_json
            FROM market_import_batches
            WHERE workspace_id=$1 AND id=$2
            "#,
        )
        .bind(workspace_id.0)
        .bind(batch_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?
        .ok_or(MarketError::ImportNotFound)?;
        let files = sqlx::query_as::<_, FileRow>(
            r#"
            SELECT mif.id,mif.batch_id,mif.sanitized_filename,mif.file_checksum,
                   mif.normalized_checksum,mif.file_size_bytes,mif.observed_at,
                   mif.timestamp_source,mif.type_id,mif.captured_type_name,mif.location_id,
                   COALESCE(mif.captured_location_name,mln.location_name) captured_location_name,
                   mif.solar_system_id,mif.captured_solar_system_name,mif.region_id,
                   mif.captured_region_name,mif.row_count,mif.buy_order_count,
                   mif.sell_order_count,mif.imported_at
            FROM market_import_files mif
            LEFT JOIN market_location_names mln
              ON mln.workspace_id=mif.workspace_id AND mln.location_id=mif.location_id
            WHERE mif.workspace_id=$1 AND mif.batch_id=$2
            ORDER BY mif.sanitized_filename,mif.id
            "#,
        )
        .bind(workspace_id.0)
        .bind(batch_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        .into_iter()
        .map(FileRow::into_file)
        .collect::<Result<Vec<_>, _>>()?;
        row.into_batch(files)
    }

    pub(super) async fn imported_file_checksums(
        &self,
        workspace_id: WorkspaceId,
        checksums: &[String],
    ) -> Result<BTreeSet<String>, MarketError> {
        if checksums.is_empty() {
            return Ok(BTreeSet::new());
        }
        sqlx::query_scalar::<_, String>(
            "SELECT file_checksum FROM market_import_files WHERE workspace_id=$1 AND file_checksum=ANY($2)",
        )
        .bind(workspace_id.0)
        .bind(checksums)
        .fetch_all(&self.pool)
        .await
        .map(|items| items.into_iter().collect())
        .map_err(map_sqlx)
    }

    pub(super) async fn commit_import(
        &self,
        workspace_id: WorkspaceId,
        files: Vec<ResolvedMarketExport>,
        skipped_duplicate_files: u64,
        warnings: Vec<String>,
    ) -> Result<MarketImportBatch, MarketError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        let batch_id = MarketImportBatchId::new();
        let imported_at = crate::db_now();
        let observed_at_min = files
            .iter()
            .map(|file| file.parsed.observed_at)
            .min()
            .ok_or_else(|| MarketError::Persistence("empty import batch".to_string()))?;
        let observed_at_max = files
            .iter()
            .map(|file| file.parsed.observed_at)
            .max()
            .ok_or_else(|| MarketError::Persistence("empty import batch".to_string()))?;
        let item_count = files
            .iter()
            .map(|file| file.parsed.type_id)
            .collect::<BTreeSet<_>>()
            .len();
        let location_count = files
            .iter()
            .map(|file| file.parsed.location_id)
            .collect::<BTreeSet<_>>()
            .len();
        let observation_count = files
            .iter()
            .try_fold(0_u64, |total, file| {
                total.checked_add(file.parsed.orders.len() as u64)
            })
            .ok_or_else(|| MarketError::Persistence("observation count overflow".to_string()))?;
        sqlx::query(
            r#"
            INSERT INTO market_import_batches (
              id,workspace_id,source_kind,status,observed_at_min,observed_at_max,imported_at,
              file_count,item_count,location_count,observation_count,
              skipped_duplicate_file_count,warnings_json
            ) VALUES ($1,$2,'eve_client_market_export',$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
            "#,
        )
        .bind(batch_id.0)
        .bind(workspace_id.0)
        .bind(if warnings.is_empty() {
            "succeeded"
        } else {
            "partially_succeeded"
        })
        .bind(observed_at_min)
        .bind(observed_at_max)
        .bind(imported_at)
        .bind(i32_from_usize(files.len())?)
        .bind(i32_from_usize(item_count)?)
        .bind(i32_from_usize(location_count)?)
        .bind(i64_from_u64(observation_count)?)
        .bind(i32_from_u64(skipped_duplicate_files)?)
        .bind(json!(warnings))
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        for file in &files {
            insert_file(&mut tx, workspace_id, batch_id, file, imported_at).await?;
        }
        tx.commit().await.map_err(map_sqlx)?;
        self.load_batch(workspace_id, batch_id).await
    }

    pub(super) async fn list_imports(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<MarketImportBatch>, MarketError> {
        let ids = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM market_import_batches WHERE workspace_id=$1 ORDER BY imported_at DESC",
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;
        let mut batches = Vec::with_capacity(ids.len());
        for id in ids {
            batches.push(
                self.load_batch(workspace_id, MarketImportBatchId(id))
                    .await?,
            );
        }
        Ok(batches)
    }

    pub(super) async fn get_import(
        &self,
        workspace_id: WorkspaceId,
        batch_id: MarketImportBatchId,
    ) -> Result<MarketImportBatch, MarketError> {
        self.load_batch(workspace_id, batch_id).await
    }
}
async fn insert_file(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: WorkspaceId,
    batch_id: MarketImportBatchId,
    file: &ResolvedMarketExport,
    imported_at: DateTime<Utc>,
) -> Result<(), MarketError> {
    let file_id = ImportedMarketFileId::new();
    let buy_count = file
        .parsed
        .orders
        .iter()
        .filter(|order| order.side == MarketOrderSide::Buy)
        .count();
    let sell_count = file.parsed.orders.len() - buy_count;
    sqlx::query(
        r#"
        INSERT INTO market_import_files (
          id,batch_id,workspace_id,original_filename,sanitized_filename,file_checksum,
          normalized_checksum,file_size_bytes,observed_at,timestamp_source,type_id,
          captured_type_name,location_id,captured_location_name,solar_system_id,
          captured_solar_system_name,region_id,captured_region_name,row_count,
          buy_order_count,sell_order_count,status,imported_at
        ) VALUES (
          $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,NULL,$14,NULL,$15,NULL,$16,$17,$18,'imported',$19
        )
        "#,
    )
    .bind(file_id.0)
    .bind(batch_id.0)
    .bind(workspace_id.0)
    .bind(&file.parsed.safe_filename)
    .bind(&file.parsed.safe_filename)
    .bind(&file.parsed.file_checksum)
    .bind(&file.parsed.normalized_checksum)
    .bind(i64_from_u64(file.parsed.file_size_bytes)?)
    .bind(file.parsed.observed_at)
    .bind(timestamp_source_str(file.parsed.timestamp_source))
    .bind(file.parsed.type_id)
    .bind(&file.type_name)
    .bind(file.parsed.location_id)
    .bind(file.parsed.solar_system_id)
    .bind(file.parsed.region_id)
    .bind(i64_from_usize(file.parsed.orders.len())?)
    .bind(i64_from_usize(buy_count)?)
    .bind(i64_from_usize(sell_count)?)
    .bind(imported_at)
    .execute(&mut **tx)
    .await
    .map_err(map_sqlx)?;

    sqlx::query(
        r#"
        INSERT INTO market_observation_batches (
          id,workspace_id,price_source_id,origin,status,type_id,captured_type_name,
          region_id,solar_system_id,location_id,observed_at,attempted_at,completed_at
        ) VALUES (
          $1,$2,NULL,'eve_client_market_export','completed',$3,$4,$5,$6,$7,$8,$9,$9
        )
        "#,
    )
    .bind(file_id.0)
    .bind(workspace_id.0)
    .bind(file.parsed.type_id)
    .bind(&file.type_name)
    .bind(file.parsed.region_id)
    .bind(file.parsed.solar_system_id)
    .bind(file.parsed.location_id)
    .bind(file.parsed.observed_at)
    .bind(imported_at)
    .execute(&mut **tx)
    .await
    .map_err(map_sqlx)?;

    for order in &file.parsed.orders {
        let proposed_id = MarketOrderObservationId::new();
        let inserted = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO market_order_observations (
              id,workspace_id,observation_batch_id,source_kind,observed_at,imported_at,
              order_id,type_id,captured_type_name,order_side,price,remaining_volume,
              entered_volume,minimum_volume,order_range,issued_at,duration_days,
              location_id,solar_system_id,region_id,jumps,normalized_row_checksum
            ) VALUES (
              $1,$2,$3,'eve_client_market_export',$4,$5,$6,$7,$8,$9,$10,$11,$12,
              $13,$14,$15,$16,$17,$18,$19,$20,$21
            )
            ON CONFLICT (workspace_id,source_kind,observed_at,order_id,normalized_row_checksum)
            DO NOTHING RETURNING id
            "#,
        )
        .bind(proposed_id.0)
        .bind(workspace_id.0)
        .bind(file_id.0)
        .bind(file.parsed.observed_at)
        .bind(imported_at)
        .bind(order.order_id)
        .bind(order.type_id)
        .bind(&file.type_name)
        .bind(order_side_str(order.side))
        .bind(order.price.0)
        .bind(i64_from_u64(order.remaining_volume)?)
        .bind(i64_from_u64(order.entered_volume)?)
        .bind(i64_from_u64(order.minimum_volume)?)
        .bind(order.order_range)
        .bind(order.issued_at)
        .bind(order.duration_days)
        .bind(order.location_id)
        .bind(order.solar_system_id)
        .bind(order.region_id)
        .bind(order.jumps)
        .bind(&order.normalized_row_checksum)
        .fetch_optional(&mut **tx)
        .await
        .map_err(map_sqlx)?;
        let observation_id = match inserted {
            Some(id) => id,
            None => sqlx::query_scalar::<_, Uuid>(
                r#"
                SELECT id FROM market_order_observations
                WHERE workspace_id=$1 AND source_kind='eve_client_market_export'
                  AND observed_at=$2 AND order_id=$3 AND normalized_row_checksum=$4
                "#,
            )
            .bind(workspace_id.0)
            .bind(file.parsed.observed_at)
            .bind(order.order_id)
            .bind(&order.normalized_row_checksum)
            .fetch_one(&mut **tx)
            .await
            .map_err(map_sqlx)?,
        };
        sqlx::query(
            "INSERT INTO market_import_file_observations (market_import_file_id,market_order_observation_id,workspace_id,source_row_number) VALUES ($1,$2,$3,$4)",
        )
        .bind(file_id.0)
        .bind(observation_id)
        .bind(workspace_id.0)
        .bind(i32::try_from(order.source_row_number).map_err(|_| overflow())?)
        .execute(&mut **tx)
        .await
        .map_err(map_sqlx)?;
    }
    Ok(())
}
