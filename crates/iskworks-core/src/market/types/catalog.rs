use super::*;

/// One node of the EVE market-group hierarchy, nested for direct display --
/// the category tree in the market browser. Built once, server-side, from
/// the flat `iskworks_sde::MarketGroupNode` listing so callers never have
/// to reconstruct parent/child relationships themselves.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketCategoryNode {
    pub market_group_id: i64,
    pub name: String,
    /// Rolled up: this group's own direct item count plus every
    /// descendant's, so a parent category (e.g. "Ships") reads as "how many
    /// items are under it" rather than just its own direct membership
    /// (almost always zero for a pure grouping node).
    pub item_count: u64,
    pub children: Vec<MarketCategoryNode>,
}

/// Nests a flat `sde_market_groups` listing into a tree, rooted at every
/// group with no parent. Deterministic (children sorted by name, then id)
/// so API responses and tests are stable regardless of SDE import order.
#[must_use]
pub fn build_market_category_tree(
    flat: Vec<iskworks_sde::MarketGroupNode>,
) -> Vec<MarketCategoryNode> {
    let mut children_of: BTreeMap<Option<i64>, Vec<iskworks_sde::MarketGroupNode>> =
        BTreeMap::new();
    for node in flat {
        children_of
            .entry(node.parent_group_id)
            .or_default()
            .push(node);
    }

    fn build(
        parent: Option<i64>,
        children_of: &BTreeMap<Option<i64>, Vec<iskworks_sde::MarketGroupNode>>,
    ) -> Vec<MarketCategoryNode> {
        let mut nodes: Vec<MarketCategoryNode> = children_of
            .get(&parent)
            .into_iter()
            .flatten()
            .map(|node| {
                let children = build(Some(node.market_group_id), children_of);
                let item_count =
                    node.item_count + children.iter().map(|child| child.item_count).sum::<u64>();
                MarketCategoryNode {
                    market_group_id: node.market_group_id,
                    name: node.name.clone(),
                    item_count,
                    children,
                }
            })
            .collect();
        nodes.sort_by(|a, b| {
            a.name
                .cmp(&b.name)
                .then(a.market_group_id.cmp(&b.market_group_id))
        });
        nodes
    }

    build(None, &children_of)
}

/// A workspace-known market location (NPC station or player structure)
/// within some region -- the non-SDE half of the location list under a
/// selected region. Player structures are already resolved and cached in
/// `market_location_names` from prior import/coverage activity; this reads
/// that cache directly (`MarketRepository::known_locations_in_region`), not
/// through `PriceSource`.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownMarketLocation {
    pub location_id: i64,
    pub location_name: String,
    pub solar_system_id: i64,
    pub solar_system_name: String,
    pub structure_type_id: Option<i64>,
}

/// Which ESI market-fetch mechanism a location needs -- the public,
/// unauthenticated per-region endpoint (NPC stations) or the authenticated
/// per-structure endpoint (player-owned Upwell structures). Built from
/// positive evidence only: a location absent from both the SDE's NPC
/// station table *and* the workspace's resolved-structure registry is
/// `Unknown`, never assumed to be an `NpcStation` by elimination.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MarketLocationClassification {
    NpcStation,
    /// `solar_system_id` is the same value ESI itself returned when the
    /// structure was resolved (`GET /universe/structures/{id}/`), not a
    /// separately-sourced or guessed value.
    Structure {
        solar_system_id: i64,
    },
    Unknown,
}

pub const DEFAULT_MARKET_ITEM_PAGE_SIZE: u32 = 50;
pub const MAX_MARKET_ITEM_PAGE_SIZE: u32 = 200;

/// The market item summary table's request: a scope plus optional
/// category/search narrowing, paginated. Bounded page size is what keeps
/// the summary endpoint from ever pulling order-book data for more than a
/// page's worth of types at once (it must never fetch the whole catalog).
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketItemFilter {
    pub scope: MarketScope,
    pub market_group_id: Option<i64>,
    #[serde(default)]
    pub search: Option<String>,
    pub page: u32,
    pub page_size: u32,
}

impl MarketItemFilter {
    pub fn validate(mut self) -> Result<Self, MarketError> {
        if self.page == 0 {
            return Err(MarketError::Validation(
                "page must be greater than zero".to_string(),
            ));
        }
        if self.page_size == 0 || self.page_size > MAX_MARKET_ITEM_PAGE_SIZE {
            return Err(MarketError::Validation(format!(
                "page size must be between 1 and {MAX_MARKET_ITEM_PAGE_SIZE}"
            )));
        }
        self.search = self
            .search
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        Ok(self)
    }
}

/// One row of the item summary table: an SDE-known, market-browsable type,
/// blended with whatever market data exists for it in the requested scope
/// (all fields `None`/zero when there's none -- an explicit no-data state,
/// not an error or an omitted row, matching the design's "show Megathron
/// with no market observations for this scope" requirement).
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketItemSummary {
    pub type_id: i64,
    pub type_name: String,
    pub best_sell: Option<Money>,
    pub best_buy: Option<Money>,
    pub spread: Option<Money>,
    pub sell_order_count: u64,
    pub buy_order_count: u64,
    pub observed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketItemPage {
    pub rows: Vec<MarketItemSummary>,
    pub total_count: u64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketItemMarketData {
    pub best_sell: Option<Money>,
    pub best_buy: Option<Money>,
    pub spread: Option<Money>,
    pub sell_order_count: u64,
    pub buy_order_count: u64,
    /// Total remaining sell-order volume across the book -- the same
    /// eligibility/quantity `summarize_market_order_book_side` already
    /// computes as `total_visible_quantity`, surfaced here as its own field
    /// so the market item detail view can show it (the "Sell Vol" stat).
    pub sell_volume: u64,
    pub observed_at: Option<DateTime<Utc>>,
}

/// One of a small, curated set of well-known EVE trade hub NPC stations,
/// offered as a "Major Hubs" shortcut in the Market Scope Selector --
/// product/UX curation metadata, not observed market data, so it lives here
/// as a plain constant rather than a database table (a hub only ever
/// changes if CCP renames/removes the underlying station, which is rare and
/// easy to update in code). IDs verified against this app's imported SDE by
/// exact `name_en` match, disambiguated from near-duplicate station names in the same
/// system (e.g. Jita's own "Moon 5" station, Rens's own "Moon 17" station).
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MajorTradeHub {
    pub station_id: i64,
    pub short_name: &'static str,
}

pub const MAJOR_TRADE_HUBS: &[MajorTradeHub] = &[
    MajorTradeHub {
        station_id: 60_003_760,
        short_name: "Jita 4-4",
    },
    MajorTradeHub {
        station_id: 60_008_494,
        short_name: "Amarr",
    },
    MajorTradeHub {
        station_id: 60_011_866,
        short_name: "Dodixie",
    },
    MajorTradeHub {
        station_id: 60_005_686,
        short_name: "Hek",
    },
    MajorTradeHub {
        station_id: 60_004_588,
        short_name: "Rens",
    },
];

/// Whether a connected character is currently known to have confirmed
/// structure-market access -- derived from `market_location_names`'
/// `market_access_connection_id` plus the joined connection's live status,
/// not stored directly.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MarketAccessState {
    /// No character has ever confirmed market access for this structure.
    Unknown,
    /// A character confirmed access and its connection is still active.
    Confirmed,
    /// A character once confirmed access, but its connection is no longer
    /// in the `Connected` state (reconnection needed, scope revoked,
    /// temporarily unavailable, or disconnected) -- the structure stays
    /// listed rather than disappearing, with this state explaining why it
    /// needs attention.
    Expired,
}

/// One workspace-known Upwell structure, enriched with its market-access
/// and freshness state -- the "My Structures" tab's data source. Distinct
/// from `PgFacilityRepository::search_known_structures`'s `KnownStructure`
/// (search-by-name for the Facilities feature) in that this is an
/// unfiltered full listing carrying access/freshness fields that feature
/// has no use for.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketStructureListing {
    pub location_id: i64,
    pub location_name: String,
    pub structure_type_id: Option<i64>,
    pub structure_type_name: Option<String>,
    pub solar_system_id: i64,
    pub solar_system_name: Option<String>,
    pub region_id: Option<i64>,
    pub region_name: Option<String>,
    pub security_class: String,
    pub access_state: MarketAccessState,
    pub access_character_name: Option<String>,
    pub access_checked_at: Option<DateTime<Utc>>,
}
