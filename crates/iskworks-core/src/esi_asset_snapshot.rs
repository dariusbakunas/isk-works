//! What an asset sync writes, as ports: a connection's asset snapshots and
//! blueprints, and the player-structure names its assets point at. Storage
//! implements both for `PgEsiRepository`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{
    AssetObservation, BlueprintAssetObservation, ConnectedCharacter, EsiSyncRun, StoreError,
    StructureInformation,
};

/// How many records a completed sync imported, and how many it found
/// unchanged.
#[derive(Debug, Clone, Copy, Default)]
pub struct SyncCompletion {
    pub imported: u64,
    pub unchanged: u64,
}

#[async_trait]
pub trait EsiAssetSnapshotRepository: Send + Sync {
    /// Stores every page of a run's assets as the connection's new active
    /// snapshot (replacing the previous one), checkpoints the ETag, and
    /// completes the run.
    async fn complete_assets(
        &self,
        run: &EsiSyncRun,
        pages: u32,
        observations: &[AssetObservation],
        etag: Option<&str>,
        skipped_blueprints: u64,
    ) -> Result<SyncCompletion, StoreError>;

    /// One durable row per `(workspace_id, owner_id, eve_item_id)`: upsert
    /// every blueprint ESI reports and delete the rows for any this
    /// connection no longer reports (sold / moved / consumed). The row `id`
    /// is preserved across syncs, so a Build's frozen
    /// `blueprintSelection.observationId` keeps resolving; `observed_at`
    /// tracks the last confirming sync while `imported_at` stays the
    /// first-seen time. The full blueprint list is always present when this
    /// runs -- the asset sync fetches every page before calling it and
    /// propagates any page error first -- so an absent blueprint is a real
    /// disappearance, not a partial sync.
    ///
    /// `retained_item_ids` are blueprints ESI did report but the caller
    /// could not use (e.g. out-of-range ME/TE): their existing rows are kept
    /// as they were rather than deleted as gone.
    async fn complete_blueprints(
        &self,
        connection: &ConnectedCharacter,
        observations: &[BlueprintAssetObservation],
        retained_item_ids: &[i64],
    ) -> Result<(), StoreError>;

    /// Stores the pages fetched so far as an inactive, incomplete snapshot
    /// and fails the run, so the previous complete snapshot stays active.
    async fn mark_incomplete_asset_sync(
        &self,
        run: &EsiSyncRun,
        pages: u32,
        observations: &[AssetObservation],
        summary: &str,
    ) -> Result<(), StoreError>;
}

#[async_trait]
pub trait StructureNameRepository: Send + Sync {
    /// Player-structure ids the connection's blueprints or orphaned assets
    /// sit in that have no workspace name yet.
    async fn unresolved_structure_ids(
        &self,
        connection: &ConnectedCharacter,
    ) -> Result<Vec<i64>, StoreError>;

    /// Which of `structure_ids` this character should look up now: those
    /// without a workspace name resolved since `stale_before` (pass `None`
    /// to ignore names entirely) and not denied to this character as of
    /// `now`.
    async fn structures_to_look_up(
        &self,
        connection: &ConnectedCharacter,
        structure_ids: &[i64],
        stale_before: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<Vec<i64>, StoreError>;

    /// Stores looked-up structure names for the connection's workspace.
    async fn cache_structure_names(
        &self,
        connection: &ConnectedCharacter,
        structures: &[StructureInformation],
    ) -> Result<(), StoreError>;

    /// Remembers that this character can't look these structures up (ESI
    /// answered 403 or 404) until `denied_until`.
    async fn record_structure_lookup_denials(
        &self,
        connection: &ConnectedCharacter,
        structure_ids: &[i64],
        denied_until: DateTime<Utc>,
    ) -> Result<(), StoreError>;
}
