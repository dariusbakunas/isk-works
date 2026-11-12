//! The inspector's depth ladder for one order: the same side's orders
//! around it, and the opposite side's best orders that trade at its
//! location.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::Serialize;

use super::competition::{trades_at_location, BookOrder, CompetitionInput, OrderStanding};
use super::OrderRange;
use crate::MarketOrderSide;

/// Orders shown on each side of the character's own order in the depth
/// ladder.
const DEPTH_LEVELS: usize = 10;
/// Orders of the opposite side shown under the ladder.
const OPPOSITE_DEPTH_LEVELS: usize = 5;

/// One order next to the character's own in the inspector's depth ladder.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DepthOrder {
    pub order_id: i64,
    pub price: Decimal,
    pub volume_remain: i64,
    pub range: OrderRange,
    pub location_id: i64,
    pub location_name: Option<String>,
    /// This order itself.
    pub is_own: bool,
    /// Another order of the workspace's characters (not competition).
    pub is_alt: bool,
    /// Whether it trades at this order's location; `None` when a jump range
    /// can't be checked.
    pub trades_here: Option<bool>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderDepth {
    /// Same side as the order, best first.
    pub same_side: Vec<DepthOrder>,
    /// The opposite side's best orders that trade here.
    pub opposite_side: Vec<DepthOrder>,
}

/// Up to `DEPTH_LEVELS` same-side orders on each side of the order's own
/// price (buy orders out of range included, flagged), plus the opposite
/// side's best orders that trade here.
pub fn depth(
    input: &CompetitionInput<'_>,
    standing: &OrderStanding,
    names: &BTreeMap<i64, String>,
) -> OrderDepth {
    let side = input.order.side;
    let row = |order: &BookOrder| DepthOrder {
        order_id: order.order_id,
        price: order.price,
        volume_remain: order.volume_remain,
        range: order.range,
        location_id: order.location_id,
        location_name: names.get(&order.location_id).cloned(),
        is_own: order.order_id == input.order.order_id,
        is_alt: order.order_id != input.order.order_id
            && input.own_order_ids.contains(&order.order_id),
        trades_here: trades_at_location(input, order),
    };
    let better_first = |a: &BookOrder, b: &BookOrder| match side {
        MarketOrderSide::Sell => a.price.cmp(&b.price),
        MarketOrderSide::Buy => b.price.cmp(&a.price),
    };
    let mut same: Vec<&BookOrder> = input
        .book
        .iter()
        .filter(|order| order.side == side)
        // Sell ladders show the order's own market only.
        .filter(|order| {
            side == MarketOrderSide::Buy || order.location_id == input.order.location_id
        })
        .collect();
    same.sort_by(|a, b| better_first(a, b).then_with(|| a.issued_at.cmp(&b.issued_at)));
    if !same
        .iter()
        .any(|order| order.order_id == input.order.order_id)
    {
        // The order isn't in the book yet: place it by its synced price.
        let own = BookOrder {
            order_id: input.order.order_id,
            side,
            price: standing.price,
            volume_remain: standing.volume_remain,
            range: input.order.range,
            location_id: input.order.location_id,
            solar_system_id: input.location_system_id.unwrap_or_default(),
            issued_at: standing.issued_at,
        };
        let position = same
            .iter()
            .position(|order| better_first(&own, order).is_lt())
            .unwrap_or(same.len());
        let mut rows: Vec<DepthOrder> = same.iter().map(|order| row(order)).collect();
        rows.insert(position, row(&own));
        return OrderDepth {
            same_side: window(rows, position),
            opposite_side: opposite(input, &row),
        };
    }
    let position = same
        .iter()
        .position(|order| order.order_id == input.order.order_id)
        .unwrap_or_default();
    let rows: Vec<DepthOrder> = same.iter().map(|order| row(order)).collect();
    OrderDepth {
        same_side: window(rows, position),
        opposite_side: opposite(input, &row),
    }
}

fn window(rows: Vec<DepthOrder>, position: usize) -> Vec<DepthOrder> {
    let start = position.saturating_sub(DEPTH_LEVELS);
    let end = (position + DEPTH_LEVELS + 1).min(rows.len());
    rows.into_iter().skip(start).take(end - start).collect()
}

fn opposite(
    input: &CompetitionInput<'_>,
    row: &dyn Fn(&BookOrder) -> DepthOrder,
) -> Vec<DepthOrder> {
    let side = match input.order.side {
        MarketOrderSide::Sell => MarketOrderSide::Buy,
        MarketOrderSide::Buy => MarketOrderSide::Sell,
    };
    let mut orders: Vec<&BookOrder> = input
        .book
        .iter()
        .filter(|order| order.side == side)
        .filter(|order| trades_at_location(input, order) == Some(true))
        .collect();
    orders.sort_by(|a, b| match side {
        MarketOrderSide::Sell => a.price.cmp(&b.price),
        MarketOrderSide::Buy => b.price.cmp(&a.price),
    });
    orders
        .into_iter()
        .take(OPPOSITE_DEPTH_LEVELS)
        .map(row)
        .collect()
}
