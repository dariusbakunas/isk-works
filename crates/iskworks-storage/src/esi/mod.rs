use chrono::{DateTime, Utc};
use iskworks_core::{
    CharacterSourceKind, CharacterSourceSyncState, ConnectedCharacter, ConnectedCharacterId,
    ConnectionStatus, EsiSyncKind, EsiSyncRun, EsiSyncRunId, EsiSyncStatus, InventoryError,
    InventoryEventId, InventoryPosting, MarketRefreshState, OwnerId, WorkspaceId,
};
use iskworks_esi::{
    AssetObservation, EncryptedSecret, EveEntityName, StructureInformation,
    WalletJournalObservation, WalletTransactionObservation,
};
use rust_decimal::Decimal;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use crate::inventory::PgInventoryRepository;

mod assets;
mod character_sync;
mod connections;
mod error;
mod planetary;
mod retention;
mod sync_runs;
mod wallet;
mod wallet_recording;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod wallet_recording_tests;

use error::*;
pub use retention::EsiObservationPruneOutcome;

#[derive(Clone)]
pub struct PgEsiRepository {
    pool: PgPool,
}

#[derive(Debug, Clone)]
pub struct PendingAuthorization {
    pub workspace_id: WorkspaceId,
    pub owner_id: OwnerId,
    pub verifier: EncryptedSecret,
    pub requested_scopes: Vec<String>,
    pub return_path: String,
}

#[derive(Debug, Clone)]
pub struct StoredRefreshToken {
    pub connection: ConnectedCharacter,
    pub envelope: EncryptedSecret,
    /// The current access token, valid until
    /// `connection.access_token_expires_at`. `None` until the first refresh.
    pub access_token: Option<EncryptedSecret>,
    pub token_revision: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SyncCompletion {
    pub imported: u64,
    pub unchanged: u64,
}

/// A best-effort display label for an industry-job facility id, assembled
/// from whatever this workspace has already resolved -- the player-structure
/// name cache (`market_location_names`) and the SDE NPC-station catalogue.
/// Nothing here triggers a live ESI lookup, so an unresolved player
/// structure simply yields no entry.
#[derive(Debug, Clone)]
pub struct FacilityLabel {
    pub name: String,
    pub solar_system_name: Option<String>,
}

impl PgEsiRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
