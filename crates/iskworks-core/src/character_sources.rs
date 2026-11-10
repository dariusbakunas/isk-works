//! The character reads application read models need from a workspace's
//! connections, as ports: storage implements them for `PgEsiRepository`.
//! The `*CharacterSource` ports return connected (not disconnected)
//! characters only.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::{
    CharacterSourceSyncState, ConnectedCharacter, ConnectedCharacterId, InventoryError, WorkspaceId,
};

/// A workspace's connections and each one's synced sources.
#[async_trait]
pub trait CharacterConnectionReader: Send + Sync {
    /// Every connection in the workspace, disconnected ones included.
    async fn list_connections(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<ConnectedCharacter>, InventoryError>;

    /// The sync state of each of a connection's sources.
    async fn character_source_state(
        &self,
        connection_id: ConnectedCharacterId,
    ) -> Result<Vec<CharacterSourceSyncState>, InventoryError>;
}

/// A workspace character as the Market Orders page needs it.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraderCharacter {
    pub connection_id: ConnectedCharacterId,
    pub eve_character_id: i64,
    pub character_name: String,
    /// The connection hasn't granted `esi-markets.read_character_orders.v1`.
    pub orders_scope_missing: bool,
    pub orders_synced_at: Option<DateTime<Utc>>,
    pub orders_sync_error: Option<String>,
    /// Trained Accounting level, when the skills source has synced.
    pub accounting_level: Option<u8>,
}

#[async_trait]
pub trait TraderCharacterSource: Send + Sync {
    async fn trader_characters(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<TraderCharacter>, InventoryError>;
}

/// One connected character's PI-relevant sync state.
#[derive(Debug, Clone)]
pub struct PlanetaryCharacter {
    pub connection_id: ConnectedCharacterId,
    pub eve_character_id: i64,
    pub name: String,
    pub scope_granted: bool,
    pub planets_source: Option<CharacterSourceSyncState>,
    pub skills_summary: Option<Value>,
}

#[async_trait]
pub trait PlanetaryCharacterSource: Send + Sync {
    /// Connected (not disconnected) characters in the workspace.
    async fn planetary_characters(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<PlanetaryCharacter>, InventoryError>;
}

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

    /// Every connected (not disconnected) character in the workspace.
    async fn skills_characters(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<SkillsCharacter>, InventoryError>;
}
