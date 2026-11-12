//! Where the character is: system, station or structure, and ship.

use iskworks_core::ConnectedCharacter;
use serde_json::Value;

use crate::character_sync::{CharacterSyncService, SourceFailure};

impl CharacterSyncService {
    pub(in crate::character_sync) async fn fetch_location_summary(
        &self,
        access_token: &str,
        connection: &ConnectedCharacter,
    ) -> Result<Value, SourceFailure> {
        let response = self
            .transport
            .character_location(access_token, connection.eve_character_id)
            .await
            .map_err(SourceFailure::from)?;
        let location = response
            .records
            .into_iter()
            .next()
            .ok_or_else(|| "ESI returned no location record".to_string())?;
        serde_json::to_value(&location).map_err(SourceFailure::from)
    }
}
