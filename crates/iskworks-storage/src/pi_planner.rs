//! PI Planner inputs: per-workspace resource readings and planet settings.

use async_trait::async_trait;
use iskworks_core::pi_planner::{
    DensityWrite, PiPlannerInputsRepository, PlanetSettingsWrite, StoredDensity,
    StoredPlanetSettings,
};
use iskworks_core::{InventoryError, WorkspaceId};
use sqlx::PgPool;

#[derive(Clone)]
pub struct PgPiPlannerInputsRepository {
    pool: PgPool,
}

impl PgPiPlannerInputsRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn persistence(error: sqlx::Error) -> InventoryError {
    InventoryError::Persistence(error.to_string())
}

#[async_trait]
impl PiPlannerInputsRepository for PgPiPlannerInputsRepository {
    async fn densities(
        &self,
        workspace_id: WorkspaceId,
        planet_ids: &[i64],
    ) -> Result<Vec<StoredDensity>, InventoryError> {
        let rows: Vec<(i64, i64, _, _, _)> = sqlx::query_as(
            "SELECT planet_id, resource_type_id, density_pct, head_value, updated_at
             FROM pi_planet_densities
             WHERE workspace_id = $1 AND planet_id = ANY($2)",
        )
        .bind(workspace_id.0)
        .bind(planet_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(persistence)?;
        Ok(rows
            .into_iter()
            .map(
                |(planet_id, resource_type_id, density_pct, head_value, updated_at)| {
                    StoredDensity {
                        planet_id,
                        resource_type_id,
                        density_pct,
                        head_value,
                        updated_at,
                    }
                },
            )
            .collect())
    }

    async fn save_densities(
        &self,
        workspace_id: WorkspaceId,
        writes: &[DensityWrite],
    ) -> Result<(), InventoryError> {
        let mut tx = self.pool.begin().await.map_err(persistence)?;
        for write in writes {
            if write.clears() {
                sqlx::query(
                    "DELETE FROM pi_planet_densities
                     WHERE workspace_id = $1 AND planet_id = $2 AND resource_type_id = $3",
                )
                .bind(workspace_id.0)
                .bind(write.planet_id)
                .bind(write.resource_type_id)
                .execute(&mut *tx)
                .await
                .map_err(persistence)?;
            } else {
                sqlx::query(
                    "INSERT INTO pi_planet_densities
                         (workspace_id, planet_id, resource_type_id, density_pct, head_value, updated_at)
                     VALUES ($1, $2, $3, $4, $5, now())
                     ON CONFLICT (workspace_id, planet_id, resource_type_id) DO UPDATE
                     SET density_pct = EXCLUDED.density_pct,
                         head_value = EXCLUDED.head_value,
                         updated_at = now()",
                )
                .bind(workspace_id.0)
                .bind(write.planet_id)
                .bind(write.resource_type_id)
                .bind(write.density_pct)
                .bind(write.head_value)
                .execute(&mut *tx)
                .await
                .map_err(persistence)?;
            }
        }
        tx.commit().await.map_err(persistence)
    }

    async fn planet_settings(
        &self,
        workspace_id: WorkspaceId,
        planet_ids: &[i64],
    ) -> Result<Vec<StoredPlanetSettings>, InventoryError> {
        let rows: Vec<(i64, _, bool, _)> = sqlx::query_as(
            "SELECT planet_id, customs_rate, contested, updated_at
             FROM pi_planet_settings
             WHERE workspace_id = $1 AND planet_id = ANY($2)",
        )
        .bind(workspace_id.0)
        .bind(planet_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(persistence)?;
        Ok(rows
            .into_iter()
            .map(
                |(planet_id, customs_rate, contested, updated_at)| StoredPlanetSettings {
                    planet_id,
                    customs_rate,
                    contested,
                    updated_at,
                },
            )
            .collect())
    }

    async fn save_planet_settings(
        &self,
        workspace_id: WorkspaceId,
        writes: &[PlanetSettingsWrite],
    ) -> Result<(), InventoryError> {
        let mut tx = self.pool.begin().await.map_err(persistence)?;
        for write in writes {
            if write.clears() {
                sqlx::query(
                    "DELETE FROM pi_planet_settings WHERE workspace_id = $1 AND planet_id = $2",
                )
                .bind(workspace_id.0)
                .bind(write.planet_id)
                .execute(&mut *tx)
                .await
                .map_err(persistence)?;
            } else {
                sqlx::query(
                    "INSERT INTO pi_planet_settings
                         (workspace_id, planet_id, customs_rate, contested, updated_at)
                     VALUES ($1, $2, $3, $4, now())
                     ON CONFLICT (workspace_id, planet_id) DO UPDATE
                     SET customs_rate = EXCLUDED.customs_rate,
                         contested = EXCLUDED.contested,
                         updated_at = now()",
                )
                .bind(workspace_id.0)
                .bind(write.planet_id)
                .bind(write.customs_rate)
                .bind(write.contested)
                .execute(&mut *tx)
                .await
                .map_err(persistence)?;
            }
        }
        tx.commit().await.map_err(persistence)
    }
}
