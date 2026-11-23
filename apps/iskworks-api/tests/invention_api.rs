//! `/api/invention/*`: product search and the decryptor comparison quote.
//!
//! The quote fixture is a job read from the in-game industry window on
//! 2026-11-23: Tristan Blueprint to Nemesis Blueprint, EIV 12,687,150,
//! invention index 7.60%, facility tax 1%, SCC surcharge 4%.

use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::{DateTime, Utc};
use iskworks_api::{build_router, AppState};
use iskworks_app::{FakeSkillsCharacters, SkillsCharacter};
use iskworks_core::{
    AdjustedPriceRepository, ConnectedCharacterId, FacilityError, FacilityKind, FacilityProfileId,
    FacilityRole, IndustryActivity, IndustryError, IndustryFacilityProfile, IndustryRepository,
    MarketError, MarketOrderSide, MarketOrderView, Money, NewWorkspace, PriceSource, PriceSourceId,
    SecurityClass, StoreError, WorkspaceId, WorkspaceState,
};
use iskworks_sde::{
    ActiveSde, Decryptor, ImportCounts, InventableProduct, InventionProduct, InventionRecipe,
    InventionSkillRequirement, ManufacturingRecipe, RecipeLine, SdeError, SdeTypeClassification,
};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

mod support;
use support::workspace::ConfiguredWorkspaceRepository;

const NEMESIS: i64 = 11_377;
const NEMESIS_BLUEPRINT: i64 = 11_378;
const TRISTAN: i64 = 593;
const TRISTAN_BLUEPRINT: i64 = 940;
const HULL: i64 = 100;
const DATACORE_STARSHIP: i64 = 20_410;
const DATACORE_MECHANICAL: i64 = 20_424;
const ENCRYPTION: i64 = 23_121;
const STARSHIP: i64 = 11_450;
const MECHANICAL: i64 = 11_452;
const PROCESS: i64 = 34_206;
const AUGMENTATION: i64 = 34_203;
const SYSTEM: i64 = 30_000_142;

struct FakeSde {
    invention_data: bool,
}

fn line(type_id: i64, name: &str, quantity: i64) -> RecipeLine {
    RecipeLine {
        type_id,
        type_name: name.to_string(),
        quantity,
    }
}

#[async_trait]
impl iskworks_sde::FakeSdeRead for FakeSde {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        Ok(Some(ActiveSde {
            import_id: Uuid::nil(),
            source_version: "test".to_string(),
            source_label: "fixture.zip".to_string(),
            source_checksum: "checksum".to_string(),
            completed_at: Utc::now(),
            counts: ImportCounts::default(),
        }))
    }
    async fn has_invention_data(&self) -> Result<bool, SdeError> {
        Ok(self.invention_data)
    }
    async fn search_inventable_products(
        &self,
        query: &str,
        _: u32,
    ) -> Result<Vec<InventableProduct>, SdeError> {
        Ok(
            if "nemesis".contains(&query.to_lowercase()) && !query.is_empty() {
                vec![InventableProduct {
                    product_type_id: NEMESIS,
                    product_name: "Nemesis".to_string(),
                    group_name: Some("Stealth Bomber".to_string()),
                    blueprint_type_id: NEMESIS_BLUEPRINT,
                    source_blueprint_type_id: TRISTAN_BLUEPRINT,
                    source_blueprint_name: "Tristan Blueprint".to_string(),
                    source_output_count: 1,
                }]
            } else {
                Vec::new()
            },
        )
    }
    async fn manufacturing_blueprint_for_product(
        &self,
        product_type_id: i64,
    ) -> Result<Option<i64>, SdeError> {
        Ok(match product_type_id {
            NEMESIS => Some(NEMESIS_BLUEPRINT),
            TRISTAN => Some(TRISTAN_BLUEPRINT),
            _ => None,
        })
    }
    async fn manufacturing_recipe(
        &self,
        blueprint_type_id: i64,
    ) -> Result<Option<ManufacturingRecipe>, SdeError> {
        let recipe = |name: &str, product: RecipeLine| ManufacturingRecipe {
            blueprint_type_id,
            blueprint_name: name.to_string(),
            duration_seconds: Some(600),
            materials: vec![line(HULL, "Hull Section", 1)],
            products: vec![product],
        };
        Ok(match blueprint_type_id {
            NEMESIS_BLUEPRINT => Some(recipe("Nemesis Blueprint", line(NEMESIS, "Nemesis", 1))),
            TRISTAN_BLUEPRINT => Some(recipe("Tristan Blueprint", line(TRISTAN, "Tristan", 1))),
            _ => None,
        })
    }
    async fn invention_recipe(
        &self,
        product_blueprint_type_id: i64,
    ) -> Result<Option<InventionRecipe>, SdeError> {
        if product_blueprint_type_id != NEMESIS_BLUEPRINT {
            return Ok(None);
        }
        let skill = |skill_type_id, name: &str| InventionSkillRequirement {
            skill_type_id,
            skill_name: name.to_string(),
            level: 1,
        };
        Ok(Some(InventionRecipe {
            source_blueprint_type_id: TRISTAN_BLUEPRINT,
            source_blueprint_name: "Tristan Blueprint".to_string(),
            duration_seconds: Some(63_900),
            copying_duration_seconds: Some(4_800),
            materials: vec![
                line(
                    DATACORE_STARSHIP,
                    "Datacore - Gallentean Starship Engineering",
                    2,
                ),
                line(DATACORE_MECHANICAL, "Datacore - Mechanical Engineering", 2),
            ],
            products: vec![InventionProduct {
                blueprint_type_id: NEMESIS_BLUEPRINT,
                blueprint_name: "Nemesis Blueprint".to_string(),
                product_type_id: Some(NEMESIS),
                product_name: Some("Nemesis".to_string()),
                base_runs: 1,
                probability: Decimal::new(30, 2),
            }],
            skills: vec![
                skill(ENCRYPTION, "Gallente Encryption Methods"),
                skill(STARSHIP, "Gallentean Starship Engineering"),
                skill(MECHANICAL, "Mechanical Engineering"),
            ],
        }))
    }
    async fn decryptors(&self) -> Result<Vec<Decryptor>, SdeError> {
        Ok(vec![
            Decryptor {
                type_id: AUGMENTATION,
                name: "Augmentation Decryptor".to_string(),
                probability_multiplier: Decimal::new(6, 1),
                me_modifier: -2,
                te_modifier: 2,
                run_modifier: 9,
            },
            Decryptor {
                type_id: PROCESS,
                name: "Process Decryptor".to_string(),
                probability_multiplier: Decimal::new(11, 1),
                me_modifier: 3,
                te_modifier: 6,
                run_modifier: 0,
            },
        ])
    }
    async fn type_classifications(
        &self,
        _: &[i64],
    ) -> Result<BTreeMap<i64, SdeTypeClassification>, SdeError> {
        Ok(BTreeMap::new())
    }
}

struct FakeIndustry {
    facilities: Vec<IndustryFacilityProfile>,
}

#[async_trait]
impl IndustryRepository for FakeIndustry {
    async fn list_builds(
        &self,
        _: WorkspaceId,
    ) -> Result<Vec<iskworks_core::Build>, IndustryError> {
        unimplemented!("not used by invention")
    }
    async fn get_build(
        &self,
        _: WorkspaceId,
        _: iskworks_core::BuildId,
    ) -> Result<iskworks_core::Build, IndustryError> {
        unimplemented!("not used by invention")
    }
    async fn create_build(
        &self,
        _: iskworks_core::NewBuild,
    ) -> Result<iskworks_core::Build, IndustryError> {
        unimplemented!("not used by invention")
    }
    async fn update_draft(
        &self,
        _: WorkspaceId,
        _: iskworks_core::BuildId,
        _: iskworks_core::DraftUpdate,
    ) -> Result<iskworks_core::Build, IndustryError> {
        unimplemented!("not used by invention")
    }
    async fn rename_build(
        &self,
        _: WorkspaceId,
        _: iskworks_core::BuildId,
        _: String,
    ) -> Result<iskworks_core::Build, IndustryError> {
        unimplemented!("not used by invention")
    }
    async fn delete_build(
        &self,
        _: WorkspaceId,
        _: iskworks_core::BuildId,
        _: u64,
        _force: bool,
    ) -> Result<(), IndustryError> {
        unimplemented!("not used by invention")
    }
    async fn list_price_sources(&self, _: WorkspaceId) -> Result<Vec<PriceSource>, IndustryError> {
        unimplemented!("not used by invention")
    }
    async fn get_price_source(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<PriceSource, IndustryError> {
        let _ = (workspace_id, source_id);
        unimplemented!("not used by invention")
    }
    async fn create_price_source(&self, _: PriceSource) -> Result<PriceSource, IndustryError> {
        unimplemented!("not used by invention")
    }
    async fn update_price_source(
        &self,
        _: WorkspaceId,
        _: PriceSourceId,
        _: iskworks_core::UpdatePriceSourceCommand,
    ) -> Result<PriceSource, IndustryError> {
        unimplemented!("not used by invention")
    }
    async fn upsert_price_items(
        &self,
        _: WorkspaceId,
        _: PriceSourceId,
        _: u64,
        _: Vec<iskworks_core::PriceSourceItem>,
    ) -> Result<PriceSource, IndustryError> {
        unimplemented!("not used by invention")
    }
    async fn remove_price_item(
        &self,
        _: WorkspaceId,
        _: PriceSourceId,
        _: i64,
        _: u64,
    ) -> Result<PriceSource, IndustryError> {
        unimplemented!("not used by invention")
    }
    async fn delete_price_source(
        &self,
        _: WorkspaceId,
        _: PriceSourceId,
        _: u64,
    ) -> Result<(), IndustryError> {
        unimplemented!("not used by invention")
    }
    async fn get_facility_profile(
        &self,
        workspace_id: WorkspaceId,
        facility_id: FacilityProfileId,
    ) -> Result<IndustryFacilityProfile, IndustryError> {
        self.facilities
            .iter()
            .find(|profile| profile.id == facility_id && profile.workspace_id == workspace_id)
            .cloned()
            .ok_or(IndustryError::Facility(FacilityError::NotFound))
    }
}

/// Best sell prices by type; a type left out has no order book.
struct FakeMarket {
    prices: BTreeMap<i64, &'static str>,
}

#[async_trait]
impl iskworks_core::FakeMarketRepository for FakeMarket {
    async fn scoped_order_books(
        &self,
        _: WorkspaceId,
        _: iskworks_core::MarketScope,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<MarketOrderView>>, MarketError> {
        let now = Utc::now();
        Ok(type_ids
            .iter()
            .filter_map(|type_id| {
                let price = self.prices.get(type_id)?;
                Some((
                    *type_id,
                    vec![MarketOrderView {
                        observation_id: None,
                        import_batch_id: None,
                        imported_file_id: None,
                        order_id: *type_id,
                        type_id: *type_id,
                        type_name: String::new(),
                        side: MarketOrderSide::Sell,
                        price: Money::parse(price).unwrap(),
                        remaining_volume: 1_000,
                        entered_volume: 1_000,
                        minimum_volume: 1,
                        order_range: -1,
                        issued_at: now,
                        duration_days: 90,
                        observed_at: now,
                        revalidated_at: None,
                        location_id: 60_003_760,
                        solar_system_id: SYSTEM,
                        region_id: 10_000_002,
                        jumps: 0,
                    }],
                ))
            })
            .collect())
    }
}

struct FakeAdjustedPrices;

#[async_trait]
impl AdjustedPriceRepository for FakeAdjustedPrices {
    async fn latest_adjusted_prices(
        &self,
        type_ids: &[i64],
        _: DateTime<Utc>,
    ) -> Result<BTreeMap<i64, Decimal>, StoreError> {
        // One hull section per run makes the EIV exactly this price.
        Ok(type_ids
            .iter()
            .filter(|type_id| **type_id == HULL)
            .map(|type_id| (*type_id, Decimal::from(12_687_150)))
            .collect())
    }
    async fn latest_system_cost_index_for(
        &self,
        solar_system_id: i64,
        activity: IndustryActivity,
    ) -> Result<Option<(Decimal, DateTime<Utc>)>, StoreError> {
        let index = match activity {
            IndustryActivity::Invention => "0.076018",
            IndustryActivity::Copying => "0.031",
            _ => return Ok(None),
        };
        Ok((solar_system_id == SYSTEM).then(|| (Decimal::from_str(index).unwrap(), Utc::now())))
    }
}

fn facility(workspace_id: WorkspaceId, role: FacilityRole) -> IndustryFacilityProfile {
    let now = Utc::now();
    IndustryFacilityProfile {
        id: FacilityProfileId(Uuid::new_v4()),
        workspace_id,
        name: "Q-3HS5 Raitaru".to_string(),
        kind: FacilityKind::Manual,
        role,
        structure_id: None,
        structure_type_id: None,
        structure_type_name: String::new(),
        solar_system_id: Some(SYSTEM),
        solar_system_name: "Q-3HS5".to_string(),
        security_class: SecurityClass::Unknown,
        material_reduction_percent: Decimal::ZERO,
        time_reduction_percent: Decimal::ZERO,
        job_cost_reduction_percent: Decimal::ZERO,
        facility_tax_percent: Decimal::ONE,
        scc_surcharge_percent: Decimal::from(4),
        alliance_surcharge_percent: Decimal::ZERO,
        fixed_supplemental_cost: Money::zero(),
        manual_system_cost_index: Some(Decimal::new(5, 2)),
        notes: String::new(),
        rigs: Vec::new(),
        archived_at: None,
        revision: 1,
        created_at: now,
        updated_at: now,
    }
}

fn character(levels: &[(i64, i64)]) -> SkillsCharacter {
    let skills = levels
        .iter()
        .map(|(skill_id, level)| {
            json!({
                "skill_id": skill_id,
                "active_skill_level": level,
                "trained_skill_level": level,
                "skillpoints_in_skill": 1,
            })
        })
        .collect::<Vec<_>>();
    SkillsCharacter {
        connection_id: ConnectedCharacterId(Uuid::new_v4()),
        name: "Ana Belle".to_string(),
        skills_scope_granted: true,
        observed_at: Some(Utc::now()),
        skills_summary: Some(json!({ "total_sp": 1, "skills": skills })),
    }
}

struct Fixture {
    app: axum::Router,
    facility_id: Uuid,
    character_id: Uuid,
}

fn all_prices() -> BTreeMap<i64, &'static str> {
    BTreeMap::from([
        (DATACORE_STARSHIP, "100000"),
        (DATACORE_MECHANICAL, "90000"),
        (PROCESS, "500000"),
        (AUGMENTATION, "250000"),
        (HULL, "20000000"),
    ])
}

fn fixture_with(invention_data: bool, prices: BTreeMap<i64, &'static str>) -> Fixture {
    let workspace = NewWorkspace::manual("Invention Test".to_string());
    let workspace_id = workspace.workspace.id;
    let facility = facility(workspace_id, FacilityRole::Manufacturing);
    let pilot = character(&[(ENCRYPTION, 5), (STARSHIP, 5), (MECHANICAL, 5)]);
    let facility_id = facility.id.0;
    let character_id = pilot.connection_id.0;
    let app = build_router(
        AppState::new(Arc::new(ConfiguredWorkspaceRepository::new(
            WorkspaceState::configured(workspace.workspace, workspace.owner),
        )))
        .with_sde_repository(Arc::new(FakeSde { invention_data }))
        .with_industry_repository(Arc::new(FakeIndustry {
            facilities: vec![facility],
        }))
        .with_market_repository(Arc::new(FakeMarket { prices }))
        .with_adjusted_price_repository(Arc::new(FakeAdjustedPrices))
        .with_skills_characters(Arc::new(FakeSkillsCharacters(vec![pilot]))),
    );
    Fixture {
        app,
        facility_id,
        character_id,
    }
}

fn fixture() -> Fixture {
    fixture_with(true, all_prices())
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

async fn quote(app: &axum::Router, body: Value) -> (StatusCode, Value) {
    send(
        app,
        Request::post("/api/invention/quote")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

fn whole(value: &Value) -> Decimal {
    Decimal::from_str(
        value
            .as_str()
            .unwrap_or_else(|| panic!("not a money string: {value}")),
    )
    .unwrap()
    .round()
}

#[tokio::test]
async fn product_search_lists_inventable_items() {
    let fixture = fixture();

    let (status, body) = send(
        &fixture.app,
        Request::get("/api/invention/products?query=neme")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["inventionDataAvailable"], true);
    assert_eq!(body["products"][0]["productTypeId"], NEMESIS);
    assert_eq!(
        body["products"][0]["sourceBlueprintName"],
        "Tristan Blueprint"
    );
    assert_eq!(body["products"][0]["sourceOutputCount"], 1);
}

#[tokio::test]
async fn a_quote_matches_the_industry_window() {
    let fixture = fixture();

    let (status, body) = quote(
        &fixture.app,
        json!({
            "productTypeId": NEMESIS,
            "characterId": fixture.character_id,
            "inventionFacilityId": fixture.facility_id,
            "manufacturingFacilityId": fixture.facility_id,
            "targetRuns": 10,
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["inventionDataAvailable"], true);
    let quote = &body["quote"];
    assert_eq!(quote["productName"], "Nemesis");
    assert_eq!(quote["sourceBlueprintName"], "Tristan Blueprint");
    assert_eq!(quote["baseChance"], "0.30");
    assert_eq!(quote["chanceWithSkills"], "0.4375");
    assert_eq!(quote["skillsDefaulted"], false);
    assert_eq!(quote["missingSkills"], json!([]));
    assert_eq!(quote["skills"][0]["role"], "encryption");
    assert_eq!(quote["skills"][1]["role"], "science");
    assert_eq!(quote["skills"][1]["level"], 5);
    // 63,900 s with Advanced Industry untrained on this character.
    assert_eq!(quote["inventionDurationSeconds"], 63_900);

    // The fee lines of the industry window, without its structure bonuses.
    let fee = &quote["jobFee"];
    assert_eq!(fee["estimatedItemValue"], "12687150");
    assert_eq!(whole(&fee["jobCostBase"]), Decimal::from(253_743));
    assert_eq!(whole(&fee["systemIndexCost"]), Decimal::from(19_289));
    assert_eq!(whole(&fee["facilityTax"]), Decimal::from(2_537));
    assert_eq!(whole(&fee["sccSurcharge"]), Decimal::from(10_150));
    assert_eq!(whole(&fee["total"]), Decimal::from(31_976));
    assert_eq!(quote["copyFee"]["systemCostIndex"], "0.031");

    let options = quote["options"].as_array().unwrap();
    assert_eq!(options.len(), 3);
    let (none, augmentation, process) = (&options[0], &options[1], &options[2]);
    assert_eq!(none["decryptor"], Value::Null);
    assert_eq!(
        none["copy"],
        json!({ "materialEfficiency": 2, "timeEfficiency": 4, "runs": 1 })
    );
    assert_eq!(none["complete"], true);
    assert_eq!(none["copiesNeeded"], 10);
    assert_eq!(process["decryptor"]["name"], "Process Decryptor");
    assert_eq!(process["chance"], "0.48125");
    assert_eq!(
        process["copy"],
        json!({ "materialEfficiency": 5, "timeEfficiency": 10, "runs": 1 })
    );
    assert_eq!(augmentation["copy"]["runs"], 10);
    assert_eq!(augmentation["copy"]["materialEfficiency"], 0);
    assert_eq!(augmentation["copiesNeeded"], 1);
    // Exactly one option is marked best.
    assert_eq!(
        options
            .iter()
            .filter(|option| option["best"] == true)
            .count(),
        1
    );
    // Datacores 2 × 100,000 + 2 × 90,000.
    assert_eq!(whole(&quote["materialsCost"]), Decimal::from(380_000));
}

#[tokio::test]
async fn skill_overrides_change_the_chance() {
    let fixture = fixture();

    let (status, body) = quote(
        &fixture.app,
        json!({
            "productTypeId": NEMESIS,
            "characterId": fixture.character_id,
            "skillOverrides": { ENCRYPTION.to_string(): 0, STARSHIP.to_string(): 0, MECHANICAL.to_string(): 0 },
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["quote"]["chanceWithSkills"], "0.3");
    assert_eq!(body["quote"]["skills"][0]["overridden"], true);
    assert_eq!(body["quote"]["skills"][0]["characterLevel"], 5);
    assert_eq!(
        body["quote"]["missingSkills"],
        json!([
            "Gallente Encryption Methods",
            "Gallentean Starship Engineering",
            "Mechanical Engineering"
        ])
    );
}

#[tokio::test]
async fn without_a_character_every_skill_is_four() {
    let fixture = fixture();

    let (status, body) = quote(&fixture.app, json!({ "productTypeId": NEMESIS })).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let quote = &body["quote"];
    assert_eq!(quote["skillsDefaulted"], true);
    assert_eq!(quote["skills"][0]["level"], 4);
    assert_eq!(quote["targetRuns"], 100);
    // No invention facility: the fee is unknown, so nothing is complete.
    assert_eq!(quote["jobFee"]["total"], Value::Null);
    assert_eq!(quote["manufacturingInstallationIncluded"], false);
    let none = &quote["options"][0];
    assert_eq!(none["complete"], false);
    assert_eq!(
        none["gaps"],
        json!([{ "kind": "jobFee" }, { "kind": "copyCost" }])
    );
    assert_eq!(none["best"], false);
}

#[tokio::test]
async fn a_decryptor_without_a_price_is_incomplete() {
    let mut prices = all_prices();
    prices.remove(&PROCESS);
    let fixture = fixture_with(true, prices);

    let (status, body) = quote(
        &fixture.app,
        json!({
            "productTypeId": NEMESIS,
            "inventionFacilityId": fixture.facility_id,
            "manufacturingFacilityId": fixture.facility_id,
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let options = body["quote"]["options"].as_array().unwrap();
    assert_eq!(options[0]["complete"], true);
    assert_eq!(options[2]["complete"], false);
    assert_eq!(options[2]["unitCost"], Value::Null);
    assert_eq!(
        options[2]["gaps"],
        json!([{ "kind": "decryptorPrice", "typeId": PROCESS }])
    );
}

#[tokio::test]
async fn a_product_that_cannot_be_invented_is_not_found() {
    let fixture = fixture();

    // The Tristan is built from the source blueprint, not invented.
    let (status, _) = quote(&fixture.app, json!({ "productTypeId": TRISTAN })).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = quote(&fixture.app, json!({ "productTypeId": 1 })).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_unknown_facility_or_character_is_not_found() {
    let fixture = fixture();

    let (status, _) = quote(
        &fixture.app,
        json!({ "productTypeId": NEMESIS, "inventionFacilityId": Uuid::new_v4() }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = quote(
        &fixture.app,
        json!({ "productTypeId": NEMESIS, "characterId": Uuid::new_v4() }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn out_of_range_inputs_are_rejected() {
    let fixture = fixture();

    let (status, _) = quote(
        &fixture.app,
        json!({ "productTypeId": NEMESIS, "targetRuns": 0 }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = quote(
        &fixture.app,
        json!({ "productTypeId": NEMESIS, "skillOverrides": { ENCRYPTION.to_string(): 6 } }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn an_import_without_invention_data_says_so() {
    let fixture = fixture_with(false, all_prices());

    let (status, body) = quote(&fixture.app, json!({ "productTypeId": NEMESIS })).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["inventionDataAvailable"], false);
    assert_eq!(body["quote"], Value::Null);
}
