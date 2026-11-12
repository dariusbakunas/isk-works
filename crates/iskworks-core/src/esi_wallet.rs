//! What a wallet sync writes, and the shared EVE entity-name cache, as
//! ports. Storage implements both for `PgEsiRepository`.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

use crate::{
    ConnectedCharacterId, EsiSyncRun, EveEntityName, StoreError, SyncCompletion,
    WalletJournalObservation, WalletTransactionObservation, WorkspaceId,
};

#[async_trait]
pub trait EsiWalletRepository: Send + Sync {
    /// Stores a run's wallet transactions, checkpoints the ETag, and
    /// completes the run.
    async fn complete_wallet(
        &self,
        run: &EsiSyncRun,
        observations: &[WalletTransactionObservation],
        etag: Option<&str>,
    ) -> Result<SyncCompletion, StoreError>;

    /// Insert journal entries not seen before; existing entries are left
    /// untouched (journal rows are immutable). Returns how many were new.
    async fn save_wallet_journal(
        &self,
        connection_id: ConnectedCharacterId,
        entries: &[WalletJournalObservation],
    ) -> Result<u64, StoreError>;

    async fn save_wallet_balance(
        &self,
        run: &EsiSyncRun,
        balance: Decimal,
        observed_at: DateTime<Utc>,
        source_checksum: &str,
    ) -> Result<(), StoreError>;

    /// Wallet transaction clients of the workspace's connected characters
    /// that have no cached name and no recent miss.
    async fn unresolved_wallet_client_ids(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<i64>, StoreError>;
}

/// Names of EVE entities (characters, corporations, systems, …) by id,
/// shared by every workspace.
#[async_trait]
pub trait EntityNameRepository: Send + Sync {
    /// Whatever names are already cached for the given ids. A pure read:
    /// unknown ids are simply absent from the result, not resolved via ESI.
    async fn entity_names(&self, ids: &[i64]) -> Result<HashMap<i64, String>, StoreError>;

    async fn cache_entity_names(&self, names: &[EveEntityName]) -> Result<(), StoreError>;

    /// Records ids `/universe/names` could not name, so wallet enrichment
    /// skips them for a week instead of resending them every sync.
    async fn record_entity_name_misses(&self, ids: &[i64]) -> Result<(), StoreError>;
}
