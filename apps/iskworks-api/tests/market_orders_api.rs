use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::{DateTime, Duration, Utc};
use iskworks_api::{build_router, AppState};
use iskworks_app::{MarketOrdersService, TraderCharacter, TraderCharacterSource};
use iskworks_core::market_orders::{
    CharacterMarketOrder, CharacterMarketOrderRepository, OrderRange, WorkspaceMarketOrder,
};
use iskworks_core::{
    ConnectedCharacterId, InventoryError, MarketCoverageRegistration, MarketError,
    MarketImportBatchId, MarketLocationClassification, MarketObservationBatchId, MarketOrderBook,
    MarketOrderSide, MarketOrderView, MarketPriceSource, MarketRepository, MarketScope, Money,
    PriceSourceId, WorkspaceId,
};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, ManufacturingRecipe, NpcStationSearchResult, SdeError,
    SdeReadRepository, TypeSearchResult,
};
use rust_decimal::Decimal;
use tower::ServiceExt;
use uuid::Uuid;

mod support;
use support::inventory::SeededInventoryRepository;
use support::workspace::configured_workspace;

const KAEL: ConnectedCharacterId = ConnectedCharacterId(Uuid::from_u128(1));
const FORGE: i64 = 10_000_002;
const JITA: i64 = 30_000_142;
const JITA_4_4: i64 = 60_003_760;
const ISHTAR: i64 = 12_005;

fn d(value: &str) -> Decimal {
    value.parse().unwrap()
}

fn issued() -> DateTime<Utc> {
    Utc::now() - Duration::hours(2)
}

struct FakeOrders;

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
        Ok(vec![WorkspaceMarketOrder {
            order: CharacterMarketOrder {
                order_id: 1,
                type_id: ISHTAR,
                region_id: FORGE,
                location_id: JITA_4_4,
                side: MarketOrderSide::Sell,
                is_corporation: false,
                price: d("312400000"),
                volume_total: 5,
                volume_remain: 5,
                min_volume: 1,
                range: OrderRange::Region,
                issued_at: issued(),
                duration_days: 90,
                escrow: None,
            },
            connection_id: KAEL,
            eve_character_id: 90_000_001,
            character_name: "Kael Varon".into(),
            first_seen_at: issued(),
            last_seen_at: Utc::now(),
        }])
    }
}

struct FakeCharacters;

#[async_trait]
impl TraderCharacterSource for FakeCharacters {
    async fn trader_characters(
        &self,
        _: WorkspaceId,
    ) -> Result<Vec<TraderCharacter>, InventoryError> {
        Ok(vec![TraderCharacter {
            connection_id: KAEL,
            eve_character_id: 90_000_001,
            character_name: "Kael Varon".into(),
            orders_scope_missing: false,
            orders_synced_at: Some(Utc::now() - Duration::minutes(12)),
            orders_sync_error: None,
            accounting_level: Some(5),
        }])
    }
}

fn view(order_id: i64, price: &str, minutes_ago: i64) -> MarketOrderView {
    MarketOrderView {
        observation_id: None,
        import_batch_id: None,
        imported_file_id: None,
        order_id,
        type_id: ISHTAR,
        type_name: "Ishtar".into(),
        side: MarketOrderSide::Sell,
        price: Money(d(price)),
        remaining_volume: 5,
        entered_volume: 5,
        minimum_volume: 1,
        order_range: 32_767,
        issued_at: Utc::now() - Duration::minutes(minutes_ago),
        duration_days: 90,
        observed_at: Utc::now() - Duration::minutes(3),
        revalidated_at: None,
        location_id: JITA_4_4,
        solar_system_id: JITA,
        region_id: FORGE,
        jumps: 0,
    }
}

#[derive(Default)]
struct FakeMarket {
    prioritized_public: Mutex<Vec<(i64, bool)>>,
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
            .filter(|id| **id == JITA_4_4)
            .map(|id| {
                (
                    *id,
                    "Jita IV - Moon 4 - Caldari Navy Assembly Plant (0.9)".to_string(),
                )
            })
            .collect())
    }
    async fn save_location_names(
        &self,
        _: WorkspaceId,
        _: Vec<iskworks_core::ResolvedMarketLocation>,
    ) -> Result<(), MarketError> {
        unimplemented!()
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
        _: i64,
    ) -> Result<MarketLocationClassification, MarketError> {
        Ok(MarketLocationClassification::NpcStation)
    }
    async fn ensure_esi_price_source_for_scope(
        &self,
        _: WorkspaceId,
        _: MarketScope,
    ) -> Result<PriceSourceId, MarketError> {
        Ok(PriceSourceId(Uuid::from_u128(7)))
    }
    async fn register_public_market_demand(
        &self,
        region_id: i64,
        _: Vec<MarketCoverageRegistration>,
        prioritize: bool,
        _: DateTime<Utc>,
    ) -> Result<(), MarketError> {
        self.prioritized_public
            .lock()
            .unwrap()
            .push((region_id, prioritize));
        Ok(())
    }
    async fn get_source_order_books(
        &self,
        _: WorkspaceId,
        _: PriceSourceId,
        _: &[i64],
        _: i64,
        _: Option<MarketImportBatchId>,
    ) -> Result<BTreeMap<i64, MarketOrderBook>, MarketError> {
        Ok(BTreeMap::from([(
            ISHTAR,
            MarketOrderBook {
                type_id: ISHTAR,
                type_name: "Ishtar".into(),
                location_id: 0,
                location_name: String::new(),
                solar_system_id: 0,
                region_id: FORGE,
                observed_at: Utc::now() - Duration::minutes(3),
                revalidated_at: None,
                observation_batch_id: MarketObservationBatchId(Uuid::from_u128(3)),
                import_batch_id: None,
                imported_file_id: None,
                buy_order_count: 0,
                sell_order_count: 2,
                total_buy_volume: 0,
                total_sell_volume: 10,
                lowest_sell: None,
                highest_buy: None,
                orders: vec![view(1, "312400000", 90), view(2, "311900000", 47)],
            },
        )]))
    }
}

struct FakeSde;

#[async_trait]
impl SdeReadRepository for FakeSde {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        Ok(None)
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
            .map(|id| (*id, "Ishtar".to_string()))
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
                station_name: "Jita IV - Moon 4 - Caldari Navy Assembly Plant".into(),
                station_type_id: 1,
                station_type_name: None,
                solar_system_id: JITA,
                solar_system_name: "Jita".into(),
                region_id: FORGE,
                region_name: "The Forge".into(),
                security_class: "high".into(),
            })
            .collect())
    }
}

fn app(market: Arc<FakeMarket>) -> axum::Router {
    let service = MarketOrdersService::new(
        Arc::new(FakeOrders),
        Arc::new(FakeCharacters),
        market,
        Arc::new(FakeSde),
        Arc::new(
            SeededInventoryRepository::new([(ISHTAR, 5)]).with_unit_basis(ISHTAR, "280000000"),
        ),
    );
    build_router(
        AppState::new(Arc::new(configured_workspace("Market Orders Test")))
            .with_market_orders_service(Arc::new(service)),
    )
}

async fn send(app: axum::Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

#[tokio::test]
async fn market_orders_are_grouped_by_location_with_standing_and_margin() {
    let (status, body) = send(
        app(Arc::default()),
        Request::get("/api/market-orders")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["summary"]["outbid"], 1);
    assert_eq!(body["stargatesMissing"], false);
    assert_eq!(body["characters"][0]["characterName"], "Kael Varon");
    assert_eq!(body["characters"][0]["accountingLevel"], 5);
    let location = &body["locations"][0];
    assert_eq!(location["kind"], "station");
    assert_eq!(
        location["locationName"],
        "Jita IV - Moon 4 - Caldari Navy Assembly Plant (0.9)"
    );
    assert!(location["unavailable"].is_null());
    let row = &location["orders"][0];
    assert_eq!(row["typeName"], "Ishtar");
    assert_eq!(row["side"], "sell");
    assert_eq!(row["range"]["kind"], "region");
    assert_eq!(row["standing"]["status"], "outbid");
    assert_eq!(row["standing"]["freshSource"], "book");
    assert_eq!(row["standing"]["suggestedPrice"], "311800000");
    assert_eq!(row["standing"]["bestCompetitor"]["orderId"], 2);
    assert!(row["standing"]["outbidSince"].is_string());
    assert_eq!(
        row["unitCost"]
            .as_str()
            .unwrap()
            .parse::<Decimal>()
            .unwrap(),
        d("280000000")
    );
    assert!(row["margin"]["marginUnit"].is_string());
    assert!(row["marginAtSuggested"]["marginPercent"].is_string());
    assert_eq!(row["salesTaxEstimated"], false);
    assert_eq!(row["depth"]["sameSide"][1]["isOwn"], true);
}

#[tokio::test]
async fn refreshing_prioritizes_the_books_and_accepts() {
    let market = Arc::new(FakeMarket::default());
    let (status, _) = send(
        app(market.clone()),
        Request::post("/api/market-orders/refresh")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(
        *market.prioritized_public.lock().unwrap(),
        vec![(FORGE, true)]
    );
}

#[tokio::test]
async fn market_orders_without_storage_are_unavailable() {
    let app = build_router(AppState::new(Arc::new(configured_workspace(
        "Market Orders Test",
    ))));
    let (status, _) = send(
        app,
        Request::get("/api/market-orders")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}
