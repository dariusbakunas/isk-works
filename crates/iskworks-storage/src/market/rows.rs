//! Storage-only row structs and their `into_*` domain mappers.

use chrono::{DateTime, Utc};
use iskworks_core::{
    ImportedMarketFile, ImportedMarketFileId, MarketAccessState, MarketCoverageItem, MarketError,
    MarketImportBatch, MarketImportBatchId, MarketOrderObservationId, MarketOrderView,
    MarketPriceSource, MarketPriceSourceConfig, MarketStructureListing, Money, PriceSourceId,
    WorkspaceId,
};
use rust_decimal::Decimal;
use uuid::Uuid;

use super::convert::*;

#[derive(sqlx::FromRow)]
pub(super) struct KnownMarketLocationRow {
    pub(super) location_id: i64,
    pub(super) location_name: String,
    pub(super) solar_system_id: i64,
    pub(super) solar_system_name: String,
    pub(super) structure_type_id: Option<i64>,
}

#[derive(sqlx::FromRow)]
pub(super) struct MarketStructureListingRow {
    pub(super) location_id: i64,
    pub(super) location_name: String,
    pub(super) structure_type_id: Option<i64>,
    pub(super) structure_type_name: Option<String>,
    pub(super) solar_system_id: i64,
    pub(super) solar_system_name: Option<String>,
    pub(super) region_id: Option<i64>,
    pub(super) region_name: Option<String>,
    pub(super) security_class: String,
    pub(super) market_access_connection_id: Option<Uuid>,
    pub(super) access_character_name: Option<String>,
    pub(super) access_connection_status: Option<String>,
    pub(super) market_access_checked_at: Option<DateTime<Utc>>,
}

impl MarketStructureListingRow {
    pub(super) fn into_listing(self) -> MarketStructureListing {
        let access_state = match (
            self.market_access_connection_id,
            self.access_connection_status.as_deref(),
        ) {
            (None, _) => MarketAccessState::Unknown,
            (Some(_), Some("connected")) => MarketAccessState::Confirmed,
            (Some(_), _) => MarketAccessState::Expired,
        };
        MarketStructureListing {
            location_id: self.location_id,
            location_name: self.location_name,
            structure_type_id: self.structure_type_id,
            structure_type_name: self.structure_type_name,
            solar_system_id: self.solar_system_id,
            solar_system_name: self.solar_system_name,
            region_id: self.region_id,
            region_name: self.region_name,
            security_class: self.security_class,
            access_state,
            access_character_name: self.access_character_name,
            access_checked_at: self.market_access_checked_at,
        }
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct ScopeFreshnessRow {
    pub(super) region_id: i64,
    pub(super) location_id: i64,
    pub(super) tracked_type_count: i64,
    pub(super) observed_type_count: i64,
    pub(super) most_recent_observed_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
pub(super) struct ScopeSourceRow {
    pub(super) price_source_id: Uuid,
    pub(super) location_id: i64,
}

#[derive(sqlx::FromRow)]
pub(super) struct CoverageRow {
    pub(super) type_id: i64,
    pub(super) type_name: String,
    pub(super) refresh_state: String,
    pub(super) observed_at: Option<DateTime<Utc>>,
    pub(super) last_attempted_at: Option<DateTime<Utc>>,
    pub(super) next_refresh_at: Option<DateTime<Utc>>,
    pub(super) last_error: Option<String>,
    /// ETag of the batch at `coverage.last_completed_batch_id` (the
    /// `LEFT JOIN`, so `None` for a never-completed row). Threaded into
    /// `MarketCoverageItem::prior_etag` for conditional refresh.
    pub(super) etag: Option<String>,
    /// `coverage.revalidated_at` -- last ESI 304 confirmation of the
    /// completed batch. Threaded into `MarketCoverageItem::revalidated_at`.
    pub(super) revalidated_at: Option<DateTime<Utc>>,
    pub(super) order_count: i64,
    pub(super) buy_order_count: i64,
    pub(super) sell_order_count: i64,
}

impl CoverageRow {
    pub(super) fn into_item(self) -> Result<MarketCoverageItem, MarketError> {
        Ok(MarketCoverageItem {
            type_id: self.type_id,
            type_name: self.type_name,
            refresh_state: parse_refresh_state(&self.refresh_state)?,
            observed_at: self.observed_at,
            last_attempted_at: self.last_attempted_at,
            next_refresh_at: self.next_refresh_at,
            last_error: self.last_error,
            prior_etag: self.etag,
            revalidated_at: self.revalidated_at,
            order_count: u64_from_i64(self.order_count)?,
            buy_order_count: u64_from_i64(self.buy_order_count)?,
            sell_order_count: u64_from_i64(self.sell_order_count)?,
        })
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct BatchRow {
    pub(super) id: Uuid,
    pub(super) workspace_id: Uuid,
    pub(super) observed_at_min: DateTime<Utc>,
    pub(super) observed_at_max: DateTime<Utc>,
    pub(super) imported_at: DateTime<Utc>,
    pub(super) file_count: i32,
    pub(super) item_count: i32,
    pub(super) location_count: i32,
    pub(super) observation_count: i64,
    pub(super) skipped_duplicate_file_count: i32,
    pub(super) warnings_json: serde_json::Value,
}

impl BatchRow {
    pub(super) fn into_batch(
        self,
        files: Vec<ImportedMarketFile>,
    ) -> Result<MarketImportBatch, MarketError> {
        Ok(MarketImportBatch {
            id: MarketImportBatchId(self.id),
            workspace_id: WorkspaceId(self.workspace_id),
            observed_at_min: self.observed_at_min,
            observed_at_max: self.observed_at_max,
            imported_at: self.imported_at,
            file_count: u64_from_i32(self.file_count)?,
            item_count: u64_from_i32(self.item_count)?,
            location_count: u64_from_i32(self.location_count)?,
            observation_count: u64_from_i64(self.observation_count)?,
            skipped_duplicate_file_count: u64_from_i32(self.skipped_duplicate_file_count)?,
            warnings: serde_json::from_value(self.warnings_json).map_err(map_json)?,
            files,
        })
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct FileRow {
    pub(super) id: Uuid,
    pub(super) batch_id: Uuid,
    pub(super) sanitized_filename: String,
    pub(super) file_checksum: String,
    pub(super) normalized_checksum: String,
    pub(super) file_size_bytes: i64,
    pub(super) observed_at: DateTime<Utc>,
    pub(super) timestamp_source: String,
    pub(super) type_id: i64,
    pub(super) captured_type_name: String,
    pub(super) location_id: i64,
    pub(super) captured_location_name: Option<String>,
    pub(super) solar_system_id: i64,
    pub(super) captured_solar_system_name: Option<String>,
    pub(super) region_id: i64,
    pub(super) captured_region_name: Option<String>,
    pub(super) row_count: i64,
    pub(super) buy_order_count: i64,
    pub(super) sell_order_count: i64,
    pub(super) imported_at: DateTime<Utc>,
}

impl FileRow {
    pub(super) fn into_file(self) -> Result<ImportedMarketFile, MarketError> {
        Ok(ImportedMarketFile {
            id: ImportedMarketFileId(self.id),
            batch_id: MarketImportBatchId(self.batch_id),
            filename: self.sanitized_filename,
            file_checksum: self.file_checksum,
            normalized_checksum: self.normalized_checksum,
            file_size_bytes: u64_from_i64(self.file_size_bytes)?,
            observed_at: self.observed_at,
            timestamp_source: parse_timestamp_source(&self.timestamp_source)?,
            type_id: self.type_id,
            type_name: self.captured_type_name,
            location_id: self.location_id,
            location_name: self
                .captured_location_name
                .unwrap_or_else(|| format!("Structure {}", self.location_id)),
            solar_system_id: self.solar_system_id,
            solar_system_name: self.captured_solar_system_name,
            region_id: self.region_id,
            region_name: self.captured_region_name,
            row_count: u64_from_i64(self.row_count)?,
            buy_order_count: u64_from_i64(self.buy_order_count)?,
            sell_order_count: u64_from_i64(self.sell_order_count)?,
            imported_at: self.imported_at,
        })
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct BookFileRow {
    pub(super) id: Uuid,
    pub(super) batch_id: Uuid,
    pub(super) captured_type_name: String,
    pub(super) location_id: i64,
    pub(super) captured_location_name: Option<String>,
    pub(super) solar_system_id: i64,
    pub(super) region_id: i64,
    pub(super) observed_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub(super) struct EsiBookBatchRow {
    pub(super) id: Uuid,
    pub(super) captured_type_name: String,
    pub(super) location_id: i64,
    pub(super) location_alias: String,
    pub(super) solar_system_id: i64,
    pub(super) region_id: i64,
    pub(super) observed_at: DateTime<Utc>,
    pub(super) revalidated_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
pub(super) struct OrderRow {
    pub(super) id: Uuid,
    pub(super) batch_id: Uuid,
    pub(super) imported_file_id: Uuid,
    pub(super) order_id: i64,
    pub(super) type_id: i64,
    pub(super) captured_type_name: String,
    pub(super) order_side: String,
    pub(super) price: Decimal,
    pub(super) remaining_volume: i64,
    pub(super) entered_volume: i64,
    pub(super) minimum_volume: i64,
    pub(super) order_range: i32,
    pub(super) issued_at: DateTime<Utc>,
    pub(super) duration_days: i32,
    pub(super) observed_at: DateTime<Utc>,
    pub(super) location_id: i64,
    pub(super) solar_system_id: i64,
    pub(super) region_id: i64,
    pub(super) jumps: i32,
}

impl OrderRow {
    pub(super) fn into_order(self) -> Result<MarketOrderView, MarketError> {
        Ok(MarketOrderView {
            observation_id: Some(MarketOrderObservationId(self.id)),
            import_batch_id: Some(MarketImportBatchId(self.batch_id)),
            imported_file_id: Some(ImportedMarketFileId(self.imported_file_id)),
            order_id: self.order_id,
            type_id: self.type_id,
            type_name: self.captured_type_name,
            side: parse_order_side(&self.order_side)?,
            price: Money(self.price),
            remaining_volume: u64_from_i64(self.remaining_volume)?,
            entered_volume: u64_from_i64(self.entered_volume)?,
            minimum_volume: u64_from_i64(self.minimum_volume)?,
            order_range: self.order_range,
            issued_at: self.issued_at,
            duration_days: self.duration_days,
            observed_at: self.observed_at,
            revalidated_at: None,
            location_id: self.location_id,
            solar_system_id: self.solar_system_id,
            region_id: self.region_id,
            jumps: self.jumps,
        })
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct EsiOrderRow {
    pub(super) id: Uuid,
    pub(super) order_id: i64,
    pub(super) type_id: i64,
    pub(super) captured_type_name: String,
    pub(super) order_side: String,
    pub(super) price: Decimal,
    pub(super) remaining_volume: i64,
    pub(super) entered_volume: i64,
    pub(super) minimum_volume: i64,
    pub(super) order_range: i32,
    pub(super) issued_at: DateTime<Utc>,
    pub(super) duration_days: i32,
    pub(super) observed_at: DateTime<Utc>,
    pub(super) location_id: i64,
    pub(super) solar_system_id: i64,
    pub(super) region_id: i64,
    pub(super) jumps: i32,
}

impl EsiOrderRow {
    pub(super) fn into_order(self) -> Result<MarketOrderView, MarketError> {
        Ok(MarketOrderView {
            observation_id: Some(MarketOrderObservationId(self.id)),
            import_batch_id: None,
            imported_file_id: None,
            order_id: self.order_id,
            type_id: self.type_id,
            type_name: self.captured_type_name,
            side: parse_order_side(&self.order_side)?,
            price: Money(self.price),
            remaining_volume: u64_from_i64(self.remaining_volume)?,
            entered_volume: u64_from_i64(self.entered_volume)?,
            minimum_volume: u64_from_i64(self.minimum_volume)?,
            order_range: self.order_range,
            issued_at: self.issued_at,
            duration_days: self.duration_days,
            observed_at: self.observed_at,
            revalidated_at: None,
            location_id: self.location_id,
            solar_system_id: self.solar_system_id,
            region_id: self.region_id,
            jumps: self.jumps,
        })
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct BatchedBookFileRow {
    pub(super) id: Uuid,
    pub(super) batch_id: Uuid,
    pub(super) type_id: i64,
    pub(super) captured_type_name: String,
    pub(super) location_id: i64,
    pub(super) captured_location_name: Option<String>,
    pub(super) solar_system_id: i64,
    pub(super) region_id: i64,
    pub(super) observed_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub(super) struct BatchedEsiBookBatchRow {
    pub(super) id: Uuid,
    pub(super) type_id: i64,
    pub(super) captured_type_name: String,
    pub(super) location_id: i64,
    pub(super) location_alias: String,
    pub(super) solar_system_id: i64,
    pub(super) region_id: i64,
    pub(super) observed_at: DateTime<Utc>,
    pub(super) revalidated_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
pub(super) struct BatchedEsiOrderRow {
    pub(super) id: Uuid,
    pub(super) observation_batch_id: Uuid,
    pub(super) order_id: i64,
    pub(super) type_id: i64,
    pub(super) captured_type_name: String,
    pub(super) order_side: String,
    pub(super) price: Decimal,
    pub(super) remaining_volume: i64,
    pub(super) entered_volume: i64,
    pub(super) minimum_volume: i64,
    pub(super) order_range: i32,
    pub(super) issued_at: DateTime<Utc>,
    pub(super) duration_days: i32,
    pub(super) observed_at: DateTime<Utc>,
    pub(super) location_id: i64,
    pub(super) solar_system_id: i64,
    pub(super) region_id: i64,
    pub(super) jumps: i32,
}

impl BatchedEsiOrderRow {
    pub(super) fn into_order(self) -> Result<MarketOrderView, MarketError> {
        Ok(MarketOrderView {
            observation_id: Some(MarketOrderObservationId(self.id)),
            import_batch_id: None,
            imported_file_id: None,
            order_id: self.order_id,
            type_id: self.type_id,
            type_name: self.captured_type_name,
            side: parse_order_side(&self.order_side)?,
            price: Money(self.price),
            remaining_volume: u64_from_i64(self.remaining_volume)?,
            entered_volume: u64_from_i64(self.entered_volume)?,
            minimum_volume: u64_from_i64(self.minimum_volume)?,
            order_range: self.order_range,
            issued_at: self.issued_at,
            duration_days: self.duration_days,
            observed_at: self.observed_at,
            revalidated_at: None,
            location_id: self.location_id,
            solar_system_id: self.solar_system_id,
            region_id: self.region_id,
            jumps: self.jumps,
        })
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct MarketSourceRow {
    pub(super) id: Uuid,
    pub(super) workspace_id: Uuid,
    pub(super) display_name: String,
    pub(super) description: String,
    pub(super) source_kind: String,
    pub(super) revision: i64,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
    pub(super) item_count: i64,
    pub(super) location_id: i64,
    pub(super) solar_system_id: i64,
    pub(super) region_id: i64,
    pub(super) location_alias: String,
    pub(super) pricing_policy: String,
    pub(super) coverage_policy: String,
    pub(super) observation_mode: String,
    pub(super) pinned_batch_id: Option<Uuid>,
    pub(super) fresh_after_hours: i32,
    pub(super) stale_after_hours: i32,
    pub(super) archived_at: Option<DateTime<Utc>>,
    pub(super) last_snapshot_at: Option<DateTime<Utc>>,
}

impl MarketSourceRow {
    pub(super) fn into_source(self) -> Result<MarketPriceSource, MarketError> {
        let id = PriceSourceId(self.id);
        let workspace_id = WorkspaceId(self.workspace_id);
        Ok(MarketPriceSource {
            id,
            workspace_id,
            name: self.display_name,
            description: self.description,
            kind: parse_price_source_kind(&self.source_kind)?,
            revision: u64_from_i64(self.revision)?,
            item_count: u64_from_i64(self.item_count)?,
            config: MarketPriceSourceConfig {
                price_source_id: id,
                workspace_id,
                location_id: self.location_id,
                solar_system_id: self.solar_system_id,
                region_id: self.region_id,
                location_alias: self.location_alias,
                pricing_policy: parse_pricing_policy(&self.pricing_policy)?,
                coverage_policy: parse_coverage_policy(&self.coverage_policy)?,
                observation_mode: parse_observation_mode(&self.observation_mode)?,
                pinned_batch_id: self.pinned_batch_id.map(MarketImportBatchId),
                fresh_after_hours: u32_from_i32(self.fresh_after_hours)?,
                stale_after_hours: u32_from_i32(self.stale_after_hours)?,
                archived_at: self.archived_at,
                last_snapshot_at: self.last_snapshot_at,
            },
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}
