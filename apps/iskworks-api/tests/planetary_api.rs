use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::{Duration, Utc};
use iskworks_api::{build_router, AppState};
use iskworks_app::{PlanetaryCharacter, PlanetaryCharacterSource};
use iskworks_core::planetary::{PlanetaryPreferences, PlanetaryPreferencesRepository};
use iskworks_core::{
    CharacterSourceKind, CharacterSourceSyncState, ConnectedCharacterId, InventoryError,
    MarketError, MarketOrderBook, MarketOrderSide, MarketOrderView, MarketPriceSource,
    MarketRefreshState, MarketRepository, MarketScope, Money, PriceSourceId, WorkspaceId,
};
use iskworks_sde::{
    ActiveSde, PlanetReference, PlanetSchematic, PlanetSchematicLine, SdeError, SdeReadRepository,
    TypeReference,
};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

mod support;
use support::workspace::configured_workspace;

// Real ids: Barren launchpad 2544 (group 1030), ECU 2848, basic factory 2473;
// Base Metals 2267 -> Reactive Metals 2398 via schematic 126.
const BASE: i64 = 2_267;
const REACTIVE: i64 = 2_398;
const LAUNCHPAD: i64 = 2_544;
const PLANET_III: i64 = 40_050_359;
const PLANET_IV: i64 = 40_050_361;

fn planets_summary(expires_in_hours: i64, launchpad_reactive: i64) -> Value {
    let now = Utc::now();
    json!({ "planets": [
        {
            "planet_id": PLANET_IV, "planet_type": "barren", "solar_system_id": 30_000_797,
            "upgrade_level": 5, "num_pins": 3, "last_update": now.to_rfc3339(),
            "pins": [
                { "pin_id": 1, "type_id": 2_848,
                  "expiry_time": (now + Duration::hours(expires_in_hours)).to_rfc3339(),
                  "extractor": { "product_type_id": BASE, "qty_per_cycle": 6_000,
                                 "cycle_time_seconds": 1_800, "head_count": 8 } },
                { "pin_id": 2, "type_id": 2_473, "schematic_id": 126 },
                { "pin_id": 3, "type_id": LAUNCHPAD,
                  "contents": [{ "type_id": REACTIVE, "amount": launchpad_reactive }] }
            ]
        },
        {
            "planet_id": PLANET_III, "planet_type": "lava", "solar_system_id": 30_000_797,
            "upgrade_level": 4, "num_pins": 0, "last_update": now.to_rfc3339(), "pins": []
        }
    ]})
}

fn character(name: &str, eve_id: i64, summary: Option<Value>) -> PlanetaryCharacter {
    PlanetaryCharacter {
        connection_id: ConnectedCharacterId(Uuid::new_v4()),
        eve_character_id: eve_id,
        name: name.to_string(),
        scope_granted: summary.is_some(),
        planets_source: Some(CharacterSourceSyncState {
            connection_id: ConnectedCharacterId(Uuid::new_v4()),
            source_kind: CharacterSourceKind::Planets,
            refresh_state: if summary.is_some() {
                MarketRefreshState::Current
            } else {
                MarketRefreshState::Failed
            },
            observed_at: summary.as_ref().map(|_| Utc::now()),
            last_error: summary
                .is_none()
                .then(|| "missing scope: esi-planets.manage_planets.v1".to_string()),
            summary,
            last_attempted_at: None,
            next_refresh_at: None,
        }),
        skills_summary: Some(json!({ "skills": [
            { "skill_id": 2_495, "trained_skill_level": 4, "active_skill_level": 4 }
        ]})),
    }
}

struct FakeCharacters(Vec<PlanetaryCharacter>);

#[async_trait]
impl PlanetaryCharacterSource for FakeCharacters {
    async fn planetary_characters(
        &self,
        _: WorkspaceId,
    ) -> Result<Vec<PlanetaryCharacter>, InventoryError> {
        Ok(self.0.clone())
    }
}

#[derive(Default)]
struct FakePreferences(Mutex<PlanetaryPreferences>);

#[async_trait]
impl PlanetaryPreferencesRepository for FakePreferences {
    async fn planetary_preferences(
        &self,
        _: WorkspaceId,
    ) -> Result<PlanetaryPreferences, InventoryError> {
        Ok(self.0.lock().unwrap().clone())
    }
    async fn save_planetary_preferences(
        &self,
        _: WorkspaceId,
        preferences: &PlanetaryPreferences,
    ) -> Result<(), InventoryError> {
        *self.0.lock().unwrap() = preferences.clone();
        Ok(())
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
    ) -> Result<Vec<iskworks_sde::BlueprintSearchResult>, SdeError> {
        unimplemented!()
    }
    async fn manufacturing_recipe(
        &self,
        _: i64,
    ) -> Result<Option<iskworks_sde::ManufacturingRecipe>, SdeError> {
        unimplemented!()
    }
    async fn search_types(
        &self,
        _: &str,
        _: u32,
    ) -> Result<Vec<iskworks_sde::TypeSearchResult>, SdeError> {
        unimplemented!()
    }
    async fn type_reference(&self, ids: &[i64]) -> Result<BTreeMap<i64, TypeReference>, SdeError> {
        let all = BTreeMap::from([
            (BASE, ("Base Metals", 1_032, Decimal::new(5, 3))),
            (REACTIVE, ("Reactive Metals", 1_042, Decimal::new(19, 2))),
            (LAUNCHPAD, ("Barren Launchpad", 1_030, Decimal::ZERO)),
            (
                2_848,
                ("Barren Extractor Control Unit", 1_063, Decimal::ZERO),
            ),
            (
                2_473,
                ("Barren Basic Industry Facility", 1_028, Decimal::ZERO),
            ),
        ]);
        Ok(ids
            .iter()
            .filter_map(|id| {
                all.get(id).map(|(name, group_id, volume)| {
                    (
                        *id,
                        TypeReference {
                            type_name: Some((*name).to_string()),
                            group_id: Some(*group_id),
                            packaged_volume_m3: Some(*volume),
                            ..TypeReference::default()
                        },
                    )
                })
            })
            .collect())
    }
    async fn planet_schematics(
        &self,
        ids: &[i64],
    ) -> Result<BTreeMap<i64, PlanetSchematic>, SdeError> {
        Ok(ids
            .iter()
            .filter(|id| **id == 126)
            .map(|id| {
                (
                    *id,
                    PlanetSchematic {
                        schematic_id: 126,
                        name: "Reactive Metals".into(),
                        cycle_time_seconds: 1_800,
                        inputs: vec![PlanetSchematicLine {
                            type_id: BASE,
                            quantity: 3_000,
                        }],
                        outputs: vec![PlanetSchematicLine {
                            type_id: REACTIVE,
                            quantity: 20,
                        }],
                    },
                )
            })
            .collect())
    }
    async fn planet_references(
        &self,
        ids: &[i64],
    ) -> Result<BTreeMap<i64, PlanetReference>, SdeError> {
        Ok(ids
            .iter()
            .map(|id| {
                let numeral = if *id == PLANET_III { "III" } else { "IV" };
                (
                    *id,
                    PlanetReference {
                        planet_id: *id,
                        name: format!("Q-3HS5 {numeral}"),
                        solar_system_id: 30_000_797,
                        solar_system_name: "Q-3HS5".into(),
                        security_status: Some(Decimal::new(-14, 2)),
                    },
                )
            })
            .collect())
    }
}

struct FakeMarket {
    requested: Mutex<Vec<i64>>,
}

#[async_trait]
impl MarketRepository for FakeMarket {
    async fn scoped_order_books(
        &self,
        _: WorkspaceId,
        _: MarketScope,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<MarketOrderView>>, MarketError> {
        self.requested.lock().unwrap().extend_from_slice(type_ids);
        let order = |price: i64, side| MarketOrderView {
            observation_id: None,
            import_batch_id: None,
            imported_file_id: None,
            order_id: price,
            type_id: REACTIVE,
            type_name: "Reactive Metals".into(),
            side,
            price: Money(Decimal::from(price)),
            remaining_volume: 1_000_000,
            entered_volume: 1_000_000,
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
        Ok(BTreeMap::from([(
            REACTIVE,
            vec![
                order(400, MarketOrderSide::Buy),
                order(390, MarketOrderSide::Buy),
                order(450, MarketOrderSide::Sell),
            ],
        )]))
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

struct Harness {
    app: axum::Router,
    preferences: Arc<FakePreferences>,
    market: Arc<FakeMarket>,
}

fn harness(characters: Vec<PlanetaryCharacter>) -> Harness {
    let preferences = Arc::new(FakePreferences::default());
    let market = Arc::new(FakeMarket {
        requested: Mutex::new(Vec::new()),
    });
    let app = build_router(
        AppState::new(Arc::new(configured_workspace("PI Test")))
            .with_sde_repository(Arc::new(FakeSde))
            .with_market_repository(market.clone())
            .with_planetary_repositories(Arc::new(FakeCharacters(characters)), preferences.clone()),
    );
    Harness {
        app,
        preferences,
        market,
    }
}

async fn send(app: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn overview(app: &axum::Router) -> Value {
    let (status, body) = send(
        app,
        Request::get("/api/planetary").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

#[tokio::test]
async fn overview_derives_named_priced_planets_per_character() {
    let harness = harness(vec![character(
        "Valka",
        2_119_000_001,
        Some(planets_summary(2, 30_000)),
    )]);

    let body = overview(&harness.app).await;

    let valka = &body["characters"][0];
    assert_eq!(valka["name"], "Valka");
    assert_eq!(valka["piSkillLevel"], 4);
    assert_eq!(valka["scopeGranted"], true);
    // Planets ordered by system then id: III (40050359) before IV.
    assert_eq!(valka["planets"][0]["name"], "Q-3HS5 III");
    let planet = &valka["planets"][1];
    assert_eq!(planet["name"], "Q-3HS5 IV");
    assert_eq!(planet["security"], "-0.14");
    assert_eq!(planet["extractors"][0]["productName"], "Base Metals");
    assert_eq!(planet["extractors"][0]["unitsPerHour"], "12000");
    assert_eq!(planet["production"][0]["name"], "Reactive Metals");
    // One basic factory: 40 Reactive/h at best buy 400 = 11.52M/mo; surplus
    // Base Metals is exported unpriced.
    let reactive = planet["exports"]
        .as_array()
        .unwrap()
        .iter()
        .find(|export| export["typeId"] == REACTIVE)
        .unwrap();
    assert_eq!(reactive["unitsPerHour"], "40");
    assert_eq!(reactive["iskPerMonth"], "11520000");
    // 30 000 x 0.19 m3 = 5 700 of 10 000 m3.
    assert_eq!(planet["storage"][0]["kind"], "L");
    assert_eq!(planet["storage"][0]["fillPercent"], "57");
    assert_eq!(
        planet["storage"][0]["contents"][0]["name"],
        "Reactive Metals"
    );
    assert_eq!(planet["attention"], "amber");

    assert_eq!(body["summary"]["iskPerMonth"], "11520000");
    assert_eq!(body["summary"]["planetCount"], 2);
    assert_eq!(body["summary"]["nextAction"]["planetName"], "Q-3HS5 IV");
    assert_eq!(body["summary"]["nextAction"]["characterName"], "Valka");
    assert!(!harness
        .market
        .requested
        .lock()
        .unwrap()
        .contains(&LAUNCHPAD));
}

#[tokio::test]
async fn a_character_without_the_scope_is_listed_with_its_error_and_no_planets() {
    let harness = harness(vec![character("Alt", 9, None)]);

    let body = overview(&harness.app).await;

    let alt = &body["characters"][0];
    assert_eq!(alt["scopeGranted"], false);
    assert_eq!(alt["planets"], json!([]));
    assert_eq!(alt["sync"]["refreshState"], "failed");
    assert!(alt["sync"]["lastError"]
        .as_str()
        .unwrap()
        .starts_with("missing scope:"));
}

#[tokio::test]
async fn saved_preferences_reorder_characters_and_exclude_exports() {
    let harness = harness(vec![
        character("Alpha", 1, Some(planets_summary(48, 10))),
        character("Bravo", 2, Some(planets_summary(48, 10))),
    ]);

    let (status, saved) = send(
        &harness.app,
        Request::put("/api/planetary/preferences")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "excludedExports": [
                        { "characterId": 2, "planetId": PLANET_IV, "typeId": REACTIVE },
                        { "characterId": 2, "planetId": PLANET_IV, "typeId": REACTIVE }
                    ],
                    "characterOrder": [2, 1, 2]
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["excludedExports"].as_array().unwrap().len(), 1);
    assert_eq!(saved["characterOrder"], json!([2, 1]));
    assert_eq!(
        harness.preferences.0.lock().unwrap().character_order,
        vec![2, 1]
    );

    let body = overview(&harness.app).await;
    assert_eq!(body["characters"][0]["name"], "Bravo");
    assert_eq!(body["characters"][0]["iskPerMonth"], "0");
    let excluded = body["characters"][0]["planets"][1]["exports"]
        .as_array()
        .unwrap()
        .iter()
        .find(|export| export["typeId"] == REACTIVE)
        .unwrap()
        .clone();
    assert_eq!(excluded["excluded"], true);
    assert_eq!(body["characters"][1]["iskPerMonth"], "11520000");
    assert_eq!(body["summary"]["iskPerMonth"], "11520000");

    let (status, read_back) = send(
        &harness.app,
        Request::get("/api/planetary/preferences")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read_back, saved);
}

#[tokio::test]
async fn planetary_routes_report_unconfigured_integration() {
    let app = build_router(AppState::new(Arc::new(configured_workspace("PI Test"))));
    let response = app
        .oneshot(Request::get("/api/planetary").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(!response.status().is_success());
}
