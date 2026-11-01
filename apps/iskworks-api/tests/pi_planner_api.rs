use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::Utc;
use iskworks_api::{build_router, AppState};
use iskworks_app::{PlanetaryCharacter, PlanetaryCharacterSource};
use iskworks_core::pi_planner::fixtures::{self, *};
use iskworks_core::pi_planner::{
    DensityWrite, PiPlannerInputsRepository, PlanetSettingsWrite, StoredDensity,
    StoredPlanetSettings,
};
use iskworks_core::planetary::{PlanetaryPreferences, PlanetaryPreferencesRepository};
use iskworks_core::{
    ConnectedCharacterId, InventoryError, MarketError, MarketOrderBook, MarketOrderSide,
    MarketOrderView, MarketPriceSource, MarketRepository, MarketScope, Money, PriceSourceId,
    WorkspaceId,
};
use iskworks_sde::{
    ActiveSde, PlanetCommodity, PlanetPinType, PlanetSchematic, PlannerPlanet, SdeError,
    SdeReadRepository, SystemDistance, TypeReference,
};
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

mod support;
use support::workspace::configured_workspace;

const HOME: i64 = 30_000_797;
const LAVA_I: i64 = 40_000_001;
const BARREN_II: i64 = 40_000_002;
const PLASMA_III: i64 = 40_000_003;

/// The planner data as the current SDE has it, or an import that predates
/// it.
struct FakeSde {
    planner_data: bool,
}

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
    async fn planet_pin_types(&self) -> Result<Vec<PlanetPinType>, SdeError> {
        Ok(if self.planner_data {
            fixtures::pin_types()
        } else {
            Vec::new()
        })
    }
    async fn planet_schematic_catalogue(&self) -> Result<Vec<PlanetSchematic>, SdeError> {
        Ok(fixtures::schematics())
    }
    async fn planet_commodities(&self) -> Result<BTreeMap<i64, PlanetCommodity>, SdeError> {
        Ok(fixtures::commodities())
    }
    async fn type_reference(&self, ids: &[i64]) -> Result<BTreeMap<i64, TypeReference>, SdeError> {
        let volumes = fixtures::volumes();
        Ok(ids
            .iter()
            .map(|id| {
                (
                    *id,
                    TypeReference {
                        type_name: Some(name(*id)),
                        packaged_volume_m3: volumes.get(id).and_then(|v| Decimal::from_f64(*v)),
                        ..TypeReference::default()
                    },
                )
            })
            .collect())
    }
    async fn type_names(&self, ids: &[i64]) -> Result<BTreeMap<i64, String>, SdeError> {
        Ok(ids.iter().map(|id| (*id, name(*id))).collect())
    }
    async fn systems_within_jumps(
        &self,
        solar_system_id: i64,
        _: u8,
    ) -> Result<Vec<SystemDistance>, SdeError> {
        Ok(if solar_system_id == HOME {
            vec![SystemDistance {
                solar_system_id: HOME,
                name: "Q-3HS5".into(),
                security_status: Some(Decimal::new(-14, 2)),
                region_name: Some("Insmother".into()),
                wormhole: false,
                jumps: 0,
            }]
        } else {
            Vec::new()
        })
    }
    async fn planets_in_systems(&self, _: &[i64]) -> Result<Vec<PlannerPlanet>, SdeError> {
        let planet = |planet_id, index: i64, type_id| PlannerPlanet {
            planet_id,
            name: format!("Q-3HS5 {}", ["I", "II", "III"][index as usize - 1]),
            solar_system_id: HOME,
            celestial_index: index,
            type_id,
            radius_m: Some(5_000_000),
        };
        Ok(vec![
            planet(LAVA_I, 1, LAVA),
            planet(BARREN_II, 2, BARREN),
            planet(PLASMA_III, 3, PLASMA),
        ])
    }
}

fn name(type_id: i64) -> String {
    match type_id {
        BASE_METALS => "Base Metals",
        NOBLE_METALS => "Noble Metals",
        HEAVY_METALS => "Heavy Metals",
        NON_CS_CRYSTALS => "Non-CS Crystals",
        ROBOTICS => "Robotics",
        BARREN => "Planet (Barren)",
        _ => return format!("Type {type_id}"),
    }
    .to_string()
}

/// Prices every product at best buy 1000 / best sell 1100, Robotics at
/// 75 000.
#[derive(Default)]
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
        let order = |type_id: i64, price: i64, side| MarketOrderView {
            observation_id: None,
            import_batch_id: None,
            imported_file_id: None,
            order_id: type_id * 10 + price % 7,
            type_id,
            type_name: String::new(),
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
        Ok(type_ids
            .iter()
            .map(|type_id| {
                let buy = if *type_id == ROBOTICS { 75_000 } else { 1_000 };
                (
                    *type_id,
                    vec![
                        order(*type_id, buy, MarketOrderSide::Buy),
                        order(*type_id, buy + 100, MarketOrderSide::Sell),
                    ],
                )
            })
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
    async fn get_market_price_source(
        &self,
        _: WorkspaceId,
        _: PriceSourceId,
    ) -> Result<MarketPriceSource, MarketError> {
        unimplemented!()
    }
}

#[derive(Default)]
struct FakeInputs {
    densities: Mutex<BTreeMap<(i64, i64), StoredDensity>>,
    settings: Mutex<BTreeMap<i64, StoredPlanetSettings>>,
}

#[async_trait]
impl PiPlannerInputsRepository for FakeInputs {
    async fn densities(
        &self,
        _: WorkspaceId,
        _: &[i64],
    ) -> Result<Vec<StoredDensity>, InventoryError> {
        Ok(self.densities.lock().unwrap().values().cloned().collect())
    }
    async fn save_densities(
        &self,
        _: WorkspaceId,
        writes: &[DensityWrite],
    ) -> Result<(), InventoryError> {
        let mut densities = self.densities.lock().unwrap();
        for write in writes {
            let key = (write.planet_id, write.resource_type_id);
            if write.clears() {
                densities.remove(&key);
            } else {
                densities.insert(
                    key,
                    StoredDensity {
                        planet_id: write.planet_id,
                        resource_type_id: write.resource_type_id,
                        density_pct: write.density_pct,
                        head_value: write.head_value,
                        updated_at: Utc::now(),
                    },
                );
            }
        }
        Ok(())
    }
    async fn planet_settings(
        &self,
        _: WorkspaceId,
        _: &[i64],
    ) -> Result<Vec<StoredPlanetSettings>, InventoryError> {
        Ok(self.settings.lock().unwrap().values().cloned().collect())
    }
    async fn save_planet_settings(
        &self,
        _: WorkspaceId,
        writes: &[PlanetSettingsWrite],
    ) -> Result<(), InventoryError> {
        let mut settings = self.settings.lock().unwrap();
        for write in writes {
            if write.clears() {
                settings.remove(&write.planet_id);
            } else {
                settings.insert(
                    write.planet_id,
                    StoredPlanetSettings {
                        planet_id: write.planet_id,
                        customs_rate: write.customs_rate,
                        contested: write.contested,
                        updated_at: Utc::now(),
                    },
                );
            }
        }
        Ok(())
    }
}

struct FakeCharacters;

#[async_trait]
impl PlanetaryCharacterSource for FakeCharacters {
    async fn planetary_characters(
        &self,
        _: WorkspaceId,
    ) -> Result<Vec<PlanetaryCharacter>, InventoryError> {
        Ok(vec![PlanetaryCharacter {
            connection_id: ConnectedCharacterId(Uuid::from_u128(1)),
            eve_character_id: 2_119_000_001,
            name: "Valka".into(),
            scope_granted: true,
            planets_source: None,
            skills_summary: Some(json!({ "skills": [
                { "skill_id": 2495, "trained_skill_level": 4 },
                { "skill_id": 2505, "trained_skill_level": 3 },
            ]})),
        }])
    }
}

#[derive(Default)]
struct FakePreferences;

#[async_trait]
impl PlanetaryPreferencesRepository for FakePreferences {
    async fn planetary_preferences(
        &self,
        _: WorkspaceId,
    ) -> Result<PlanetaryPreferences, InventoryError> {
        Ok(PlanetaryPreferences::default())
    }
    async fn save_planetary_preferences(
        &self,
        _: WorkspaceId,
        _: &PlanetaryPreferences,
    ) -> Result<(), InventoryError> {
        Ok(())
    }
}

struct Harness {
    app: axum::Router,
    market: Arc<FakeMarket>,
}

fn harness(planner_data: bool) -> Harness {
    let market = Arc::new(FakeMarket::default());
    let app = build_router(
        AppState::new(Arc::new(configured_workspace("PI Planner Test")))
            .with_sde_repository(Arc::new(FakeSde { planner_data }))
            .with_market_repository(market.clone())
            .with_pi_planner_repository(Arc::new(FakeInputs::default()))
            .with_planetary_repositories(Arc::new(FakeCharacters), Arc::new(FakePreferences)),
    );
    Harness { app, market }
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

fn get(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}

fn with_json(method: &str, uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn scenario() -> Value {
    json!({
        "homeSystemId": HOME,
        "radius": 0,
        "characters": [{ "interplanetaryConsolidation": 4, "commandCenterUpgrades": 4 }],
        "defaultCustomsRate": "0.1",
        "resetDays": 3,
        "haulDays": 3,
        "upliftAtLogin": true,
        "buyIntermediates": false
    })
}

const PLANETS: &str = "/api/planetary/planner/planets?systemId=30000797&radius=0";

#[tokio::test]
async fn entered_densities_and_settings_show_on_the_planets() {
    let harness = harness(true);
    let (status, _) = send(
        &harness.app,
        with_json(
            "PUT",
            "/api/planetary/planner/densities",
            &json!([
                { "planetId": BARREN_II, "resourceTypeId": BASE_METALS, "densityPct": "58" },
                { "planetId": BARREN_II, "resourceTypeId": NOBLE_METALS, "headValue": "160.5" },
            ]),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = send(
        &harness.app,
        with_json(
            "PUT",
            "/api/planetary/planner/planet-settings",
            &json!([{ "planetId": BARREN_II, "customsRate": "0.05", "contested": true }]),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = send(&harness.app, get(PLANETS)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["systems"][0]["name"], "Q-3HS5");
    assert_eq!(body["systems"][0]["securityStatus"], "-0.14");
    assert_eq!(body["systems"][0]["band"], "null");
    let barren = &body["planets"][1];
    assert_eq!(barren["name"], "Q-3HS5 II");
    assert_eq!(barren["typeName"], "Planet (Barren)");
    assert_eq!(barren["customsRate"], "0.05");
    assert_eq!(barren["contested"], true);
    let resource = |name: &str| {
        barren["resources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|resource| resource["name"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(resource("Base Metals")["densityPct"], "58");
    assert_eq!(resource("Base Metals")["defaultDensityPct"], "25");
    assert_eq!(resource("Noble Metals")["headValue"], "160.5");
    assert!(resource("Noble Metals")["updatedAt"].is_string());
}

#[tokio::test]
async fn out_of_range_inputs_are_rejected() {
    let harness = harness(true);
    for (uri, body) in [
        (
            "/api/planetary/planner/densities",
            json!([{ "planetId": LAVA_I, "resourceTypeId": BASE_METALS, "densityPct": "101" }]),
        ),
        (
            "/api/planetary/planner/densities",
            json!([{ "planetId": LAVA_I, "resourceTypeId": BASE_METALS, "headValue": "0" }]),
        ),
        (
            "/api/planetary/planner/planet-settings",
            json!([{ "planetId": LAVA_I, "customsRate": "1.2" }]),
        ),
    ] {
        let (status, response) = send(&harness.app, with_json("PUT", uri, &body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}: {response}");
        assert_eq!(response["error"]["code"], "validation_failed");
    }

    let (status, _) = send(
        &harness.app,
        get("/api/planetary/planner/planets?systemId=30000797&radius=6"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let mut bad = scenario();
    bad["resetDays"] = json!(5);
    let (status, _) = send(
        &harness.app,
        with_json("POST", "/api/planetary/planner/rank", &bad),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rank_returns_priced_products_as_decimal_strings() {
    let harness = harness(true);
    send(
        &harness.app,
        with_json(
            "PUT",
            "/api/planetary/planner/densities",
            &json!([
                { "planetId": LAVA_I, "resourceTypeId": HEAVY_METALS, "densityPct": "50" },
                { "planetId": LAVA_I, "resourceTypeId": NON_CS_CRYSTALS, "densityPct": "44" },
                { "planetId": BARREN_II, "resourceTypeId": BASE_METALS, "densityPct": "58" },
                { "planetId": BARREN_II, "resourceTypeId": NOBLE_METALS, "headValue": "160" },
            ]),
        ),
    )
    .await;

    let (status, body) = send(
        &harness.app,
        with_json("POST", "/api/planetary/planner/rank", &scenario()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["availableColonies"], 5);
    let robotics = body["ranked"]
        .as_array()
        .unwrap()
        .iter()
        .find(|product| product["typeId"] == ROBOTICS)
        .expect("Robotics is ranked");
    assert_eq!(robotics["name"], "Robotics");
    assert_eq!(robotics["tier"], 3);
    assert!(robotics["iskPerMonth"].is_string());
    assert!(robotics["confidence"].is_string());
    assert!(robotics["effort"]["haulRunsPerWeek"].is_string());
    let noble = robotics["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|input| input["resourceTypeId"] == NOBLE_METALS)
        .unwrap();
    assert_eq!(noble["planetName"], "Q-3HS5 II");
    assert_eq!(noble["source"], "headValue");
    // Every product was priced.
    assert!(harness.market.requested.lock().unwrap().contains(&ROBOTICS));
}

#[tokio::test]
async fn unscanned_planets_rank_only_when_estimated() {
    let harness = harness(true);
    let (status, body) = send(
        &harness.app,
        with_json("POST", "/api/planetary/planner/rank", &scenario()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["ranked"], json!([]));
    let robotics = body["outOfReach"]
        .as_array()
        .unwrap()
        .iter()
        .find(|product| product["typeId"] == ROBOTICS)
        .unwrap();
    assert_eq!(robotics["reason"]["kind"], "notScanned");
    assert!(robotics["reason"]["name"].is_string());

    let mut estimating = scenario();
    estimating["estimateUnscanned"] = json!(true);
    let (status, body) = send(
        &harness.app,
        with_json("POST", "/api/planetary/planner/rank", &estimating),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!body["ranked"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn characters_prefill_pi_skills() {
    let harness = harness(true);
    let (status, body) = send(&harness.app, get("/api/planetary/planner/characters")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!([{
            "eveCharacterId": 2_119_000_001_i64,
            "name": "Valka",
            "interplanetaryConsolidation": 4,
            "commandCenterUpgrades": 3,
        }])
    );
}

#[tokio::test]
async fn solve_returns_a_plan_with_what_ifs() {
    let harness = harness(true);
    send(
        &harness.app,
        with_json(
            "PUT",
            "/api/planetary/planner/densities",
            &json!([
                { "planetId": LAVA_I, "resourceTypeId": HEAVY_METALS, "densityPct": "50" },
                { "planetId": LAVA_I, "resourceTypeId": NON_CS_CRYSTALS, "densityPct": "44" },
                { "planetId": BARREN_II, "resourceTypeId": BASE_METALS, "densityPct": "58" },
                { "planetId": BARREN_II, "resourceTypeId": NOBLE_METALS, "headValue": "160" },
            ]),
        ),
    )
    .await;
    let mut two = scenario();
    two["characters"] = json!([
        { "interplanetaryConsolidation": 2, "commandCenterUpgrades": 4 },
        { "interplanetaryConsolidation": 2, "commandCenterUpgrades": 4 },
    ]);

    let (status, body) = send(
        &harness.app,
        with_json("POST", "/api/planetary/planner/solve", &two),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let plan = &body["plan"];
    assert!(plan["iskPerMonth"].is_string());
    let colonies = plan["colonies"].as_array().unwrap();
    assert!(!colonies.is_empty() && colonies.len() <= 6, "{plan}");
    let first = &colonies[0];
    assert_eq!(first["index"], 0);
    assert!(first["planetName"].as_str().unwrap().starts_with("Q-3HS5"));
    assert!(["extraction", "factory"].contains(&first["role"].as_str().unwrap()));
    assert!(first["cpuUsed"].is_string());
    // Flows end at the market or a colony index.
    assert!(plan["flows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|flow| flow["to"].is_null()));
    let what_ifs = &body["whatIfs"];
    assert_eq!(what_ifs["grid"].as_array().unwrap().len(), 12);
    assert_eq!(what_ifs["perCharacter"].as_array().unwrap().len(), 2);
    assert_eq!(what_ifs["perRadius"][0]["jumps"], 0);
}

#[tokio::test]
async fn solve_for_a_target_sells_only_that_product() {
    let harness = harness(true);
    let mut targeted = scenario();
    targeted["estimateUnscanned"] = json!(true);
    targeted["targetTypeId"] = json!(TOXIC_METALS);
    let (status, body) = send(
        &harness.app,
        with_json("POST", "/api/planetary/planner/solve", &targeted),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let market = body["plan"]["flows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|flow| flow["to"].is_null())
        .collect::<Vec<_>>();
    assert!(!market.is_empty(), "{body}");
    assert!(market.iter().all(|flow| flow["typeId"] == TOXIC_METALS));
}

#[tokio::test]
async fn solve_validates_the_scenario() {
    let harness = harness(true);
    let mut bad = scenario();
    bad["haulDays"] = json!(5);
    let (status, body) = send(
        &harness.app,
        with_json("POST", "/api/planetary/planner/solve", &bad),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn an_sde_without_planner_data_is_a_409() {
    let harness = harness(false);
    for request in [
        get(PLANETS),
        with_json("POST", "/api/planetary/planner/rank", &scenario()),
        with_json("POST", "/api/planetary/planner/solve", &scenario()),
    ] {
        let (status, body) = send(&harness.app, request).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["error"]["code"], "sde_missing_planner_data");
    }
}
