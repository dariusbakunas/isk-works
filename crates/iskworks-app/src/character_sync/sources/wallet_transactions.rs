//! Wallet transactions, through the ESI application service's pipeline.

use iskworks_core::ConnectedCharacter;
use serde_json::Value;

use crate::character_sync::{CharacterSyncService, SourceFailure};

impl CharacterSyncService {
    /// See `fetch_assets_summary` -- same reasoning, delegates to
    /// `EsiApplicationService::sync_wallet`'s existing paginated,
    /// ETag-checkpointed wallet-transactions fetch instead of duplicating it.
    pub(in crate::character_sync) async fn fetch_wallet_transactions_summary(
        &self,
        connection: &ConnectedCharacter,
        token: &str,
    ) -> Result<Value, SourceFailure> {
        let run = self
            .esi_sync
            .sync_wallet_transactions(connection, token)
            .await
            .map_err(SourceFailure::from)?;
        serde_json::to_value(&run).map_err(SourceFailure::from)
    }
}
