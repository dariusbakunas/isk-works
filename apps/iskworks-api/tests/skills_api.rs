use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::Utc;
use iskworks_api::{build_router, AppState};
use iskworks_app::{SkillsCharacter, SkillsCharacterSource};
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

const DRONES: i64 = 3_436;
const DRONE_INTERFACING: i64 = 3_442;

struct FakeSde {
    skills: Vec<SkillDefinition>,
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
        Ok(self.skills.clone())
    }
}

/// The test router has one workspace, so the fake matches on the
/// connection alone.
struct FakeCharacters(Vec<SkillsCharacter>);

#[async_trait]
impl SkillsCharacterSource for FakeCharacters {
    async fn skills_character(
        &self,
        _workspace_id: WorkspaceId,
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
        _workspace_id: WorkspaceId,
    ) -> Result<Vec<SkillsCharacter>, InventoryError> {
        unimplemented!("the catalogue reads one character")
    }
}

fn definitions() -> Vec<SkillDefinition> {
    let skill = |type_id, name: &str, rank, prerequisites| SkillDefinition {
        type_id,
        name: name.to_string(),
        group_id: 273,
        group_name: "Drones".to_string(),
        rank,
        primary_attribute: SkillAttribute::Memory,
        secondary_attribute: SkillAttribute::Perception,
        omega_only: false,
        prerequisites,
    };
    vec![
        skill(
            DRONE_INTERFACING,
            "Drone Interfacing",
            5,
            vec![RequiredSkill {
                skill_type_id: DRONES,
                level: 5,
            }],
        ),
        skill(DRONES, "Drones", 1, Vec::new()),
    ]
}

fn aura() -> SkillsCharacter {
    SkillsCharacter {
        connection_id: ConnectedCharacterId(Uuid::new_v4()),
        name: "Aura Valex".to_string(),
        skills_scope_granted: true,
        observed_at: Some(Utc::now()),
        skills_summary: Some(json!({
            "total_sp": 112_822_873,
            "unallocated_sp": 25_000,
            "skills": [
                { "skill_id": DRONES, "active_skill_level": 5, "trained_skill_level": 5,
                  "skillpoints_in_skill": 256_000 },
                { "skill_id": DRONE_INTERFACING, "active_skill_level": 3,
                  "trained_skill_level": 3, "skillpoints_in_skill": 40_000 }
            ],
            "attributes": { "charisma": 19, "intelligence": 20, "memory": 27,
                            "perception": 27, "willpower": 17 }
        })),
    }
}

fn app(skills: Vec<SkillDefinition>, characters: Vec<SkillsCharacter>) -> axum::Router {
    build_router(
        AppState::new(Arc::new(configured_workspace("Skills Test")))
            .with_sde_repository(Arc::new(FakeSde { skills }))
            .with_skills_characters(Arc::new(FakeCharacters(characters))),
    )
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

#[tokio::test]
async fn catalogue_for_a_character_is_camel_case_with_progress() {
    let aura = aura();
    let app = app(definitions(), vec![aura.clone()]);

    let (status, body) = get(
        &app,
        &format!("/api/skills/catalogue?characterId={}", aura.connection_id.0),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["skillDataAvailable"], true);
    assert_eq!(body["character"]["name"], "Aura Valex");
    assert_eq!(body["character"]["totalSp"], 112_822_873);
    assert_eq!(body["character"]["unallocatedSp"], 25_000);
    assert_eq!(body["character"]["alphaDetected"], false);
    assert_eq!(body["character"]["rate"]["basis"]["kind"], "attributes");
    assert_eq!(
        body["groups"],
        json!([{
            "groupId": 273, "name": "Drones", "skillCount": 2,
            "maxSp": 1_280_000 + 256_000, "injectedCount": 2,
            "trainedSp": 256_000 + 40_000
        }])
    );
    let interfacing = &body["skills"][0];
    assert_eq!(interfacing["typeId"], DRONE_INTERFACING);
    assert_eq!(interfacing["groupName"], "Drones");
    assert_eq!(interfacing["primaryAttribute"], "memory");
    assert_eq!(
        interfacing["prerequisites"],
        json!([{ "skillTypeId": DRONES, "level": 5 }])
    );
    assert_eq!(
        interfacing["spPerLevel"],
        json!([1_250, 7_072, 40_000, 226_275, 1_280_000])
    );
    assert_eq!(interfacing["progress"]["state"], "canTrain");
    assert_eq!(interfacing["progress"]["trainedLevel"], 3);
    // (226,275 - 40,000) SP at 27 + 27/2 = 40.5 SP/min, rounded up.
    assert_eq!(interfacing["progress"]["secondsToNextLevel"], 275_963);
    assert_eq!(interfacing["progress"]["estimated"], false);
}

#[tokio::test]
async fn catalogue_without_a_character_has_no_overlay() {
    let (status, body) = get(&app(definitions(), Vec::new()), "/api/skills/catalogue").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["character"], Value::Null);
    assert_eq!(body["skills"][1]["progress"], Value::Null);
    assert_eq!(body["groups"][0]["injectedCount"], Value::Null);
}

#[tokio::test]
async fn an_unknown_character_is_not_found() {
    let (status, _) = get(
        &app(definitions(), vec![aura()]),
        &format!("/api/skills/catalogue?characterId={}", Uuid::new_v4()),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_sde_without_skills_reports_no_skill_data() {
    let (status, body) = get(&app(Vec::new(), Vec::new()), "/api/skills/catalogue").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["skillDataAvailable"], false);
    assert_eq!(body["skills"], json!([]));
}
