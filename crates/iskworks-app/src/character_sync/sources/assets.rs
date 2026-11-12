//! Assets, through the ESI application service's pipeline.

use iskworks_core::ConnectedCharacter;
use serde_json::Value;

use crate::character_sync::{CharacterSyncService, SourceFailure};

impl CharacterSyncService {
    /// Delegates to `esi_sync` (backed by `EsiApplicationService::sync_assets`
    /// in production) rather than calling `self.transport` directly like the
    /// sources above -- assets need the full paginate/ETag-checkpoint/
    /// persist-to-`esi_asset_observations` pipeline that method already
    /// implements and tests, not a single-response fetch. The
    /// `character_source_sync_state` row this populates is a health/staleness
    /// signal for the roster only; the actual holdings data lives in
    /// `esi_asset_snapshots`/`esi_asset_observations`, written by that
    /// pipeline directly.
    pub(in crate::character_sync) async fn fetch_assets_summary(
        &self,
        connection: &ConnectedCharacter,
        token: &str,
    ) -> Result<Value, SourceFailure> {
        let run = self
            .esi_sync
            .sync_assets(connection, token)
            .await
            .map_err(SourceFailure::from)?;
        serde_json::to_value(&run).map_err(SourceFailure::from)
    }
}
