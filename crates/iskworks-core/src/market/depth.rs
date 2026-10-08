//! Pure market valuation helpers: order-book side summarisation,
//! quantity-aware acquisition/liquidation depth, and multi-request price
//! resolution.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{IndustryError, Money, PriceSourceItem};

use super::errors::MarketError;
use super::types::{
    market_freshness, MarketDepthResult, MarketFreshnessState, MarketItemMarketData,
    MarketOrderSide, MarketOrderView, MarketPriceQuality, MarketPriceRequest, MarketPricingPolicy,
};

pub const MARKET_DEPTH_FORMULA_VERSION: &str = "market-depth-v1";

/// Derives an item's summary market data from its already-scope-merged raw
/// orders (`MarketRepository::scoped_order_books`) -- shared by the item
/// summary table (many types, aggregate only) and the order-book detail
/// endpoint (one type, aggregate plus the full order lists). `observed_at`
/// is the freshest observation among the merged orders; `spread` is
/// `best_sell - best_buy` whenever both sides resolved (left as computed,
/// including negative, if a merged region-wide scope's sides cross).
#[must_use]
pub fn summarize_scoped_orders(orders: &[MarketOrderView]) -> MarketItemMarketData {
    let sell = summarize_market_order_book_side(orders, MarketOrderSide::Sell);
    let buy = summarize_market_order_book_side(orders, MarketOrderSide::Buy);
    let spread = sell
        .best_unit_price
        .zip(buy.best_unit_price)
        .and_then(|(best_sell, best_buy)| best_sell.checked_sub(best_buy).ok());
    MarketItemMarketData {
        best_sell: sell.best_unit_price,
        best_buy: buy.best_unit_price,
        spread,
        sell_order_count: sell.order_count,
        buy_order_count: buy.order_count,
        sell_volume: sell.total_visible_quantity,
        // Physical provenance only. This value is the Market Browser item
        // *table*'s `observedAt` column (`MarketItemSummary.observedAt`), a
        // serialized API field that means "when we last saw order rows".
        // Switching it to effective freshness would silently redefine a
        // public field; the item *detail* panel's staleness gate already
        // uses `item_freshness` (effective) instead. Deferred -- see the PR
        // report.
        observed_at: orders.iter().map(|order| order.observed_at).max(),
    }
}

pub fn calculate_market_depth(
    orders: &[MarketOrderView],
    policy: MarketPricingPolicy,
    requested_quantity: u64,
) -> Result<MarketDepthResult, MarketError> {
    if requested_quantity == 0 {
        return Err(MarketError::InvalidUpload(
            "Requested quantity must be positive.".to_string(),
        ));
    }
    let side =
        match policy {
            MarketPricingPolicy::LowestSell
            | MarketPricingPolicy::AcquireQuantityFromSellOrders => MarketOrderSide::Sell,
            MarketPricingPolicy::HighestBuy
            | MarketPricingPolicy::LiquidateQuantityIntoBuyOrders => MarketOrderSide::Buy,
        };
    let effective_request = match policy {
        MarketPricingPolicy::LowestSell | MarketPricingPolicy::HighestBuy => 1,
        _ => requested_quantity,
    };
    let applicable = applicable_orders_sorted(orders, side);
    let available_volume = applicable
        .iter()
        .try_fold(0_u64, |sum, order| sum.checked_add(order.remaining_volume))
        .ok_or_else(|| MarketError::Persistence("market volume overflow".to_string()))?;
    let best_unit_price = applicable.first().map(|order| order.price);
    let mut remaining = effective_request;
    let mut covered = 0_u64;
    let mut total = Money::zero();
    let mut marginal = None;
    let mut observation_ids = Vec::new();
    let mut order_ids = Vec::new();

    for order in applicable {
        if remaining == 0 {
            break;
        }
        let take = remaining.min(order.remaining_volume);
        if side == MarketOrderSide::Buy && take < order.minimum_volume {
            continue;
        }
        total = total
            .checked_add(
                order
                    .price
                    .checked_mul_quantity(take)
                    .map_err(map_industry_error)?,
            )
            .map_err(map_industry_error)?;
        covered = covered
            .checked_add(take)
            .ok_or_else(|| MarketError::Persistence("market volume overflow".to_string()))?;
        remaining -= take;
        marginal = Some(order.price);
        if let Some(id) = order.observation_id {
            observation_ids.push(id);
        }
        order_ids.push(order.order_id);
    }
    let uncovered = effective_request - covered;
    let fully_covered = uncovered == 0;
    let quality = if covered == 0 {
        MarketPriceQuality::Unavailable
    } else if fully_covered {
        MarketPriceQuality::Direct
    } else {
        MarketPriceQuality::InsufficientVolume
    };
    let mut warnings = Vec::new();
    if covered == 0 {
        warnings.push("No applicable orders are available.".to_string());
    } else if !fully_covered {
        warnings.push(format!("Order depth leaves {uncovered} units uncovered."));
    }
    Ok(MarketDepthResult {
        policy,
        requested_quantity,
        covered_quantity: if matches!(
            policy,
            MarketPricingPolicy::LowestSell | MarketPricingPolicy::HighestBuy
        ) && fully_covered
        {
            requested_quantity
        } else {
            covered
        },
        uncovered_quantity: if matches!(
            policy,
            MarketPricingPolicy::LowestSell | MarketPricingPolicy::HighestBuy
        ) && fully_covered
        {
            0
        } else {
            requested_quantity.saturating_sub(covered)
        },
        total,
        average_unit_price: if covered == 0 {
            None
        } else {
            Some(
                total
                    .checked_div_quantity(covered)
                    .map_err(map_industry_error)?,
            )
        },
        marginal_unit_price: marginal,
        best_unit_price,
        orders_used: order_ids.len() as u64,
        available_volume,
        fully_covered,
        quality,
        formula_version: MARKET_DEPTH_FORMULA_VERSION.to_string(),
        observation_ids,
        order_ids,
        warnings,
    })
}

/// The shared result of resolving a batch of `MarketPriceRequest`s against
/// already-fetched order books (`resolve_market_price_items`).
/// Unlike a bare `Vec<PriceSourceItem>`, this also keeps every request's
/// raw `MarketDepthResult` -- including one that never became a priced
/// item -- because a caller's own warning/missing-price derivation (e.g.
/// Opportunities' `MissingOutputPrice`/`InsufficientMarketDepth`) needs to
/// see *why* a request didn't price, not just that it didn't.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MarketPriceResolution {
    pub items: Vec<PriceSourceItem>,
    pub depth: BTreeMap<i64, MarketDepthResult>,
}

/// Resolves `requests` against already-fetched order books -- the shared
/// core that both `derive_market_price_items` (Build/Order pricing,
/// `crates/iskworks-storage`) and `resolve_opportunity_market_from_books`
/// (Opportunities scanning) delegate to, so the order-book-to-price-line
/// logic exists once.
/// Pure and stateless: fetching the books themselves is entirely the
/// caller's job (a live DB read for Build/Order, an already-batched-once-
/// per-evaluation read for Opportunities) -- `orders_for` looks up one
/// type's orders however the caller happens to have them keyed (a
/// `MarketOrderBook` map's `.orders`, a bare order-vector map, etc.).
///
/// `require_full_coverage` is a genuine behavioral fork between the two
/// callers, not an accident of duplication: Build/Order pricing (`false`)
/// still prices a partially-covered request off its best-available depth,
/// matching `PriceSnapshotLine.missing`'s existing "partial is still
/// data" convention; Opportunities scanning (`true`) requires full
/// coverage before treating a request as priced at all, so a thin book
/// stays `MissingMaterialPrice`/`MissingOutputPrice` evidence rather than
/// a partial number that could misrepresent a candidate's real margin.
/// Either way, an unpriced (or gated-out) request's `MarketDepthResult`
/// is still recorded in `depth` -- evidence is never erased, only
/// `items` membership is gated.
///
/// The note records provenance succinctly: formula version, requested/
/// covered/marginal quantities, order count, observed timestamp, and a
/// staleness marker when the freshest order backing this price is older
/// than `stale_after_hours`.
pub fn resolve_market_price_items<'a>(
    requests: &[MarketPriceRequest],
    orders_for: impl Fn(i64) -> Option<&'a [MarketOrderView]>,
    now: DateTime<Utc>,
    fresh_after_hours: u32,
    stale_after_hours: u32,
    require_full_coverage: bool,
) -> Result<MarketPriceResolution, MarketError> {
    let mut items = Vec::with_capacity(requests.len());
    let mut depth = BTreeMap::new();
    for request in requests {
        let Some(orders) = orders_for(request.type_id) else {
            continue;
        };
        if orders.is_empty() {
            continue;
        }
        // `observed_at` (physical fetch time) is what the note records as
        // provenance; the staleness decision uses effective freshness
        // per-order, so an ESI `304` re-confirmation clears the
        // `STALE_MARKET_PROVENANCE` prefix without a fresh fetch and an old
        // import in a mixed scope is not dragged forward.
        let observed_at = orders
            .iter()
            .map(|order| order.observed_at)
            .max()
            .expect("orders is non-empty here, so it has a maximum observed_at");
        let effective_observed_at = orders
            .iter()
            .map(MarketOrderView::effective_observed_at)
            .max()
            .expect("orders is non-empty here, so it has a maximum effective_observed_at");
        let result =
            calculate_market_depth(orders, request.pricing_policy, request.requested_quantity)?;
        let priceable = !require_full_coverage || result.fully_covered;
        if priceable {
            if let Some(price) = result.average_unit_price {
                let freshness = market_freshness(
                    effective_observed_at,
                    now,
                    fresh_after_hours,
                    stale_after_hours,
                );
                items.push(PriceSourceItem {
                    type_id: request.type_id,
                    type_name: request.type_name.clone(),
                    price,
                    note: format!(
                        "{}{}; requested {}; covered {}; marginal {}; {} orders; observed {}",
                        if freshness == MarketFreshnessState::Stale {
                            format!("{}; ", crate::STALE_MARKET_PROVENANCE)
                        } else {
                            String::new()
                        },
                        result.formula_version,
                        result.requested_quantity,
                        result.covered_quantity,
                        result
                            .marginal_unit_price
                            .map(|value| value.0.to_string())
                            .unwrap_or_else(|| "unavailable".to_string()),
                        result.orders_used,
                        observed_at,
                    ),
                    updated_at: now,
                });
            }
        }
        depth.insert(request.type_id, result);
    }
    Ok(MarketPriceResolution { items, depth })
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketOrderBookSideSummary {
    pub best_unit_price: Option<Money>,
    pub best_level_quantity: Option<u64>,
    pub total_visible_quantity: u64,
    pub order_count: u64,
}

/// Applicable orders on `side`, filtered (visible remaining volume, and for
/// buys, in-range per `buy_order_applies`) and sorted best-first -- the exact
/// eligibility/ordering `calculate_market_depth` uses, shared here so a
/// future change to either rule can't silently diverge between the two
/// functions that both need it. `pub`: the market item/order-book read
/// endpoints need the actual sorted order list, not just a
/// summary, to render an order book -- not only the aggregate
/// `summarize_market_order_book_side` produces.
#[must_use]
pub fn applicable_orders_sorted(
    orders: &[MarketOrderView],
    side: MarketOrderSide,
) -> Vec<&MarketOrderView> {
    let mut applicable: Vec<_> = orders
        .iter()
        .filter(|order| {
            order.side == side
                && order.remaining_volume > 0
                && (side == MarketOrderSide::Sell || buy_order_applies(order))
        })
        .collect();
    if side == MarketOrderSide::Sell {
        applicable.sort_by_key(|order| (order.price, order.order_id));
    } else {
        applicable.sort_by_key(|order| (std::cmp::Reverse(order.price), order.order_id));
    }
    applicable
}

#[must_use]
pub fn summarize_market_order_book_side(
    orders: &[MarketOrderView],
    side: MarketOrderSide,
) -> MarketOrderBookSideSummary {
    let applicable = applicable_orders_sorted(orders, side);
    let best_unit_price = applicable.first().map(|order| order.price);
    let best_level_quantity = applicable.first().map(|best| {
        applicable
            .iter()
            .take_while(|order| order.price == best.price)
            .map(|order| order.remaining_volume)
            .sum()
    });
    let total_visible_quantity = applicable.iter().map(|order| order.remaining_volume).sum();
    let order_count = applicable.len() as u64;
    MarketOrderBookSideSummary {
        best_unit_price,
        best_level_quantity,
        total_visible_quantity,
        order_count,
    }
}

fn buy_order_applies(order: &MarketOrderView) -> bool {
    match order.order_range {
        -1 => order.jumps == 0,
        32767 => true,
        range if range >= 0 => order.jumps <= range,
        _ => false,
    }
}

/// ESI's `range` encoding for a buy order (`-1`=station, `0`=solar system,
/// `32767`=region, else a jump count) rendered as the short label the
/// market item detail view shows next to each buy order.
#[must_use]
pub fn format_order_range(range: i32) -> String {
    match range {
        -1 => "Station".to_string(),
        0 => "System".to_string(),
        32767 => "Region".to_string(),
        jumps if jumps > 0 => format!("{jumps} jumps"),
        _ => "Unknown".to_string(),
    }
}

fn map_industry_error(error: IndustryError) -> MarketError {
    MarketError::Persistence(error.to_string())
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::market::MarketOrderObservationId;

    #[test]
    fn summarize_scoped_orders_derives_best_prices_spread_counts_and_freshness() {
        let now = Utc::now();
        let mut older = order(1, MarketOrderSide::Sell, "10.0000", 5);
        older.observed_at = now - chrono::Duration::hours(1);
        let mut newer_sell = order(2, MarketOrderSide::Sell, "12.0000", 5);
        newer_sell.observed_at = now;
        let mut buy = order(3, MarketOrderSide::Buy, "8.0000", 5);
        buy.observed_at = now - chrono::Duration::minutes(30);

        let data = summarize_scoped_orders(&[older, newer_sell, buy]);
        assert_eq!(data.best_sell, Some(Money::parse("10.0000").unwrap()));
        assert_eq!(data.best_buy, Some(Money::parse("8.0000").unwrap()));
        assert_eq!(data.spread, Some(Money::parse("2.0000").unwrap()));
        assert_eq!(data.sell_order_count, 2);
        assert_eq!(data.buy_order_count, 1);
        assert_eq!(data.observed_at, Some(now));
    }

    #[test]
    fn summarize_scoped_orders_of_an_empty_book_is_the_explicit_no_data_state() {
        let data = summarize_scoped_orders(&[]);
        assert_eq!(data, MarketItemMarketData::default());
        assert_eq!(data.best_sell, None);
        assert_eq!(data.observed_at, None);
    }

    fn order(id: i64, side: MarketOrderSide, price: &str, volume: u64) -> MarketOrderView {
        MarketOrderView {
            observation_id: Some(MarketOrderObservationId(Uuid::from_u128(id as u128))),
            import_batch_id: None,
            imported_file_id: None,
            order_id: id,
            type_id: 34,
            type_name: "Tritanium".to_string(),
            side,
            price: Money::parse(price).unwrap(),
            remaining_volume: volume,
            entered_volume: volume,
            minimum_volume: 1,
            order_range: 32767,
            issued_at: Utc::now(),
            duration_days: 90,
            observed_at: Utc::now(),
            revalidated_at: None,
            location_id: 1,
            solar_system_id: 2,
            region_id: 3,
            jumps: 0,
        }
    }

    #[test]
    fn resolve_market_price_items_with_full_coverage_required_skips_a_partially_covered_request() {
        let now = Utc::now();
        let orders = vec![order(1, MarketOrderSide::Sell, "5.0000", 5)];
        let resolution = resolve_market_price_items(
            &[MarketPriceRequest {
                type_id: 34,
                type_name: "Tritanium".to_string(),
                requested_quantity: 12,
                pricing_policy: MarketPricingPolicy::AcquireQuantityFromSellOrders,
            }],
            |type_id| (type_id == 34).then_some(orders.as_slice()),
            now,
            1,
            24,
            true,
        )
        .unwrap();

        // Evidence is retained even though it never becomes a priced item --
        // the same "evidence isn't erased, only item membership is gated"
        // contract Opportunities' warning derivation depends on.
        assert_eq!(resolution.depth[&34].covered_quantity, 5);
        assert_eq!(resolution.depth[&34].uncovered_quantity, 7);
        assert!(resolution.items.is_empty());
    }

    #[test]
    fn resolve_market_price_items_with_full_coverage_required_prices_a_fully_covered_request() {
        let now = Utc::now();
        let tritanium_orders = vec![
            order(1, MarketOrderSide::Sell, "5.0000", 5),
            order(2, MarketOrderSide::Sell, "7.0000", 10),
        ];
        let rifter_orders = vec![MarketOrderView {
            type_id: 5_876,
            type_name: "Rifter".to_string(),
            ..order(3, MarketOrderSide::Sell, "500000", 1)
        }];
        let resolution = resolve_market_price_items(
            &[
                MarketPriceRequest {
                    type_id: 34,
                    type_name: "Tritanium".to_string(),
                    requested_quantity: 12,
                    pricing_policy: MarketPricingPolicy::AcquireQuantityFromSellOrders,
                },
                MarketPriceRequest {
                    type_id: 5_876,
                    type_name: "Rifter".to_string(),
                    requested_quantity: 1,
                    pricing_policy: MarketPricingPolicy::LowestSell,
                },
            ],
            |type_id| match type_id {
                34 => Some(tritanium_orders.as_slice()),
                5_876 => Some(rifter_orders.as_slice()),
                _ => None,
            },
            now,
            1,
            24,
            true,
        )
        .unwrap();

        assert_eq!(resolution.depth[&34].covered_quantity, 12);
        assert_eq!(resolution.items.len(), 2);
        assert_eq!(resolution.items[0].price, Money::parse("6.1667").unwrap());
    }

    #[test]
    fn acquisition_walks_depth_with_exact_total_and_marginal_price() {
        let orders = vec![
            order(1, MarketOrderSide::Sell, "3.9700", 100),
            order(2, MarketOrderSide::Sell, "3.9800", 50),
            order(3, MarketOrderSide::Sell, "4.1000", 500),
        ];
        let result = calculate_market_depth(
            &orders,
            MarketPricingPolicy::AcquireQuantityFromSellOrders,
            125,
        )
        .unwrap();
        assert_eq!(result.total, Money::parse("496.5000").unwrap());
        assert_eq!(
            result.average_unit_price,
            Some(Money::parse("3.9720").unwrap())
        );
        assert_eq!(
            result.marginal_unit_price,
            Some(Money::parse("3.9800").unwrap())
        );
        assert_eq!(result.orders_used, 2);
        assert!(result.fully_covered);
    }

    #[test]
    fn liquidation_respects_minimum_volume_and_reports_insufficient_depth() {
        let mut high = order(1, MarketOrderSide::Buy, "4.0000", 100);
        high.minimum_volume = 60;
        let low = order(2, MarketOrderSide::Buy, "3.8000", 20);
        let result = calculate_market_depth(
            &[high, low],
            MarketPricingPolicy::LiquidateQuantityIntoBuyOrders,
            50,
        )
        .unwrap();
        assert_eq!(result.covered_quantity, 20);
        assert_eq!(result.uncovered_quantity, 30);
        assert_eq!(result.total, Money::parse("76.0000").unwrap());
        assert!(!result.fully_covered);
        assert_eq!(result.quality, MarketPriceQuality::InsufficientVolume);
    }
}
