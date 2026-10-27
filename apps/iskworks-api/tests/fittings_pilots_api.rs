//! Fit stats, pilots (who can fly a fit, with stats for each) and skill
//! plans from a fit, over fakes: a small skill tree, four characters and
//! an engine whose numbers say which skills it was given.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::{DateTime, Utc};
use iskworks_api::{build_router, AppState};
use iskworks_app::{SkillsCharacter, SkillsCharacterSource};
use iskworks_core::fitting::{
    AttributeExplanation, DamageProfile, EftError, ExplainTarget, Fit, FitCalculation, FitDraft,
    FitId, FitInput, FitItem, FitRepository, FitStats, FittingEngine, FittingEngineInfo,
    FittingError, ItemStats, ModifierSource, ModuleState, ParsedFit, Pilot, SlotKind, SourceRef,
};
use iskworks_core::skill_plans::{SkillPlan, SkillPlanId, SkillPlanInput, SkillPlanRepository};
use iskworks_core::{ConnectedCharacterId, InventoryError, WorkspaceId};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, ManufacturingRecipe, RequiredSkill, SdeError,
    SdeReadRepository, SkillAttribute, SkillDefinition, TypeSearchResult,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

mod support;
use support::workspace::configured_workspace;

const SPACESHIP_COMMAND: i64 = 100;
const CALDARI_HAULER: i64 = 101;
const TRANSPORT_SHIPS: i64 = 102;
const SHIELD_UPGRADES: i64 = 103;

const BUSTARD: i64 = 12_731;
const EXTENDER: i64 = 3_841;

// --- SDE: the Bustard needs Transport Ships I + Caldari Hauler V, the
// extender Shield Upgrades IV.

struct FakeSde;

fn skill(type_id: i64, name: &str, rank: i32, prerequisites: Vec<(i64, i16)>) -> SkillDefinition {
    SkillDefinition {
        type_id,
        name: name.to_string(),
        group_id: 257,
        group_name: "Spaceship Command".to_string(),
        rank,
        primary_attribute: SkillAttribute::Perception,
        secondary_attribute: SkillAttribute::Willpower,
        omega_only: false,
        prerequisites: prerequisites
            .into_iter()
            .map(|(skill_type_id, level)| RequiredSkill {
                skill_type_id,
                level,
            })
            .collect(),
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
    ) -> Result<Vec<BlueprintSearchResult>, SdeError> {
        unimplemented!()
    }
    async fn manufacturing_recipe(&self, _: i64) -> Result<Option<ManufacturingRecipe>, SdeError> {
        unimplemented!()
    }
    async fn search_types(&self, _: &str, _: u32) -> Result<Vec<TypeSearchResult>, SdeError> {
        unimplemented!()
    }
    async fn skill_catalogue(&self) -> Result<Vec<SkillDefinition>, SdeError> {
        Ok(vec![
            skill(SPACESHIP_COMMAND, "Spaceship Command", 1, Vec::new()),
            skill(
                CALDARI_HAULER,
                "Caldari Hauler",
                4,
                vec![(SPACESHIP_COMMAND, 3)],
            ),
            skill(
                TRANSPORT_SHIPS,
                "Transport Ships",
                6,
                vec![(CALDARI_HAULER, 5)],
            ),
            skill(SHIELD_UPGRADES, "Shield Upgrades", 2, Vec::new()),
        ])
    }
    async fn type_reference(
        &self,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, iskworks_sde::TypeReference>, SdeError> {
        let names = [
            (BUSTARD, "Bustard"),
            (EXTENDER, "Medium Shield Extender II"),
            (SHIELD_UPGRADES, "Shield Upgrades"),
        ];
        Ok(names
            .into_iter()
            .filter(|(type_id, _)| type_ids.contains(type_id))
            .map(|(type_id, name)| {
                (
                    type_id,
                    iskworks_sde::TypeReference {
                        type_name: Some(name.to_string()),
                        group_id: None,
                        group_name: None,
                        category_id: None,
                        category_name: None,
                        packaged_volume_m3: None,
                    },
                )
            })
            .collect())
    }
    async fn required_skills(
        &self,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<RequiredSkill>>, SdeError> {
        let requirement = |skill_type_id, level| RequiredSkill {
            skill_type_id,
            level,
        };
        Ok([
            (
                BUSTARD,
                vec![
                    requirement(TRANSPORT_SHIPS, 1),
                    requirement(CALDARI_HAULER, 5),
                ],
            ),
            (EXTENDER, vec![requirement(SHIELD_UPGRADES, 4)]),
        ]
        .into_iter()
        .filter(|(type_id, _)| type_ids.contains(type_id))
        .collect())
    }
}

// --- Characters.

struct FakeCharacters(Vec<SkillsCharacter>);

#[async_trait]
impl SkillsCharacterSource for FakeCharacters {
    async fn skills_character(
        &self,
        _: WorkspaceId,
        connection_id: ConnectedCharacterId,
    ) -> Result<Option<SkillsCharacter>, InventoryError> {
        Ok(self
            .0
            .iter()
            .find(|character| character.connection_id == connection_id)
            .cloned())
    }
    async fn skills_characters(
        &self,
        _: WorkspaceId,
    ) -> Result<Vec<SkillsCharacter>, InventoryError> {
        Ok(self.0.clone())
    }
}

/// `(skill, trained, active)`; SP is exactly the trained level's.
fn character(id: u128, name: &str, skills: &[(i64, i16, i16)]) -> SkillsCharacter {
    let ranks = BTreeMap::from([
        (SPACESHIP_COMMAND, 1),
        (CALDARI_HAULER, 4),
        (TRANSPORT_SHIPS, 6),
        (SHIELD_UPGRADES, 2),
    ]);
    let skills: Vec<Value> = skills
        .iter()
        .map(|(skill_id, trained, active)| {
            json!({
                "skill_id": skill_id,
                "trained_skill_level": trained,
                "active_skill_level": active,
                "skillpoints_in_skill":
                    iskworks_core::skills::sp_for_level(ranks[skill_id], *trained),
            })
        })
        .collect();
    SkillsCharacter {
        connection_id: ConnectedCharacterId(Uuid::from_u128(id)),
        name: name.to_string(),
        skills_scope_granted: true,
        observed_at: Some(Utc::now()),
        skills_summary: Some(json!({
            "total_sp": 1_000_000,
            "skills": skills,
            "attributes": { "charisma": 20, "intelligence": 20, "memory": 20,
                            "perception": 20, "willpower": 20 }
        })),
    }
}

const MIRA: u128 = 1;
const AURA: u128 = 2;
const FRESH: u128 = 3;
const SCOUT: u128 = 4;

fn characters() -> Vec<SkillsCharacter> {
    let mut scout = character(SCOUT, "Scout", &[]);
    scout.skills_scope_granted = false;
    scout.skills_summary = None;
    vec![
        character(FRESH, "Fresh Char", &[]),
        // Flies the hull, can't use the extender.
        character(
            AURA,
            "Aura",
            &[
                (SPACESHIP_COMMAND, 5, 5),
                (CALDARI_HAULER, 5, 5),
                (TRANSPORT_SHIPS, 1, 1),
                (SHIELD_UPGRADES, 2, 2),
            ],
        ),
        scout,
        character(
            MIRA,
            "Mira",
            &[
                (SPACESHIP_COMMAND, 5, 5),
                (CALDARI_HAULER, 5, 5),
                (TRANSPORT_SHIPS, 1, 1),
                (SHIELD_UPGRADES, 4, 4),
            ],
        ),
    ]
}

// --- Engine: EHP says which pilot it calculated for (All V 1000, All 0 0,
// a character the sum of its skill levels), DPS says the profile's EM share.

struct FakeEngine;

impl FittingEngine for FakeEngine {
    fn catalogue(&self) -> std::sync::Arc<iskworks_core::fitting::FitCatalogue> {
        std::sync::Arc::default()
    }

    fn info(&self) -> FittingEngineInfo {
        FittingEngineInfo {
            data_version: "13.3650286.0".to_string(),
            major: 13,
            build_number: 3_650_286,
        }
    }
    fn parse_eft(&self, _: &str) -> Result<ParsedFit, EftError> {
        unimplemented!()
    }
    fn write_eft(&self, _: &str, _: &FitInput) -> Result<String, FittingError> {
        unimplemented!()
    }
    fn calculate(
        &self,
        fit: &FitInput,
        pilot: &Pilot,
        profile: &DamageProfile,
    ) -> Result<FitCalculation, FittingError> {
        let mut stats = FitStats::default();
        stats.defense.ehp = match pilot {
            Pilot::AllV => 1000.0,
            Pilot::AllZero => 0.0,
            Pilot::Skills(skills) => skills
                .levels()
                .values()
                .map(|level| f64::from(*level))
                .sum(),
        };
        stats.offense.total_dps = profile.normalized()[0];
        Ok(FitCalculation {
            stats,
            items: fit
                .items
                .iter()
                .map(|_| {
                    Some(ItemStats {
                        reached_state: ModuleState::Online,
                        max_state: ModuleState::Online,
                        cpu: 20.0,
                        powergrid: 30.0,
                        calibration: 0.0,
                        capacitor_per_second: 0.0,
                        optimal: 0.0,
                        falloff: 0.0,
                        dps: 0.0,
                        headline: None,
                    })
                })
                .collect(),
            violations: Vec::new(),
            unknown_items: Vec::new(),
        })
    }

    /// `nope` is no attribute; item 7 is no item; otherwise a skill and
    /// the first item modify it.
    fn explain(
        &self,
        _: &FitInput,
        _: &Pilot,
        _: &DamageProfile,
        target: ExplainTarget,
        attribute: &str,
    ) -> Result<AttributeExplanation, FittingError> {
        if attribute == "nope" {
            return Err(FittingError::UnknownAttribute);
        }
        if target == ExplainTarget::Item(7) {
            return Err(FittingError::UnknownItem(7));
        }
        let source = |from, operator: &str, value| ModifierSource {
            from,
            operator: operator.to_string(),
            value,
            penalty: None,
            applied: true,
        };
        Ok(AttributeExplanation {
            base: 100.0,
            value: 137.5,
            sources: vec![
                source(
                    SourceRef::Skill {
                        type_id: SHIELD_UPGRADES,
                    },
                    "post_percent",
                    25.0,
                ),
                source(SourceRef::Item { index: 0 }, "mod_add", 10.0),
                source(SourceRef::Buff { id: 10 }, "post_percent", 5.0),
            ],
        })
    }
}

// --- Storage.

#[derive(Default)]
struct FakeFits(Mutex<Vec<Fit>>);

#[async_trait]
impl FitRepository for FakeFits {
    async fn list_fits(&self, _: WorkspaceId) -> Result<Vec<Fit>, InventoryError> {
        Ok(self.0.lock().unwrap().clone())
    }
    async fn fit(&self, _: WorkspaceId, id: FitId) -> Result<Option<Fit>, InventoryError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .find(|fit| fit.id == id)
            .cloned())
    }
    async fn create_fit(
        &self,
        _: WorkspaceId,
        draft: &FitDraft,
        now: DateTime<Utc>,
    ) -> Result<Fit, InventoryError> {
        let fit = Fit {
            id: FitId(Uuid::new_v4()),
            name: draft.name.clone(),
            description: draft.description.clone(),
            ship_type_id: draft.ship_type_id,
            items: draft.items.clone(),
            created_at: now,
            updated_at: now,
        };
        self.0.lock().unwrap().push(fit.clone());
        Ok(fit)
    }
    async fn update_fit(
        &self,
        _: WorkspaceId,
        _: FitId,
        _: &FitDraft,
        _: DateTime<Utc>,
    ) -> Result<Option<Fit>, InventoryError> {
        unimplemented!()
    }
    async fn delete_fit(&self, _: WorkspaceId, _: FitId) -> Result<bool, InventoryError> {
        unimplemented!()
    }
}

#[derive(Default)]
struct FakePlans(Mutex<Vec<SkillPlan>>);

#[async_trait]
impl SkillPlanRepository for FakePlans {
    async fn list_skill_plans(&self, _: WorkspaceId) -> Result<Vec<SkillPlan>, InventoryError> {
        Ok(self.0.lock().unwrap().clone())
    }
    async fn skill_plan(
        &self,
        _: WorkspaceId,
        _: SkillPlanId,
    ) -> Result<Option<SkillPlan>, InventoryError> {
        unimplemented!()
    }
    async fn create_skill_plan(
        &self,
        _: WorkspaceId,
        input: &SkillPlanInput,
        now: DateTime<Utc>,
    ) -> Result<SkillPlan, InventoryError> {
        let plan = SkillPlan {
            id: SkillPlanId(Uuid::new_v4()),
            name: input.name.clone(),
            description: input.description.clone(),
            entries: input.entries.clone(),
            created_at: now,
            updated_at: now,
        };
        self.0.lock().unwrap().push(plan.clone());
        Ok(plan)
    }
    async fn update_skill_plan(
        &self,
        _: WorkspaceId,
        _: SkillPlanId,
        _: &SkillPlanInput,
        _: DateTime<Utc>,
    ) -> Result<Option<SkillPlan>, InventoryError> {
        unimplemented!()
    }
    async fn delete_skill_plan(
        &self,
        _: WorkspaceId,
        _: SkillPlanId,
    ) -> Result<bool, InventoryError> {
        unimplemented!()
    }
}

fn extender() -> FitItem {
    FitItem {
        type_id: EXTENDER,
        slot_kind: SlotKind::Med,
        slot_index: Some(0),
        quantity: 1,
        state: ModuleState::Online,
        charge_type_id: None,
        mutation: None,
    }
}

struct TestApp {
    router: axum::Router,
    fits: Arc<FakeFits>,
    plans: Arc<FakePlans>,
}

fn test_app(engine: bool) -> TestApp {
    let fits = Arc::new(FakeFits::default());
    let plans = Arc::new(FakePlans::default());
    let state = AppState::new(Arc::new(configured_workspace("Pilots Test")))
        .with_sde_repository(Arc::new(FakeSde))
        .with_fit_repository(fits.clone())
        .with_skill_plan_repository(plans.clone())
        .with_skills_characters(Arc::new(FakeCharacters(characters())));
    let state = if engine {
        state.with_fitting_engine(Arc::new(FakeEngine))
    } else {
        state
    };
    TestApp {
        router: build_router(state),
        fits,
        plans,
    }
}

async fn saved_fit(app: &TestApp) -> String {
    let fit = app
        .fits
        .create_fit(
            WorkspaceId(Uuid::nil()),
            &FitDraft {
                name: "Null Transport".to_string(),
                description: String::new(),
                ship_type_id: BUSTARD,
                items: vec![extender()],
            },
            Utc::now(),
        )
        .await
        .unwrap();
    fit.id.0.to_string()
}

async fn send(app: &TestApp, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, body)
}

async fn get(app: &TestApp, uri: &str) -> (StatusCode, Value) {
    send(app, Request::get(uri).body(Body::empty()).unwrap()).await
}

async fn post(app: &TestApp, uri: &str, body: Value) -> (StatusCode, Value) {
    send(
        app,
        Request::post(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

fn connection(id: u128) -> String {
    Uuid::from_u128(id).to_string()
}

#[tokio::test]
async fn stats_default_to_all_v_and_a_uniform_profile() {
    let app = test_app(true);
    let id = saved_fit(&app).await;
    let (status, body) = get(&app, &format!("/api/fittings/{id}/stats")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stats"]["defense"]["ehp"], 1000.0);
    assert_eq!(body["stats"]["offense"]["totalDps"], 0.25);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["cpu"], 20.0);
    assert_eq!(body["flyability"], Value::Null);
}

#[tokio::test]
async fn stats_for_a_character_use_its_active_levels_and_say_what_it_lacks() {
    let app = test_app(true);
    let id = saved_fit(&app).await;
    let (status, body) = get(
        &app,
        &format!(
            "/api/fittings/{id}/stats?pilot={}&profile=em",
            connection(AURA)
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // 5 + 5 + 1 + 2
    assert_eq!(body["stats"]["defense"]["ehp"], 13.0);
    assert_eq!(body["stats"]["offense"]["totalDps"], 1.0);
    assert_eq!(body["flyability"]["status"], "canFlyHull");
    assert_eq!(body["flyability"]["blockedItems"], json!([0]));

    let (status, body) = get(&app, &format!("/api/fittings/{id}/stats?pilot=allZero")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["stats"]["defense"]["ehp"], 0.0);
}

#[tokio::test]
async fn stats_refuse_an_unknown_pilot_profile_or_a_character_without_skills() {
    let app = test_app(true);
    let id = saved_fit(&app).await;
    for query in [
        "pilot=nobody".to_string(),
        format!("pilot={}", Uuid::from_u128(99)),
        format!("pilot={}", connection(SCOUT)),
        "profile=lasers".to_string(),
    ] {
        let (status, body) = get(&app, &format!("/api/fittings/{id}/stats?{query}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {body}");
    }
    let (status, _) = get(&app, &format!("/api/fittings/{}/stats", Uuid::new_v4())).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_unsaved_fit_is_calculated_from_the_request() {
    let app = test_app(true);
    let (status, body) = post(
        &app,
        "/api/fittings/calculate",
        json!({
            "fit": { "shipTypeId": BUSTARD, "items": [extender(), extender()] },
            "pilot": "allV",
            "profile": "thermal"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_eq!(body["stats"]["offense"]["totalDps"], 0.0);
}

#[tokio::test]
async fn stats_need_the_engine() {
    let app = test_app(false);
    let id = saved_fit(&app).await;
    let (status, body) = get(&app, &format!("/api/fittings/{id}/stats")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "fitting_engine_unavailable");
}

#[tokio::test]
async fn pilots_are_sorted_by_how_close_they_are_with_stats_for_each() {
    let app = test_app(true);
    let id = saved_fit(&app).await;
    let (status, body) = get(&app, &format!("/api/fittings/{id}/pilots")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows = body.as_array().unwrap();
    let order: Vec<(&str, &Value)> = rows
        .iter()
        .map(|row| (row["name"].as_str().unwrap(), &row["status"]))
        .collect();
    assert_eq!(
        order,
        [
            ("Mira", &json!("canUseAll")),
            ("Aura", &json!("canFlyHull")),
            ("Fresh Char", &json!("cannot")),
            ("Scout", &Value::Null),
        ]
    );
    // Each pilot's stats use its own skills (fake EHP = sum of levels).
    assert_eq!(rows[0]["stats"]["ehp"], 15.0);
    assert_eq!(rows[1]["stats"]["ehp"], 13.0);
    assert_eq!(rows[3]["stats"], Value::Null);
    assert_eq!(rows[3]["flyability"], Value::Null);
    assert_eq!(rows[0]["connectionId"], connection(MIRA));

    let aura = &rows[1]["flyability"];
    assert_eq!(aura["missing"][0]["skillTypeId"], SHIELD_UPGRADES);
    assert_eq!(aura["missing"][0]["name"], "Shield Upgrades");
    assert_eq!(aura["missing"][0]["requiredLevel"], 4);
    assert!(aura["remainingSeconds"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn pilots_without_the_engine_still_say_who_can_fly() {
    let app = test_app(false);
    let id = saved_fit(&app).await;
    let (status, body) = get(&app, &format!("/api/fittings/{id}/pilots")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body[0]["status"], "canUseAll");
    assert_eq!(body[0]["stats"], Value::Null);
}

#[tokio::test]
async fn the_fit_list_says_who_can_fly_each_fit() {
    let app = test_app(false);
    saved_fit(&app).await;
    let (status, body) = get(&app, "/api/fittings").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let pilots: BTreeMap<String, Value> = body[0]["pilots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pilot| {
            (
                pilot["name"].as_str().unwrap().to_string(),
                pilot["status"].clone(),
            )
        })
        .collect();
    assert_eq!(pilots["Mira"], "canUseAll");
    assert_eq!(pilots["Aura"], "canFlyHull");
    assert_eq!(pilots["Fresh Char"], "cannot");
    assert_eq!(pilots["Scout"], Value::Null);
}

#[tokio::test]
async fn a_skill_plan_for_a_fit_holds_its_closure_or_what_a_pilot_lacks() {
    let app = test_app(false);
    let id = saved_fit(&app).await;

    let (status, body) = post(&app, &format!("/api/fittings/{id}/skill-plan"), json!({})).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["name"], "Fit: Null Transport");
    // Spaceship Command I-III, Caldari Hauler I-V, Transport Ships I,
    // Shield Upgrades I-IV.
    assert_eq!(body["entries"].as_array().unwrap().len(), 3 + 5 + 1 + 4);

    let (status, body) = post(
        &app,
        &format!("/api/fittings/{id}/skill-plan"),
        json!({ "connectionId": connection(AURA) }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(
        body["entries"],
        json!([
            { "skillTypeId": SHIELD_UPGRADES, "level": 3 },
            { "skillTypeId": SHIELD_UPGRADES, "level": 4 }
        ])
    );
    assert_eq!(app.plans.0.lock().unwrap().len(), 2);

    let (status, body) = post(
        &app,
        &format!("/api/fittings/{id}/skill-plan"),
        json!({ "connectionId": connection(MIRA) }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(app.plans.0.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn an_attribute_is_explained_with_named_sources() {
    let app = test_app(true);
    let id = saved_fit(&app).await;
    let (status, body) = get(
        &app,
        &format!("/api/fittings/{id}/explain?target=ship&attribute=shieldCapacity"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["base"], 100.0);
    assert_eq!(body["value"], 137.5);
    assert_eq!(
        body["sources"][0],
        json!({ "from": { "kind": "skill", "typeId": SHIELD_UPGRADES }, "name": "Shield Upgrades",
                "operator": "post_percent", "value": 25.0, "penalty": null, "applied": true })
    );
    assert_eq!(
        body["sources"][1]["from"],
        json!({ "kind": "item", "index": 0 })
    );
    assert_eq!(body["sources"][1]["name"], "Medium Shield Extender II");
    assert_eq!(body["sources"][2]["name"], Value::Null);

    let (status, _) = get(
        &app,
        &format!("/api/fittings/{id}/explain?target=0&attribute=maxRange"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn explaining_refuses_a_bad_target_or_attribute_and_needs_the_engine() {
    let app = test_app(true);
    let id = saved_fit(&app).await;
    for query in [
        "target=hull&attribute=cpuOutput",
        "target=ship&attribute=nope",
        "target=7&attribute=cpuOutput",
        "target=ship",
    ] {
        let (status, body) = get(&app, &format!("/api/fittings/{id}/explain?{query}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {body}");
    }

    let app = test_app(false);
    let id = saved_fit(&app).await;
    let (status, _) = get(
        &app,
        &format!("/api/fittings/{id}/explain?target=ship&attribute=cpuOutput"),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}
