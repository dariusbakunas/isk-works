//! Read model for the Skills page: the SDE skill catalogue, overlaid with
//! one connected character's synced skills, queue and attributes through
//! `iskworks_core::skills`. No ESI calls happen here; freshness comes from
//! the character sync loop.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_core::skills::{
    build_catalogue, parse_skills_summary, CharacterAttributes, CloneState, SkillCatalogue,
    TrainingRate,
};
use iskworks_core::{CharacterSourceKind, ConnectedCharacterId, InventoryError, WorkspaceId};
use iskworks_esi::SKILLS_SCOPE;
use iskworks_sde::SdeReadRepository;
use iskworks_storage::PgEsiRepository;
use serde::Serialize;
use serde_json::Value;

/// One connected character's skills source.
#[derive(Debug, Clone)]
pub struct SkillsCharacter {
    pub connection_id: ConnectedCharacterId,
    pub name: String,
    pub skills_scope_granted: bool,
    pub skills_summary: Option<Value>,
    pub observed_at: Option<DateTime<Utc>>,
}

#[async_trait]
pub trait SkillsCharacterSource: Send + Sync {
    /// The character if it is connected (not disconnected) in the workspace.
    async fn skills_character(
        &self,
        workspace_id: WorkspaceId,
        connection_id: ConnectedCharacterId,
    ) -> Result<Option<SkillsCharacter>, InventoryError>;
}

#[async_trait]
impl SkillsCharacterSource for PgEsiRepository {
    async fn skills_character(
        &self,
        workspace_id: WorkspaceId,
        connection_id: ConnectedCharacterId,
    ) -> Result<Option<SkillsCharacter>, InventoryError> {
        let Some(connection) =
            self.list_connections(workspace_id)
                .await?
                .into_iter()
                .find(|connection| {
                    connection.id == connection_id && connection.disconnected_at.is_none()
                })
        else {
            return Ok(None);
        };
        let skills = self
            .character_source_state(connection.id)
            .await?
            .into_iter()
            .find(|source| source.source_kind == CharacterSourceKind::Skills);
        Ok(Some(SkillsCharacter {
            connection_id: connection.id,
            name: connection.character_name,
            skills_scope_granted: connection
                .granted_scopes
                .iter()
                .any(|scope| scope == SKILLS_SCOPE),
            observed_at: skills.as_ref().and_then(|source| source.observed_at),
            skills_summary: skills.and_then(|source| source.summary),
        }))
    }
}

/// The selected character, for the page header.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCatalogueCharacter {
    pub connection_id: ConnectedCharacterId,
    pub name: String,
    pub skills_scope_missing: bool,
    pub observed_at: Option<DateTime<Utc>>,
    pub total_sp: Option<i64>,
    pub unallocated_sp: Option<i64>,
    pub alpha_detected: bool,
    pub attributes: Option<CharacterAttributes>,
    /// `None` when the character has no readable skills data yet.
    pub rate: Option<TrainingRate>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCatalogueResponse {
    /// False when the active SDE import has no skill dogma (re-import).
    pub skill_data_available: bool,
    pub character: Option<SkillCatalogueCharacter>,
    #[serde(flatten)]
    pub catalogue: SkillCatalogue,
}

#[derive(Clone)]
pub struct SkillCatalogueService {
    characters: Arc<dyn SkillsCharacterSource>,
    sde: Arc<dyn SdeReadRepository>,
}

impl SkillCatalogueService {
    #[must_use]
    pub fn new(
        characters: Arc<dyn SkillsCharacterSource>,
        sde: Arc<dyn SdeReadRepository>,
    ) -> Self {
        Self { characters, sde }
    }

    /// The catalogue, overlaid with `character` when given. `Ok(None)` when
    /// that character isn't connected in the workspace.
    pub async fn catalogue(
        &self,
        workspace_id: WorkspaceId,
        character: Option<ConnectedCharacterId>,
        now: DateTime<Utc>,
    ) -> Result<Option<SkillCatalogueResponse>, InventoryError> {
        let character = match character {
            Some(connection_id) => {
                match self
                    .characters
                    .skills_character(workspace_id, connection_id)
                    .await?
                {
                    Some(character) => Some(character),
                    None => return Ok(None),
                }
            }
            None => None,
        };
        let definitions = self
            .sde
            .skill_catalogue()
            .await
            .map_err(|error| InventoryError::Persistence(error.to_string()))?;
        let snapshot = character.as_ref().and_then(|character| {
            let summary = character.skills_summary.as_ref()?;
            let snapshot = parse_skills_summary(summary);
            if snapshot.is_none() {
                tracing::warn!(connection = %character.connection_id.0, "unreadable skills summary");
            }
            snapshot
        });
        let catalogue = build_catalogue(&definitions, snapshot.as_ref(), now);
        let character = character.map(|character| SkillCatalogueCharacter {
            connection_id: character.connection_id,
            name: character.name,
            skills_scope_missing: !character.skills_scope_granted,
            observed_at: character.observed_at,
            total_sp: snapshot.as_ref().map(|snapshot| snapshot.total_sp),
            unallocated_sp: snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.unallocated_sp),
            alpha_detected: catalogue
                .rate
                .is_some_and(|rate| rate.clone == CloneState::Alpha),
            attributes: snapshot.as_ref().and_then(|snapshot| snapshot.attributes),
            rate: catalogue.rate,
        });
        Ok(Some(SkillCatalogueResponse {
            skill_data_available: !definitions.is_empty(),
            character,
            catalogue,
        }))
    }
}

#[cfg(test)]
mod tests;
