use std::sync::Mutex;

use iskworks_core::market_orders::{CharacterMarketOrder, WorkspaceMarketOrder};
use iskworks_core::{
    InventoryBalance, InventoryEventId, InventoryHistory, InventoryItemKey, InventoryPosting,
    MarketCoverageItem, MarketImportBatchId, MarketObservationBatchId, MarketOrderView,
    MarketPriceSource, Money,
};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, ManufacturingRecipe, NpcStationSearchResult, TypeSearchResult,
};
use uuid::Uuid;

use super::*;

const FORGE: i64 = 10_000_002;
const JITA: i64 = 30_000_142;
const JITA_4_4: i64 = 60_003_760;
const FORTIZAR: i64 = 1_035_466_617_946;
const UNKNOWN_CITADEL: i64 = 1_040_000_000_001;
const ISHTAR: i64 = 12_005;
const MEGACYTE: i64 = 40;

fn workspace() -> WorkspaceId {
    WorkspaceId(Uuid::from_u128(1))
}

fn owner() -> OwnerId {
    OwnerId(Uuid::from_u128(2))
}

fn kael() -> ConnectedCharacterId {
    ConnectedCharacterId(Uuid::from_u128(10))
}

fn now() -> DateTime<Utc> {
    "2026-10-21T12:00:00Z".parse().unwrap()
}

fn d(value: &str) -> Decimal {
    value.parse().unwrap()
}

fn stored(
    order_id: i64,
    type_id: i64,
    location_id: i64,
    side: MarketOrderSide,
    price: &str,
) -> WorkspaceMarketOrder {
    WorkspaceMarketOrder {
        order: CharacterMarketOrder {
            order_id,
            type_id,
            region_id: FORGE,
            location_id,
            side,
            is_corporation: false,
            price: d(price),
            volume_total: 5,
            volume_remain: 5,
            min_volume: 1,
            range: OrderRange::Region,
            issued_at: now() - Duration::hours(2),
            duration_days: 90,
            escrow: (side == MarketOrderSide::Buy).then(|| d("1000")),
        },
        connection_id: kael(),
        eve_character_id: 90_000_001,
        character_name: "Kael Varon".to_string(),
        first_seen_at: now() - Duration::days(1),
        last_seen_at: now() - Duration::minutes(10),
    }
}

fn view(
    order_id: i64,
    type_id: i64,
    location_id: i64,
    side: MarketOrderSide,
    price: &str,
) -> MarketOrderView {
    MarketOrderView {
        observation_id: None,
        import_batch_id: None,
        imported_file_id: None,
        order_id,
        type_id,
        type_name: String::new(),
        side,
        price: Money(d(price)),
        remaining_volume: 3,
        entered_volume: 3,
        minimum_volume: 1,
        order_range: 32_767,
        issued_at: now() - Duration::minutes(30),
        duration_days: 90,
        observed_at: now() - Duration::minutes(3),
        revalidated_at: None,
        location_id,
        solar_system_id: JITA,
        region_id: FORGE,
        jumps: 0,
    }
}

fn book(type_id: i64, location_id: i64, orders: Vec<MarketOrderView>) -> MarketOrderBook {
    MarketOrderBook {
        type_id,
        type_name: String::new(),
        location_id,
        location_name: String::new(),
        solar_system_id: JITA,
        region_id: FORGE,
        observed_at: now() - Duration::minutes(3),
        revalidated_at: None,
        observation_batch_id: MarketObservationBatchId(Uuid::from_u128(99)),
        import_batch_id: None,
        imported_file_id: None,
        buy_order_count: 0,
        sell_order_count: 0,
        total_buy_volume: 0,
        total_sell_volume: 0,
        lowest_sell: None,
        highest_buy: None,
        orders,
    }
}

struct FakeOrders(Vec<WorkspaceMarketOrder>);

#[async_trait]
impl CharacterMarketOrderRepository for FakeOrders {
    async fn replace_open_orders(
        &self,
        _: ConnectedCharacterId,
        _: &[CharacterMarketOrder],
        _: DateTime<Utc>,
    ) -> Result<(), InventoryError> {
        unimplemented!()
    }

    async fn open_market_orders(
        &self,
        _: WorkspaceId,
    ) -> Result<Vec<WorkspaceMarketOrder>, InventoryError> {
        Ok(self.0.clone())
    }
}

struct FakeCharacters(Option<u8>);

#[async_trait]
impl TraderCharacterSource for FakeCharacters {
    async fn trader_characters(
        &self,
        _: WorkspaceId,
    ) -> Result<Vec<TraderCharacter>, InventoryError> {
        Ok(vec![TraderCharacter {
            connection_id: kael(),
            eve_character_id: 90_000_001,
            character_name: "Kael Varon".to_string(),
            orders_scope_missing: false,
            orders_synced_at: Some(now() - Duration::minutes(12)),
            orders_sync_error: None,
            accounting_level: self.0,
        }])
    }
}

fn source_id(region_id: i64, location_id: Option<i64>) -> PriceSourceId {
    PriceSourceId(Uuid::from_u128(
        (region_id as u128) << 64 | location_id.unwrap_or(0) as u128,
    ))
}

#[derive(Default)]
struct FakeMarket {
    classifications: Mutex<HashMap<i64, MarketLocationClassification>>,
    /// Region-wide public books by type.
    public_books: HashMap<i64, MarketOrderBook>,
    /// Structure books by (structure, type).
    structure_books: HashMap<(i64, i64), MarketOrderBook>,
    structure_failure: Option<String>,
    public_demand: Mutex<Vec<(i64, Vec<i64>, bool)>>,
    structure_coverage: Mutex<Vec<(PriceSourceId, Vec<i64>)>>,
    prioritized: Mutex<Vec<(PriceSourceId, Vec<i64>)>>,
}

#[async_trait]
impl MarketRepository for FakeMarket {
    async fn resolve_type_name(&self, _: i64) -> Result<Option<String>, MarketError> {
        unimplemented!()
    }
    async fn location_names(
        &self,
        _: WorkspaceId,
        _: &[i64],
    ) -> Result<BTreeMap<i64, String>, MarketError> {
        unimplemented!()
    }
    async fn resolve_order_location_names(
        &self,
        _: WorkspaceId,
        location_ids: &[i64],
    ) -> Result<BTreeMap<i64, String>, MarketError> {
        Ok(location_ids
            .iter()
            .map(|id| (*id, format!("Location {id}")))
            .collect())
    }
    async fn save_location_names(
        &self,
        _: WorkspaceId,
        locations: Vec<iskworks_core::ResolvedMarketLocation>,
    ) -> Result<(), MarketError> {
        let mut classifications = self.classifications.lock().unwrap();
        for location in locations {
            classifications.insert(
                location.location_id,
                MarketLocationClassification::Structure {
                    solar_system_id: location.solar_system_id,
                },
            );
        }
        Ok(())
    }
    async fn imported_file_checksums(
        &self,
        _: WorkspaceId,
        _: &[String],
    ) -> Result<BTreeSet<String>, MarketError> {
        unimplemented!()
    }
    async fn commit_import(
        &self,
        _: WorkspaceId,
        _: Vec<iskworks_core::ResolvedMarketExport>,
        _: u64,
        _: Vec<String>,
    ) -> Result<iskworks_core::MarketImportBatch, MarketError> {
        unimplemented!()
    }
    async fn list_imports(
        &self,
        _: WorkspaceId,
    ) -> Result<Vec<iskworks_core::MarketImportBatch>, MarketError> {
        unimplemented!()
    }
    async fn get_import(
        &self,
        _: WorkspaceId,
        _: MarketImportBatchId,
    ) -> Result<iskworks_core::MarketImportBatch, MarketError> {
        unimplemented!()
    }
    async fn get_order_book(
        &self,
        _: WorkspaceId,
        _: i64,
        _: i64,
        _: Option<MarketImportBatchId>,
    ) -> Result<MarketOrderBook, MarketError> {
        unimplemented!()
    }
    async fn get_market_price_source(
        &self,
        _: WorkspaceId,
        _: PriceSourceId,
    ) -> Result<MarketPriceSource, MarketError> {
        unimplemented!()
    }
    async fn classify_location(
        &self,
        _: WorkspaceId,
        location_id: i64,
    ) -> Result<MarketLocationClassification, MarketError> {
        Ok(self
            .classifications
            .lock()
            .unwrap()
            .get(&location_id)
            .copied()
            .unwrap_or(MarketLocationClassification::Unknown))
    }
    async fn ensure_esi_price_source_for_scope(
        &self,
        _: WorkspaceId,
        scope: MarketScope,
    ) -> Result<PriceSourceId, MarketError> {
        Ok(source_id(scope.region_id, scope.location_id))
    }
    async fn register_public_market_demand(
        &self,
        region_id: i64,
        items: Vec<MarketCoverageRegistration>,
        prioritize: bool,
        _: DateTime<Utc>,
    ) -> Result<(), MarketError> {
        self.public_demand.lock().unwrap().push((
            region_id,
            items.into_iter().map(|item| item.type_id).collect(),
            prioritize,
        ));
        Ok(())
    }
    async fn upsert_market_coverage(
        &self,
        _: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<(), MarketError> {
        self.structure_coverage.lock().unwrap().push((
            source_id,
            items.into_iter().map(|item| item.type_id).collect(),
        ));
        Ok(())
    }
    async fn prioritize_market_refresh(
        &self,
        _: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        _: DateTime<Utc>,
    ) -> Result<bool, MarketError> {
        self.prioritized
            .lock()
            .unwrap()
            .push((source_id, type_ids.to_vec()));
        Ok(true)
    }
    async fn get_source_order_books(
        &self,
        _: WorkspaceId,
        _: PriceSourceId,
        type_ids: &[i64],
        location_id: i64,
        _: Option<MarketImportBatchId>,
    ) -> Result<BTreeMap<i64, MarketOrderBook>, MarketError> {
        Ok(type_ids
            .iter()
            .filter_map(|type_id| {
                let book = if location_id == 0 {
                    self.public_books.get(type_id)
                } else {
                    self.structure_books.get(&(location_id, *type_id))
                };
                book.map(|book| (*type_id, book.clone()))
            })
            .collect())
    }
    async fn market_refresh_candidates_for_type_ids(
        &self,
        _: WorkspaceId,
        _: PriceSourceId,
        type_ids: &[i64],
        _: DateTime<Utc>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        Ok(type_ids
            .iter()
            .map(|type_id| MarketCoverageItem {
                type_id: *type_id,
                type_name: String::new(),
                refresh_state: if self.structure_failure.is_some() {
                    MarketRefreshState::Failed
                } else {
                    MarketRefreshState::Missing
                },
                observed_at: None,
                last_attempted_at: None,
                next_refresh_at: None,
                last_error: self.structure_failure.clone(),
                order_count: 0,
                buy_order_count: 0,
                sell_order_count: 0,
                prior_etag: None,
                revalidated_at: None,
            })
            .collect())
    }
}

#[derive(Default)]
struct FakeSde {
    edges: Vec<(i64, i64)>,
}

#[async_trait]
impl SdeReadRepository for FakeSde {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        unimplemented!()
    }
    async fn search_manufacturing_blueprints(
        &self,
        _: &str,
        _: u32,
    ) -> Result<Vec<BlueprintSearchResult>, SdeError> {
        unimplemented!()
    }
    async fn manufacturing_recipe(&self, _: i64) -> Result<Option<ManufacturingRecipe>, SdeError> {
        unimplemented!()
    }
    async fn search_types(&self, _: &str, _: u32) -> Result<Vec<TypeSearchResult>, SdeError> {
        unimplemented!()
    }
    async fn type_names(&self, type_ids: &[i64]) -> Result<BTreeMap<i64, String>, SdeError> {
        Ok(type_ids
            .iter()
            .map(|id| (*id, format!("Type {id}")))
            .collect())
    }
    async fn resolve_npc_stations(
        &self,
        station_ids: &[i64],
    ) -> Result<Vec<NpcStationSearchResult>, SdeError> {
        Ok(station_ids
            .iter()
            .map(|id| NpcStationSearchResult {
                station_id: *id,
                station_name: format!("Station {id}"),
                station_type_id: 1,
                station_type_name: None,
                solar_system_id: JITA,
                solar_system_name: "Jita".to_string(),
                region_id: FORGE,
                region_name: "The Forge".to_string(),
                security_class: "high".to_string(),
            })
            .collect())
    }
    async fn region_stargate_edges(&self, _: i64) -> Result<Vec<(i64, i64)>, SdeError> {
        Ok(self.edges.clone())
    }
}

struct FakeInventory(Vec<(i64, u64, &'static str)>);

#[async_trait]
impl InventoryRepository for FakeInventory {
    async fn list_balances(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
    ) -> Result<Vec<InventoryBalance>, InventoryError> {
        Ok(self
            .0
            .iter()
            .map(|(type_id, quantity, unit)| InventoryBalance {
                key: InventoryItemKey {
                    workspace_id,
                    owner_id,
                    type_id: *type_id,
                },
                type_name: String::new(),
                quantity: *quantity,
                total_historical_cost: Money(d(unit) * Decimal::from(*quantity)),
                average_unit_cost: Some(Money(d(unit))),
                revision: 1,
                last_activity_at: None,
            })
            .collect())
    }
    async fn get_history(&self, _: &InventoryItemKey) -> Result<InventoryHistory, InventoryError> {
        unimplemented!()
    }
    async fn post(&self, _: InventoryPosting) -> Result<InventoryHistory, InventoryError> {
        unimplemented!()
    }
    async fn reverse_latest(
        &self,
        _: &InventoryItemKey,
        _: InventoryEventId,
        _: u64,
        _: String,
    ) -> Result<InventoryHistory, InventoryError> {
        unimplemented!()
    }
    async fn rebuild(
        &self,
        _: WorkspaceId,
        _: OwnerId,
    ) -> Result<Vec<InventoryBalance>, InventoryError> {
        unimplemented!()
    }
}

#[derive(Default)]
struct RecordingResolver(Mutex<Vec<Vec<i64>>>);

#[async_trait]
impl StructureLocationResolver for Arc<FakeMarket> {
    async fn resolve_structure_locations(&self, workspace_id: WorkspaceId, structure_ids: &[i64]) {
        let locations = structure_ids
            .iter()
            .map(|id| iskworks_core::ResolvedMarketLocation {
                location_id: *id,
                location_name: "Resolved".to_string(),
                owner_id: 1,
                solar_system_id: JITA,
                structure_type_id: Some(35_833),
                resolved_by_connection_id: kael(),
                resolved_at: now(),
            })
            .collect();
        self.save_location_names(workspace_id, locations)
            .await
            .unwrap();
    }
}

#[async_trait]
impl StructureLocationResolver for RecordingResolver {
    async fn resolve_structure_locations(&self, _: WorkspaceId, structure_ids: &[i64]) {
        self.0.lock().unwrap().push(structure_ids.to_vec());
    }
}

fn service(
    orders: Vec<WorkspaceMarketOrder>,
    market: Arc<FakeMarket>,
    sde: FakeSde,
    inventory: FakeInventory,
    accounting: Option<u8>,
) -> MarketOrdersService {
    MarketOrdersService::new(
        Arc::new(FakeOrders(orders)),
        Arc::new(FakeCharacters(accounting)),
        market,
        Arc::new(sde),
        Arc::new(inventory),
    )
}

fn classified(
    entries: &[(i64, MarketLocationClassification)],
) -> Mutex<HashMap<i64, MarketLocationClassification>> {
    Mutex::new(entries.iter().copied().collect())
}

#[tokio::test]
async fn an_outbid_station_sell_order_carries_its_margin_and_counts() {
    use MarketOrderSide::{Buy, Sell};
    let market = Arc::new(FakeMarket {
        classifications: classified(&[(JITA_4_4, MarketLocationClassification::NpcStation)]),
        public_books: HashMap::from([(
            ISHTAR,
            book(
                ISHTAR,
                0,
                vec![
                    view(1, ISHTAR, JITA_4_4, Sell, "312400000"),
                    view(2, ISHTAR, JITA_4_4, Sell, "311900000"),
                    view(3, ISHTAR, JITA_4_4, Buy, "290000000"),
                ],
            ),
        )]),
        ..FakeMarket::default()
    });
    let mut corporate = stored(50, ISHTAR, JITA_4_4, Sell, "1");
    corporate.order.is_corporation = true;
    let service = service(
        vec![stored(1, ISHTAR, JITA_4_4, Sell, "312400000"), corporate],
        market.clone(),
        FakeSde::default(),
        FakeInventory(vec![(ISHTAR, 5, "280000000")]),
        Some(5),
    );

    let overview = service.overview(workspace(), owner(), now()).await.unwrap();

    assert_eq!(
        *market.public_demand.lock().unwrap(),
        vec![(FORGE, vec![ISHTAR], false)]
    );
    assert_eq!(overview.summary.outbid, 1);
    assert_eq!(overview.summary.active_sell_value, d("312400000") * d("3"));
    assert!(overview.summary.oldest_book_at.is_some());
    assert_eq!(overview.locations.len(), 1);
    let location = &overview.locations[0];
    assert_eq!(location.kind, MarketOrderLocationKind::Station);
    assert_eq!(location.location_name.as_deref(), Some("Location 60003760"));
    // The corporation order is left out.
    assert_eq!(location.orders.len(), 1);
    let row = &location.orders[0];
    assert_eq!(row.type_name.as_deref(), Some("Type 12005"));
    assert_eq!(row.standing.status, StandingStatus::Outbid);
    assert_eq!(row.standing.suggested_price, Some(d("311800000")));
    assert_eq!(row.unit_cost, Some(d("280000000")));
    assert_eq!(row.stock_quantity, 5);
    assert_eq!(row.sales_tax_rate, d("0.033750"));
    assert!(!row.sales_tax_estimated);
    let margin = row.margin.as_ref().unwrap();
    assert!(margin.margin_unit > Decimal::ZERO);
    assert!(row.margin_at_suggested.is_some());
    // Depth: own order and the competitor, plus the buy order opposite.
    assert_eq!(
        row.depth
            .same_side
            .iter()
            .map(|order| (order.order_id, order.is_own))
            .collect::<Vec<_>>(),
        vec![(2, false), (1, true)]
    );
    assert_eq!(row.depth.opposite_side.len(), 1);
}

#[tokio::test]
async fn an_unknown_accounting_level_uses_the_base_tax_and_says_so() {
    let market = Arc::new(FakeMarket {
        classifications: classified(&[(JITA_4_4, MarketLocationClassification::NpcStation)]),
        ..FakeMarket::default()
    });
    let service = service(
        vec![stored(
            1,
            ISHTAR,
            JITA_4_4,
            MarketOrderSide::Sell,
            "312400000",
        )],
        market,
        FakeSde::default(),
        FakeInventory(vec![]),
        None,
    );

    let overview = service.overview(workspace(), owner(), now()).await.unwrap();

    let row = &overview.locations[0].orders[0];
    assert_eq!(row.sales_tax_rate, BASE_SALES_TAX);
    assert!(row.sales_tax_estimated);
    // No stock: no cost basis, no margin.
    assert_eq!(row.unit_cost, None);
    assert!(row.margin.is_none());
    // No book yet.
    assert_eq!(row.standing.status, StandingStatus::Unknown);
    assert_eq!(
        overview.locations[0].unavailable,
        Some(BookUnavailable::NoDataYet)
    );
}

#[tokio::test]
async fn a_structure_without_market_access_reports_why() {
    let market = Arc::new(FakeMarket {
        classifications: classified(&[(
            FORTIZAR,
            MarketLocationClassification::Structure {
                solar_system_id: JITA,
            },
        )]),
        structure_failure: Some(
            "no connected character has granted esi-markets.structure_markets.v1 access"
                .to_string(),
        ),
        ..FakeMarket::default()
    });
    let service = service(
        vec![stored(
            1,
            ISHTAR,
            FORTIZAR,
            MarketOrderSide::Sell,
            "312400000",
        )],
        market.clone(),
        FakeSde::default(),
        FakeInventory(vec![]),
        Some(5),
    );

    let overview = service.overview(workspace(), owner(), now()).await.unwrap();

    assert_eq!(
        *market.structure_coverage.lock().unwrap(),
        vec![(source_id(FORGE, Some(FORTIZAR)), vec![ISHTAR])]
    );
    // A structure sell order needs no public book.
    assert!(market.public_demand.lock().unwrap().is_empty());
    let location = &overview.locations[0];
    assert_eq!(location.kind, MarketOrderLocationKind::Structure);
    assert_eq!(
        location.unavailable,
        Some(BookUnavailable::RefreshFailed {
            detail: "no connected character has granted esi-markets.structure_markets.v1 access"
                .to_string()
        })
    );
    assert_eq!(location.orders[0].standing.visibility, BookVisibility::None);
}

#[tokio::test]
async fn a_structure_buy_order_also_reads_the_public_book_and_is_partial_without_its_own() {
    use MarketOrderSide::Buy;
    let mut competitor = view(7, MEGACYTE, JITA_4_4, Buy, "9215");
    competitor.order_range = 32_767;
    let market = Arc::new(FakeMarket {
        classifications: classified(&[(
            FORTIZAR,
            MarketLocationClassification::Structure {
                solar_system_id: JITA,
            },
        )]),
        public_books: HashMap::from([(MEGACYTE, book(MEGACYTE, 0, vec![competitor]))]),
        ..FakeMarket::default()
    });
    let service = service(
        vec![stored(1, MEGACYTE, FORTIZAR, Buy, "9210")],
        market.clone(),
        FakeSde {
            edges: vec![(JITA, 30_000_144)],
        },
        FakeInventory(vec![]),
        Some(5),
    );

    let overview = service.overview(workspace(), owner(), now()).await.unwrap();

    assert_eq!(
        *market.public_demand.lock().unwrap(),
        vec![(FORGE, vec![MEGACYTE], false)]
    );
    assert!(!overview.stargates_missing);
    let row = &overview.locations[0].orders[0];
    assert_eq!(row.standing.visibility, BookVisibility::Partial);
    assert_eq!(row.standing.status, StandingStatus::Outbid);
    assert_eq!(row.standing.suggested_price, Some(d("9216")));
    assert!(row.margin.is_none());
    assert_eq!(overview.summary.buy_escrow, d("1000"));
}

#[tokio::test]
async fn buy_orders_without_imported_stargates_flag_it() {
    let market = Arc::new(FakeMarket {
        classifications: classified(&[(JITA_4_4, MarketLocationClassification::NpcStation)]),
        ..FakeMarket::default()
    });
    let service = service(
        vec![stored(1, MEGACYTE, JITA_4_4, MarketOrderSide::Buy, "9210")],
        market,
        FakeSde::default(),
        FakeInventory(vec![]),
        Some(5),
    );

    let overview = service.overview(workspace(), owner(), now()).await.unwrap();

    assert!(overview.stargates_missing);
}

#[tokio::test]
async fn an_unknown_structure_is_resolved_once_and_then_read() {
    let market = Arc::new(FakeMarket::default());
    let service = service(
        vec![stored(
            1,
            ISHTAR,
            UNKNOWN_CITADEL,
            MarketOrderSide::Sell,
            "312400000",
        )],
        market.clone(),
        FakeSde::default(),
        FakeInventory(vec![]),
        Some(5),
    )
    .with_structure_resolver(Arc::new(market.clone()));

    let overview = service.overview(workspace(), owner(), now()).await.unwrap();

    assert_eq!(
        overview.locations[0].kind,
        MarketOrderLocationKind::Structure
    );
    assert_eq!(
        *market.structure_coverage.lock().unwrap(),
        vec![(source_id(FORGE, Some(UNKNOWN_CITADEL)), vec![ISHTAR])]
    );
}

#[tokio::test]
async fn a_structure_nobody_can_resolve_stays_unknown() {
    let market = Arc::new(FakeMarket::default());
    let resolver = Arc::new(RecordingResolver::default());
    let service = service(
        vec![stored(
            1,
            ISHTAR,
            UNKNOWN_CITADEL,
            MarketOrderSide::Sell,
            "312400000",
        )],
        market.clone(),
        FakeSde::default(),
        FakeInventory(vec![]),
        Some(5),
    )
    .with_structure_resolver(resolver.clone());

    let overview = service.overview(workspace(), owner(), now()).await.unwrap();

    assert_eq!(*resolver.0.lock().unwrap(), vec![vec![UNKNOWN_CITADEL]]);
    let location = &overview.locations[0];
    assert_eq!(location.kind, MarketOrderLocationKind::Unknown);
    assert_eq!(location.unavailable, Some(BookUnavailable::UnknownLocation));
    assert!(market.structure_coverage.lock().unwrap().is_empty());
}

#[tokio::test]
async fn prioritizing_moves_every_book_to_the_front() {
    let market = Arc::new(FakeMarket {
        classifications: classified(&[
            (JITA_4_4, MarketLocationClassification::NpcStation),
            (
                FORTIZAR,
                MarketLocationClassification::Structure {
                    solar_system_id: JITA,
                },
            ),
        ]),
        ..FakeMarket::default()
    });
    let service = service(
        vec![
            stored(1, ISHTAR, JITA_4_4, MarketOrderSide::Sell, "312400000"),
            stored(2, MEGACYTE, FORTIZAR, MarketOrderSide::Sell, "9500"),
        ],
        market.clone(),
        FakeSde::default(),
        FakeInventory(vec![]),
        Some(5),
    );

    service.prioritize_books(workspace(), now()).await.unwrap();

    assert_eq!(
        *market.public_demand.lock().unwrap(),
        vec![(FORGE, vec![ISHTAR], true)]
    );
    assert_eq!(
        *market.prioritized.lock().unwrap(),
        vec![(source_id(FORGE, Some(FORTIZAR)), vec![MEGACYTE])]
    );
}

#[tokio::test]
async fn an_order_in_both_the_structure_and_public_book_counts_once() {
    use MarketOrderSide::Buy;
    // ESI's public region book also carries orders from public structures,
    // so a structure buy order sees the same competitor in both books.
    let mut structure_competitor = view(7, MEGACYTE, FORTIZAR, Buy, "9215");
    structure_competitor.observed_at = now() - Duration::minutes(2);
    let mut public_copy = view(7, MEGACYTE, FORTIZAR, Buy, "9200");
    public_copy.observed_at = now() - Duration::minutes(9);
    let own_copy = view(1, MEGACYTE, FORTIZAR, Buy, "9210");
    let market = Arc::new(FakeMarket {
        classifications: classified(&[(
            FORTIZAR,
            MarketLocationClassification::Structure {
                solar_system_id: JITA,
            },
        )]),
        structure_books: HashMap::from([(
            (FORTIZAR, MEGACYTE),
            book(
                MEGACYTE,
                FORTIZAR,
                vec![structure_competitor, own_copy.clone()],
            ),
        )]),
        public_books: HashMap::from([(MEGACYTE, book(MEGACYTE, 0, vec![public_copy, own_copy]))]),
        ..FakeMarket::default()
    });
    let service = service(
        vec![stored(1, MEGACYTE, FORTIZAR, Buy, "9210")],
        market,
        FakeSde::default(),
        FakeInventory(vec![]),
        Some(5),
    );

    let overview = service.overview(workspace(), owner(), now()).await.unwrap();

    let row = &overview.locations[0].orders[0];
    assert_eq!(row.standing.competitors_ahead, 1);
    assert_eq!(row.standing.volume_ahead, 3);
    // The freshest copy wins: the structure book's 9,215.
    assert_eq!(
        row.standing.best_competitor.as_ref().unwrap().price,
        d("9215")
    );
    let ids: Vec<i64> = row
        .depth
        .same_side
        .iter()
        .map(|order| order.order_id)
        .collect();
    assert_eq!(ids, vec![7, 1]);
}

fn fortizar_book_without_the_order() -> Arc<FakeMarket> {
    Arc::new(FakeMarket {
        classifications: classified(&[(
            FORTIZAR,
            MarketLocationClassification::Structure {
                solar_system_id: JITA,
            },
        )]),
        // Fetched 3 minutes ago; lists a competitor but not order 1.
        structure_books: HashMap::from([(
            (FORTIZAR, ISHTAR),
            book(
                ISHTAR,
                FORTIZAR,
                vec![view(2, ISHTAR, FORTIZAR, MarketOrderSide::Sell, "4901000")],
            ),
        )]),
        ..FakeMarket::default()
    })
}

#[tokio::test]
async fn an_order_a_newer_book_no_longer_lists_is_likely_filled() {
    // Last returned by the orders sync 10 minutes ago.
    let service = service(
        vec![stored(
            1,
            ISHTAR,
            FORTIZAR,
            MarketOrderSide::Sell,
            "4903000",
        )],
        fortizar_book_without_the_order(),
        FakeSde::default(),
        FakeInventory(vec![]),
        Some(5),
    );

    let overview = service.overview(workspace(), owner(), now()).await.unwrap();

    let row = &overview.locations[0].orders[0];
    assert_eq!(row.standing.status, StandingStatus::LikelyFilled);
    assert_eq!(overview.summary.outbid, 0);
    assert_eq!(overview.summary.likely_filled, 1);
    assert_eq!(overview.summary.active_sell_value, Decimal::ZERO);
}

#[tokio::test]
async fn a_book_within_its_cache_window_of_the_sync_proves_nothing() {
    // Synced 6 minutes ago; ESI's copy behind a 3-minute-old book can be
    // up to 5 minutes older, so it may predate the order.
    let mut order = stored(1, ISHTAR, FORTIZAR, MarketOrderSide::Sell, "4903000");
    order.last_seen_at = now() - Duration::minutes(6);
    let service = service(
        vec![order],
        fortizar_book_without_the_order(),
        FakeSde::default(),
        FakeInventory(vec![]),
        Some(5),
    );

    let overview = service.overview(workspace(), owner(), now()).await.unwrap();

    let row = &overview.locations[0].orders[0];
    assert_eq!(row.standing.status, StandingStatus::Outbid);
    assert_eq!(overview.summary.likely_filled, 0);
}
