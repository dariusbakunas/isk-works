use super::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarketPricingPolicy {
    LowestSell,
    HighestBuy,
    AcquireQuantityFromSellOrders,
    LiquidateQuantityIntoBuyOrders,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarketCoveragePolicy {
    RequireFullCoverage,
    AllowPartialWithWarning,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarketObservationSetMode {
    LatestCompatibleImport,
    PinnedImportBatch,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarketFreshnessState {
    Fresh,
    Aging,
    Stale,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarketPriceQuality {
    Direct,
    Stale,
    InsufficientVolume,
    Unavailable,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketOrderView {
    pub observation_id: Option<MarketOrderObservationId>,
    pub import_batch_id: Option<MarketImportBatchId>,
    pub imported_file_id: Option<ImportedMarketFileId>,
    pub order_id: i64,
    pub type_id: i64,
    pub type_name: String,
    pub side: MarketOrderSide,
    pub price: Money,
    pub remaining_volume: u64,
    pub entered_volume: u64,
    pub minimum_volume: u64,
    pub order_range: i32,
    pub issued_at: DateTime<Utc>,
    pub duration_days: i32,
    /// When this order row was physically fetched from ESI (immutable
    /// provenance). Never redefine.
    pub observed_at: DateTime<Utc>,
    /// Most recent time ESI confirmed the owning ESI snapshot was still
    /// current via a `304 Not Modified` (from `market_source_coverage`).
    /// `None` for import-derived orders and for legacy rows. Internal only
    /// -- `#[serde(skip)]` keeps it off every serialized order-book DTO;
    /// callers use `effective_observed_at()`.
    #[serde(skip)]
    pub revalidated_at: Option<DateTime<Utc>>,
    pub location_id: i64,
    pub solar_system_id: i64,
    pub region_id: i64,
    pub jumps: i32,
}

impl MarketOrderView {
    /// `max(observed_at, revalidated_at)` -- the effective freshness of
    /// this order: its physical fetch time, or a later ESI confirmation
    /// that the snapshot it belongs to is unchanged. `GREATEST`-style: a
    /// malformed earlier `revalidated_at` can never make it look older.
    #[must_use]
    pub fn effective_observed_at(&self) -> DateTime<Utc> {
        self.observed_at
            .max(self.revalidated_at.unwrap_or(self.observed_at))
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketDepthResult {
    pub policy: MarketPricingPolicy,
    pub requested_quantity: u64,
    pub covered_quantity: u64,
    pub uncovered_quantity: u64,
    pub total: Money,
    pub average_unit_price: Option<Money>,
    pub marginal_unit_price: Option<Money>,
    pub best_unit_price: Option<Money>,
    pub orders_used: u64,
    pub available_volume: u64,
    pub fully_covered: bool,
    pub quality: MarketPriceQuality,
    pub formula_version: String,
    pub observation_ids: Vec<MarketOrderObservationId>,
    pub order_ids: Vec<i64>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketPriceSourceConfig {
    pub price_source_id: PriceSourceId,
    pub workspace_id: WorkspaceId,
    pub location_id: i64,
    pub solar_system_id: i64,
    pub region_id: i64,
    pub location_alias: String,
    pub pricing_policy: MarketPricingPolicy,
    pub coverage_policy: MarketCoveragePolicy,
    pub observation_mode: MarketObservationSetMode,
    pub pinned_batch_id: Option<MarketImportBatchId>,
    pub fresh_after_hours: u32,
    pub stale_after_hours: u32,
    pub archived_at: Option<DateTime<Utc>>,
    pub last_snapshot_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketOrderBook {
    pub type_id: i64,
    pub type_name: String,
    pub location_id: i64,
    pub location_name: String,
    pub solar_system_id: i64,
    pub region_id: i64,
    /// When the order rows were physically fetched from ESI (or imported)
    /// -- immutable provenance. Never redefine.
    pub observed_at: DateTime<Utc>,
    /// Most recent time ESI confirmed this exact snapshot is still current
    /// via a `304 Not Modified`, without fetching new rows (from
    /// `market_source_coverage.revalidated_at`). `None` for import books
    /// and legacy rows. Internal only (`#[serde(skip)]`); callers use
    /// `effective_observed_at()`.
    #[serde(skip)]
    pub revalidated_at: Option<DateTime<Utc>>,
    pub observation_batch_id: MarketObservationBatchId,
    pub import_batch_id: Option<MarketImportBatchId>,
    pub imported_file_id: Option<ImportedMarketFileId>,
    pub buy_order_count: u64,
    pub sell_order_count: u64,
    pub total_buy_volume: u64,
    pub total_sell_volume: u64,
    pub lowest_sell: Option<Money>,
    pub highest_buy: Option<Money>,
    pub orders: Vec<MarketOrderView>,
}

impl MarketOrderBook {
    /// `max(observed_at, revalidated_at)` -- effective market freshness:
    /// when the snapshot was fetched, or a later ESI `304` confirming it
    /// unchanged. `GREATEST`-style, so a malformed earlier `revalidated_at`
    /// can never make the book look older than its physical fetch.
    #[must_use]
    pub fn effective_observed_at(&self) -> DateTime<Utc> {
        self.observed_at
            .max(self.revalidated_at.unwrap_or(self.observed_at))
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketPriceSource {
    pub id: PriceSourceId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub description: String,
    pub kind: PriceSourceKind,
    pub revision: u64,
    pub item_count: u64,
    pub config: MarketPriceSourceConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketPricePreviewCommand {
    pub type_id: i64,
    pub requested_quantity: u64,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MarketPriceRequest {
    pub type_id: i64,
    pub type_name: String,
    pub requested_quantity: u64,
    pub pricing_policy: MarketPricingPolicy,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketPricePreview {
    pub source_id: PriceSourceId,
    pub type_id: i64,
    pub type_name: String,
    pub location_id: i64,
    pub location_name: String,
    pub observed_at: DateTime<Utc>,
    pub freshness: MarketFreshnessState,
    pub depth: MarketDepthResult,
}

#[derive(Debug, Clone)]
pub struct ResolvedMarketExport {
    pub parsed: ParsedMarketExport,
    pub type_name: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedMarketLocation {
    pub location_id: i64,
    pub location_name: String,
    pub owner_id: i64,
    pub solar_system_id: i64,
    pub structure_type_id: Option<i64>,
    pub resolved_by_connection_id: ConnectedCharacterId,
    pub resolved_at: DateTime<Utc>,
}
