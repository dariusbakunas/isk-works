//! Per-workspace Planetary Interaction page preferences (excluded exports,
//! character group order), stored as one jsonb row per workspace.

use async_trait::async_trait;
use iskworks_core::planetary::{PlanetaryPreferences, PlanetaryPreferencesRepository};
use iskworks_core::{InventoryError, WorkspaceId};
use sqlx::types::Json;

use super::{map_sqlx, PgEsiRepository};

#[async_trait]
impl PlanetaryPreferencesRepository for PgEsiRepository {
    async fn planetary_preferences(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<PlanetaryPreferences, InventoryError> {
        let row: Option<(Json<serde_json::Value>, Json<serde_json::Value>)> = sqlx::query_as(
            "SELECT excluded_exports, character_order FROM pi_preferences WHERE workspace_id = $1",
        )
        .bind(workspace_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;
        let Some((Json(excluded_exports), Json(character_order))) = row else {
            return Ok(PlanetaryPreferences::default());
        };
        Ok(PlanetaryPreferences {
            excluded_exports: serde_json::from_value(excluded_exports)
                .map_err(|error| InventoryError::Persistence(error.to_string()))?,
            character_order: serde_json::from_value(character_order)
                .map_err(|error| InventoryError::Persistence(error.to_string()))?,
        })
    }

    async fn save_planetary_preferences(
        &self,
        workspace_id: WorkspaceId,
        preferences: &PlanetaryPreferences,
    ) -> Result<(), InventoryError> {
        sqlx::query(
            "INSERT INTO pi_preferences (workspace_id, excluded_exports, character_order, updated_at)
             VALUES ($1, $2, $3, now())
             ON CONFLICT (workspace_id) DO UPDATE
             SET excluded_exports = EXCLUDED.excluded_exports,
                 character_order = EXCLUDED.character_order,
                 updated_at = now()",
        )
        .bind(workspace_id.0)
        .bind(Json(&preferences.excluded_exports))
        .bind(Json(&preferences.character_order))
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }
}
