//! The editor's catalogue routes over a fake engine with a small catalogue
//! and a character whose skills decide what it can use.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::Utc;
use iskworks_api::{build_router, AppState};
use iskworks_app::{SkillsCharacter, SkillsCharacterSource};
use iskworks_core::fitting::{
    AttributeExplanation, CatalogueEntry, CatalogueKind, DamageProfile, EftError, ExplainTarget,
    FitCalculation, FitCatalogue, FitInput, FittingEngine, FittingEngineInfo, FittingError,
    Hardpoint, MarketGroup, ParsedFit, Pilot, ShipSlots, SlotKind,
};
use iskworks_core::{ConnectedCharacterId, InventoryError, WorkspaceId};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, ManufacturingRecipe, RequiredSkill, SdeError,
    SdeReadRepository, TypeSearchResult,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

mod support;
use support::workspace::configured_workspace;

const RIFTER: i64 = 587;
const AUTOCANNON: i64 = 2_873;
const AUTOCANNON_I: i64 = 2_881;
const DAMAGE_CONTROL: i64 = 2_048;
const LARGE_RIG: i64 = 26_000;
const SMALL_RIG: i64 = 31_000;
const EMP_S: i64 = 185;
const ARCH_ANGEL_EMP_S: i64 = 20979;
const EMP_M: i64 = 186;
const WARRIOR: i64 = 2_486;
const GUNNERY: i64 = 3_300;

fn entry(type_id: i64, name: &str, kind: CatalogueKind, slot: Option<SlotKind>) -> CatalogueEntry {
    CatalogueEntry {
        type_id,
        name: name.to_string(),
        kind,
        group_id: 1,
        slot,
        market_group_id: Some(2),
        meta_group_id: None,
        variation_parent: None,
        cpu: 10.0,
        powergrid: 5.0,
        calibration: 0.0,
        rig_size: None,
        hardpoint: None,
        charge_groups: Vec::new(),
        charge_size: None,
        fits_ship_types: Vec::new(),
        fits_ship_groups: Vec::new(),
        ship_slots: None,
    }
}

fn catalogue() -> FitCatalogue {
    let mut rifter = entry(RIFTER, "Rifter", CatalogueKind::Ship, None);
    rifter.rig_size = Some(1);
    rifter.ship_slots = Some(ShipSlots {
        high: 4,
        med: 3,
        low: 3,
        rig: 3,
    });
    let mut gun = entry(
        AUTOCANNON,
        "200mm AutoCannon II",
        CatalogueKind::Module,
        Some(SlotKind::High),
    );
    gun.hardpoint = Some(Hardpoint::Turret);
    gun.charge_groups = vec![83];
    gun.charge_size = Some(1);
    gun.variation_parent = Some(AUTOCANNON_I);
    let mut gun_i = entry(
        AUTOCANNON_I,
        "200mm AutoCannon I",
        CatalogueKind::Module,
        Some(SlotKind::High),
    );
    gun_i.charge_groups = vec![83];
    let mut large_rig = entry(
        LARGE_RIG,
        "Large Rig",
        CatalogueKind::Module,
        Some(SlotKind::Rig),
    );
    large_rig.rig_size = Some(3);
    let mut small_rig = entry(
        SMALL_RIG,
        "Small Rig",
        CatalogueKind::Module,
        Some(SlotKind::Rig),
    );
    small_rig.rig_size = Some(1);
    let charge = |type_id, name: &str, size| CatalogueEntry {
        group_id: 83,
        charge_size: Some(size),
        ..entry(type_id, name, CatalogueKind::Charge, None)
    };
    FitCatalogue::new(
        vec![
            rifter,
            gun,
            gun_i,
            entry(
                DAMAGE_CONTROL,
                "Damage Control II",
                CatalogueKind::Module,
                Some(SlotKind::Low),
            ),
            large_rig,
            small_rig,
            charge(EMP_S, "EMP S", 1),
            CatalogueEntry {
                meta_group_id: Some(4),
                variation_parent: Some(EMP_S),
                ..charge(ARCH_ANGEL_EMP_S, "Arch Angel EMP S", 1)
            },
            charge(EMP_M, "EMP M", 2),
            entry(WARRIOR, "Warrior II", CatalogueKind::Drone, None),
        ],
        BTreeMap::from([
            (
                1,
                MarketGroup {
                    id: 1,
                    name: "Ship Equipment".to_string(),
                    parent: None,
                },
            ),
            (
                2,
                MarketGroup {
                    id: 2,
                    name: "Turrets".to_string(),
                    parent: Some(1),
                },
            ),
        ]),
    )
}

struct FakeEngine(Arc<FitCatalogue>);

impl FittingEngine for FakeEngine {
    fn info(&self) -> FittingEngineInfo {
        FittingEngineInfo {
            data_version: "13.3650286.0".to_string(),
            major: 13,
            build_number: 3_650_286,
        }
    }
    fn catalogue(&self) -> Arc<FitCatalogue> {
        Arc::clone(&self.0)
    }
    fn parse_eft(&self, _: &str) -> Result<ParsedFit, EftError> {
        unimplemented!()
    }
    fn write_eft(&self, _: &str, _: &FitInput) -> Result<String, FittingError> {
        unimplemented!()
    }
    fn calculate(
        &self,
        _: &FitInput,
        _: &Pilot,
        _: &DamageProfile,
    ) -> Result<FitCalculation, FittingError> {
        unimplemented!()
    }
    fn explain(
        &self,
        _: &FitInput,
        _: &Pilot,
        _: &DamageProfile,
        _: ExplainTarget,
        _: &str,
    ) -> Result<AttributeExplanation, FittingError> {
        unimplemented!()
    }
}

/// The autocannon II needs Gunnery V.
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
    async fn required_skills(
        &self,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<RequiredSkill>>, SdeError> {
        Ok(type_ids
            .iter()
            .filter(|type_id| **type_id == AUTOCANNON)
            .map(|type_id| {
                (
                    *type_id,
                    vec![RequiredSkill {
                        skill_type_id: GUNNERY,
                        level: 5,
                    }],
                )
            })
            .collect())
    }
}

const PILOT: u128 = 1;

struct FakeCharacters;

#[async_trait]
impl SkillsCharacterSource for FakeCharacters {
    async fn skills_character(
        &self,
        _: WorkspaceId,
        connection_id: ConnectedCharacterId,
    ) -> Result<Option<SkillsCharacter>, InventoryError> {
        Ok(
            (connection_id == ConnectedCharacterId(Uuid::from_u128(PILOT))).then(|| {
                SkillsCharacter {
                    connection_id,
                    name: "Gunner".to_string(),
                    skills_scope_granted: true,
                    observed_at: Some(Utc::now()),
                    skills_summary: Some(json!({
                        "total_sp": 1000,
                        "skills": [{ "skill_id": GUNNERY, "trained_skill_level": 4,
                                     "active_skill_level": 4, "skillpoints_in_skill": 45_255 }]
                    })),
                }
            }),
        )
    }
    async fn skills_characters(
        &self,
        _: WorkspaceId,
    ) -> Result<Vec<SkillsCharacter>, InventoryError> {
        unimplemented!()
    }
}

fn app(engine: bool) -> axum::Router {
    let state = AppState::new(Arc::new(configured_workspace("Catalogue Test")))
        .with_sde_repository(Arc::new(FakeSde))
        .with_fit_repository(Arc::new(NoFits))
        .with_skills_characters(Arc::new(FakeCharacters));
    build_router(if engine {
        state.with_fitting_engine(Arc::new(FakeEngine(Arc::new(catalogue()))))
    } else {
        state
    })
}

/// The catalogue routes never read saved fits.
struct NoFits;

#[async_trait]
impl iskworks_core::fitting::FitRepository for NoFits {
    async fn list_fits(
        &self,
        _: WorkspaceId,
    ) -> Result<Vec<iskworks_core::fitting::Fit>, InventoryError> {
        unimplemented!()
    }
    async fn fit(
        &self,
        _: WorkspaceId,
        _: iskworks_core::fitting::FitId,
    ) -> Result<Option<iskworks_core::fitting::Fit>, InventoryError> {
        unimplemented!()
    }
    async fn create_fit(
        &self,
        _: WorkspaceId,
        _: &iskworks_core::fitting::FitDraft,
        _: chrono::DateTime<Utc>,
    ) -> Result<iskworks_core::fitting::Fit, InventoryError> {
        unimplemented!()
    }
    async fn update_fit(
        &self,
        _: WorkspaceId,
        _: iskworks_core::fitting::FitId,
        _: &iskworks_core::fitting::FitDraft,
        _: chrono::DateTime<Utc>,
    ) -> Result<Option<iskworks_core::fitting::Fit>, InventoryError> {
        unimplemented!()
    }
    async fn delete_fit(
        &self,
        _: WorkspaceId,
        _: iskworks_core::fitting::FitId,
    ) -> Result<bool, InventoryError> {
        unimplemented!()
    }
}

async fn get(app: axum::Router, uri: &str) -> (StatusCode, Value) {
    let response = app
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

fn names(body: &Value) -> Vec<&str> {
    body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn modules_for_a_ship_fit_its_racks_and_rig_size() {
    let (status, body) = get(
        app(true),
        &format!("/api/fittings/catalogue?kind=modules&shipTypeId={RIFTER}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        names(&body),
        [
            "200mm AutoCannon I",
            "200mm AutoCannon II",
            "Damage Control II",
            "Small Rig"
        ]
    );
    assert_eq!(body["truncated"], false);
    let gun = &body["entries"][1];
    assert_eq!(gun["slot"], "high");
    assert_eq!(gun["hardpoint"], "turret");
    assert_eq!(gun["cpu"], 10.0);
    assert_eq!(gun["marketPath"], json!(["Ship Equipment", "Turrets"]));
    assert_eq!(gun["canUse"], Value::Null);

    let (_, body) = get(
        app(true),
        &format!("/api/fittings/catalogue?kind=modules&shipTypeId={RIFTER}&slot=low"),
    )
    .await;
    assert_eq!(names(&body), ["Damage Control II"]);
    let (_, body) = get(
        app(true),
        &format!("/api/fittings/catalogue?kind=modules&shipTypeId={RIFTER}&q=autocannon%20ii"),
    )
    .await;
    assert_eq!(names(&body), ["200mm AutoCannon II"]);
}

#[tokio::test]
async fn a_character_pilot_says_what_it_can_use() {
    let pilot = Uuid::from_u128(PILOT);
    let (status, body) = get(
        app(true),
        &format!(
            "/api/fittings/catalogue?kind=modules&shipTypeId={RIFTER}&slot=high&pilot={pilot}"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let can_use: BTreeMap<&str, &Value> = body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| (entry["name"].as_str().unwrap(), &entry["canUse"]))
        .collect();
    assert_eq!(can_use["200mm AutoCannon II"], &json!(false));
    assert_eq!(can_use["200mm AutoCannon I"], &json!(true));
}

#[tokio::test]
async fn charges_follow_the_module_and_drones_and_ships_list_by_name() {
    let (_, body) = get(
        app(true),
        &format!("/api/fittings/catalogue?kind=charges&moduleTypeId={AUTOCANNON}"),
    )
    .await;
    assert_eq!(names(&body), ["Arch Angel EMP S", "EMP S"]);
    // A faction charge says what it is a variation of, for the charge menu.
    let faction = &body["entries"][0];
    assert_eq!(faction["metaGroupId"], 4);
    assert_eq!(faction["variationParent"], EMP_S);
    assert_eq!(body["entries"][1]["variationParent"], Value::Null);
    let (_, body) = get(app(true), "/api/fittings/catalogue?kind=drones").await;
    assert_eq!(names(&body), ["Warrior II"]);
    let (_, body) = get(app(true), "/api/fittings/catalogue?kind=ships&q=rif").await;
    assert_eq!(names(&body), ["Rifter"]);
}

#[tokio::test]
async fn variations_list_the_family() {
    let (status, body) = get(
        app(true),
        &format!("/api/fittings/catalogue/variations?typeId={AUTOCANNON}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(names(&body), ["200mm AutoCannon I", "200mm AutoCannon II"]);
}

#[tokio::test]
async fn bad_or_missing_parameters_are_refused() {
    for query in [
        "kind=modules",
        "kind=modules&shipTypeId=999",
        "kind=charges",
        "kind=charges&moduleTypeId=999",
        "kind=lasers",
        "",
    ] {
        let (status, body) = get(app(true), &format!("/api/fittings/catalogue?{query}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {body}");
    }
    let (status, _) = get(app(true), "/api/fittings/catalogue/variations").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_catalogue_needs_the_engine() {
    let (status, body) = get(app(false), "/api/fittings/catalogue?kind=drones").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "fitting_engine_unavailable");
}
