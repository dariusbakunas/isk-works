//! Planetary colonies and each one's layout.

use iskworks_core::ConnectedCharacter;
use serde_json::Value;

use crate::character_sync::{CharacterSyncService, SourceFailure};

impl CharacterSyncService {
    /// One list call plus one layout call per colony. Each planet's header
    /// fields are flattened next to its `pins`, so the stored summary is
    /// `{"planets": [{planet_id, planet_type, ..., last_update, pins: [...]}]}`
    /// -- the shape `iskworks_core::planetary::PlanetLayout` deserializes.
    /// Any single planet failing fails the whole source: a partial colony
    /// list would silently drop alerts.
    pub(in crate::character_sync) async fn fetch_planets_summary(
        &self,
        access_token: &str,
        connection: &ConnectedCharacter,
    ) -> Result<Value, SourceFailure> {
        let planets = self
            .transport
            .character_planets(access_token, connection.eve_character_id)
            .await
            .map_err(SourceFailure::from)?;
        let mut layouts = Vec::with_capacity(planets.records.len());
        for planet in planets.records {
            let detail = self
                .transport
                .character_planet_detail(
                    access_token,
                    connection.eve_character_id,
                    planet.planet_id,
                )
                .await
                .map_err(SourceFailure::from)?
                .records
                .into_iter()
                .next()
                .ok_or_else(|| format!("ESI returned no layout for planet {}", planet.planet_id))?;
            let mut layout = serde_json::to_value(&planet).map_err(SourceFailure::from)?;
            layout["pins"] = serde_json::to_value(&detail.pins).map_err(SourceFailure::from)?;
            layouts.push(layout);
        }
        Ok(serde_json::json!({ "planets": layouts }))
    }
}
