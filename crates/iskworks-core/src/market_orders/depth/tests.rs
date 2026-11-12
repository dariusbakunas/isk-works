use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;

use super::*;
use crate::market_orders::{evaluate_order, BookVisibility, CharacterMarketOrder, OrderRange};
use MarketOrderSide::{Buy, Sell};

const JITA: i64 = 30_000_142;
const JITA_4_4: i64 = 60_003_760;
const PERIMETER_TTT: i64 = 1_028_858_195_912;

fn d(value: &str) -> Decimal {
    value.parse().unwrap()
}

fn at(minutes: i64) -> DateTime<Utc> {
    "2026-10-21T10:00:00Z".parse::<DateTime<Utc>>().unwrap() + Duration::minutes(minutes)
}

fn own(side: MarketOrderSide, price: &str) -> CharacterMarketOrder {
    CharacterMarketOrder {
        order_id: 1,
        type_id: 12_005,
        region_id: 10_000_002,
        location_id: JITA_4_4,
        side,
        is_corporation: false,
        price: d(price),
        volume_total: 5,
        volume_remain: 5,
        min_volume: 1,
        range: OrderRange::Region,
        issued_at: at(0),
        duration_days: 90,
        escrow: None,
    }
}

fn book(order_id: i64, side: MarketOrderSide, price: i64, location_id: i64) -> BookOrder {
    BookOrder {
        order_id,
        side,
        price: Decimal::from(price),
        volume_remain: 2,
        range: if side == Buy {
            OrderRange::Station
        } else {
            OrderRange::Region
        },
        location_id,
        solar_system_id: JITA,
        issued_at: at(order_id),
    }
}

/// The ladder for `order` against `book`, with `alts` as the workspace's
/// other orders.
fn ladder(order: &CharacterMarketOrder, book: &[BookOrder], alts: &[i64]) -> OrderDepth {
    let own_order_ids: HashSet<i64> = alts.iter().copied().chain([order.order_id]).collect();
    let input = CompetitionInput {
        order,
        location_system_id: Some(JITA),
        book,
        visibility: BookVisibility::Full,
        own_order_ids: &own_order_ids,
        jump_distances: None,
        gone_from_book: false,
    };
    let standing = evaluate_order(&input);
    let names = BTreeMap::from([(JITA_4_4, "Jita IV - Moon 4".to_string())]);
    depth(&input, &standing, &names)
}

fn ids(rows: &[DepthOrder]) -> Vec<i64> {
    rows.iter().map(|row| row.order_id).collect()
}

#[test]
fn a_sell_ladder_is_cheapest_first_around_the_order_at_its_own_market() {
    let order = own(Sell, "150");
    // 30 sells at 101..=130 and 31 more above the order, all at Jita 4-4,
    // plus one elsewhere that would sit inside the window, which a sell
    // ladder leaves out.
    let mut orders: Vec<BookOrder> = (101..=130)
        .chain(151..=181)
        .map(|price| book(price, Sell, price, JITA_4_4))
        .collect();
    orders.push(book(1, Sell, 150, JITA_4_4));
    orders.push(book(9_125, Sell, 125, PERIMETER_TTT));

    let depth = ladder(&order, &orders, &[]);

    let mut expected: Vec<i64> = (121..=130).collect();
    expected.push(1);
    expected.extend(151..=160);
    assert_eq!(ids(&depth.same_side), expected);
    let own_row = &depth.same_side[DEPTH_LEVELS];
    assert!(own_row.is_own && !own_row.is_alt);
    assert_eq!(own_row.location_name.as_deref(), Some("Jita IV - Moon 4"));
}

#[test]
fn an_order_missing_from_the_book_is_placed_by_its_synced_price() {
    let order = own(Sell, "105");
    let orders = [book(2, Sell, 100, JITA_4_4), book(3, Sell, 110, JITA_4_4)];

    let depth = ladder(&order, &orders, &[3]);

    assert_eq!(ids(&depth.same_side), vec![2, 1, 3]);
    assert_eq!(depth.same_side[1].price, d("105"));
    assert!(depth.same_side[1].is_own);
    // An alt's order is flagged, not counted as the order itself.
    assert!(depth.same_side[2].is_alt && !depth.same_side[2].is_own);
}

#[test]
fn the_opposite_side_lists_the_best_buyers_that_trade_here() {
    let order = own(Sell, "200");
    let mut orders: Vec<BookOrder> = (90..=99)
        .map(|price| book(price, Buy, price, JITA_4_4))
        .collect();
    // A better buyer at another station, whose Station range doesn't reach.
    orders.push(book(150, Buy, 150, PERIMETER_TTT));
    orders.push(book(1, Sell, 200, JITA_4_4));

    let depth = ladder(&order, &orders, &[]);

    assert_eq!(depth.opposite_side.len(), OPPOSITE_DEPTH_LEVELS);
    assert_eq!(ids(&depth.opposite_side), vec![99, 98, 97, 96, 95]);
    assert!(depth
        .opposite_side
        .iter()
        .all(|row| row.trades_here == Some(true)));
}
