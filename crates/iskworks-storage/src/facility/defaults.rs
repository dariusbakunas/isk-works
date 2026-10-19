//! `industry_facility_defaults`: the workspace default facility per EVE
//! industry target filter (`target_filter_id NULL` is the role's fallback).

use async_trait::async_trait;
use iskworks_core::facility_defaults::{
    FacilityDefault, FacilityDefaultChange, FacilityDefaultsRepository,
};
use iskworks_core::{FacilityError, FacilityProfileId, WorkspaceId};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use super::{map_error, parse_role, role_db, PgFacilityRepository};

#[derive(sqlx::FromRow)]
struct DefaultRow {
    role: String,
    target_filter_id: Option<i64>,
    facility_profile_id: Uuid,
    rig_covered_at_assignment: bool,
}

pub(crate) async fn list_defaults(
    pool: &PgPool,
    workspace_id: WorkspaceId,
) -> Result<Vec<FacilityDefault>, FacilityError> {
    let rows = sqlx::query_as::<_, DefaultRow>(
        "SELECT role, target_filter_id, facility_profile_id, rig_covered_at_assignment \
         FROM industry_facility_defaults WHERE workspace_id = $1 \
         ORDER BY role, target_filter_id NULLS LAST",
    )
    .bind(workspace_id.0)
    .fetch_all(pool)
    .await
    .map_err(map_error)?;
    rows.into_iter()
        .map(|row| {
            Ok(FacilityDefault {
                role: parse_role(&row.role)?,
                target_filter_id: row.target_filter_id,
                facility_profile_id: FacilityProfileId(row.facility_profile_id),
                rig_covered_at_assignment: row.rig_covered_at_assignment,
            })
        })
        .collect()
}

/// Removes every default pointing at `facility_id` (archived facilities drop
/// out of the defaults; deletion is handled by the FK cascade).
pub(super) async fn clear_facility<'e>(
    executor: impl PgExecutor<'e>,
    facility_id: FacilityProfileId,
) -> Result<(), FacilityError> {
    sqlx::query("DELETE FROM industry_facility_defaults WHERE facility_profile_id = $1")
        .bind(facility_id.0)
        .execute(executor)
        .await
        .map_err(map_error)?;
    Ok(())
}

#[async_trait]
impl FacilityDefaultsRepository for PgFacilityRepository {
    async fn list_facility_defaults(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<FacilityDefault>, FacilityError> {
        list_defaults(&self.pool, workspace_id).await
    }

    async fn list_facility_profiles(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<iskworks_core::IndustryFacilityProfile>, FacilityError> {
        self.list(workspace_id).await
    }

    async fn apply_facility_default_changes(
        &self,
        workspace_id: WorkspaceId,
        changes: Vec<FacilityDefaultChange>,
    ) -> Result<(), FacilityError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        for change in changes {
            match change {
                FacilityDefaultChange::Clear {
                    role,
                    target_filter_id,
                } => {
                    sqlx::query(
                        "DELETE FROM industry_facility_defaults \
                         WHERE workspace_id = $1 AND role = $2 \
                           AND target_filter_id IS NOT DISTINCT FROM $3",
                    )
                    .bind(workspace_id.0)
                    .bind(role_db(role))
                    .bind(target_filter_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(map_error)?;
                }
                FacilityDefaultChange::Set(default) => {
                    // FOR SHARE keeps the profile from being archived or
                    // re-roled underneath this write.
                    let profile = sqlx::query_as::<_, (String, bool)>(
                        "SELECT role, archived_at IS NOT NULL \
                         FROM industry_facility_profiles \
                         WHERE workspace_id = $1 AND id = $2 FOR SHARE",
                    )
                    .bind(workspace_id.0)
                    .bind(default.facility_profile_id.0)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(map_error)?;
                    let Some((role, archived)) = profile else {
                        return Err(FacilityError::Validation(
                            "default facility was not found".into(),
                        ));
                    };
                    if archived {
                        return Err(FacilityError::Validation(
                            "default facility is archived".into(),
                        ));
                    }
                    if parse_role(&role)? != default.role {
                        return Err(FacilityError::Validation(
                            "default facility has a different role".into(),
                        ));
                    }
                    sqlx::query(
                        "INSERT INTO industry_facility_defaults \
                           (workspace_id, role, target_filter_id, facility_profile_id, \
                            rig_covered_at_assignment, updated_at) \
                         VALUES ($1, $2, $3, $4, $5, now()) \
                         ON CONFLICT (workspace_id, role, coalesce(target_filter_id, 0)) \
                         DO UPDATE SET facility_profile_id = EXCLUDED.facility_profile_id, \
                           rig_covered_at_assignment = EXCLUDED.rig_covered_at_assignment, \
                           updated_at = EXCLUDED.updated_at",
                    )
                    .bind(workspace_id.0)
                    .bind(role_db(default.role))
                    .bind(default.target_filter_id)
                    .bind(default.facility_profile_id.0)
                    .bind(default.rig_covered_at_assignment)
                    .execute(&mut *tx)
                    .await
                    .map_err(map_error)?;
                }
            }
        }
        tx.commit().await.map_err(map_error)
    }
}

#[cfg(test)]
mod tests;
