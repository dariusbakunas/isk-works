//! The character reads application read models need from a workspace's
//! connections, as ports: storage implements them for `PgEsiRepository`.
//! The `*CharacterSource` ports return connected (not disconnected)
//! characters only.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use std::collections::HashMap;

use crate::calendar_event::CharacterCalendarEvent;
use crate::market_orders::CharacterMarketOrder;
use crate::{
    CharacterIndustryJobObservation, CharacterSourceKind, CharacterSourceSyncState,
    ConnectedCharacter, ConnectedCharacterId, EveEntityName, InventoryError, WorkspaceId,
};

/// A workspace's connections and each one's synced sources.
#[async_trait]
pub trait CharacterConnectionReader: Send + Sync {
    /// Every connection in the workspace, disconnected ones included.
    async fn list_connections(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<ConnectedCharacter>, InventoryError>;

    /// One connection, disconnected or not.
    async fn get_connection(
        &self,
        id: ConnectedCharacterId,
    ) -> Result<ConnectedCharacter, InventoryError>;

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

/// Persistence for `character_source_sync_state`, kept narrow (register,
/// claim, complete, fail) and separate from `EsiTransport`/token concerns so
/// the app's character sync can be driven by fakes in tests, matching
/// `market_esi.rs`'s `MarketRefreshRepository` shape.
#[async_trait]
pub trait CharacterSyncRepository: Send + Sync {
    async fn register_character_sources(
        &self,
        connection_id: ConnectedCharacterId,
    ) -> Result<(), InventoryError>;
    async fn begin_character_source_refresh(
        &self,
        connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, InventoryError>;
    async fn complete_character_source_refresh(
        &self,
        connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        summary: Value,
        observed_at: DateTime<Utc>,
    ) -> Result<bool, InventoryError>;
    async fn fail_character_source_refresh(
        &self,
        connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        claim: DateTime<Utc>,
        attempted_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        error_message: String,
    ) -> Result<bool, InventoryError>;
    /// Pushes every due source of this connection out to `until` -- for a
    /// failure that blocks all of them at once (the token refresh).
    async fn defer_character_sources(
        &self,
        connection_id: ConnectedCharacterId,
        until: DateTime<Utc>,
    ) -> Result<(), InventoryError>;
    /// Whatever names are already cached in `eve_entity_names` for these
    /// IDs -- unresolved IDs are simply absent, never fetched here.
    async fn entity_names(&self, ids: &[i64]) -> Result<HashMap<i64, String>, InventoryError>;
    async fn cache_entity_names(&self, names: &[EveEntityName]) -> Result<(), InventoryError>;
    /// Upserts every job of one jobs sync (running and finished) into the
    /// Jobs history. The source summary keeps only running jobs.
    async fn record_industry_jobs(
        &self,
        connection_id: ConnectedCharacterId,
        jobs: &[CharacterIndustryJobObservation],
        observed_at: DateTime<Utc>,
    ) -> Result<(), InventoryError>;
    /// Every source's current persisted state for this connection --
    /// needed so name resolution can also pick up a corporation/solar-system
    /// id that was fetched on some earlier pass (before this ID's name was
    /// ever cached), not just one freshly fetched in *this* pass. Without
    /// this, a connection whose CharacterInfo/Location is still fresh but
    /// whose names were never cached would wait for its next natural
    /// refresh (up to 24h for CharacterInfo) before ever resolving.
    async fn character_source_state(
        &self,
        connection_id: ConnectedCharacterId,
    ) -> Result<Vec<CharacterSourceSyncState>, InventoryError>;
    /// Stored copies of these calendar events in the workspace, any
    /// connection's (see `CalendarEventRepository`).
    async fn calendar_event_copies(
        &self,
        workspace_id: WorkspaceId,
        event_ids: &[i64],
    ) -> Result<Vec<CharacterCalendarEvent>, InventoryError>;
    /// Makes `events` the connection's whole stored calendar.
    async fn store_calendar_events(
        &self,
        connection_id: ConnectedCharacterId,
        events: &[CharacterCalendarEvent],
    ) -> Result<(), InventoryError>;
    /// Makes `orders` the connection's open market orders, closing the rest
    /// (see `CharacterMarketOrderRepository`).
    async fn store_market_orders(
        &self,
        connection_id: ConnectedCharacterId,
        orders: &[CharacterMarketOrder],
        observed_at: DateTime<Utc>,
    ) -> Result<(), InventoryError>;
}
