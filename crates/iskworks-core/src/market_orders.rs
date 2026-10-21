//! The workspace characters' own market orders, synced per connection by the
//! `market_orders` character source.

use std::collections::{HashMap, VecDeque};

mod competition;
mod margin;
mod pricing;

pub use competition::{
    evaluate_order, BookOrder, BookVisibility, CompetitionInput, FreshSource, OrderStanding,
    StandingStatus, MODIFY_COOLDOWN,
};
pub use margin::{sell_margin, SellMargin};
pub use pricing::{
    next_price_step, sales_tax_rate, PriceDirection, ACCOUNTING_REDUCTION_PER_LEVEL,
    ACCOUNTING_SKILL_ID, BASE_SALES_TAX,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{ConnectedCharacterId, InventoryError, MarketOrderSide, WorkspaceId};

/// How far a buy order reaches. Sell orders are always `Region` in ESI, but
/// only sell to buyers at their own location.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "jumps")]
pub enum OrderRange {
    Station,
    SolarSystem,
    /// Up to this many jumps from the order's system (1-40 in game).
    Jumps(u8),
    Region,
}

impl OrderRange {
    /// ESI's spelling: `station`, `solarsystem`, `region` or a jump count.
    #[must_use]
    pub fn from_esi(value: &str) -> Option<Self> {
        match value {
            "station" => Some(Self::Station),
            "solarsystem" => Some(Self::SolarSystem),
            "region" => Some(Self::Region),
            value => value
                .parse::<u8>()
                .ok()
                .filter(|jumps| *jumps > 0)
                .map(Self::Jumps),
        }
    }

    /// The `order_range` integer `market_order_observations` already uses:
    /// station -1, system 0, region 32767, otherwise the jump count.
    #[must_use]
    pub fn as_db_i32(self) -> i32 {
        match self {
            Self::Station => -1,
            Self::SolarSystem => 0,
            Self::Jumps(jumps) => i32::from(jumps),
            Self::Region => 32_767,
        }
    }

    #[must_use]
    pub fn from_db_i32(value: i32) -> Option<Self> {
        match value {
            -1 => Some(Self::Station),
            0 => Some(Self::SolarSystem),
            32_767 => Some(Self::Region),
            value => u8::try_from(value)
                .ok()
                .filter(|jumps| *jumps > 0)
                .map(Self::Jumps),
        }
    }
}

/// Stargate connections between solar systems, for deciding whether a buy
/// order's jump range reaches a location. Gates work both ways, so every
/// edge is walked in both directions.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct JumpGraph {
    neighbours: HashMap<i64, Vec<i64>>,
}

impl JumpGraph {
    pub fn from_edges(edges: impl IntoIterator<Item = (i64, i64)>) -> Self {
        let mut neighbours: HashMap<i64, Vec<i64>> = HashMap::new();
        for (from, to) in edges {
            neighbours.entry(from).or_default().push(to);
            neighbours.entry(to).or_default().push(from);
        }
        for systems in neighbours.values_mut() {
            systems.sort_unstable();
            systems.dedup();
        }
        Self { neighbours }
    }

    /// No gates at all: the SDE was imported without stargates, so no jump
    /// range can be decided.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.neighbours.is_empty()
    }

    /// Every system within `max_jumps` of `from`, with its shortest jump
    /// count (`from` itself is 0). Systems farther away, or not connected,
    /// are absent.
    #[must_use]
    pub fn jump_distances(&self, from: i64, max_jumps: u8) -> HashMap<i64, u8> {
        let mut distances = HashMap::from([(from, 0_u8)]);
        let mut queue = VecDeque::from([from]);
        while let Some(system) = queue.pop_front() {
            let jumps = distances[&system];
            if jumps >= max_jumps {
                continue;
            }
            for next in self.neighbours.get(&system).into_iter().flatten() {
                if !distances.contains_key(next) {
                    distances.insert(*next, jumps + 1);
                    queue.push_back(*next);
                }
            }
        }
        distances
    }
}

/// One active order as ESI reported it for one connection.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CharacterMarketOrder {
    pub order_id: i64,
    pub type_id: i64,
    pub region_id: i64,
    pub location_id: i64,
    pub side: MarketOrderSide,
    /// Placed on behalf of the character's corporation.
    pub is_corporation: bool,
    pub price: Decimal,
    pub volume_total: i64,
    pub volume_remain: i64,
    pub min_volume: i64,
    pub range: OrderRange,
    /// Reset by the game whenever the price is modified.
    pub issued_at: DateTime<Utc>,
    pub duration_days: i32,
    /// ISK held for a buy order; `None` on sell orders.
    pub escrow: Option<Decimal>,
}

/// A stored open order with the character that placed it.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorkspaceMarketOrder {
    pub order: CharacterMarketOrder,
    pub connection_id: ConnectedCharacterId,
    pub eve_character_id: i64,
    pub character_name: String,
    pub first_seen_at: DateTime<Utc>,
    /// The last sync that still returned the order.
    pub last_seen_at: DateTime<Utc>,
}

#[async_trait]
pub trait CharacterMarketOrderRepository: Send + Sync {
    /// Makes `orders` the connection's open set as of `observed_at`: they
    /// are inserted or updated (and reopened if they had been closed), and
    /// every other open order of the connection is closed. Closed orders
    /// filled, expired or were cancelled.
    async fn replace_open_orders(
        &self,
        connection_id: ConnectedCharacterId,
        orders: &[CharacterMarketOrder],
        observed_at: DateTime<Utc>,
    ) -> Result<(), InventoryError>;

    /// Every open order of the workspace's connected characters.
    async fn open_market_orders(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<WorkspaceMarketOrder>, InventoryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(edges: &[(i64, i64)]) -> JumpGraph {
        JumpGraph::from_edges(edges.iter().copied())
    }

    #[test]
    fn jump_distances_follow_a_line_of_systems() {
        let line = graph(&[(1, 2), (2, 3)]);
        assert_eq!(
            line.jump_distances(1, 40),
            HashMap::from([(1, 0), (2, 1), (3, 2)])
        );
    }

    #[test]
    fn jump_distances_take_the_shortest_way_round_a_cycle() {
        // 1-2-3-4-5-1: going backwards from 1 reaches 5 in one jump.
        let cycle = graph(&[(1, 2), (2, 3), (3, 4), (4, 5), (5, 1)]);
        let distances = cycle.jump_distances(1, 40);
        assert_eq!(distances[&5], 1);
        assert_eq!(distances[&4], 2);
        assert_eq!(distances[&3], 2);
    }

    #[test]
    fn jump_distances_stop_at_the_cap() {
        let line = graph(&[(1, 2), (2, 3), (3, 4)]);
        assert_eq!(
            line.jump_distances(1, 2),
            HashMap::from([(1, 0), (2, 1), (3, 2)])
        );
    }

    #[test]
    fn unreachable_and_unknown_systems_are_absent() {
        let islands = graph(&[(1, 2), (3, 4)]);
        assert!(!islands.jump_distances(1, 40).contains_key(&3));
        assert_eq!(islands.jump_distances(99, 40), HashMap::from([(99, 0)]));
    }

    #[test]
    fn a_graph_without_gates_is_empty() {
        assert!(graph(&[]).is_empty());
        assert!(!graph(&[(1, 2)]).is_empty());
    }

    #[test]
    fn order_range_reads_every_esi_spelling() {
        assert_eq!(OrderRange::from_esi("station"), Some(OrderRange::Station));
        assert_eq!(
            OrderRange::from_esi("solarsystem"),
            Some(OrderRange::SolarSystem)
        );
        assert_eq!(OrderRange::from_esi("region"), Some(OrderRange::Region));
        assert_eq!(OrderRange::from_esi("5"), Some(OrderRange::Jumps(5)));
        assert_eq!(OrderRange::from_esi("40"), Some(OrderRange::Jumps(40)));
        assert_eq!(OrderRange::from_esi("0"), None);
        assert_eq!(OrderRange::from_esi("constellation"), None);
    }

    #[test]
    fn order_range_round_trips_the_observation_encoding() {
        for range in [
            OrderRange::Station,
            OrderRange::SolarSystem,
            OrderRange::Jumps(1),
            OrderRange::Jumps(40),
            OrderRange::Region,
        ] {
            assert_eq!(OrderRange::from_db_i32(range.as_db_i32()), Some(range));
        }
        assert_eq!(OrderRange::Station.as_db_i32(), -1);
        assert_eq!(OrderRange::Region.as_db_i32(), 32_767);
        assert_eq!(OrderRange::from_db_i32(-2), None);
    }
}
