//! The Market Orders page: every open order of the workspace's characters,
//! judged against the market books ISK Works keeps.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use iskworks_core::market_orders::{
    evaluate_order, sales_tax_rate, sell_margin, trades_at_location, BookOrder, BookVisibility,
    CharacterMarketOrderRepository, CompetitionInput, JumpGraph, OrderRange, OrderStanding,
    SellMargin, StandingStatus, ACCOUNTING_SKILL_ID, BASE_SALES_TAX,
};
use iskworks_core::{
    CharacterSourceKind, ConnectedCharacterId, InventoryError, InventoryRepository,
    MarketCoverageRegistration, MarketError, MarketLocationClassification, MarketOrderBook,
    MarketOrderSide, MarketRefreshState, MarketRepository, MarketScope, OwnerId, PriceSourceId,
    WorkspaceId,
};
use iskworks_esi::MARKET_ORDERS_SCOPE;
use iskworks_sde::{SdeError, SdeReadRepository};
use iskworks_storage::PgEsiRepository;
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

use crate::esi_service::EsiApplicationService;

/// How far a buy order's range can reach, in jumps.
const MAX_ORDER_RANGE_JUMPS: u8 = 40;
/// Orders shown on each side of the character's own order in the depth
/// ladder.
const DEPTH_LEVELS: usize = 10;
/// Orders of the opposite side shown under the ladder.
const OPPOSITE_DEPTH_LEVELS: usize = 5;
/// Structure IDs are far above every NPC station ID; only those are worth
/// resolving through ESI.
const MIN_STRUCTURE_ID: i64 = 1_000_000_000_000;

#[derive(Debug, Error)]
pub enum MarketOrdersError {
    #[error(transparent)]
    Market(#[from] MarketError),
    #[error(transparent)]
    Inventory(#[from] InventoryError),
    #[error(transparent)]
    Sde(#[from] SdeError),
}

/// A workspace character as the Market Orders page needs it.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraderCharacter {
    pub connection_id: ConnectedCharacterId,
    pub eve_character_id: i64,
    pub character_name: String,
    /// The connection hasn't granted `esi-markets.read_character_orders.v1`.
    pub orders_scope_missing: bool,
    pub orders_synced_at: Option<DateTime<Utc>>,
    pub orders_sync_error: Option<String>,
    /// Trained Accounting level, when the skills source has synced.
    pub accounting_level: Option<u8>,
}

#[async_trait]
pub trait TraderCharacterSource: Send + Sync {
    async fn trader_characters(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<TraderCharacter>, InventoryError>;
}

fn accounting_level(skills_summary: Option<&Value>) -> Option<u8> {
    let skills = skills_summary?.get("skills")?.as_array()?;
    let level = skills
        .iter()
        .find(|skill| skill.get("skill_id").and_then(Value::as_i64) == Some(ACCOUNTING_SKILL_ID))
        .and_then(|skill| skill.get("active_skill_level"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    Some(u8::try_from(level.min(5)).unwrap_or(5))
}

#[async_trait]
impl TraderCharacterSource for PgEsiRepository {
    async fn trader_characters(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<TraderCharacter>, InventoryError> {
        let mut characters = Vec::new();
        for connection in self.list_connections(workspace_id).await? {
            if connection.disconnected_at.is_some() {
                continue;
            }
            let sources = self.character_source_state(connection.id).await?;
            let source = |kind| sources.iter().find(|source| source.source_kind == kind);
            let orders = source(CharacterSourceKind::MarketOrders);
            characters.push(TraderCharacter {
                connection_id: connection.id,
                eve_character_id: connection.eve_character_id,
                character_name: connection.character_name.clone(),
                orders_scope_missing: !connection
                    .granted_scopes
                    .iter()
                    .any(|scope| scope == MARKET_ORDERS_SCOPE),
                orders_synced_at: orders.and_then(|source| source.observed_at),
                orders_sync_error: orders
                    .filter(|source| source.refresh_state == MarketRefreshState::Failed)
                    .and_then(|source| source.last_error.clone()),
                accounting_level: accounting_level(
                    source(CharacterSourceKind::Skills).and_then(|source| source.summary.as_ref()),
                ),
            });
        }
        characters.sort_by(|a, b| a.character_name.cmp(&b.character_name));
        Ok(characters)
    }
}

/// Resolves unknown player structure IDs (name and system) so their market
/// can be read. Best-effort: failures leave the location unknown.
#[async_trait]
pub trait StructureLocationResolver: Send + Sync {
    async fn resolve_structure_locations(&self, workspace_id: WorkspaceId, structure_ids: &[i64]);
}

/// Resolves structures through ESI and saves them as known market
/// locations, the same way the market import's location resolution does.
pub struct EsiStructureLocationResolver {
    esi: Arc<EsiApplicationService>,
    market: Arc<dyn MarketRepository>,
}

impl EsiStructureLocationResolver {
    #[must_use]
    pub fn new(esi: Arc<EsiApplicationService>, market: Arc<dyn MarketRepository>) -> Self {
        Self { esi, market }
    }
}

#[async_trait]
impl StructureLocationResolver for EsiStructureLocationResolver {
    async fn resolve_structure_locations(&self, workspace_id: WorkspaceId, structure_ids: &[i64]) {
        for chunk in structure_ids.chunks(50) {
            let Ok(resolution) = self.esi.resolve_structures(workspace_id, chunk).await else {
                continue;
            };
            let now = Utc::now();
            let locations = resolution
                .resolved
                .into_iter()
                .map(|item| iskworks_core::ResolvedMarketLocation {
                    location_id: item.structure.structure_id,
                    location_name: item.structure.name,
                    owner_id: item.structure.owner_id,
                    solar_system_id: item.structure.solar_system_id,
                    structure_type_id: item.structure.type_id,
                    resolved_by_connection_id: item.connection_id,
                    resolved_at: now,
                })
                .collect::<Vec<_>>();
            if !locations.is_empty() {
                let _ = self
                    .market
                    .save_location_names(workspace_id, locations)
                    .await;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MarketOrderLocationKind {
    Station,
    Structure,
    Unknown,
}

/// Why a location's book couldn't be read.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "reason")]
pub enum BookUnavailable {
    /// The structure isn't known to ISK Works and no character could
    /// resolve it.
    UnknownLocation,
    /// The last refresh failed; `detail` says why (typically that no
    /// character can access the structure's market).
    RefreshFailed { detail: String },
    /// Registered, but the worker hasn't fetched the book yet.
    NoDataYet,
}

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

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketOrderRow {
    pub order_id: i64,
    pub type_id: i64,
    pub type_name: Option<String>,
    pub side: MarketOrderSide,
    pub connection_id: ConnectedCharacterId,
    pub eve_character_id: i64,
    pub character_name: String,
    pub range: OrderRange,
    pub volume_total: i64,
    pub min_volume: i64,
    pub duration_days: i32,
    pub expires_at: DateTime<Utc>,
    pub escrow: Option<Decimal>,
    pub first_seen_at: DateTime<Utc>,
    pub standing: OrderStanding,
    /// The workspace's average unit cost of the type, and the stock behind
    /// it.
    pub unit_cost: Option<Decimal>,
    pub stock_quantity: u64,
    pub sales_tax_rate: Decimal,
    /// The character's Accounting level is unknown, so the base rate is
    /// used.
    pub sales_tax_estimated: bool,
    /// Sell orders with a cost basis only.
    pub margin: Option<SellMargin>,
    pub margin_at_suggested: Option<SellMargin>,
    pub depth: OrderDepth,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketOrderLocation {
    pub location_id: i64,
    pub location_name: Option<String>,
    pub region_id: i64,
    pub kind: MarketOrderLocationKind,
    pub unavailable: Option<BookUnavailable>,
    /// When the oldest book read for this location was fetched.
    pub book_observed_at: Option<DateTime<Utc>>,
    pub orders: Vec<MarketOrderRow>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketOrdersSummary {
    pub outbid: u32,
    pub tied: u32,
    pub best: u32,
    pub unknown: u32,
    pub active_sell_value: Decimal,
    pub buy_escrow: Decimal,
    pub oldest_book_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketOrdersOverview {
    pub generated_at: DateTime<Utc>,
    pub characters: Vec<TraderCharacter>,
    /// No stargates are imported, so jump-range buy orders can't be judged.
    pub stargates_missing: bool,
    pub summary: MarketOrdersSummary,
    pub locations: Vec<MarketOrderLocation>,
}

pub struct MarketOrdersService {
    orders: Arc<dyn CharacterMarketOrderRepository>,
    characters: Arc<dyn TraderCharacterSource>,
    market: Arc<dyn MarketRepository>,
    sde: Arc<dyn SdeReadRepository>,
    inventory: Arc<dyn InventoryRepository>,
    structures: Option<Arc<dyn StructureLocationResolver>>,
}

/// What the service knows about one order location before reading books.
struct LocationFacts {
    region_id: i64,
    kind: MarketOrderLocationKind,
    solar_system_id: Option<i64>,
}

/// A book read for one source, by type.
struct SourceBooks {
    source_id: PriceSourceId,
    books: BTreeMap<i64, MarketOrderBook>,
}

impl MarketOrdersService {
    #[must_use]
    pub fn new(
        orders: Arc<dyn CharacterMarketOrderRepository>,
        characters: Arc<dyn TraderCharacterSource>,
        market: Arc<dyn MarketRepository>,
        sde: Arc<dyn SdeReadRepository>,
        inventory: Arc<dyn InventoryRepository>,
    ) -> Self {
        Self {
            orders,
            characters,
            market,
            sde,
            inventory,
            structures: None,
        }
    }

    #[must_use]
    pub fn with_structure_resolver(mut self, resolver: Arc<dyn StructureLocationResolver>) -> Self {
        self.structures = Some(resolver);
        self
    }

    /// Every open personal order, judged against the current books. Also
    /// registers the books it needs, so the worker keeps them fresh; this
    /// never calls ESI for market data itself.
    pub async fn overview(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        now: DateTime<Utc>,
    ) -> Result<MarketOrdersOverview, MarketOrdersError> {
        let characters = self.characters.trader_characters(workspace_id).await?;
        let mut stored = self.orders.open_market_orders(workspace_id).await?;
        stored.retain(|row| !row.order.is_corporation);
        let own_order_ids: HashSet<i64> = stored.iter().map(|row| row.order.order_id).collect();

        let type_ids: Vec<i64> = stored
            .iter()
            .map(|row| row.order.type_id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let type_names = self.sde.type_names(&type_ids).await?;
        let locations = self.location_facts(workspace_id, &stored).await?;

        // Register and read the books: one public read per region (every
        // location, so other stations' buy orders are visible), one read
        // per structure.
        let (public_types, structure_types) = books_needed(&stored, &locations);
        let registration = |types: &BTreeSet<i64>| {
            types
                .iter()
                .map(|type_id| MarketCoverageRegistration {
                    type_id: *type_id,
                    type_name: type_names.get(type_id).cloned().unwrap_or_default(),
                })
                .collect::<Vec<_>>()
        };
        let mut public_books: HashMap<i64, SourceBooks> = HashMap::new();
        for (region_id, types) in &public_types {
            self.market
                .register_public_market_demand(*region_id, registration(types), false, now)
                .await?;
            let source_id = self
                .market
                .ensure_esi_price_source_for_scope(
                    workspace_id,
                    MarketScope {
                        region_id: *region_id,
                        location_id: None,
                    },
                )
                .await?;
            let types: Vec<i64> = types.iter().copied().collect();
            let books = self
                .market
                .get_source_order_books(workspace_id, source_id, &types, 0, None)
                .await?;
            public_books.insert(*region_id, SourceBooks { source_id, books });
        }
        let mut structure_books: HashMap<i64, SourceBooks> = HashMap::new();
        for ((region_id, location_id), types) in &structure_types {
            let source_id = self
                .market
                .ensure_esi_price_source_for_scope(
                    workspace_id,
                    MarketScope {
                        region_id: *region_id,
                        location_id: Some(*location_id),
                    },
                )
                .await?;
            self.market
                .upsert_market_coverage(workspace_id, source_id, registration(types))
                .await?;
            let types: Vec<i64> = types.iter().copied().collect();
            let books = self
                .market
                .get_source_order_books(workspace_id, source_id, &types, *location_id, None)
                .await?;
            structure_books.insert(*location_id, SourceBooks { source_id, books });
        }

        let mut graphs: HashMap<i64, JumpGraph> = HashMap::new();
        let mut stargates_missing = false;
        for facts in locations.values() {
            if graphs.contains_key(&facts.region_id) {
                continue;
            }
            let has_buys = stored.iter().any(|row| {
                row.order.region_id == facts.region_id && row.order.side == MarketOrderSide::Buy
            });
            if !has_buys {
                continue;
            }
            let graph =
                JumpGraph::from_edges(self.sde.region_stargate_edges(facts.region_id).await?);
            stargates_missing |= graph.is_empty();
            graphs.insert(facts.region_id, graph);
        }

        let balances: HashMap<i64, (Option<Decimal>, u64)> = self
            .inventory
            .list_balances(workspace_id, owner_id)
            .await?
            .into_iter()
            .map(|balance| {
                (
                    balance.key.type_id,
                    (
                        balance.average_unit_cost.map(|cost| cost.0),
                        balance.quantity,
                    ),
                )
            })
            .collect();
        let accounting: HashMap<ConnectedCharacterId, Option<u8>> = characters
            .iter()
            .map(|character| (character.connection_id, character.accounting_level))
            .collect();

        // Names for every location shown, including depth ladder rows.
        let mut name_ids: BTreeSet<i64> = locations.keys().copied().collect();
        for books in public_books.values().chain(structure_books.values()) {
            for book in books.books.values() {
                name_ids.extend(book.orders.iter().map(|order| order.location_id));
            }
        }
        let name_ids: Vec<i64> = name_ids.into_iter().collect();
        let location_names = self
            .market
            .resolve_order_location_names(workspace_id, &name_ids)
            .await?;

        let mut groups: BTreeMap<i64, MarketOrderLocation> = BTreeMap::new();
        let mut summary = MarketOrdersSummary::default();
        for row in &stored {
            let order = &row.order;
            let facts = locations.get(&order.location_id);
            let kind = facts.map_or(MarketOrderLocationKind::Unknown, |facts| facts.kind);
            let mut book_orders: Vec<BookOrder> = Vec::new();
            let mut observed: Vec<DateTime<Utc>> = Vec::new();
            let structure_book = structure_books
                .get(&order.location_id)
                .and_then(|books| books.books.get(&order.type_id));
            let public_book = public_books
                .get(&order.region_id)
                .and_then(|books| books.books.get(&order.type_id));
            let needs_structure = kind == MarketOrderLocationKind::Structure;
            let needs_public = kind == MarketOrderLocationKind::Station
                || (needs_structure && order.side == MarketOrderSide::Buy);
            for book in [
                structure_book.filter(|_| needs_structure),
                public_book.filter(|_| needs_public),
            ]
            .into_iter()
            .flatten()
            {
                observed.push(book.effective_observed_at());
                book_orders.extend(book.orders.iter().filter_map(book_order));
            }
            let wanted = usize::from(needs_structure) + usize::from(needs_public);
            let read = usize::from(needs_structure && structure_book.is_some())
                + usize::from(needs_public && public_book.is_some());
            let visibility = if read == 0 {
                BookVisibility::None
            } else if read < wanted {
                BookVisibility::Partial
            } else {
                BookVisibility::Full
            };
            let location_system_id = facts.and_then(|facts| facts.solar_system_id);
            let distances = match (location_system_id, graphs.get(&order.region_id)) {
                (Some(system), Some(graph)) if !graph.is_empty() => {
                    Some(graph.jump_distances(system, MAX_ORDER_RANGE_JUMPS))
                }
                _ => None,
            };
            let input = CompetitionInput {
                order,
                location_system_id,
                book: &book_orders,
                visibility,
                own_order_ids: &own_order_ids,
                jump_distances: distances.as_ref(),
            };
            let standing = evaluate_order(&input);
            let depth = depth(&input, &standing, &location_names);

            let (unit_cost, stock_quantity) =
                balances.get(&order.type_id).copied().unwrap_or((None, 0));
            let level = accounting.get(&row.connection_id).copied().flatten();
            let tax = level.map_or(BASE_SALES_TAX, sales_tax_rate);
            let (margin, margin_at_suggested) = match (order.side, unit_cost) {
                (MarketOrderSide::Sell, Some(cost)) => (
                    Some(sell_margin(
                        standing.price,
                        cost,
                        tax,
                        standing.volume_remain,
                    )),
                    standing
                        .suggested_price
                        .map(|price| sell_margin(price, cost, tax, standing.volume_remain)),
                ),
                _ => (None, None),
            };

            match standing.status {
                StandingStatus::Outbid => summary.outbid += 1,
                StandingStatus::Tied => summary.tied += 1,
                StandingStatus::Best => summary.best += 1,
                StandingStatus::Unknown => summary.unknown += 1,
            }
            match order.side {
                MarketOrderSide::Sell => {
                    summary.active_sell_value +=
                        standing.price * Decimal::from(standing.volume_remain);
                }
                MarketOrderSide::Buy => {
                    summary.buy_escrow += order.escrow.unwrap_or_default();
                }
            }
            let oldest = observed.iter().min().copied();
            if let Some(oldest) = oldest {
                summary.oldest_book_at = Some(
                    summary
                        .oldest_book_at
                        .map_or(oldest, |current| current.min(oldest)),
                );
            }

            let group = groups
                .entry(order.location_id)
                .or_insert_with(|| MarketOrderLocation {
                    location_id: order.location_id,
                    location_name: location_names.get(&order.location_id).cloned(),
                    region_id: order.region_id,
                    kind,
                    unavailable: None,
                    book_observed_at: None,
                    orders: Vec::new(),
                });
            if let Some(oldest) = oldest {
                group.book_observed_at =
                    Some(group.book_observed_at.map_or(oldest, |at| at.min(oldest)));
            }
            group.orders.push(MarketOrderRow {
                order_id: order.order_id,
                type_id: order.type_id,
                type_name: type_names.get(&order.type_id).cloned(),
                side: order.side,
                connection_id: row.connection_id,
                eve_character_id: row.eve_character_id,
                character_name: row.character_name.clone(),
                range: order.range,
                volume_total: order.volume_total,
                min_volume: order.min_volume,
                duration_days: order.duration_days,
                expires_at: order.issued_at + Duration::days(i64::from(order.duration_days)),
                escrow: order.escrow,
                first_seen_at: row.first_seen_at,
                standing,
                unit_cost,
                stock_quantity,
                sales_tax_rate: tax,
                sales_tax_estimated: level.is_none(),
                margin,
                margin_at_suggested,
                depth,
            });
        }

        for group in groups.values_mut() {
            group.unavailable = self
                .unavailable(workspace_id, group, &structure_books, &public_books, now)
                .await?;
            group.orders.sort_by(|a, b| {
                status_rank(a.standing.status)
                    .cmp(&status_rank(b.standing.status))
                    .then_with(|| {
                        b.standing
                            .gap_percent
                            .unwrap_or_default()
                            .cmp(&a.standing.gap_percent.unwrap_or_default())
                    })
                    .then_with(|| a.order_id.cmp(&b.order_id))
            });
        }
        let mut locations: Vec<MarketOrderLocation> = groups.into_values().collect();
        locations.sort_by(|a, b| {
            let outbid = |location: &MarketOrderLocation| {
                location
                    .orders
                    .iter()
                    .filter(|row| row.standing.status == StandingStatus::Outbid)
                    .count()
            };
            outbid(b)
                .cmp(&outbid(a))
                .then_with(|| a.location_name.cmp(&b.location_name))
        });

        Ok(MarketOrdersOverview {
            generated_at: now,
            characters,
            stargates_missing,
            summary,
            locations,
        })
    }

    /// Moves every book the page shows to the front of the refresh queue.
    pub async fn prioritize_books(
        &self,
        workspace_id: WorkspaceId,
        now: DateTime<Utc>,
    ) -> Result<(), MarketOrdersError> {
        let mut stored = self.orders.open_market_orders(workspace_id).await?;
        stored.retain(|row| !row.order.is_corporation);
        let locations = self.location_facts(workspace_id, &stored).await?;
        let (public_types, structure_types) = books_needed(&stored, &locations);
        let all_types: Vec<i64> = public_types
            .values()
            .chain(structure_types.values())
            .flatten()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let type_names = self.sde.type_names(&all_types).await?;
        let registration = |types: &BTreeSet<i64>| {
            types
                .iter()
                .map(|type_id| MarketCoverageRegistration {
                    type_id: *type_id,
                    type_name: type_names.get(type_id).cloned().unwrap_or_default(),
                })
                .collect::<Vec<_>>()
        };
        for (region_id, types) in &public_types {
            self.market
                .register_public_market_demand(*region_id, registration(types), true, now)
                .await?;
        }
        for ((region_id, location_id), types) in &structure_types {
            let source_id = self
                .market
                .ensure_esi_price_source_for_scope(
                    workspace_id,
                    MarketScope {
                        region_id: *region_id,
                        location_id: Some(*location_id),
                    },
                )
                .await?;
            self.market
                .upsert_market_coverage(workspace_id, source_id, registration(types))
                .await?;
            let types: Vec<i64> = types.iter().copied().collect();
            self.market
                .prioritize_market_refresh(workspace_id, source_id, &types, now)
                .await?;
        }
        Ok(())
    }

    /// Classifies every order location, resolving unknown structures once.
    async fn location_facts(
        &self,
        workspace_id: WorkspaceId,
        stored: &[iskworks_core::market_orders::WorkspaceMarketOrder],
    ) -> Result<HashMap<i64, LocationFacts>, MarketOrdersError> {
        let regions: BTreeMap<i64, i64> = stored
            .iter()
            .map(|row| (row.order.location_id, row.order.region_id))
            .collect();
        let mut classified = BTreeMap::new();
        for location_id in regions.keys() {
            classified.insert(
                *location_id,
                self.market
                    .classify_location(workspace_id, *location_id)
                    .await?,
            );
        }
        let unknown: Vec<i64> = classified
            .iter()
            .filter(|(id, kind)| {
                **id >= MIN_STRUCTURE_ID && **kind == MarketLocationClassification::Unknown
            })
            .map(|(id, _)| *id)
            .collect();
        if let (Some(resolver), false) = (&self.structures, unknown.is_empty()) {
            resolver
                .resolve_structure_locations(workspace_id, &unknown)
                .await;
            for location_id in unknown {
                classified.insert(
                    location_id,
                    self.market
                        .classify_location(workspace_id, location_id)
                        .await?,
                );
            }
        }
        let stations: Vec<i64> = classified
            .iter()
            .filter(|(_, kind)| **kind == MarketLocationClassification::NpcStation)
            .map(|(id, _)| *id)
            .collect();
        let station_systems: HashMap<i64, i64> = self
            .sde
            .resolve_npc_stations(&stations)
            .await?
            .into_iter()
            .map(|station| (station.station_id, station.solar_system_id))
            .collect();
        Ok(classified
            .into_iter()
            .map(|(location_id, classification)| {
                let region_id = regions[&location_id];
                let facts = match classification {
                    MarketLocationClassification::NpcStation => LocationFacts {
                        region_id,
                        kind: MarketOrderLocationKind::Station,
                        solar_system_id: station_systems.get(&location_id).copied(),
                    },
                    MarketLocationClassification::Structure { solar_system_id } => LocationFacts {
                        region_id,
                        kind: MarketOrderLocationKind::Structure,
                        solar_system_id: Some(solar_system_id),
                    },
                    MarketLocationClassification::Unknown => LocationFacts {
                        region_id,
                        kind: MarketOrderLocationKind::Unknown,
                        solar_system_id: None,
                    },
                };
                (location_id, facts)
            })
            .collect())
    }

    /// Why a location shows no book, if every order there lacks one.
    async fn unavailable(
        &self,
        workspace_id: WorkspaceId,
        group: &MarketOrderLocation,
        structure_books: &HashMap<i64, SourceBooks>,
        public_books: &HashMap<i64, SourceBooks>,
        now: DateTime<Utc>,
    ) -> Result<Option<BookUnavailable>, MarketOrdersError> {
        if group
            .orders
            .iter()
            .any(|row| row.standing.visibility != BookVisibility::None)
        {
            return Ok(None);
        }
        let source = match group.kind {
            MarketOrderLocationKind::Unknown => return Ok(Some(BookUnavailable::UnknownLocation)),
            MarketOrderLocationKind::Structure => structure_books.get(&group.location_id),
            MarketOrderLocationKind::Station => public_books.get(&group.region_id),
        };
        let Some(source) = source else {
            return Ok(Some(BookUnavailable::NoDataYet));
        };
        if group.kind == MarketOrderLocationKind::Station {
            return Ok(Some(BookUnavailable::NoDataYet));
        }
        let type_ids: Vec<i64> = group.orders.iter().map(|row| row.type_id).collect();
        let failure = self
            .market
            .market_refresh_candidates_for_type_ids(workspace_id, source.source_id, &type_ids, now)
            .await?
            .into_iter()
            .filter(|item| item.refresh_state == MarketRefreshState::Failed)
            .find_map(|item| item.last_error);
        Ok(Some(match failure {
            Some(detail) => BookUnavailable::RefreshFailed { detail },
            None => BookUnavailable::NoDataYet,
        }))
    }
}

/// Types per region whose public book is needed (station orders, and
/// structure buy orders that compete with station buy orders), and types per
/// (region, structure) whose structure book is needed.
#[allow(clippy::type_complexity)]
fn books_needed(
    stored: &[iskworks_core::market_orders::WorkspaceMarketOrder],
    locations: &HashMap<i64, LocationFacts>,
) -> (
    BTreeMap<i64, BTreeSet<i64>>,
    BTreeMap<(i64, i64), BTreeSet<i64>>,
) {
    let mut public_types: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
    let mut structure_types: BTreeMap<(i64, i64), BTreeSet<i64>> = BTreeMap::new();
    for row in stored {
        let order = &row.order;
        let Some(facts) = locations.get(&order.location_id) else {
            continue;
        };
        match facts.kind {
            MarketOrderLocationKind::Station => {
                public_types
                    .entry(facts.region_id)
                    .or_default()
                    .insert(order.type_id);
            }
            MarketOrderLocationKind::Structure => {
                structure_types
                    .entry((facts.region_id, order.location_id))
                    .or_default()
                    .insert(order.type_id);
                if order.side == MarketOrderSide::Buy {
                    public_types
                        .entry(facts.region_id)
                        .or_default()
                        .insert(order.type_id);
                }
            }
            MarketOrderLocationKind::Unknown => {}
        }
    }
    (public_types, structure_types)
}

fn status_rank(status: StandingStatus) -> u8 {
    match status {
        StandingStatus::Outbid => 0,
        StandingStatus::Unknown => 1,
        StandingStatus::Tied => 2,
        StandingStatus::Best => 3,
    }
}

fn book_order(order: &iskworks_core::MarketOrderView) -> Option<BookOrder> {
    Some(BookOrder {
        order_id: order.order_id,
        side: order.side,
        price: order.price.0,
        volume_remain: i64::try_from(order.remaining_volume).ok()?,
        range: OrderRange::from_db_i32(order.order_range)?,
        location_id: order.location_id,
        solar_system_id: order.solar_system_id,
        issued_at: order.issued_at,
    })
}

/// Up to `DEPTH_LEVELS` same-side orders on each side of the order's own
/// price (buy orders out of range included, flagged), plus the opposite
/// side's best orders that trade here.
fn depth(
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

#[cfg(test)]
mod tests;
