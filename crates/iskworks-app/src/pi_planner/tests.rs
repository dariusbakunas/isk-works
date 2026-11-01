use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;
use iskworks_core::pi_planner::fixtures::{self, *};
use iskworks_core::pi_planner::{StoredDensity, StoredPlanetSettings};
use iskworks_core::{
    ConnectedCharacterId, MarketImportBatch, MarketImportBatchId, MarketOrderBook, MarketOrderSide,
    MarketOrderView, MarketPriceSource, Money, PriceSourceId, ResolvedMarketExport,
    ResolvedMarketLocation,
};
use iskworks_sde::{
    ActiveSde, PlanetCommodity, PlanetPinType, PlanetSchematic, PlannerPlanet, TypeReference,
};
use serde_json::json;

use super::*;
use crate::PlanetaryCharacter;

const HOME: i64 = 30_000_001;
const NEXT_DOOR: i64 = 30_000_002;
const LAVA_I: i64 = 40_000_001;
const BARREN_II: i64 = 40_000_002;
const PLASMA_III: i64 = 40_000_003;
const BARREN_NEXT_DOOR: i64 = 40_000_011;

fn d(value: &str) -> Decimal {
    Decimal::from_str(value).unwrap()
}

struct FakeSde {
    pins: Vec<PlanetPinType>,
}

impl FakeSde {
    fn planner() -> Self {
        Self {
            pins: fixtures::pin_types(),
        }
    }

    /// An import that predates the PI Planner data.
    fn old() -> Self {
        Self { pins: Vec::new() }
    }
}

fn system(solar_system_id: i64, name: &str, jumps: u8) -> SystemDistance {
    SystemDistance {
        solar_system_id,
        name: name.to_string(),
        security_status: Some(d("-0.4")),
        region_name: Some("Insmother".to_string()),
        wormhole: false,
        jumps,
    }
}

fn planet(
    planet_id: i64,
    name: &str,
    solar_system_id: i64,
    index: i64,
    type_id: i64,
) -> PlannerPlanet {
    PlannerPlanet {
        planet_id,
        name: name.to_string(),
        solar_system_id,
        celestial_index: index,
        type_id,
        radius_m: Some(5_000_000),
    }
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
        Ok(self.pins.clone())
    }
    async fn planet_schematic_catalogue(&self) -> Result<Vec<PlanetSchematic>, SdeError> {
        Ok(fixtures::schematics())
    }
    async fn planet_commodities(&self) -> Result<BTreeMap<i64, PlanetCommodity>, SdeError> {
        Ok(fixtures::commodities())
    }
    async fn type_reference(
        &self,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, TypeReference>, SdeError> {
        let volumes = fixtures::volumes();
        Ok(type_ids
            .iter()
            .map(|type_id| {
                (
                    *type_id,
                    TypeReference {
                        type_name: Some(type_name(*type_id)),
                        group_id: None,
                        group_name: None,
                        category_id: None,
                        category_name: None,
                        packaged_volume_m3: volumes
                            .get(type_id)
                            .and_then(|volume| Decimal::from_f64(*volume)),
                    },
                )
            })
            .collect())
    }
    /// Like the real import: planet types have no `sde_types` row.
    async fn type_names(&self, type_ids: &[i64]) -> Result<BTreeMap<i64, String>, SdeError> {
        Ok(type_ids
            .iter()
            .filter(|type_id| ![BARREN, LAVA, PLASMA].contains(type_id))
            .map(|type_id| (*type_id, type_name(*type_id)))
            .collect())
    }
    async fn systems_within_jumps(
        &self,
        solar_system_id: i64,
        max_jumps: u8,
    ) -> Result<Vec<SystemDistance>, SdeError> {
        if solar_system_id != HOME {
            return Ok(Vec::new());
        }
        let mut systems = vec![system(HOME, "Q-3HS5", 0)];
        if max_jumps > 0 {
            systems.push(system(NEXT_DOOR, "N-NEXT", 1));
        }
        Ok(systems)
    }
    async fn planets_in_systems(
        &self,
        solar_system_ids: &[i64],
    ) -> Result<Vec<PlannerPlanet>, SdeError> {
        Ok([
            planet(BARREN_NEXT_DOOR, "N-NEXT I", NEXT_DOOR, 1, BARREN),
            planet(PLASMA_III, "Q-3HS5 III", HOME, 3, PLASMA),
            planet(LAVA_I, "Q-3HS5 I", HOME, 1, LAVA),
            planet(BARREN_II, "Q-3HS5 II", HOME, 2, BARREN),
        ]
        .into_iter()
        .filter(|planet| solar_system_ids.contains(&planet.solar_system_id))
        .collect())
    }
}

fn type_name(type_id: i64) -> String {
    match type_id {
        BASE_METALS => "Base Metals",
        AQUEOUS_LIQUIDS => "Aqueous Liquids",
        NOBLE_METALS => "Noble Metals",
        HEAVY_METALS => "Heavy Metals",
        NON_CS_CRYSTALS => "Non-CS Crystals",
        2073 => "Microorganisms",
        2288 => "Carbon Compounds",
        REACTIVE_METALS => "Reactive Metals",
        PRECIOUS_METALS => "Precious Metals",
        TOXIC_METALS => "Toxic Metals",
        CHIRAL_STRUCTURES => "Chiral Structures",
        WATER => "Water",
        MECHANICAL_PARTS => "Mechanical Parts",
        CONSUMER_ELECTRONICS => "Consumer Electronics",
        ROBOTICS => "Robotics",
        _ => return format!("Type {type_id}"),
    }
    .to_string()
}

/// Best buy and best sell by type id.
struct FakeMarket(BTreeMap<i64, (&'static str, &'static str)>);

fn order(type_id: i64, side: MarketOrderSide, price: &str) -> MarketOrderView {
    MarketOrderView {
        observation_id: None,
        import_batch_id: None,
        imported_file_id: None,
        order_id: type_id * 10 + i64::from(side == MarketOrderSide::Buy),
        type_id,
        type_name: String::new(),
        side,
        price: Money(d(price)),
        remaining_volume: 1_000,
        entered_volume: 1_000,
        minimum_volume: 1,
        order_range: 32_767,
        issued_at: Utc::now() - chrono::Duration::minutes(30),
        duration_days: 90,
        observed_at: Utc::now() - chrono::Duration::minutes(3),
        revalidated_at: None,
        location_id: 60_003_760,
        solar_system_id: HOME,
        region_id: 10_000_001,
        jumps: 0,
    }
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
    async fn save_location_names(
        &self,
        _: WorkspaceId,
        _: Vec<ResolvedMarketLocation>,
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
        _: Vec<ResolvedMarketExport>,
        _: u64,
        _: Vec<String>,
    ) -> Result<MarketImportBatch, MarketError> {
        unimplemented!()
    }
    async fn list_imports(&self, _: WorkspaceId) -> Result<Vec<MarketImportBatch>, MarketError> {
        unimplemented!()
    }
    async fn get_import(
        &self,
        _: WorkspaceId,
        _: MarketImportBatchId,
    ) -> Result<MarketImportBatch, MarketError> {
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
    async fn scoped_order_books(
        &self,
        _: WorkspaceId,
        _: MarketScope,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<MarketOrderView>>, MarketError> {
        Ok(type_ids
            .iter()
            .filter_map(|type_id| {
                self.0.get(type_id).map(|(buy, sell)| {
                    (
                        *type_id,
                        vec![
                            order(*type_id, MarketOrderSide::Buy, buy),
                            order(*type_id, MarketOrderSide::Sell, sell),
                        ],
                    )
                })
            })
            .collect())
    }
}

/// Every product priced except Consumer Electronics.
fn market() -> FakeMarket {
    FakeMarket(BTreeMap::from([
        (WATER, ("300", "320")),
        (REACTIVE_METALS, ("350", "370")),
        (PRECIOUS_METALS, ("400", "420")),
        (TOXIC_METALS, ("380", "400")),
        (CHIRAL_STRUCTURES, ("390", "410")),
        (MECHANICAL_PARTS, ("9000", "9500")),
        (ROBOTICS, ("75000", "80000")),
    ]))
}

#[derive(Default)]
struct FakeInputs {
    densities: Mutex<BTreeMap<(uuid::Uuid, i64, i64), StoredDensity>>,
    settings: Mutex<BTreeMap<(uuid::Uuid, i64), StoredPlanetSettings>>,
}

#[async_trait]
impl PiPlannerInputsRepository for FakeInputs {
    async fn densities(
        &self,
        workspace_id: WorkspaceId,
        planet_ids: &[i64],
    ) -> Result<Vec<StoredDensity>, InventoryError> {
        Ok(self
            .densities
            .lock()
            .unwrap()
            .iter()
            .filter(|((ws, planet, _), _)| *ws == workspace_id.0 && planet_ids.contains(planet))
            .map(|(_, density)| density.clone())
            .collect())
    }
    async fn save_densities(
        &self,
        workspace_id: WorkspaceId,
        writes: &[DensityWrite],
    ) -> Result<(), InventoryError> {
        let mut densities = self.densities.lock().unwrap();
        for write in writes {
            let key = (workspace_id.0, write.planet_id, write.resource_type_id);
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
        workspace_id: WorkspaceId,
        planet_ids: &[i64],
    ) -> Result<Vec<StoredPlanetSettings>, InventoryError> {
        Ok(self
            .settings
            .lock()
            .unwrap()
            .iter()
            .filter(|((ws, planet), _)| *ws == workspace_id.0 && planet_ids.contains(planet))
            .map(|(_, settings)| settings.clone())
            .collect())
    }
    async fn save_planet_settings(
        &self,
        workspace_id: WorkspaceId,
        writes: &[PlanetSettingsWrite],
    ) -> Result<(), InventoryError> {
        let mut settings = self.settings.lock().unwrap();
        for write in writes {
            let key = (workspace_id.0, write.planet_id);
            if write.clears() {
                settings.remove(&key);
            } else {
                settings.insert(
                    key,
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

fn workspace() -> WorkspaceId {
    WorkspaceId(uuid::Uuid::from_u128(1))
}

fn service(sde: FakeSde) -> (PiPlannerService, Arc<FakeInputs>) {
    let inputs = Arc::new(FakeInputs::default());
    (
        PiPlannerService::new(Arc::new(sde), Arc::new(market()), inputs.clone()),
        inputs,
    )
}

fn density(
    planet_id: i64,
    resource_type_id: i64,
    pct: Option<&str>,
    head: Option<&str>,
) -> DensityWriteDto {
    DensityWriteDto {
        planet_id,
        resource_type_id,
        density_pct: pct.map(d),
        head_value: head.map(d),
    }
}

/// The Q-3HS5 readings the rank tests use.
async fn enter_q3hs5(service: &PiPlannerService) {
    service
        .save_densities(
            workspace(),
            vec![
                density(LAVA_I, BASE_METALS, Some("40"), None),
                density(LAVA_I, HEAVY_METALS, Some("50"), None),
                density(LAVA_I, NON_CS_CRYSTALS, Some("44"), None),
                density(BARREN_II, BASE_METALS, Some("58"), None),
                density(BARREN_II, NOBLE_METALS, None, Some("160")),
                density(BARREN_II, AQUEOUS_LIQUIDS, Some("50"), None),
                density(PLASMA_III, HEAVY_METALS, Some("61"), None),
                density(PLASMA_III, NON_CS_CRYSTALS, Some("57"), None),
            ],
        )
        .await
        .unwrap();
}

fn scenario_request(radius: u8) -> PlannerScenarioDto {
    PlannerScenarioDto {
        home_system_id: HOME,
        radius,
        characters: vec![PlannerCharacterDto {
            interplanetary_consolidation: 4,
            command_center_upgrades: 4,
        }],
        default_customs_rate: d("0.1"),
        reset_days: 3,
        haul_days: 3,
        uplift_at_login: true,
        buy_intermediates: false,
        max_colonies: None,
        estimate_unscanned: true,
        target_type_id: None,
    }
}

fn scope() -> MarketScope {
    iskworks_core::DEFAULT_MARKET_SCOPE
}

#[tokio::test]
async fn planets_lists_systems_in_range_and_their_resources_with_entries() {
    let (service, _) = service(FakeSde::planner());
    enter_q3hs5(&service).await;
    service
        .save_planet_settings(
            workspace(),
            vec![PlanetSettingsWriteDto {
                planet_id: BARREN_II,
                customs_rate: Some(d("0.05")),
                contested: true,
            }],
        )
        .await
        .unwrap();

    let home = service.planets(workspace(), HOME, 0).await.unwrap();
    assert_eq!(home.systems.len(), 1);
    let names: Vec<&str> = home
        .planets
        .iter()
        .map(|planet| planet.name.as_str())
        .collect();
    assert_eq!(names, ["Q-3HS5 I", "Q-3HS5 II", "Q-3HS5 III"]);

    let barren = &home.planets[1];
    assert_eq!(barren.type_name, "Planet (Barren)");
    assert_eq!(barren.customs_rate.as_deref(), Some("0.05"));
    assert!(barren.contested);
    let resources: Vec<(&str, Option<&str>, Option<&str>)> = barren
        .resources
        .iter()
        .map(|resource| {
            (
                resource.name.as_str(),
                resource.density_pct.as_deref(),
                resource.head_value.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        resources,
        [
            ("Aqueous Liquids", Some("50"), None),
            ("Base Metals", Some("58"), None),
            ("Carbon Compounds", None, None),
            ("Microorganisms", None, None),
            ("Noble Metals", None, Some("160")),
        ]
    );
    assert_eq!(barren.resources[0].default_density_pct, "25");
    assert!(barren.resources[0].updated_at.is_some());

    let wider = service.planets(workspace(), HOME, 1).await.unwrap();
    assert_eq!(wider.systems.len(), 2);
    assert_eq!(wider.planets.last().unwrap().name, "N-NEXT I");
    assert_eq!(wider.planets.last().unwrap().jumps, 1);
}

#[tokio::test]
async fn planets_rejects_an_unknown_system_and_a_wide_radius() {
    let (service, _) = service(FakeSde::planner());
    assert!(matches!(
        service.planets(workspace(), 1, 0).await,
        Err(PiPlannerError::Inventory(InventoryError::Validation(_)))
    ));
    assert!(matches!(
        service.planets(workspace(), HOME, 6).await,
        Err(PiPlannerError::Inventory(InventoryError::Validation(_)))
    ));
}

#[tokio::test]
async fn an_old_sde_is_missing_planner_data() {
    let (service, _) = service(FakeSde::old());
    assert!(matches!(
        service.planets(workspace(), HOME, 0).await,
        Err(PiPlannerError::SdeMissingPlannerData)
    ));
    assert!(matches!(
        service
            .rank(workspace(), scope(), scenario_request(0))
            .await,
        Err(PiPlannerError::SdeMissingPlannerData)
    ));
}

#[tokio::test]
async fn saving_validates_and_clears() {
    let (service, inputs) = service(FakeSde::planner());
    assert!(service
        .save_densities(
            workspace(),
            vec![density(LAVA_I, BASE_METALS, Some("101"), None)]
        )
        .await
        .is_err());
    assert!(service
        .save_planet_settings(
            workspace(),
            vec![PlanetSettingsWriteDto {
                planet_id: LAVA_I,
                customs_rate: Some(d("2")),
                contested: false,
            }],
        )
        .await
        .is_err());

    enter_q3hs5(&service).await;
    service
        .save_densities(workspace(), vec![density(LAVA_I, BASE_METALS, None, None)])
        .await
        .unwrap();
    assert_eq!(inputs.densities.lock().unwrap().len(), 7);
}

#[tokio::test]
async fn rank_prices_products_and_names_their_inputs() {
    let (service, _) = service(FakeSde::planner());
    enter_q3hs5(&service).await;
    service
        .save_planet_settings(
            workspace(),
            vec![PlanetSettingsWriteDto {
                planet_id: PLASMA_III,
                customs_rate: None,
                contested: true,
            }],
        )
        .await
        .unwrap();

    let ranking = service
        .rank(workspace(), scope(), scenario_request(0))
        .await
        .unwrap();
    assert_eq!(ranking.available_colonies, 5);
    assert!(ranking.price_observed_at.is_some());

    let robotics = ranking
        .ranked
        .iter()
        .find(|product| product.type_id == ROBOTICS)
        .expect("Robotics is in reach");
    assert_eq!(robotics.name, "Robotics");
    assert_eq!(robotics.tier, 3);
    assert!(d(&robotics.isk_per_month) > Decimal::ZERO);
    let noble = robotics
        .inputs
        .iter()
        .find(|input| input.resource_type_id == NOBLE_METALS)
        .unwrap();
    assert_eq!(noble.planet_name, "Q-3HS5 II");
    assert!(matches!(noble.source, YieldSourceDto::HeadValue));
    let heavy = robotics
        .inputs
        .iter()
        .find(|input| input.resource_type_id == HEAVY_METALS)
        .unwrap();
    assert_eq!(heavy.planet_name, "Q-3HS5 III");
    assert!(heavy.contested);

    let electronics = ranking
        .out_of_reach
        .iter()
        .find(|product| product.type_id == CONSUMER_ELECTRONICS)
        .unwrap();
    assert!(matches!(electronics.reason, OutOfReachReasonDto::NoPrice));

    let json = serde_json::to_value(&ranking).unwrap();
    assert_eq!(json["outOfReach"][0]["reason"]["kind"], "noPrice");
}

#[tokio::test]
async fn rank_validates_the_scenario() {
    let (service, _) = service(FakeSde::planner());
    let invalid = |change: fn(&mut PlannerScenarioDto)| {
        let mut request = scenario_request(0);
        change(&mut request);
        request
    };
    for request in [
        invalid(|request| request.reset_days = 5),
        invalid(|request| request.haul_days = 0),
        invalid(|request| request.characters.clear()),
        invalid(|request| request.characters[0].command_center_upgrades = 6),
        invalid(|request| request.default_customs_rate = d("1.5")),
        invalid(|request| request.max_colonies = Some(0)),
        invalid(|request| request.radius = 6),
    ] {
        assert!(
            matches!(
                service.rank(workspace(), scope(), request).await,
                Err(PiPlannerError::Inventory(InventoryError::Validation(_)))
            ),
            "should be invalid"
        );
    }
}

#[tokio::test]
async fn characters_prefill_pi_skills_from_the_skills_sync() {
    let (service, _) = service(FakeSde::planner());
    assert!(service.characters(workspace()).await.unwrap().is_empty());

    let character = |id: i64, summary: Option<serde_json::Value>| PlanetaryCharacter {
        connection_id: ConnectedCharacterId(uuid::Uuid::from_u128(id as u128)),
        eve_character_id: id,
        name: format!("Pilot {id}"),
        scope_granted: true,
        planets_source: None,
        skills_summary: summary,
    };
    let service = service.with_characters(Arc::new(FakeCharacters(vec![
        character(
            1,
            Some(json!({ "skills": [
                { "skill_id": 2495, "trained_skill_level": 4 },
                { "skill_id": 2505, "trained_skill_level": 5 },
            ]})),
        ),
        character(2, None),
    ])));
    let characters = service.characters(workspace()).await.unwrap();
    assert_eq!(characters[0].interplanetary_consolidation, Some(4));
    assert_eq!(characters[0].command_center_upgrades, Some(5));
    assert_eq!(characters[1].interplanetary_consolidation, None);
}
