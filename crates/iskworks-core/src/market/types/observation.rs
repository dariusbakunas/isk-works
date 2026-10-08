use super::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarketOrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarketImportTimestampSource {
    Filename,
    UserSupplied,
    ImportTime,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarketRefreshState {
    Missing,
    Current,
    Refreshing,
    Failed,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketCoverageRegistration {
    pub type_id: i64,
    pub type_name: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketCoverageItem {
    pub type_id: i64,
    pub type_name: String,
    pub refresh_state: MarketRefreshState,
    pub observed_at: Option<DateTime<Utc>>,
    pub last_attempted_at: Option<DateTime<Utc>>,
    pub next_refresh_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub order_count: u64,
    pub buy_order_count: u64,
    pub sell_order_count: u64,
    /// ETag of the batch at `last_completed_batch_id`, used as the
    /// `If-None-Match` value for an eligible conditional regional-market
    /// refresh. Internal refresh-scheduling state only -- `#[serde(skip)]`
    /// keeps it out of every serialized API surface (`MarketCoverageItem`
    /// is `Serialize`, but this value is a private cache token).
    #[serde(skip)]
    pub prior_etag: Option<String>,
    /// `market_source_coverage.revalidated_at`: most recent time ESI
    /// confirmed the snapshot at `last_completed_batch_id` is still current
    /// (a 200 sets it `= observed_at`, a 304 advances it). Internal only
    /// (`#[serde(skip)]`); effective freshness is
    /// `max(observed_at, revalidated_at)`.
    #[serde(skip)]
    pub revalidated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EsiMarketOrder {
    pub order_id: i64,
    pub side: MarketOrderSide,
    pub price: Money,
    pub remaining_volume: u64,
    pub entered_volume: u64,
    pub minimum_volume: u64,
    pub order_range: String,
    pub issued_at: DateTime<Utc>,
    pub duration_days: u32,
    pub location_id: i64,
    pub solar_system_id: i64,
}

impl EsiMarketOrder {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        order_id: i64,
        side: MarketOrderSide,
        price: &str,
        remaining_volume: u64,
        entered_volume: u64,
        minimum_volume: u64,
        order_range: String,
        issued_at: DateTime<Utc>,
        duration_days: u32,
        location_id: i64,
        solar_system_id: i64,
        expected_location_id: Option<i64>,
    ) -> Result<Self, MarketError> {
        if order_id <= 0 || location_id <= 0 || solar_system_id <= 0 {
            return Err(MarketError::InvalidEsiOrder(
                "order and location identifiers must be positive".to_string(),
            ));
        }
        if duration_days == 0 {
            return Err(MarketError::InvalidEsiOrder(
                "duration must be positive".to_string(),
            ));
        }
        // `None` means a region-wide scope, where every location in the
        // region is valid -- only a location-scoped fetch has a single
        // expected location to enforce.
        if let Some(expected_location_id) = expected_location_id {
            if location_id != expected_location_id {
                return Err(MarketError::InvalidEsiOrder(
                    "order does not belong to the configured market location".to_string(),
                ));
            }
        }
        let price =
            Money::parse(price).map_err(|error| MarketError::InvalidEsiOrder(error.to_string()))?;
        Ok(Self {
            order_id,
            side,
            price,
            remaining_volume,
            entered_volume,
            minimum_volume,
            order_range,
            issued_at,
            duration_days,
            location_id,
            solar_system_id,
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EsiMarketObservationBatch {
    pub id: MarketObservationBatchId,
    pub source_id: PriceSourceId,
    pub type_id: i64,
    pub type_name: String,
    pub region_id: i64,
    pub solar_system_id: i64,
    pub location_id: i64,
    pub observed_at: DateTime<Utc>,
    pub etag: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub orders: Vec<EsiMarketOrder>,
}

/// One app-wide fetch of a region's public ESI order book for one type
/// (`public_market_coverage`). Unlike `EsiMarketObservationBatch` it has no
/// price source and no location: it holds the whole regional book, stored
/// with `workspace_id = NULL`, and station scopes filter it by location on
/// read.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PublicMarketObservationBatch {
    pub id: MarketObservationBatchId,
    pub type_id: i64,
    pub type_name: String,
    pub region_id: i64,
    pub observed_at: DateTime<Utc>,
    pub etag: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub orders: Vec<EsiMarketOrder>,
}

/// A due `public_market_coverage` row: its region plus the same refresh view
/// a per-workspace coverage row gives (`prior_etag` for a conditional fetch).
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PublicMarketCoverageWork {
    pub region_id: i64,
    pub item: MarketCoverageItem,
}

/// A failed refresh of one type's market data, recorded against its
/// coverage row. `claim` is the claim the refresh ran under; a failure from
/// a claim that has since been superseded is not recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketRefreshFailure {
    pub type_id: i64,
    pub claim: DateTime<Utc>,
    pub attempted_at: DateTime<Utc>,
    pub next_refresh_at: DateTime<Utc>,
    pub error_message: String,
}
