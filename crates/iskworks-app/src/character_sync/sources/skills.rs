//! Skills, attributes and the skill queue.

use iskworks_core::ConnectedCharacter;
use iskworks_esi::SKILL_QUEUE_SCOPE;
use serde_json::Value;

use crate::character_sync::{granted, CharacterSyncService, SourceFailure};

impl CharacterSyncService {
    pub(in crate::character_sync) async fn fetch_skills_summary(
        &self,
        access_token: &str,
        connection: &ConnectedCharacter,
    ) -> Result<Value, SourceFailure> {
        let response = self
            .transport
            .character_skills(access_token, connection.eve_character_id)
            .await
            .map_err(SourceFailure::from)?;
        let skills = response
            .records
            .into_iter()
            .next()
            .ok_or_else(|| "ESI returned no skills record".to_string())?;
        let mut summary = serde_json::to_value(&skills).map_err(SourceFailure::from)?;
        // Same scope as the skills read. Training times degrade to estimates
        // without it, so a failure here leaves the key out rather than
        // failing the whole source.
        if let Ok(attributes) = self
            .transport
            .character_attributes(access_token, connection.eve_character_id)
            .await
        {
            if let (Value::Object(map), Some(Ok(attributes_value))) = (
                &mut summary,
                attributes.records.first().map(serde_json::to_value),
            ) {
                map.insert("attributes".to_string(), attributes_value);
            }
        }
        if granted(connection, SKILL_QUEUE_SCOPE) {
            if let Ok(queue) = self
                .transport
                .character_skill_queue(access_token, connection.eve_character_id)
                .await
            {
                if let (Value::Object(map), Ok(queue_value)) =
                    (&mut summary, serde_json::to_value(&queue.records))
                {
                    map.insert("skillQueue".to_string(), queue_value);
                }
            }
        }
        Ok(summary)
    }
}
