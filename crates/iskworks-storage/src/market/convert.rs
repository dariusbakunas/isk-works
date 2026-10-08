//! Small enum <-> string parsers, display formatters, error mappers, and
//! checked numeric conversions shared across the market persistence modules.

use iskworks_core::{
    MarketCoveragePolicy, MarketError, MarketImportTimestampSource, MarketObservationSetMode,
    MarketOrderSide, MarketPricingPolicy, MarketRefreshState, PriceSourceKind,
};
use rust_decimal::Decimal;

pub(super) fn timestamp_source_str(value: MarketImportTimestampSource) -> &'static str {
    match value {
        MarketImportTimestampSource::Filename => "filename",
        MarketImportTimestampSource::UserSupplied => "user_supplied",
        MarketImportTimestampSource::ImportTime => "import_time",
    }
}

pub(super) fn parse_timestamp_source(
    value: &str,
) -> Result<MarketImportTimestampSource, MarketError> {
    match value {
        "filename" => Ok(MarketImportTimestampSource::Filename),
        "user_supplied" => Ok(MarketImportTimestampSource::UserSupplied),
        "import_time" => Ok(MarketImportTimestampSource::ImportTime),
        _ => Err(invalid_db("timestamp source")),
    }
}

/// "Jita IV - Moon 4 - Caldari Navy Assembly Plant (0.9)" -- the station's
/// own SDE name plus its solar system's security status, rounded to one
/// decimal with a trailing ".0" dropped (matches "1" rather than "1.0" for
/// highsec systems that round exactly). `security_status` is `None` only if
/// the SDE import somehow lacks it for this system; the name still renders
/// on its own rather than being withheld over a missing decoration.
pub(super) fn format_station_display_name(name: &str, security_status: Option<Decimal>) -> String {
    match security_status {
        Some(security) => format!("{name} ({})", format_security_status(security)),
        None => name.to_string(),
    }
}

pub(super) fn format_security_status(security: Decimal) -> String {
    let rounded = security.round_dp(1);
    let text = format!("{rounded:.1}");
    text.strip_suffix(".0").map(str::to_string).unwrap_or(text)
}

pub(super) fn order_side_str(value: MarketOrderSide) -> &'static str {
    match value {
        MarketOrderSide::Buy => "buy",
        MarketOrderSide::Sell => "sell",
    }
}

pub(super) fn parse_order_side(value: &str) -> Result<MarketOrderSide, MarketError> {
    match value {
        "buy" => Ok(MarketOrderSide::Buy),
        "sell" => Ok(MarketOrderSide::Sell),
        _ => Err(invalid_db("order side")),
    }
}

pub(super) fn parse_refresh_state(value: &str) -> Result<MarketRefreshState, MarketError> {
    match value {
        "missing" => Ok(MarketRefreshState::Missing),
        "current" => Ok(MarketRefreshState::Current),
        "refreshing" => Ok(MarketRefreshState::Refreshing),
        "failed" => Ok(MarketRefreshState::Failed),
        _ => Err(invalid_db("market refresh state")),
    }
}

pub(super) fn parse_price_source_kind(value: &str) -> Result<PriceSourceKind, MarketError> {
    match value {
        "manual" => Ok(PriceSourceKind::Manual),
        "eve_client_market_export" => Ok(PriceSourceKind::EveClientMarketExport),
        "esi_market_orders" => Ok(PriceSourceKind::EsiMarketOrders),
        _ => Err(invalid_db("price source kind")),
    }
}

pub(super) fn parse_pricing_policy(value: &str) -> Result<MarketPricingPolicy, MarketError> {
    match value {
        "lowest_sell" => Ok(MarketPricingPolicy::LowestSell),
        "highest_buy" => Ok(MarketPricingPolicy::HighestBuy),
        "acquire_quantity_from_sell_orders" => {
            Ok(MarketPricingPolicy::AcquireQuantityFromSellOrders)
        }
        "liquidate_quantity_into_buy_orders" => {
            Ok(MarketPricingPolicy::LiquidateQuantityIntoBuyOrders)
        }
        _ => Err(invalid_db("pricing policy")),
    }
}

pub(super) fn parse_coverage_policy(value: &str) -> Result<MarketCoveragePolicy, MarketError> {
    match value {
        "require_full_coverage" => Ok(MarketCoveragePolicy::RequireFullCoverage),
        "allow_partial_with_warning" => Ok(MarketCoveragePolicy::AllowPartialWithWarning),
        _ => Err(invalid_db("coverage policy")),
    }
}

pub(super) fn parse_observation_mode(value: &str) -> Result<MarketObservationSetMode, MarketError> {
    match value {
        "latest_compatible_import" => Ok(MarketObservationSetMode::LatestCompatibleImport),
        "pinned_import_batch" => Ok(MarketObservationSetMode::PinnedImportBatch),
        _ => Err(invalid_db("observation mode")),
    }
}

pub(super) fn map_sqlx(error: sqlx::Error) -> MarketError {
    MarketError::Persistence(error.to_string())
}

pub(super) fn map_json(error: serde_json::Error) -> MarketError {
    MarketError::Persistence(error.to_string())
}

pub(super) fn invalid_db(field: &str) -> MarketError {
    MarketError::Persistence(format!("invalid persisted {field}"))
}

pub(super) fn overflow() -> MarketError {
    MarketError::Persistence("market numeric value overflow".to_string())
}

pub(super) fn i64_from_u64(value: u64) -> Result<i64, MarketError> {
    i64::try_from(value).map_err(|_| overflow())
}

pub(super) fn i64_from_usize(value: usize) -> Result<i64, MarketError> {
    i64::try_from(value).map_err(|_| overflow())
}

pub(super) fn i32_from_u64(value: u64) -> Result<i32, MarketError> {
    i32::try_from(value).map_err(|_| overflow())
}

pub(super) fn i32_from_usize(value: usize) -> Result<i32, MarketError> {
    i32::try_from(value).map_err(|_| overflow())
}

pub(super) fn u64_from_i64(value: i64) -> Result<u64, MarketError> {
    u64::try_from(value).map_err(|_| invalid_db("unsigned integer"))
}

pub(super) fn u64_from_i32(value: i32) -> Result<u64, MarketError> {
    u64::try_from(value).map_err(|_| invalid_db("unsigned integer"))
}

pub(super) fn u32_from_i32(value: i32) -> Result<u32, MarketError> {
    u32::try_from(value).map_err(|_| invalid_db("unsigned integer"))
}

pub(super) fn esi_order_range(value: &str) -> Result<i32, MarketError> {
    match value {
        "station" => Ok(-1),
        "solarsystem" => Ok(0),
        "region" => Ok(32_767),
        value => value
            .parse::<i32>()
            .ok()
            .filter(|value| *value >= 0)
            .ok_or_else(|| MarketError::InvalidEsiOrder("order range is invalid".to_string())),
    }
}
