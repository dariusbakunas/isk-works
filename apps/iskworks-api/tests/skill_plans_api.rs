use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use chrono::{DateTime, Utc};
use iskworks_api::{build_router, AppState};
use iskworks_app::{SkillsCharacter, SkillsCharacterSource};
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

const DRONES: i64 = 3_436;
const SCOUT_DRONES: i64 = 3_437;

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
    async fn skill_catalogue(&self) -> Result<Vec<SkillDefinition>, SdeError> {
        let skill = |type_id, name: &str, prerequisites| SkillDefinition {
            type_id,
            name: name.to_string(),
            group_id: 273,
            group_name: "Drones".to_string(),
            rank: 1,
            primary_attribute: SkillAttribute::Memory,
            secondary_attribute: SkillAttribute::Perception,
            omega_only: false,
            prerequisites,
        };
        Ok(vec![
            skill(DRONES, "Drones", Vec::new()),
            skill(
                SCOUT_DRONES,
                "Scout Drone Operation",
                vec![RequiredSkill {
                    skill_type_id: DRONES,
                    level: 1,
                }],
            ),
        ])
    }
}

/// One workspace in the test router, so plans are kept by id only.
#[derive(Default)]
struct FakePlans(Mutex<HashMap<SkillPlanId, SkillPlan>>);

#[async_trait]
impl SkillPlanRepository for FakePlans {
    async fn list_skill_plans(&self, _: WorkspaceId) -> Result<Vec<SkillPlan>, InventoryError> {
        Ok(self.0.lock().unwrap().values().cloned().collect())
    }
    async fn skill_plan(
        &self,
        _: WorkspaceId,
        id: SkillPlanId,
    ) -> Result<Option<SkillPlan>, InventoryError> {
        Ok(self.0.lock().unwrap().get(&id).cloned())
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
        self.0.lock().unwrap().insert(plan.id, plan.clone());
        Ok(plan)
    }
    async fn update_skill_plan(
        &self,
        _: WorkspaceId,
        id: SkillPlanId,
        input: &SkillPlanInput,
        now: DateTime<Utc>,
    ) -> Result<Option<SkillPlan>, InventoryError> {
        let mut plans = self.0.lock().unwrap();
        Ok(plans.get_mut(&id).map(|plan| {
            plan.name = input.name.clone();
            plan.description = input.description.clone();
            plan.entries = input.entries.clone();
            plan.updated_at = now;
            plan.clone()
        }))
    }
    async fn delete_skill_plan(
        &self,
        _: WorkspaceId,
        id: SkillPlanId,
    ) -> Result<bool, InventoryError> {
        Ok(self.0.lock().unwrap().remove(&id).is_some())
    }
}

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

fn aura() -> SkillsCharacter {
    SkillsCharacter {
        connection_id: ConnectedCharacterId(Uuid::new_v4()),
        name: "Aura Valex".to_string(),
        skills_scope_granted: true,
        observed_at: Some(Utc::now()),
        skills_summary: Some(json!({
            "total_sp": 250,
            "skills": [{ "skill_id": DRONES, "active_skill_level": 1,
                         "trained_skill_level": 1, "skillpoints_in_skill": 250 }],
            "attributes": { "charisma": 20, "intelligence": 20, "memory": 20,
                            "perception": 20, "willpower": 20 }
        })),
    }
}

fn app(characters: Vec<SkillsCharacter>) -> axum::Router {
    build_router(
        AppState::new(Arc::new(configured_workspace("Plans Test")))
            .with_sde_repository(Arc::new(FakeSde))
            .with_skills_characters(Arc::new(FakeCharacters(characters)))
            .with_skill_plan_repository(Arc::new(FakePlans::default())),
    )
}

async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let request = Request::builder().method(method).uri(uri);
    let request = match body {
        Some(body) => request
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    }
    .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn scouts() -> Value {
    json!({
        "name": "Scouts",
        "description": "Light drone support",
        "entries": [{ "skillTypeId": SCOUT_DRONES, "level": 1 }]
    })
}

#[tokio::test]
async fn creating_a_plan_returns_its_detail_with_inserted_prerequisites() {
    let aura = aura();
    let app = app(vec![aura.clone()]);

    let (status, body) = send(&app, "POST", "/api/skill-plans", Some(scouts())).await;

    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["plan"]["name"], "Scouts");
    assert_eq!(
        body["inserted"],
        json!([{ "skillTypeId": DRONES, "level": 1 }])
    );
    assert_eq!(body["entries"][0]["name"], "Drones");
    assert_eq!(body["entries"][1]["name"], "Scout Drone Operation");
    let character = &body["characters"][0];
    assert_eq!(character["name"], "Aura Valex");
    assert_eq!(character["evaluation"]["entries"][0]["status"], "done");
    assert_eq!(
        character["evaluation"]["missingSkillbooks"],
        json!([SCOUT_DRONES])
    );
}

#[tokio::test]
async fn plans_list_reads_updates_and_deletes() {
    let aura = aura();
    let app = app(vec![aura.clone()]);
    let (_, created) = send(&app, "POST", "/api/skill-plans", Some(scouts())).await;
    let id = created["plan"]["id"].as_str().unwrap().to_string();

    let (status, list) = send(
        &app,
        "GET",
        &format!("/api/skill-plans?characterId={}", aura.connection_id.0),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(list[0]["entryCount"], 2);
    assert_eq!(list[0]["totalSp"], 500);
    assert_eq!(list[0]["progress"]["missingSkillbooks"], 1);

    let (status, read) = send(&app, "GET", &format!("/api/skill-plans/{id}"), None).await;
    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(read["inserted"], json!([]));

    let (status, updated) = send(
        &app,
        "PUT",
        &format!("/api/skill-plans/{id}"),
        Some(json!({ "name": "Drones", "entries": [{ "skillTypeId": DRONES, "level": 2 }] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["plan"]["name"], "Drones");
    assert_eq!(updated["plan"]["description"], "");

    let (status, _) = send(&app, "DELETE", &format!("/api/skill-plans/{id}"), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = send(&app, "GET", &format!("/api/skill-plans/{id}"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&app, "DELETE", &format!("/api/skill-plans/{id}"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_broken_order_is_rejected_with_the_reason() {
    let app = app(Vec::new());

    let (status, body) = send(
        &app,
        "POST",
        "/api/skill-plans",
        Some(json!({
            "name": "Bad",
            "entries": [
                { "skillTypeId": SCOUT_DRONES, "level": 1 },
                { "skillTypeId": DRONES, "level": 1 }
            ]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body.to_string()
            .contains("Drones I must come before Scout Drone Operation I"),
        "{body}"
    );
}

#[tokio::test]
async fn unknown_plans_and_characters_are_not_found() {
    let app = app(Vec::new());

    let (status, _) = send(
        &app,
        "GET",
        &format!("/api/skill-plans/{}", Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(
        &app,
        "PUT",
        &format!("/api/skill-plans/{}", Uuid::new_v4()),
        Some(scouts()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(
        &app,
        "GET",
        &format!("/api/skill-plans?characterId={}", Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn previewing_pasted_text_saves_nothing() {
    let app = app(Vec::new());

    let (status, preview) = send(
        &app,
        "POST",
        "/api/skill-plans/import/preview",
        Some(json!({ "text": "Scout Drone Operation 1\nArmoured Comand 3" })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["entries"][0]["name"], "Drones");
    assert_eq!(
        preview["inserted"],
        json!([{ "skillTypeId": DRONES, "level": 1 }])
    );
    assert_eq!(
        preview["unrecognized"],
        json!([{ "line": 2, "text": "Armoured Comand 3", "reason": "unknownSkill" }])
    );
    assert_eq!(preview["error"], Value::Null);
    let (_, list) = send(&app, "GET", "/api/skill-plans", None).await;
    assert_eq!(list, json!([]));
}

#[tokio::test]
async fn importing_text_creates_a_plan_that_exports_as_client_text() {
    let app = app(Vec::new());

    let (status, imported) = send(
        &app,
        "POST",
        "/api/skill-plans/import",
        Some(json!({ "name": "Scouts", "text": "Scout Drone Operation 1\nBogus 2" })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{imported}");
    assert_eq!(imported["plan"]["name"], "Scouts");
    assert_eq!(imported["unrecognized"][0]["line"], 2);
    let id = imported["plan"]["id"].as_str().unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::get(format!("/api/skill-plans/{id}/export"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/plain; charset=utf-8"
    );
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&bytes[..], b"Drones 1\nScout Drone Operation 1");

    let (status, _) = send(
        &app,
        "GET",
        &format!("/api/skill-plans/{}/export", Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn importing_text_without_entries_is_rejected() {
    let (status, body) = send(
        &app(Vec::new()),
        "POST",
        "/api/skill-plans/import",
        Some(json!({ "name": "Empty", "text": "nothing useful here" })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}
