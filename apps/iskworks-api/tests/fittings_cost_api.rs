//! A saved fit's cost at the workspace's market and where the workspace's
//! characters already hold it, over fakes.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::Utc;
use iskworks_api::{build_router, AppState};
use iskworks_core::fitting::{
    AssetHolding, AssetHoldingsRepository, Fit, FitDraft, FitId, FitItem, FitRepository,
    ModuleState, SlotKind,
};
use iskworks_core::{
    ConnectedCharacterId, InventoryError, MarketError, MarketOrderBook, MarketOrderSide,
    MarketOrderView, MarketPriceSource, MarketRepository, MarketScope, Money, PriceSourceId,
    WorkspaceId,
};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, ImportCounts, ManufacturingRecipe, SdeError,
    SdeReadRepository, TypeReference, TypeSearchResult,
};
use rust_decimal::Decimal;
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

mod support;
use support::workspace::configured_workspace;

const BUSTARD: i64 = 12_731;
const AUTOCANNON: i64 = 2_873;
const EMP_S: i64 = 185;
const HOBGOBLIN: i64 = 2_456;

struct FakeSde;

#[async_trait]
impl SdeReadRepository for FakeSde {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        Ok(Some(ActiveSde {
            import_id: Uuid::new_v4(),
            source_version: "3482594".to_string(),
            source_label: "eve-online-static-data-3482594-jsonl.zip".to_string(),
            source_checksum: "abc".to_string(),
            completed_at: Utc::now(),
            counts: ImportCounts::default(),
        }))
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
    async fn type_reference(
        &self,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, TypeReference>, SdeError> {
        let known = [
            (BUSTARD, "Bustard"),
            (AUTOCANNON, "200mm AutoCannon II"),
            (EMP_S, "EMP S"),
            (HOBGOBLIN, "Hobgoblin II"),
        ];
        Ok(known
            .into_iter()
            .filter(|(type_id, _)| type_ids.contains(type_id))
            .map(|(type_id, name)| {
                (
                    type_id,
                    TypeReference {
                        type_name: Some(name.to_string()),
                        group_id: Some(1),
                        group_name: None,
                        category_id: Some(7),
                        category_name: None,
                        packaged_volume_m3: None,
                    },
                )
            })
            .collect())
    }
}

/// One saved fit: a Bustard with two loaded autocannons and five
/// Hobgoblins.
struct FakeFits(Fit);

fn fit() -> Fit {
    let gun = |slot_index| FitItem {
        type_id: AUTOCANNON,
        slot_kind: SlotKind::High,
        slot_index: Some(slot_index),
        quantity: 1,
        state: ModuleState::Active,
        charge_type_id: Some(EMP_S),
        mutation: None,
    };
    Fit {
        id: FitId(Uuid::new_v4()),
        name: "Hauler".to_string(),
        description: String::new(),
        ship_type_id: BUSTARD,
        items: vec![
            gun(0),
            gun(1),
            FitItem {
                type_id: HOBGOBLIN,
                slot_kind: SlotKind::DroneBay,
                slot_index: None,
                quantity: 5,
                state: ModuleState::Active,
                charge_type_id: None,
                mutation: None,
            },
        ],
        tags: Vec::new(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[async_trait]
impl FitRepository for FakeFits {
    async fn list_fits(&self, _: WorkspaceId) -> Result<Vec<Fit>, InventoryError> {
        Ok(vec![self.0.clone()])
    }
    async fn fit(&self, _: WorkspaceId, id: FitId) -> Result<Option<Fit>, InventoryError> {
        Ok((self.0.id == id).then(|| self.0.clone()))
    }
    async fn create_fit(
        &self,
        _: WorkspaceId,
        _: &FitDraft,
        _: chrono::DateTime<Utc>,
    ) -> Result<Fit, InventoryError> {
        unimplemented!()
    }
    async fn update_fit(
        &self,
        _: WorkspaceId,
        _: FitId,
        _: &FitDraft,
        _: chrono::DateTime<Utc>,
    ) -> Result<Option<Fit>, InventoryError> {
        unimplemented!()
    }
    async fn delete_fit(&self, _: WorkspaceId, _: FitId) -> Result<bool, InventoryError> {
        unimplemented!()
    }
    async fn fit_tags_in_use(&self, _: WorkspaceId) -> Result<Vec<String>, InventoryError> {
        unimplemented!()
    }
    async fn replace_fit_tags(
        &self,
        _: WorkspaceId,
        _: FitId,
        _: &[String],
    ) -> Result<Option<Vec<String>>, InventoryError> {
        unimplemented!()
    }
    async fn edit_fit_tags(
        &self,
        _: WorkspaceId,
        _: &[FitId],
        _: &[String],
        _: &[String],
    ) -> Result<(), InventoryError> {
        unimplemented!()
    }
}

/// Sells the hull at 100M (and 110M) and the guns at 1M; nothing for the
/// drones. Records the scope it was asked for.
#[derive(Default)]
struct FakeMarket {
    scopes: Mutex<Vec<MarketScope>>,
}

#[async_trait]
impl MarketRepository for FakeMarket {
    async fn scoped_order_books(
        &self,
        _: WorkspaceId,
        scope: MarketScope,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<MarketOrderView>>, MarketError> {
        self.scopes.lock().unwrap().push(scope);
        let order = |type_id: i64, price: i64, side| MarketOrderView {
            observation_id: None,
            import_batch_id: None,
            imported_file_id: None,
            order_id: price,
            type_id,
            type_name: String::new(),
            side,
            price: Money(Decimal::from(price)),
            remaining_volume: 10,
            entered_volume: 10,
            minimum_volume: 1,
            order_range: 0,
            issued_at: Utc::now(),
            duration_days: 90,
            observed_at: Utc::now(),
            revalidated_at: None,
            location_id: 60_003_760,
            solar_system_id: 30_000_142,
            region_id: 10_000_002,
            jumps: 0,
        };
        let books = BTreeMap::from([
            (
                BUSTARD,
                vec![
                    order(BUSTARD, 110_000_000, MarketOrderSide::Sell),
                    order(BUSTARD, 100_000_000, MarketOrderSide::Sell),
                    order(BUSTARD, 90_000_000, MarketOrderSide::Buy),
                ],
            ),
            (
                AUTOCANNON,
                vec![order(AUTOCANNON, 1_000_000, MarketOrderSide::Sell)],
            ),
        ]);
        Ok(books
            .into_iter()
            .filter(|(type_id, _)| type_ids.contains(type_id))
            .collect())
    }
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
    ) -> Result<std::collections::BTreeSet<String>, MarketError> {
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
        _: iskworks_core::MarketImportBatchId,
    ) -> Result<iskworks_core::MarketImportBatch, MarketError> {
        unimplemented!()
    }
    async fn get_order_book(
        &self,
        _: WorkspaceId,
        _: i64,
        _: i64,
        _: Option<iskworks_core::MarketImportBatchId>,
    ) -> Result<MarketOrderBook, MarketError> {
        unimplemented!()
    }
    async fn get_source_order_books(
        &self,
        _: WorkspaceId,
        _: PriceSourceId,
        _: &[i64],
        _: i64,
        _: Option<iskworks_core::MarketImportBatchId>,
    ) -> Result<BTreeMap<i64, MarketOrderBook>, MarketError> {
        unimplemented!()
    }
    async fn get_market_price_source(
        &self,
        _: WorkspaceId,
        _: PriceSourceId,
    ) -> Result<MarketPriceSource, MarketError> {
        unimplemented!()
    }
}

/// Valka has the hull and both guns in Jita; Rook has one gun in Amarr.
struct FakeHoldings;

#[async_trait]
impl AssetHoldingsRepository for FakeHoldings {
    async fn type_holdings(
        &self,
        _: WorkspaceId,
        type_ids: &[i64],
    ) -> Result<Vec<AssetHolding>, InventoryError> {
        let holding =
            |character: u128, name: &str, location_id, location: &str, type_id, quantity| {
                AssetHolding {
                    connection_id: ConnectedCharacterId(Uuid::from_u128(character)),
                    character_name: name.to_string(),
                    location_id,
                    location_name: location.to_string(),
                    type_id,
                    quantity,
                }
            };
        Ok(vec![
            holding(1, "Valka", 60_003_760, "Jita IV - Moon 4", BUSTARD, 1),
            holding(1, "Valka", 60_003_760, "Jita IV - Moon 4", AUTOCANNON, 4),
            holding(2, "Rook", 60_008_494, "Amarr VIII", AUTOCANNON, 1),
        ]
        .into_iter()
        .filter(|holding| type_ids.contains(&holding.type_id))
        .collect())
    }
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn app(fit: &Fit, market: Arc<FakeMarket>) -> axum::Router {
    build_router(
        AppState::new(Arc::new(configured_workspace("Fit Cost Test")))
            .with_sde_repository(Arc::new(FakeSde))
            .with_fit_repository(Arc::new(FakeFits(fit.clone())))
            .with_market_repository(market)
            .with_fit_asset_holdings(Arc::new(FakeHoldings)),
    )
}

#[tokio::test]
async fn a_fit_is_priced_at_the_lowest_sell_in_the_workspace_market() {
    let fit = fit();
    let market = Arc::new(FakeMarket::default());
    let (status, body) = get(
        &app(&fit, market.clone()),
        &format!("/api/fittings/{}/cost", fit.id.0),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    // The workspace has no market set: Jita 4-4.
    assert_eq!(body["scope"]["regionId"], 10_000_002);
    assert_eq!(body["scope"]["locationId"], 60_003_760);
    assert_eq!(
        market.scopes.lock().unwrap()[0].location_id,
        Some(60_003_760)
    );
    let lines = body["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 3, "hull, guns, drones; no charges");
    assert_eq!(lines[0]["name"], "Bustard");
    assert_eq!(lines[0]["section"], "hull");
    assert_eq!(lines[0]["unitPrice"], "100000000");
    assert_eq!(lines[1]["quantity"], 2);
    assert_eq!(lines[1]["total"], "2000000");
    assert_eq!(lines[2]["name"], "Hobgoblin II");
    assert_eq!(lines[2]["unitPrice"], Value::Null);
    assert_eq!(body["hull"], "100000000");
    assert_eq!(body["fittings"], "2000000");
    assert_eq!(body["drones"], "0");
    assert_eq!(body["total"], "102000000");
    assert_eq!(body["missing"], serde_json::json!([HOBGOBLIN]));
    assert!(body["observedAt"].is_string());
}

#[tokio::test]
async fn assets_list_where_each_character_holds_each_type_of_the_fit() {
    let fit = fit();
    let (status, body) = get(
        &app(&fit, Arc::new(FakeMarket::default())),
        &format!("/api/fittings/{}/assets", fit.id.0),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let holdings = body["holdings"].as_array().unwrap();
    let rows: Vec<_> = holdings
        .iter()
        .map(|holding| {
            (
                holding["typeId"].as_i64().unwrap(),
                holding["characterName"].as_str().unwrap(),
                holding["locationName"].as_str().unwrap(),
                holding["quantity"].as_i64().unwrap(),
            )
        })
        .collect();
    // By type, the most held first.
    assert_eq!(
        rows,
        [
            (AUTOCANNON, "Valka", "Jita IV - Moon 4", 4),
            (AUTOCANNON, "Rook", "Amarr VIII", 1),
            (BUSTARD, "Valka", "Jita IV - Moon 4", 1),
        ]
    );
    assert!(holdings[0]["connectionId"].is_string());
    assert_eq!(holdings[0]["locationId"], 60_003_760);
}

#[tokio::test]
async fn cost_and_assets_of_an_unknown_fit_are_not_found() {
    let app = app(&fit(), Arc::new(FakeMarket::default()));
    for path in ["cost", "assets"] {
        let (status, _) = get(&app, &format!("/api/fittings/{}/{path}", Uuid::new_v4())).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
}
