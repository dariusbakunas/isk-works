//! The character's public info (name, corporation, birthday).

use iskworks_core::ConnectedCharacter;
use serde_json::Value;

use crate::character_sync::{CharacterSyncService, SourceFailure};

impl CharacterSyncService {
    pub(in crate::character_sync) async fn fetch_character_info_summary(
        &self,
        connection: &ConnectedCharacter,
    ) -> Result<Value, SourceFailure> {
        let response = self
            .transport
            .character_public_info(connection.eve_character_id)
            .await
            .map_err(SourceFailure::from)?;
        let info = response
            .records
            .into_iter()
            .next()
            .ok_or_else(|| "ESI returned no character info record".to_string())?;
        serde_json::to_value(&info).map_err(SourceFailure::from)
    }
}
