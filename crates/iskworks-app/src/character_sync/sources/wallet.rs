//! The wallet balance.

use iskworks_core::ConnectedCharacter;
use serde_json::Value;

use crate::character_sync::{CharacterSyncService, SourceFailure};

impl CharacterSyncService {
    pub(in crate::character_sync) async fn fetch_wallet_summary(
        &self,
        access_token: &str,
        connection: &ConnectedCharacter,
    ) -> Result<Value, SourceFailure> {
        let response = self
            .transport
            .wallet_balance(access_token, connection.eve_character_id)
            .await
            .map_err(SourceFailure::from)?;
        let balance = response
            .records
            .into_iter()
            .next()
            .ok_or_else(|| "ESI returned no wallet balance record".to_string())?;
        serde_json::to_value(&balance).map_err(SourceFailure::from)
    }
}
