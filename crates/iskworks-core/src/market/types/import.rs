use super::*;

/// Defensive-only ceiling on a single "request prices for this category"
/// call (bulk registration against `market_source_coverage`), not a
/// product-level cap: verified live against the real imported SDE, the
/// largest rolled-up market group ("Ship SKINs") has 5,003 items, so this
/// leaves roughly 4x headroom for catalog growth while still catching a
/// pathological/malformed request. Registration and worker-drain are
/// already bounded per `item_batch_size`/the worker's poll loop regardless
/// of how many items are registered in one call -- this exists purely to
/// reject something absurd, not to steer normal category sizes.
pub const MAX_MARKET_GROUP_BULK_REQUEST_ITEMS: usize = 20_000;

pub const MAX_MARKET_FILES_PER_BATCH: usize = 20;
pub const MAX_MARKET_FILE_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_MARKET_BATCH_BYTES: usize = 25 * 1024 * 1024;
pub const MAX_MARKET_ROWS_PER_FILE: usize = 100_000;
pub const MAX_MARKET_ROWS_PER_BATCH: usize = 250_000;
pub const MAX_MARKET_FILENAME_BYTES: usize = 180;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MarketImportBatchId(pub Uuid);

impl MarketImportBatchId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MarketImportBatchId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ImportedMarketFileId(pub Uuid);

impl ImportedMarketFileId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ImportedMarketFileId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MarketOrderObservationId(pub Uuid);

impl MarketOrderObservationId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MarketOrderObservationId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MarketObservationBatchId(pub Uuid);

impl MarketObservationBatchId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MarketObservationBatchId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MarketUpload {
    pub filename: String,
    pub content: Vec<u8>,
    pub user_observed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedMarketOrder {
    pub order_id: i64,
    pub type_id: i64,
    pub side: MarketOrderSide,
    pub price: Money,
    pub remaining_volume: u64,
    pub entered_volume: u64,
    pub minimum_volume: u64,
    pub order_range: i32,
    pub issued_at: DateTime<Utc>,
    pub duration_days: i32,
    pub location_id: i64,
    pub solar_system_id: i64,
    pub region_id: i64,
    pub jumps: i32,
    pub normalized_row_checksum: String,
    pub source_row_number: u32,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ParsedMarketExport {
    pub original_filename: String,
    pub safe_filename: String,
    pub file_checksum: String,
    pub normalized_checksum: String,
    pub file_size_bytes: u64,
    pub observed_at: DateTime<Utc>,
    pub timestamp_source: MarketImportTimestampSource,
    pub type_id: i64,
    pub location_id: i64,
    pub solar_system_id: i64,
    pub region_id: i64,
    pub orders: Vec<ParsedMarketOrder>,
    pub duplicate_order_count: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketImportPreviewFile {
    pub filename: String,
    pub file_checksum: String,
    pub normalized_checksum: String,
    pub file_size_bytes: u64,
    pub type_id: Option<i64>,
    pub type_name: Option<String>,
    pub location_id: Option<i64>,
    pub location_name: Option<String>,
    pub solar_system_id: Option<i64>,
    pub solar_system_name: Option<String>,
    pub region_id: Option<i64>,
    pub region_name: Option<String>,
    pub observed_at: Option<DateTime<Utc>>,
    pub timestamp_source: Option<MarketImportTimestampSource>,
    pub row_count: u64,
    pub buy_order_count: u64,
    pub sell_order_count: u64,
    pub lowest_sell: Option<Money>,
    pub highest_buy: Option<Money>,
    pub total_buy_volume: u64,
    pub total_sell_volume: u64,
    pub earliest_issue_at: Option<DateTime<Utc>>,
    pub latest_issue_at: Option<DateTime<Utc>>,
    pub duplicate_order_count: u64,
    pub already_imported: bool,
    pub can_import: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<MarketImportProblem>,
}

impl MarketImportPreviewFile {
    #[must_use]
    pub fn from_parsed(parsed: &ParsedMarketExport, type_name: String) -> Self {
        let buy_orders: Vec<_> = parsed
            .orders
            .iter()
            .filter(|order| order.side == MarketOrderSide::Buy)
            .collect();
        let sell_orders: Vec<_> = parsed
            .orders
            .iter()
            .filter(|order| order.side == MarketOrderSide::Sell)
            .collect();
        Self {
            filename: parsed.safe_filename.clone(),
            file_checksum: parsed.file_checksum.clone(),
            normalized_checksum: parsed.normalized_checksum.clone(),
            file_size_bytes: parsed.file_size_bytes,
            type_id: Some(parsed.type_id),
            type_name: Some(type_name),
            location_id: Some(parsed.location_id),
            location_name: Some(format!("Structure {}", parsed.location_id)),
            solar_system_id: Some(parsed.solar_system_id),
            solar_system_name: None,
            region_id: Some(parsed.region_id),
            region_name: None,
            observed_at: Some(parsed.observed_at),
            timestamp_source: Some(parsed.timestamp_source),
            row_count: parsed.orders.len() as u64,
            buy_order_count: buy_orders.len() as u64,
            sell_order_count: sell_orders.len() as u64,
            lowest_sell: sell_orders.iter().map(|order| order.price).min(),
            highest_buy: buy_orders.iter().map(|order| order.price).max(),
            total_buy_volume: buy_orders.iter().map(|order| order.remaining_volume).sum(),
            total_sell_volume: sell_orders.iter().map(|order| order.remaining_volume).sum(),
            earliest_issue_at: parsed.orders.iter().map(|order| order.issued_at).min(),
            latest_issue_at: parsed.orders.iter().map(|order| order.issued_at).max(),
            duplicate_order_count: parsed.duplicate_order_count,
            already_imported: false,
            can_import: true,
            warnings: parsed.warnings.clone(),
            errors: Vec::new(),
        }
    }

    #[must_use]
    pub fn failed(filename: String, file_size_bytes: u64, error: MarketImportProblem) -> Self {
        Self {
            filename,
            file_checksum: String::new(),
            normalized_checksum: String::new(),
            file_size_bytes,
            type_id: None,
            type_name: None,
            location_id: None,
            location_name: None,
            solar_system_id: None,
            solar_system_name: None,
            region_id: None,
            region_name: None,
            observed_at: None,
            timestamp_source: None,
            row_count: 0,
            buy_order_count: 0,
            sell_order_count: 0,
            lowest_sell: None,
            highest_buy: None,
            total_buy_volume: 0,
            total_sell_volume: 0,
            earliest_issue_at: None,
            latest_issue_at: None,
            duplicate_order_count: 0,
            already_imported: false,
            can_import: false,
            warnings: Vec::new(),
            errors: vec![error],
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketImportPreview {
    pub files: Vec<MarketImportPreviewFile>,
    pub total_files: u64,
    pub valid_files: u64,
    pub invalid_files: u64,
    pub duplicate_files: u64,
    pub item_count: u64,
    pub location_count: u64,
    pub total_rows: u64,
    pub oldest_observation_at: Option<DateTime<Utc>>,
    pub newest_observation_at: Option<DateTime<Utc>>,
}

impl MarketImportPreview {
    #[must_use]
    pub fn from_files(files: Vec<MarketImportPreviewFile>) -> Self {
        let items: BTreeSet<_> = files.iter().filter_map(|file| file.type_id).collect();
        let locations: BTreeSet<_> = files.iter().filter_map(|file| file.location_id).collect();
        Self {
            total_files: files.len() as u64,
            valid_files: files.iter().filter(|file| file.can_import).count() as u64,
            invalid_files: files
                .iter()
                .filter(|file| !file.can_import && !file.already_imported)
                .count() as u64,
            duplicate_files: files.iter().filter(|file| file.already_imported).count() as u64,
            item_count: items.len() as u64,
            location_count: locations.len() as u64,
            total_rows: files.iter().map(|file| file.row_count).sum(),
            oldest_observation_at: files.iter().filter_map(|file| file.observed_at).min(),
            newest_observation_at: files.iter().filter_map(|file| file.observed_at).max(),
            files,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketImportProblem {
    pub code: String,
    pub message: String,
    pub row: Option<u32>,
    pub column: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedMarketFile {
    pub id: ImportedMarketFileId,
    pub batch_id: MarketImportBatchId,
    pub filename: String,
    pub file_checksum: String,
    pub normalized_checksum: String,
    pub file_size_bytes: u64,
    pub observed_at: DateTime<Utc>,
    pub timestamp_source: MarketImportTimestampSource,
    pub type_id: i64,
    pub type_name: String,
    pub location_id: i64,
    pub location_name: String,
    pub solar_system_id: i64,
    pub solar_system_name: Option<String>,
    pub region_id: i64,
    pub region_name: Option<String>,
    pub row_count: u64,
    pub buy_order_count: u64,
    pub sell_order_count: u64,
    pub imported_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketImportBatch {
    pub id: MarketImportBatchId,
    pub workspace_id: WorkspaceId,
    pub observed_at_min: DateTime<Utc>,
    pub observed_at_max: DateTime<Utc>,
    pub imported_at: DateTime<Utc>,
    pub file_count: u64,
    pub item_count: u64,
    pub location_count: u64,
    pub observation_count: u64,
    pub skipped_duplicate_file_count: u64,
    pub warnings: Vec<String>,
    pub files: Vec<ImportedMarketFile>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketImportResult {
    pub batch: Option<MarketImportBatch>,
    pub imported_files: u64,
    pub skipped_duplicate_files: u64,
    pub failed_files: Vec<MarketImportPreviewFile>,
    pub imported_observations: u64,
    pub warnings: Vec<String>,
}
