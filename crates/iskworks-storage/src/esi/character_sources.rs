//! `PgEsiRepository` as the character-read ports application read models
//! use (`iskworks_core::character_sources`).

use async_trait::async_trait;
use iskworks_core::esi_scope::{MARKET_ORDERS_SCOPE, PLANETS_SCOPE, SKILLS_SCOPE};
use iskworks_core::market_orders::accounting_level;
use iskworks_core::{
    CharacterConnectionReader, CharacterSourceKind, ConnectedCharacter, ConnectedCharacterId,
    MarketRefreshState, PlanetaryCharacter, PlanetaryCharacterSource, SkillsCharacter,
    SkillsCharacterSource, StoreError, TraderCharacter, TraderCharacterSource, WorkspaceId,
};

use super::PgEsiRepository;

#[async_trait]
impl TraderCharacterSource for PgEsiRepository {
    async fn trader_characters(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<TraderCharacter>, StoreError> {
        let mut characters = Vec::new();
        for connection in self.list_connections(workspace_id).await? {
            if connection.disconnected_at.is_some() {
                continue;
            }
            let sources = self.character_source_state(connection.id).await?;
            let source = |kind| sources.iter().find(|source| source.source_kind == kind);
            let orders = source(CharacterSourceKind::MarketOrders);
            characters.push(TraderCharacter {
                connection_id: connection.id,
                eve_character_id: connection.eve_character_id,
                character_name: connection.character_name.clone(),
                orders_scope_missing: !connection
                    .granted_scopes
                    .iter()
                    .any(|scope| scope == MARKET_ORDERS_SCOPE),
                orders_synced_at: orders.and_then(|source| source.observed_at),
                orders_sync_error: orders
                    .filter(|source| source.refresh_state == MarketRefreshState::Failed)
                    .and_then(|source| source.last_error.clone()),
                accounting_level: accounting_level(
                    source(CharacterSourceKind::Skills).and_then(|source| source.summary.as_ref()),
                ),
            });
        }
        characters.sort_by(|a, b| a.character_name.cmp(&b.character_name));
        Ok(characters)
    }
}

#[async_trait]
impl PlanetaryCharacterSource for PgEsiRepository {
    async fn planetary_characters(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<PlanetaryCharacter>, StoreError> {
        let connections = self.list_connections(workspace_id).await?;
        let mut characters = Vec::with_capacity(connections.len());
        for connection in connections
            .into_iter()
            .filter(|connection| connection.disconnected_at.is_none())
        {
            let sources = self.character_source_state(connection.id).await?;
            let find = |kind| {
                sources
                    .iter()
                    .find(|source| source.source_kind == kind)
                    .cloned()
            };
            characters.push(PlanetaryCharacter {
                connection_id: connection.id,
                eve_character_id: connection.eve_character_id,
                name: connection.character_name.clone(),
                scope_granted: connection
                    .granted_scopes
                    .iter()
                    .any(|scope| scope == PLANETS_SCOPE),
                planets_source: find(CharacterSourceKind::Planets),
                skills_summary: find(CharacterSourceKind::Skills).and_then(|state| state.summary),
            });
        }
        Ok(characters)
    }
}

#[async_trait]
impl SkillsCharacterSource for PgEsiRepository {
    async fn skills_character(
        &self,
        workspace_id: WorkspaceId,
        connection_id: ConnectedCharacterId,
    ) -> Result<Option<SkillsCharacter>, StoreError> {
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
        Ok(Some(skills_character(self, connection).await?))
    }

    async fn skills_characters(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<SkillsCharacter>, StoreError> {
        let mut characters = Vec::new();
        for connection in self
            .list_connections(workspace_id)
            .await?
            .into_iter()
            .filter(|connection| connection.disconnected_at.is_none())
        {
            characters.push(skills_character(self, connection).await?);
        }
        Ok(characters)
    }
}

async fn skills_character(
    repository: &PgEsiRepository,
    connection: ConnectedCharacter,
) -> Result<SkillsCharacter, StoreError> {
    let skills = repository
        .character_source_state(connection.id)
        .await?
        .into_iter()
        .find(|source| source.source_kind == CharacterSourceKind::Skills);
    Ok(SkillsCharacter {
        connection_id: connection.id,
        name: connection.character_name,
        skills_scope_granted: connection
            .granted_scopes
            .iter()
            .any(|scope| scope == SKILLS_SCOPE),
        observed_at: skills.as_ref().and_then(|source| source.observed_at),
        skills_summary: skills.and_then(|source| source.summary),
    })
}
