//! Whether a workspace's own order is still the best price where it trades.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::pricing::{next_price_step, PriceDirection};
use super::{CharacterMarketOrder, OrderRange};
use crate::MarketOrderSide;

/// How long the game blocks another price change after one.
// Check against current game rules if CCP changes the modification timer.
pub const MODIFY_COOLDOWN: Duration = Duration::minutes(5);

/// One order from a market order book (public region data or a player
/// structure), for the same type as the order being judged.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookOrder {
    pub order_id: i64,
    pub side: MarketOrderSide,
    pub price: Decimal,
    pub volume_remain: i64,
    pub range: OrderRange,
    pub location_id: i64,
    pub solar_system_id: i64,
    pub issued_at: DateTime<Utc>,
}

/// How much of the competition the books could show.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BookVisibility {
    /// Every book that matters was read.
    Full,
    /// Some books were read, but competitors could hide where ISK Works
    /// can't look (buy orders inside other player structures).
    Partial,
    /// No book could be read: no character can access the structure's
    /// market, the location is unknown, or no data has arrived yet.
    None,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StandingStatus {
    Best,
    Tied,
    Outbid,
    /// No verdict: nothing to compare against, or a better-priced buy order
    /// whose jump range can't be checked (no stargates imported).
    Unknown,
}

/// Where the order's current price and volume came from.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FreshSource {
    /// The order's own copy in the book (refreshed every 5 minutes).
    Book,
    /// The character orders sync (ESI caches it for 20 minutes).
    CharacterSync,
}

/// Everything the evaluator needs about one order.
pub struct CompetitionInput<'a> {
    pub order: &'a CharacterMarketOrder,
    /// The system of the order's location; `None` when the location is
    /// unknown (then only same-location buy orders can be judged).
    pub location_system_id: Option<i64>,
    /// Book orders of the same type, from every book read for the order.
    pub book: &'a [BookOrder],
    pub visibility: BookVisibility,
    /// Every open order ID of the workspace's characters. They never count
    /// as competition, so alts don't outbid each other.
    pub own_order_ids: &'a HashSet<i64>,
    /// Jumps from the order's system to every system within 40 jumps;
    /// `None` when no stargates are imported.
    pub jump_distances: Option<&'a HashMap<i64, u8>>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderStanding {
    pub status: StandingStatus,
    pub visibility: BookVisibility,
    pub price: Decimal,
    pub volume_remain: i64,
    pub issued_at: DateTime<Utc>,
    pub fresh_source: FreshSource,
    /// The best-priced competitor whose reach is certain, ahead or not.
    pub best_competitor: Option<BookOrder>,
    /// How far the best competitor beats this order: positive when outbid,
    /// zero when tied, negative when this order leads.
    pub gap: Option<Decimal>,
    /// `gap` as a percentage of this order's price.
    pub gap_percent: Option<Decimal>,
    /// Competitors a trade would fill first: a better price, or the same
    /// price issued earlier.
    pub competitors_ahead: u32,
    pub volume_ahead: i64,
    /// One valid step past the best competitor. Below or at this order's
    /// price when it is outbid; above it ("room to move") when it leads.
    pub suggested_price: Option<Decimal>,
    pub modifiable_at: DateTime<Utc>,
    /// When the oldest competitor ahead appeared, if every one of them was
    /// placed or repriced after this order. `None` otherwise.
    pub outbid_since: Option<DateTime<Utc>>,
}

/// Whether a competing buy order reaches the order's location.
enum Reach {
    Yes,
    No,
    Unknown,
}

fn buy_reach(input: &CompetitionInput<'_>, competitor: &BookOrder) -> Reach {
    if competitor.location_id == input.order.location_id {
        return Reach::Yes;
    }
    let Some(system) = input.location_system_id else {
        return match competitor.range {
            OrderRange::Region => Reach::Yes,
            _ => Reach::Unknown,
        };
    };
    match competitor.range {
        OrderRange::Station => Reach::No,
        OrderRange::SolarSystem => {
            if competitor.solar_system_id == system {
                Reach::Yes
            } else {
                Reach::No
            }
        }
        OrderRange::Region => Reach::Yes,
        OrderRange::Jumps(jumps) => {
            if competitor.solar_system_id == system {
                return Reach::Yes;
            }
            match input.jump_distances {
                None => Reach::Unknown,
                Some(distances) => match distances.get(&competitor.solar_system_id) {
                    Some(distance) if *distance <= jumps => Reach::Yes,
                    _ => Reach::No,
                },
            }
        }
    }
}

/// `Ordering::Greater` when `a` is the better price for this side.
fn better(side: MarketOrderSide, a: Decimal, b: Decimal) -> std::cmp::Ordering {
    match side {
        MarketOrderSide::Sell => b.cmp(&a),
        MarketOrderSide::Buy => a.cmp(&b),
    }
}

#[must_use]
pub fn evaluate_order(input: &CompetitionInput<'_>) -> OrderStanding {
    let order = input.order;
    let side = order.side;
    let own_copy = input
        .book
        .iter()
        .find(|book_order| book_order.order_id == order.order_id);
    let (price, volume_remain, issued_at, fresh_source) = match own_copy {
        Some(copy) => (
            copy.price,
            copy.volume_remain,
            copy.issued_at,
            FreshSource::Book,
        ),
        None => (
            order.price,
            order.volume_remain,
            order.issued_at,
            FreshSource::CharacterSync,
        ),
    };
    let mut standing = OrderStanding {
        status: StandingStatus::Unknown,
        visibility: input.visibility,
        price,
        volume_remain,
        issued_at,
        fresh_source,
        best_competitor: None,
        gap: None,
        gap_percent: None,
        competitors_ahead: 0,
        volume_ahead: 0,
        suggested_price: None,
        modifiable_at: issued_at + MODIFY_COOLDOWN,
        outbid_since: None,
    };
    if input.visibility == BookVisibility::None {
        return standing;
    }

    let mut certain = Vec::new();
    let mut undecided_better = false;
    for competitor in input.book.iter().filter(|competitor| {
        competitor.side == side && !input.own_order_ids.contains(&competitor.order_id)
    }) {
        let reach = match side {
            MarketOrderSide::Sell => {
                if competitor.location_id == order.location_id {
                    Reach::Yes
                } else {
                    Reach::No
                }
            }
            MarketOrderSide::Buy => buy_reach(input, competitor),
        };
        match reach {
            Reach::Yes => certain.push(competitor),
            Reach::No => {}
            Reach::Unknown => {
                if better(side, competitor.price, price).is_ge() {
                    undecided_better = true;
                }
            }
        }
    }

    let ahead: Vec<&BookOrder> = certain
        .iter()
        .copied()
        .filter(|competitor| match better(side, competitor.price, price) {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Equal => competitor.issued_at < issued_at,
            std::cmp::Ordering::Less => false,
        })
        .collect();
    standing.competitors_ahead = u32::try_from(ahead.len()).unwrap_or(u32::MAX);
    standing.volume_ahead = ahead
        .iter()
        .map(|competitor| competitor.volume_remain)
        .sum();
    let strictly_better = ahead
        .iter()
        .any(|competitor| better(side, competitor.price, price).is_gt());
    if strictly_better
        && ahead
            .iter()
            .all(|competitor| competitor.issued_at > issued_at)
    {
        standing.outbid_since = ahead.iter().map(|competitor| competitor.issued_at).min();
    }

    let best = certain
        .iter()
        .copied()
        .max_by(|a, b| better(side, a.price, b.price).then_with(|| b.issued_at.cmp(&a.issued_at)));
    if let Some(best) = best {
        let gap = match side {
            MarketOrderSide::Sell => price - best.price,
            MarketOrderSide::Buy => best.price - price,
        };
        standing.gap = Some(gap);
        standing.gap_percent =
            (price > Decimal::ZERO).then(|| (gap / price * Decimal::ONE_HUNDRED).round_dp(2));
        let direction = match side {
            MarketOrderSide::Sell => PriceDirection::Down,
            MarketOrderSide::Buy => PriceDirection::Up,
        };
        standing.suggested_price = next_price_step(best.price, direction);
        standing.best_competitor = Some(best.clone());
    }

    standing.status = if strictly_better {
        StandingStatus::Outbid
    } else if undecided_better {
        StandingStatus::Unknown
    } else if standing
        .best_competitor
        .as_ref()
        .is_some_and(|best| best.price == price)
    {
        StandingStatus::Tied
    } else {
        StandingStatus::Best
    };
    standing
}

#[cfg(test)]
mod tests {
    use super::*;

    const JITA: i64 = 30_000_142;
    const PERIMETER: i64 = 30_000_144;
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

    fn book(order_id: i64, side: MarketOrderSide, price: &str, issued: i64) -> BookOrder {
        BookOrder {
            order_id,
            side,
            price: d(price),
            volume_remain: 2,
            range: OrderRange::Region,
            location_id: JITA_4_4,
            solar_system_id: JITA,
            issued_at: at(issued),
        }
    }

    fn evaluate(
        order: &CharacterMarketOrder,
        book: &[BookOrder],
        own_ids: &[i64],
        distances: Option<&HashMap<i64, u8>>,
    ) -> OrderStanding {
        let own_order_ids: HashSet<i64> = own_ids.iter().copied().collect();
        evaluate_order(&CompetitionInput {
            order,
            location_system_id: Some(JITA),
            book,
            visibility: BookVisibility::Full,
            own_order_ids: &own_order_ids,
            jump_distances: distances,
        })
    }

    use MarketOrderSide::{Buy, Sell};

    #[test]
    fn a_cheaper_sell_order_at_the_same_location_outbids() {
        let order = own(Sell, "312400000");
        let standing = evaluate(
            &order,
            &[
                book(2, Sell, "311900000", 30),
                book(3, Sell, "315000000", 5),
            ],
            &[1],
            None,
        );
        assert_eq!(standing.status, StandingStatus::Outbid);
        assert_eq!(standing.best_competitor.unwrap().order_id, 2);
        assert_eq!(standing.gap, Some(d("500000")));
        assert_eq!(standing.gap_percent, Some(d("0.16")));
        assert_eq!(standing.competitors_ahead, 1);
        assert_eq!(standing.volume_ahead, 2);
        assert_eq!(standing.suggested_price, Some(d("311800000")));
        assert_eq!(standing.outbid_since, Some(at(30)));
        assert_eq!(standing.modifiable_at, at(5));
    }

    #[test]
    fn sell_orders_elsewhere_do_not_compete() {
        let order = own(Sell, "312400000");
        let mut perimeter = book(2, Sell, "300000000", 30);
        perimeter.location_id = PERIMETER_TTT;
        perimeter.solar_system_id = PERIMETER;
        let standing = evaluate(&order, &[perimeter], &[1], None);
        assert_eq!(standing.status, StandingStatus::Best);
        assert!(standing.best_competitor.is_none());
        assert!(standing.suggested_price.is_none());
    }

    #[test]
    fn an_alts_cheaper_order_is_not_competition() {
        let order = own(Sell, "312400000");
        let standing = evaluate(&order, &[book(9, Sell, "300000000", 30)], &[1, 9], None);
        assert_eq!(standing.status, StandingStatus::Best);
        assert_eq!(standing.competitors_ahead, 0);
    }

    #[test]
    fn the_books_copy_of_the_order_corrects_a_stale_sync_price() {
        // Repriced in game to 311.8m; the character sync still says 312.4m.
        let order = own(Sell, "312400000");
        let mut copy = book(1, Sell, "311800000", 40);
        copy.volume_remain = 4;
        let standing = evaluate(&order, &[copy, book(2, Sell, "311900000", 30)], &[1], None);
        assert_eq!(standing.fresh_source, FreshSource::Book);
        assert_eq!(standing.price, d("311800000"));
        assert_eq!(standing.volume_remain, 4);
        assert_eq!(standing.status, StandingStatus::Best);
        assert_eq!(standing.gap, Some(d("-100000")));
        // Room to raise: just under the next competitor.
        assert_eq!(standing.suggested_price, Some(d("311800000")));
        assert_eq!(standing.modifiable_at, at(45));
    }

    #[test]
    fn the_same_price_is_a_tie_and_an_older_one_is_ahead() {
        let order = own(Sell, "2480000");
        let tied = evaluate(&order, &[book(2, Sell, "2480000", 30)], &[1], None);
        assert_eq!(tied.status, StandingStatus::Tied);
        assert_eq!(tied.competitors_ahead, 0);
        assert_eq!(tied.suggested_price, Some(d("2479000")));

        let older = evaluate(&order, &[book(2, Sell, "2480000", -30)], &[1], None);
        assert_eq!(older.status, StandingStatus::Tied);
        assert_eq!(older.competitors_ahead, 1);
    }

    #[test]
    fn outbid_since_is_unknown_when_a_competitor_was_already_ahead() {
        let order = own(Sell, "312400000");
        let standing = evaluate(
            &order,
            &[
                book(2, Sell, "311900000", 30),
                book(3, Sell, "312000000", -60),
            ],
            &[1],
            None,
        );
        assert_eq!(standing.status, StandingStatus::Outbid);
        assert_eq!(standing.competitors_ahead, 2);
        assert_eq!(standing.outbid_since, None);
    }

    fn buy_in(system: i64, location: i64, range: OrderRange, price: &str) -> BookOrder {
        BookOrder {
            range,
            location_id: location,
            solar_system_id: system,
            ..book(5, Buy, price, 30)
        }
    }

    #[test]
    fn a_region_range_buy_order_anywhere_outbids_even_without_stargates() {
        let order = own(Buy, "9210");
        let standing = evaluate(
            &order,
            &[buy_in(PERIMETER, PERIMETER_TTT, OrderRange::Region, "9215")],
            &[1],
            None,
        );
        assert_eq!(standing.status, StandingStatus::Outbid);
        assert_eq!(standing.gap, Some(d("5")));
        assert_eq!(standing.suggested_price, Some(d("9216")));
    }

    #[test]
    fn a_jump_range_buy_order_counts_only_within_its_reach() {
        let order = own(Buy, "9210");
        let distances = HashMap::from([(JITA, 0), (PERIMETER, 1), (30_000_999, 6)]);
        let within = evaluate(
            &order,
            &[buy_in(
                PERIMETER,
                PERIMETER_TTT,
                OrderRange::Jumps(1),
                "9215",
            )],
            &[1],
            Some(&distances),
        );
        assert_eq!(within.status, StandingStatus::Outbid);

        let too_far = evaluate(
            &order,
            &[buy_in(30_000_999, 60_000_001, OrderRange::Jumps(5), "9215")],
            &[1],
            Some(&distances),
        );
        assert_eq!(too_far.status, StandingStatus::Best);
        assert!(too_far.best_competitor.is_none());
    }

    #[test]
    fn station_and_system_range_buy_orders_reach_only_their_own_station_or_system() {
        let order = own(Buy, "9210");
        let elsewhere_station = evaluate(
            &order,
            &[buy_in(JITA, 60_003_761, OrderRange::Station, "9215")],
            &[1],
            None,
        );
        assert_eq!(elsewhere_station.status, StandingStatus::Best);
        let same_system = evaluate(
            &order,
            &[buy_in(JITA, 60_003_761, OrderRange::SolarSystem, "9215")],
            &[1],
            None,
        );
        assert_eq!(same_system.status, StandingStatus::Outbid);
    }

    #[test]
    fn a_higher_jump_range_buy_order_without_stargates_is_unknown() {
        let order = own(Buy, "9210");
        let standing = evaluate(
            &order,
            &[buy_in(
                PERIMETER,
                PERIMETER_TTT,
                OrderRange::Jumps(5),
                "9215",
            )],
            &[1],
            None,
        );
        assert_eq!(standing.status, StandingStatus::Unknown);
    }

    #[test]
    fn a_certain_outbid_wins_over_an_undecidable_one() {
        let order = own(Buy, "9210");
        let standing = evaluate(
            &order,
            &[
                buy_in(PERIMETER, PERIMETER_TTT, OrderRange::Jumps(5), "9300"),
                buy_in(PERIMETER, PERIMETER_TTT, OrderRange::Region, "9215"),
            ],
            &[1],
            None,
        );
        assert_eq!(standing.status, StandingStatus::Outbid);
        assert_eq!(standing.best_competitor.unwrap().price, d("9215"));
    }

    #[test]
    fn nothing_to_read_is_unknown() {
        let order = own(Sell, "312400000");
        let own_order_ids = HashSet::from([1]);
        let standing = evaluate_order(&CompetitionInput {
            order: &order,
            location_system_id: None,
            book: &[book(2, Sell, "1", 0)],
            visibility: BookVisibility::None,
            own_order_ids: &own_order_ids,
            jump_distances: None,
        });
        assert_eq!(standing.status, StandingStatus::Unknown);
        assert!(standing.best_competitor.is_none());
    }

    #[test]
    fn buy_orders_ignore_sell_orders_and_the_reverse() {
        let order = own(Buy, "9210");
        let standing = evaluate(&order, &[book(2, Sell, "9000", 30)], &[1], None);
        assert_eq!(standing.status, StandingStatus::Best);
        assert!(standing.best_competitor.is_none());
    }
}
