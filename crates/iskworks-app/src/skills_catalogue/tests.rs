use std::sync::Arc;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use iskworks_core::skills::SkillState;
use iskworks_core::{ConnectedCharacterId, InventoryError, WorkspaceId};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, ManufacturingRecipe, RequiredSkill, SdeError,
    SdeReadRepository, SkillAttribute, SkillDefinition, TypeSearchResult,
};
use serde_json::json;
use uuid::Uuid;

use super::*;

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

struct FakeCharacters(Vec<(WorkspaceId, SkillsCharacter)>);

#[async_trait]
impl SkillsCharacterSource for FakeCharacters {
    async fn skills_character(
        &self,
        workspace_id: WorkspaceId,
        connection_id: ConnectedCharacterId,
    ) -> Result<Option<SkillsCharacter>, InventoryError> {
        Ok(self
            .0
            .iter()
            .find(|(workspace, character)| {
                *workspace == workspace_id && character.connection_id == connection_id
            })
            .map(|(_, character)| character.clone()))
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

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 25, 12, 0, 0).unwrap()
}

fn character(summary: Option<serde_json::Value>) -> SkillsCharacter {
    SkillsCharacter {
        connection_id: ConnectedCharacterId(Uuid::new_v4()),
        name: "Aura Valex".to_string(),
        skills_scope_granted: true,
        skills_summary: summary,
        observed_at: Some(now()),
    }
}

fn service(
    skills: Vec<SkillDefinition>,
    characters: Vec<(WorkspaceId, SkillsCharacter)>,
) -> SkillCatalogueService {
    SkillCatalogueService::new(
        Arc::new(FakeCharacters(characters)),
        Arc::new(FakeSde { skills }),
    )
}

fn summary() -> serde_json::Value {
    json!({
        "total_sp": 112_822_873,
        "unallocated_sp": 25_000,
        "skills": [
            { "skill_id": DRONES, "active_skill_level": 5, "trained_skill_level": 5,
              "skillpoints_in_skill": 256_000 }
        ],
        "attributes": { "charisma": 19, "intelligence": 20, "memory": 27,
                        "perception": 27, "willpower": 17 }
    })
}

#[tokio::test]
async fn catalogue_for_a_character_overlays_its_skills() {
    let workspace = WorkspaceId::new();
    let aura = character(Some(summary()));
    let service = service(definitions(), vec![(workspace, aura.clone())]);

    let response = service
        .catalogue(workspace, Some(aura.connection_id), now())
        .await
        .unwrap()
        .expect("known character");

    assert!(response.skill_data_available);
    let header = response.character.expect("character header");
    assert_eq!(header.name, "Aura Valex");
    assert_eq!(header.total_sp, Some(112_822_873));
    assert_eq!(header.unallocated_sp, Some(25_000));
    assert!(!header.alpha_detected);
    assert!(!header.skills_scope_missing);
    assert_eq!(header.attributes.map(|a| a.memory), Some(27));
    let state = |type_id| {
        response
            .catalogue
            .skills
            .iter()
            .find(|skill| skill.definition.type_id == type_id)
            .and_then(|skill| skill.progress.as_ref())
            .map(|progress| progress.state)
    };
    assert_eq!(state(DRONES), Some(SkillState::Max));
    assert_eq!(state(DRONE_INTERFACING), Some(SkillState::CanInject));
}

#[tokio::test]
async fn without_a_character_the_catalogue_has_no_overlay() {
    let response = service(definitions(), Vec::new())
        .catalogue(WorkspaceId::new(), None, now())
        .await
        .unwrap()
        .unwrap();

    assert!(response.character.is_none());
    assert_eq!(response.catalogue.skills.len(), 2);
    assert!(response
        .catalogue
        .skills
        .iter()
        .all(|skill| skill.progress.is_none()));
}

#[tokio::test]
async fn an_unknown_or_other_workspace_character_is_not_found() {
    let aura = character(Some(summary()));
    let service = service(definitions(), vec![(WorkspaceId::new(), aura.clone())]);

    let response = service
        .catalogue(WorkspaceId::new(), Some(aura.connection_id), now())
        .await
        .unwrap();

    assert!(response.is_none());
}

#[tokio::test]
async fn a_character_without_skills_data_still_gets_a_header() {
    let workspace = WorkspaceId::new();
    let mut aura = character(None);
    aura.skills_scope_granted = false;
    let service = service(definitions(), vec![(workspace, aura.clone())]);

    let response = service
        .catalogue(workspace, Some(aura.connection_id), now())
        .await
        .unwrap()
        .unwrap();

    let header = response.character.unwrap();
    assert!(header.skills_scope_missing);
    assert_eq!(header.total_sp, None);
    assert!(header.rate.is_none());
    assert!(response
        .catalogue
        .skills
        .iter()
        .all(|skill| skill.progress.is_none()));
}

#[tokio::test]
async fn an_sde_without_skill_dogma_reports_no_skill_data() {
    let response = service(Vec::new(), Vec::new())
        .catalogue(WorkspaceId::new(), None, now())
        .await
        .unwrap()
        .unwrap();

    assert!(!response.skill_data_available);
    assert!(response.catalogue.groups.is_empty());
}
