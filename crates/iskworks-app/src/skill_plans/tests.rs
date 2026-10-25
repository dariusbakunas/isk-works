use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use iskworks_core::skill_plans::{
    PlanEntry, SkillPlan, SkillPlanId, SkillPlanInput, SkillPlanRepository,
};
use iskworks_core::{ConnectedCharacterId, InventoryError, WorkspaceId};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, ManufacturingRecipe, RequiredSkill, SdeError,
    SdeReadRepository, SkillAttribute, SkillDefinition, TypeSearchResult,
};
use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::{SkillsCharacter, SkillsCharacterSource};

const DRONES: i64 = 3_436;
const SCOUT_DRONES: i64 = 3_437;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 25, 12, 0, 0).unwrap()
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

#[derive(Default)]
struct FakePlans(Mutex<HashMap<SkillPlanId, (WorkspaceId, SkillPlan)>>);

#[async_trait]
impl SkillPlanRepository for FakePlans {
    async fn list_skill_plans(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<SkillPlan>, InventoryError> {
        let mut plans = self
            .0
            .lock()
            .unwrap()
            .values()
            .filter(|(workspace, _)| *workspace == workspace_id)
            .map(|(_, plan)| plan.clone())
            .collect::<Vec<_>>();
        plans.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(plans)
    }
    async fn skill_plan(
        &self,
        workspace_id: WorkspaceId,
        id: SkillPlanId,
    ) -> Result<Option<SkillPlan>, InventoryError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(&id)
            .filter(|(workspace, _)| *workspace == workspace_id)
            .map(|(_, plan)| plan.clone()))
    }
    async fn create_skill_plan(
        &self,
        workspace_id: WorkspaceId,
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
        self.0
            .lock()
            .unwrap()
            .insert(plan.id, (workspace_id, plan.clone()));
        Ok(plan)
    }
    async fn update_skill_plan(
        &self,
        workspace_id: WorkspaceId,
        id: SkillPlanId,
        input: &SkillPlanInput,
        now: DateTime<Utc>,
    ) -> Result<Option<SkillPlan>, InventoryError> {
        let mut plans = self.0.lock().unwrap();
        let Some((workspace, plan)) = plans.get_mut(&id) else {
            return Ok(None);
        };
        if *workspace != workspace_id {
            return Ok(None);
        }
        plan.name = input.name.clone();
        plan.description = input.description.clone();
        plan.entries = input.entries.clone();
        plan.updated_at = now;
        Ok(Some(plan.clone()))
    }
    async fn delete_skill_plan(
        &self,
        workspace_id: WorkspaceId,
        id: SkillPlanId,
    ) -> Result<bool, InventoryError> {
        let mut plans = self.0.lock().unwrap();
        if plans
            .get(&id)
            .is_some_and(|(workspace, _)| *workspace == workspace_id)
        {
            plans.remove(&id);
            return Ok(true);
        }
        Ok(false)
    }
}

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
        Ok(self.0.clone())
    }
}

fn character(name: &str, drones_level: i64) -> SkillsCharacter {
    let sp = [0, 250, 1_415, 8_000, 45_255, 256_000][drones_level as usize];
    let skills = if drones_level == 0 {
        json!([])
    } else {
        json!([{ "skill_id": DRONES, "active_skill_level": drones_level,
                 "trained_skill_level": drones_level, "skillpoints_in_skill": sp }])
    };
    SkillsCharacter {
        connection_id: ConnectedCharacterId(Uuid::new_v4()),
        name: name.to_string(),
        skills_scope_granted: true,
        observed_at: Some(now()),
        skills_summary: Some(json!({
            "total_sp": sp,
            "skills": skills,
            "attributes": { "charisma": 20, "intelligence": 20, "memory": 20,
                            "perception": 20, "willpower": 20 }
        })),
    }
}

fn entry(skill_type_id: i64, level: i16) -> PlanEntry {
    PlanEntry {
        skill_type_id,
        level,
    }
}

fn input(name: &str, entries: Vec<PlanEntry>) -> SkillPlanInput {
    SkillPlanInput {
        name: name.to_string(),
        description: String::new(),
        entries,
    }
}

fn service(characters: Vec<SkillsCharacter>) -> SkillPlanService {
    SkillPlanService::new(
        Arc::new(FakePlans::default()),
        Arc::new(FakeCharacters(characters)),
        Arc::new(FakeSde),
    )
}

#[tokio::test]
async fn creating_a_plan_inserts_prerequisites_and_names_entries() {
    let service = service(vec![character("Aura", 0)]);
    let workspace = WorkspaceId::new();

    let created = service
        .create(
            workspace,
            input("  Scouts  ", vec![entry(SCOUT_DRONES, 1)]),
            now(),
        )
        .await
        .unwrap();

    assert_eq!(created.plan.name, "Scouts");
    assert_eq!(created.inserted, vec![entry(DRONES, 1)]);
    let names = created
        .entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry.level))
        .collect::<Vec<_>>();
    assert_eq!(names, vec![("Drones", 1), ("Scout Drone Operation", 1)]);
    assert_eq!(created.entries[0].sp, 250);
}

#[tokio::test]
async fn a_blank_name_or_broken_order_is_a_validation_error() {
    let service = service(Vec::new());
    let workspace = WorkspaceId::new();

    let blank = service
        .create(workspace, input("   ", vec![entry(DRONES, 1)]), now())
        .await
        .unwrap_err();
    assert!(matches!(blank, InventoryError::Validation(_)));

    let order = service
        .create(
            workspace,
            input("Bad", vec![entry(SCOUT_DRONES, 1), entry(DRONES, 1)]),
            now(),
        )
        .await
        .unwrap_err();
    match order {
        InventoryError::Validation(message) => {
            assert_eq!(message, "Drones I must come before Scout Drone Operation I")
        }
        other => panic!("expected a validation error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_plan_shows_every_character_closest_first() {
    let far = character("Fresh", 0);
    let close = character("Veteran", 3);
    let service = service(vec![far.clone(), close.clone()]);
    let workspace = WorkspaceId::new();
    let plan = service
        .create(
            workspace,
            input("Drones", vec![entry(DRONES, 4), entry(SCOUT_DRONES, 1)]),
            now(),
        )
        .await
        .unwrap();

    let detail = service
        .get(workspace, plan.plan.id, now())
        .await
        .unwrap()
        .unwrap();

    let order = detail
        .characters
        .iter()
        .map(|character| character.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(order, vec!["Veteran", "Fresh"]);
    let veteran = detail.characters[0].evaluation.as_ref().unwrap();
    assert_eq!(veteran.missing_skillbooks, vec![SCOUT_DRONES]);
    assert!(veteran.percent_complete > 0.0);
}

#[tokio::test]
async fn listing_plans_summarizes_progress_for_one_character() {
    let veteran = character("Veteran", 5);
    let service = service(vec![veteran.clone()]);
    let workspace = WorkspaceId::new();
    service
        .create(workspace, input("Drones", vec![entry(DRONES, 5)]), now())
        .await
        .unwrap();

    let plans = service
        .list(workspace, Some(veteran.connection_id), now())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].entry_count, 5);
    assert_eq!(plans[0].total_sp, 256_000);
    let progress = plans[0].progress.as_ref().unwrap();
    assert!((progress.percent_complete - 100.0).abs() < f64::EPSILON);
    assert_eq!(progress.remaining_seconds, Some(0));

    let unknown = service
        .list(workspace, Some(ConnectedCharacterId(Uuid::new_v4())), now())
        .await
        .unwrap();
    assert!(unknown.is_none());
}

#[tokio::test]
async fn updating_and_deleting_are_workspace_scoped() {
    let service = service(Vec::new());
    let workspace = WorkspaceId::new();
    let created = service
        .create(workspace, input("Drones", vec![entry(DRONES, 1)]), now())
        .await
        .unwrap();

    let updated = service
        .update(
            workspace,
            created.plan.id,
            input("Drones II", vec![entry(DRONES, 2)]),
            now(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.plan.name, "Drones II");
    assert_eq!(updated.inserted, vec![entry(DRONES, 1)]);

    assert!(service
        .update(
            WorkspaceId::new(),
            created.plan.id,
            input("X", vec![]),
            now()
        )
        .await
        .unwrap()
        .is_none());
    assert!(!service
        .delete(WorkspaceId::new(), created.plan.id)
        .await
        .unwrap());
    assert!(service.delete(workspace, created.plan.id).await.unwrap());
}

// --- Text import / export ---------------------------------------------------

#[tokio::test]
async fn previewing_pasted_text_normalizes_and_reports_unusable_lines() {
    let preview = service(Vec::new())
        .preview_text("Scout Drone Operation 1\nBogus Skill 2")
        .await
        .unwrap();

    let names = preview
        .entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry.level))
        .collect::<Vec<_>>();
    assert_eq!(names, vec![("Drones", 1), ("Scout Drone Operation", 1)]);
    assert_eq!(preview.inserted, vec![entry(DRONES, 1)]);
    assert_eq!(preview.unrecognized.len(), 1);
    assert_eq!(preview.unrecognized[0].line, 2);
    assert_eq!(preview.error, None);
}

#[tokio::test]
async fn previewing_out_of_order_text_explains_the_problem() {
    let preview = service(Vec::new())
        .preview_text("Scout Drone Operation 1\nDrones 1")
        .await
        .unwrap();

    assert_eq!(
        preview.error.as_deref(),
        Some("Drones I must come before Scout Drone Operation I")
    );
    assert_eq!(preview.entries.len(), 2);
    assert!(preview.inserted.is_empty());
}

#[tokio::test]
async fn importing_text_creates_the_plan_and_keeps_the_unusable_lines() {
    let service = service(Vec::new());
    let workspace = WorkspaceId::new();

    let imported = service
        .import_text(
            workspace,
            "Scouts".to_string(),
            String::new(),
            "Scout Drone Operation 1\nBogus Skill 2",
            now(),
        )
        .await
        .unwrap();

    assert_eq!(imported.detail.plan.name, "Scouts");
    assert_eq!(imported.detail.entries.len(), 2);
    assert_eq!(imported.unrecognized.len(), 1);

    let nothing = service
        .import_text(
            workspace,
            "Empty".to_string(),
            String::new(),
            "Bogus 1\n\n",
            now(),
        )
        .await
        .unwrap_err();
    assert!(matches!(nothing, InventoryError::Validation(_)));
}

#[tokio::test]
async fn exporting_writes_the_client_text() {
    let service = service(Vec::new());
    let workspace = WorkspaceId::new();
    let created = service
        .create(
            workspace,
            input("Scouts", vec![entry(SCOUT_DRONES, 1)]),
            now(),
        )
        .await
        .unwrap();

    let text = service
        .export_text(workspace, created.plan.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(text, "Drones 1\nScout Drone Operation 1");
    assert!(service
        .export_text(WorkspaceId::new(), created.plan.id)
        .await
        .unwrap()
        .is_none());
}
