//! A connection's ESI sync runs and their per-endpoint ETag checkpoints, as
//! a port: the sync flows record progress through it, and storage
//! implements it for `PgEsiRepository`.

use async_trait::async_trait;

use crate::{
    ConnectedCharacter, ConnectedCharacterId, EsiResponseMetadata, EsiSyncKind, EsiSyncRun,
    EsiSyncRunId, InventoryError, WorkspaceId,
};

#[async_trait]
pub trait EsiSyncRunRepository: Send + Sync {
    /// Records a new running sync of `kind` for the connection.
    async fn start_sync(
        &self,
        connection: &ConnectedCharacter,
        kind: EsiSyncKind,
    ) -> Result<EsiSyncRun, InventoryError>;

    /// Marks the run failed with an error code and summary.
    async fn fail_sync(
        &self,
        run_id: EsiSyncRunId,
        code: &str,
        summary: &str,
    ) -> Result<EsiSyncRun, InventoryError>;

    /// Stores the response headers (cache expiry, rate limits) ESI sent.
    async fn record_sync_metadata(
        &self,
        run_id: EsiSyncRunId,
        metadata: &EsiResponseMetadata,
    ) -> Result<(), InventoryError>;

    /// The ETag of the endpoint's last stored response, if any.
    async fn checkpoint_etag(
        &self,
        connection_id: ConnectedCharacterId,
        endpoint_kind: &str,
    ) -> Result<Option<String>, InventoryError>;

    /// Completes a run whose response was unchanged (ESI answered 304).
    async fn complete_unchanged_sync(
        &self,
        run: &EsiSyncRun,
        endpoint_kind: &str,
        summary: &str,
    ) -> Result<EsiSyncRun, InventoryError>;

    async fn get_sync_run(&self, id: EsiSyncRunId) -> Result<EsiSyncRun, InventoryError>;

    /// The workspace's 50 most recent runs, newest first.
    async fn list_sync_runs(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<EsiSyncRun>, InventoryError>;
}
