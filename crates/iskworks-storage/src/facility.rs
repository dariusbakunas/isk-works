use chrono::{DateTime, Utc};
use iskworks_core::{
    export_command, facility_identity, FacilityError, FacilityImportAction, FacilityImportDecision,
    FacilityImportItemOutcome, FacilityImportStatus, FacilityKind, FacilityProfileId, FacilityRig,
    FacilityRole, IndustryFacilityProfile, Money, RigApplicability, SecurityClass, WorkspaceId,
};
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::sde_read::SECURITY_CLASS_CASE_SQL;

#[derive(Clone)]
pub struct PgFacilityRepository {
    pool: PgPool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KnownStructure {
    pub structure_id: i64,
    pub structure_name: String,
    pub structure_type_id: Option<i64>,
    pub structure_type_name: Option<String>,
    pub solar_system_id: i64,
    pub solar_system_name: Option<String>,
    pub security_class: String,
}

impl PgFacilityRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn search_known_structures(
        &self,
        workspace_id: WorkspaceId,
        query: &str,
        limit: u32,
    ) -> Result<Vec<KnownStructure>, FacilityError> {
        let contains = format!("%{}%", query.trim());
        let sql = format!(
            r#"SELECT location.location_id AS structure_id,
                      location.location_name AS structure_name,
                      location.structure_type_id,
                      structure_type.name_en AS structure_type_name,
                      location.solar_system_id,
                      system.name_en AS solar_system_name,
                      {SECURITY_CLASS_CASE_SQL} AS security_class
               FROM market_location_names location
               LEFT JOIN sde_imports import ON import.active=true
               LEFT JOIN sde_types structure_type
                 ON structure_type.import_id=import.id
                AND structure_type.type_id=location.structure_type_id
               LEFT JOIN sde_solar_systems system
                 ON system.import_id=import.id
                AND system.solar_system_id=location.solar_system_id
               WHERE location.workspace_id=$1
                 AND lower(location.location_name) LIKE lower($2)
               ORDER BY
                 CASE WHEN lower(location.location_name)=lower($3) THEN 0 ELSE 1 END,
                 location.location_name, location.location_id
               LIMIT $4"#
        );
        sqlx::query_as::<_, KnownStructure>(&sql)
            .bind(workspace_id.0)
            .bind(contains)
            .bind(query.trim())
            .bind(i64::from(limit.clamp(1, 100)))
            .fetch_all(&self.pool)
            .await
            .map_err(map_error)
    }

    /// Looks up a single structure by exact ID, regardless of whether the
    /// workspace has ever observed it through assets/wallet/market activity
    /// -- callers resolve-then-cache via ESI first (see
    /// `resolve_structure` in `routes/facilities.rs`), then call this to
    /// read the freshly cached row back in the same shape as
    /// `search_known_structures`.
    pub async fn get_known_structure(
        &self,
        workspace_id: WorkspaceId,
        structure_id: i64,
    ) -> Result<Option<KnownStructure>, FacilityError> {
        let sql = format!(
            r#"SELECT location.location_id AS structure_id,
                      location.location_name AS structure_name,
                      location.structure_type_id,
                      structure_type.name_en AS structure_type_name,
                      location.solar_system_id,
                      system.name_en AS solar_system_name,
                      {SECURITY_CLASS_CASE_SQL} AS security_class
               FROM market_location_names location
               LEFT JOIN sde_imports import ON import.active=true
               LEFT JOIN sde_types structure_type
                 ON structure_type.import_id=import.id
                AND structure_type.type_id=location.structure_type_id
               LEFT JOIN sde_solar_systems system
                 ON system.import_id=import.id
                AND system.solar_system_id=location.solar_system_id
               WHERE location.workspace_id=$1
                 AND location.location_id=$2"#
        );
        sqlx::query_as::<_, KnownStructure>(&sql)
            .bind(workspace_id.0)
            .bind(structure_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_error)
    }

    pub async fn list(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<IndustryFacilityProfile>, FacilityError> {
        let ids = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM industry_facility_profiles WHERE workspace_id = $1 ORDER BY archived_at NULLS FIRST, display_name",
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?;
        let mut profiles = Vec::with_capacity(ids.len());
        for id in ids {
            profiles.push(load_profile(&self.pool, workspace_id, FacilityProfileId(id)).await?);
        }
        Ok(profiles)
    }

    pub async fn get(
        &self,
        workspace_id: WorkspaceId,
        id: FacilityProfileId,
    ) -> Result<IndustryFacilityProfile, FacilityError> {
        load_profile(&self.pool, workspace_id, id).await
    }

    pub async fn create(
        &self,
        profile: IndustryFacilityProfile,
    ) -> Result<IndustryFacilityProfile, FacilityError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        insert_profile(&mut tx, &profile).await?;
        tx.commit().await.map_err(map_error)?;
        self.get(profile.workspace_id, profile.id).await
    }

    pub async fn update(
        &self,
        workspace_id: WorkspaceId,
        id: FacilityProfileId,
        expected_revision: u64,
        mut replacement: IndustryFacilityProfile,
    ) -> Result<IndustryFacilityProfile, FacilityError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        replace_profile(
            &mut tx,
            workspace_id,
            id,
            expected_revision,
            &mut replacement,
        )
        .await?;
        tx.commit().await.map_err(map_error)?;
        self.get(workspace_id, id).await
    }

    /// Applies a whole facility import in one transaction.
    ///
    /// The workspace's profiles are loaded (and row-locked) once; each action
    /// is then decided by [`iskworks_core::decide_facility_import`] against
    /// that set as updated by the items before it, exactly as if the profiles
    /// had been re-listed per item. An item the decision rejects (validation,
    /// duplicate, stale revision) is reported in its outcome and writes
    /// nothing. A write that fails aborts the import: the transaction rolls
    /// back, so none of its items persist, and that error is returned.
    pub async fn import(
        &self,
        workspace_id: WorkspaceId,
        actions: Vec<FacilityImportAction>,
    ) -> Result<Vec<FacilityImportItemOutcome>, FacilityError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let ids = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM industry_facility_profiles WHERE workspace_id = $1 ORDER BY archived_at NULLS FIRST, display_name FOR UPDATE",
        )
        .bind(workspace_id.0)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_error)?;
        let mut current = Vec::with_capacity(ids.len() + actions.len());
        for id in ids {
            current.push(load_profile_on(&mut tx, workspace_id, FacilityProfileId(id)).await?);
        }

        let mut outcomes = Vec::with_capacity(actions.len());
        for action in actions {
            let name = action.item.name.clone();
            let result = match iskworks_core::decide_facility_import(workspace_id, action, &current)
            {
                Err(error) => Err(error),
                Ok(FacilityImportDecision::Skip) => Ok(FacilityImportStatus::Skipped),
                Ok(FacilityImportDecision::Create(profile)) => {
                    insert_profile(&mut tx, &profile).await?;
                    // Its identity matched no active profile, so it is the
                    // only active member of its identity group: where it sits
                    // in `current` cannot change any later match.
                    current.push(profile);
                    Ok(FacilityImportStatus::Created)
                }
                Ok(FacilityImportDecision::Replace {
                    existing_id,
                    expected_revision,
                    mut replacement,
                }) => {
                    replace_profile(
                        &mut tx,
                        workspace_id,
                        existing_id,
                        expected_revision,
                        &mut replacement,
                    )
                    .await?;
                    let slot = current
                        .iter()
                        .position(|profile| profile.id == existing_id)
                        .ok_or(FacilityError::NotFound)?;
                    current[slot] = replacement;
                    reorder_identity_group(&mut tx, workspace_id, &mut current, slot).await?;
                    Ok(FacilityImportStatus::Replaced)
                }
            };
            outcomes.push(FacilityImportItemOutcome { name, result });
        }
        tx.commit().await.map_err(map_error)?;
        Ok(outcomes)
    }

    pub async fn archive(
        &self,
        workspace_id: WorkspaceId,
        id: FacilityProfileId,
        expected_revision: u64,
    ) -> Result<IndustryFacilityProfile, FacilityError> {
        let now = crate::db_now();
        let changed = sqlx::query(
            "UPDATE industry_facility_profiles SET archived_at = $1, updated_at = $1, revision = revision + 1 WHERE workspace_id = $2 AND id = $3 AND revision = $4 AND archived_at IS NULL",
        )
        .bind(now)
        .bind(workspace_id.0)
        .bind(id.0)
        .bind(as_i64(expected_revision)?)
        .execute(&self.pool)
        .await
        .map_err(map_error)?
        .rows_affected();
        if changed == 0 {
            let existing = self.get(workspace_id, id).await?;
            return Err(if existing.archived_at.is_some() {
                FacilityError::Archived
            } else {
                FacilityError::RevisionConflict
            });
        }
        self.get(workspace_id, id).await
    }

    pub async fn delete(
        &self,
        workspace_id: WorkspaceId,
        id: FacilityProfileId,
        expected_revision: u64,
    ) -> Result<(), FacilityError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let current = sqlx::query_as::<_, FacilityRevisionRow>(
            "SELECT revision, archived_at FROM industry_facility_profiles WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(workspace_id.0)
        .bind(id.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?
        .ok_or(FacilityError::NotFound)?;
        if as_u64(current.revision)? != expected_revision {
            return Err(FacilityError::RevisionConflict);
        }

        sqlx::query(
            r#"
            UPDATE build_draft_planning draft
            SET planning_input =
                CASE
                  WHEN draft.planning_input->'manufacturingFacility'->>'facilityProfileId' = $1
                  THEN draft.planning_input - 'manufacturingFacility'
                  ELSE draft.planning_input
                END,
                updated_at = $2
            FROM builds build
            WHERE build.id = draft.build_id
              AND build.workspace_id = $3
              AND draft.planning_input->'manufacturingFacility'->>'facilityProfileId' = $1
            "#,
        )
        .bind(id.0.to_string())
        .bind(crate::db_now())
        .bind(workspace_id.0)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
        sqlx::query(
            r#"
            UPDATE build_draft_planning draft
            SET planning_input = draft.planning_input - 'reactionFacility',
                updated_at = $2
            FROM builds build
            WHERE build.id = draft.build_id
              AND build.workspace_id = $3
              AND draft.planning_input->'reactionFacility'->>'facilityProfileId' = $1
            "#,
        )
        .bind(id.0.to_string())
        .bind(crate::db_now())
        .bind(workspace_id.0)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
        let deleted = sqlx::query(
            "DELETE FROM industry_facility_profiles WHERE workspace_id = $1 AND id = $2 AND revision = $3",
        )
        .bind(workspace_id.0)
        .bind(id.0)
        .bind(as_i64(expected_revision)?)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?
        .rows_affected();
        if deleted != 1 {
            return Err(FacilityError::RevisionConflict);
        }
        tx.commit().await.map_err(map_error)?;
        Ok(())
    }
}

pub(crate) async fn load_profile(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    id: FacilityProfileId,
) -> Result<IndustryFacilityProfile, FacilityError> {
    let mut connection = pool.acquire().await.map_err(map_error)?;
    load_profile_on(&mut connection, workspace_id, id).await
}

async fn load_profile_on(
    connection: &mut PgConnection,
    workspace_id: WorkspaceId,
    id: FacilityProfileId,
) -> Result<IndustryFacilityProfile, FacilityError> {
    let row = sqlx::query_as::<_, FacilityRow>(
        r#"
        SELECT id, workspace_id, display_name, facility_kind, role, structure_id,
          structure_type_id, structure_type_name, solar_system_id, solar_system_name,
          security_class, material_reduction_percent, time_reduction_percent,
          job_cost_reduction_percent, facility_tax_percent, scc_surcharge_percent, alliance_surcharge_percent,
          fixed_supplemental_cost, manual_system_cost_index, notes, archived_at,
          revision, created_at, updated_at
        FROM industry_facility_profiles WHERE workspace_id = $1 AND id = $2
        "#,
    )
    .bind(workspace_id.0)
    .bind(id.0)
    .fetch_optional(&mut *connection)
    .await
    .map_err(map_error)?
    .ok_or(FacilityError::NotFound)?;
    let role_table = if row.role == "reaction" {
        "sde_reaction_rig_modifiers"
    } else {
        "sde_structure_rig_modifiers"
    };
    let rigs = sqlx::query_as::<_, RigRow>(&rig_select_query(role_table))
        .bind(id.0)
        .fetch_all(&mut *connection)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(RigRow::into_rig)
        .collect::<Result<Vec<_>, _>>()?;
    row.into_profile(rigs)
}

/// Replaces an active profile at `expected_revision` inside `tx`, stamping
/// `replacement` with the persisted id, workspace, and bumped revision.
async fn replace_profile(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: WorkspaceId,
    id: FacilityProfileId,
    expected_revision: u64,
    replacement: &mut IndustryFacilityProfile,
) -> Result<(), FacilityError> {
    let current = sqlx::query_as::<_, FacilityRevisionRow>(
        "SELECT revision, archived_at FROM industry_facility_profiles WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(workspace_id.0)
    .bind(id.0)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_error)?
    .ok_or(FacilityError::NotFound)?;
    if current.archived_at.is_some() {
        return Err(FacilityError::Archived);
    }
    if as_u64(current.revision)? != expected_revision {
        return Err(FacilityError::RevisionConflict);
    }
    replacement.id = id;
    replacement.workspace_id = workspace_id;
    replacement.revision = expected_revision
        .checked_add(1)
        .ok_or(FacilityError::ArithmeticOverflow)?;
    replacement.created_at = crate::db_now();
    replacement.updated_at = crate::db_now();
    update_profile(tx, replacement).await?;
    sqlx::query("DELETE FROM industry_facility_profile_rigs WHERE facility_profile_id = $1")
        .bind(id.0)
        .execute(&mut **tx)
        .await
        .map_err(map_error)?;
    insert_rigs(tx, replacement).await
}

/// After a replace (which may rename `current[slot]`), restores list order
/// among the active profiles sharing its identity -- the only order
/// `find_active_facility_match` (first match wins) can observe. The order
/// comes from Postgres so it matches `list` collation exactly; a profile
/// with no active duplicate (the normal case) costs no query.
async fn reorder_identity_group(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: WorkspaceId,
    current: &mut [IndustryFacilityProfile],
    slot: usize,
) -> Result<(), FacilityError> {
    let Ok((identity, _)) = facility_identity(&export_command(&current[slot])) else {
        return Ok(());
    };
    let slots = current
        .iter()
        .enumerate()
        .filter(|(_, profile)| {
            profile.archived_at.is_none()
                && facility_identity(&export_command(profile))
                    .is_ok_and(|(other, _)| other == identity)
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if slots.len() < 2 {
        return Ok(());
    }
    let ids = slots
        .iter()
        .map(|&index| current[index].id.0)
        .collect::<Vec<_>>();
    let ordered = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM industry_facility_profiles WHERE workspace_id = $1 AND id = ANY($2) ORDER BY archived_at NULLS FIRST, display_name",
    )
    .bind(workspace_id.0)
    .bind(&ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(map_error)?;
    let mut members = slots
        .iter()
        .map(|&index| current[index].clone())
        .collect::<Vec<_>>();
    for (&index, id) in slots.iter().zip(ordered) {
        let position = members
            .iter()
            .position(|profile| profile.id.0 == id)
            .ok_or(FacilityError::NotFound)?;
        current[index] = members.swap_remove(position);
    }
    Ok(())
}

async fn insert_profile(
    tx: &mut Transaction<'_, Postgres>,
    profile: &IndustryFacilityProfile,
) -> Result<(), FacilityError> {
    sqlx::query(
        r#"
        INSERT INTO industry_facility_profiles (
          id, workspace_id, display_name, facility_kind, role, structure_id, structure_type_id,
          structure_type_name, solar_system_id, solar_system_name, security_class,
          material_reduction_percent, time_reduction_percent, job_cost_reduction_percent,
          facility_tax_percent, scc_surcharge_percent, alliance_surcharge_percent, fixed_supplemental_cost,
          manual_system_cost_index, notes, revision, created_at, updated_at
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23)
        "#,
    )
    .bind(profile.id.0)
    .bind(profile.workspace_id.0)
    .bind(&profile.name)
    .bind(kind_db(profile.kind))
    .bind(role_db(profile.role))
    .bind(profile.structure_id)
    .bind(profile.structure_type_id)
    .bind(&profile.structure_type_name)
    .bind(profile.solar_system_id)
    .bind(&profile.solar_system_name)
    .bind(security_db(profile.security_class))
    .bind(profile.material_reduction_percent)
    .bind(profile.time_reduction_percent)
    .bind(profile.job_cost_reduction_percent)
    .bind(profile.facility_tax_percent)
    .bind(profile.scc_surcharge_percent)
    .bind(profile.alliance_surcharge_percent)
    .bind(profile.fixed_supplemental_cost.0)
    .bind(profile.manual_system_cost_index)
    .bind(&profile.notes)
    .bind(as_i64(profile.revision)?)
    .bind(profile.created_at)
    .bind(profile.updated_at)
    .execute(&mut **tx)
    .await
    .map_err(map_error)?;
    insert_rigs(tx, profile).await
}

async fn update_profile(
    tx: &mut Transaction<'_, Postgres>,
    profile: &IndustryFacilityProfile,
) -> Result<(), FacilityError> {
    sqlx::query(
        r#"
        UPDATE industry_facility_profiles SET display_name=$1, facility_kind=$2, role=$3,
          structure_id=$4, structure_type_id=$5, structure_type_name=$6,
          solar_system_id=$7, solar_system_name=$8, security_class=$9,
          material_reduction_percent=$10, time_reduction_percent=$11,
          job_cost_reduction_percent=$12, facility_tax_percent=$13, scc_surcharge_percent=$14,
          alliance_surcharge_percent=$15, fixed_supplemental_cost=$16,
          manual_system_cost_index=$17, notes=$18, revision=$19, updated_at=$20
        WHERE workspace_id=$21 AND id=$22
        "#,
    )
    .bind(&profile.name)
    .bind(kind_db(profile.kind))
    .bind(role_db(profile.role))
    .bind(profile.structure_id)
    .bind(profile.structure_type_id)
    .bind(&profile.structure_type_name)
    .bind(profile.solar_system_id)
    .bind(&profile.solar_system_name)
    .bind(security_db(profile.security_class))
    .bind(profile.material_reduction_percent)
    .bind(profile.time_reduction_percent)
    .bind(profile.job_cost_reduction_percent)
    .bind(profile.facility_tax_percent)
    .bind(profile.scc_surcharge_percent)
    .bind(profile.alliance_surcharge_percent)
    .bind(profile.fixed_supplemental_cost.0)
    .bind(profile.manual_system_cost_index)
    .bind(&profile.notes)
    .bind(as_i64(profile.revision)?)
    .bind(profile.updated_at)
    .bind(profile.workspace_id.0)
    .bind(profile.id.0)
    .execute(&mut **tx)
    .await
    .map_err(map_error)?;
    Ok(())
}

async fn insert_rigs(
    tx: &mut Transaction<'_, Postgres>,
    profile: &IndustryFacilityProfile,
) -> Result<(), FacilityError> {
    for rig in &profile.rigs {
        sqlx::query(
            "INSERT INTO industry_facility_profile_rigs (facility_profile_id, slot_number, type_id, captured_name, material_reduction_percent, time_reduction_percent) VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(profile.id.0).bind(i16::from(rig.slot_number)).bind(rig.type_id)
        .bind(&rig.type_name).bind(rig.material_reduction_percent)
        .bind(rig.time_reduction_percent)
        .execute(&mut **tx).await.map_err(map_error)?;
    }
    Ok(())
}

#[derive(sqlx::FromRow)]
struct FacilityRevisionRow {
    revision: i64,
    archived_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct FacilityRow {
    id: Uuid,
    workspace_id: Uuid,
    display_name: String,
    facility_kind: String,
    role: String,
    structure_id: Option<i64>,
    structure_type_id: Option<i64>,
    structure_type_name: String,
    solar_system_id: Option<i64>,
    solar_system_name: String,
    security_class: String,
    material_reduction_percent: Decimal,
    time_reduction_percent: Decimal,
    job_cost_reduction_percent: Decimal,
    facility_tax_percent: Decimal,
    scc_surcharge_percent: Decimal,
    alliance_surcharge_percent: Decimal,
    fixed_supplemental_cost: Decimal,
    manual_system_cost_index: Option<Decimal>,
    notes: String,
    archived_at: Option<DateTime<Utc>>,
    revision: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl FacilityRow {
    fn into_profile(
        self,
        rigs: Vec<FacilityRig>,
    ) -> Result<IndustryFacilityProfile, FacilityError> {
        Ok(IndustryFacilityProfile {
            id: FacilityProfileId(self.id),
            workspace_id: WorkspaceId(self.workspace_id),
            name: self.display_name,
            kind: parse_kind(&self.facility_kind)?,
            role: parse_role(&self.role)?,
            structure_id: self.structure_id,
            structure_type_id: self.structure_type_id,
            structure_type_name: self.structure_type_name,
            solar_system_id: self.solar_system_id,
            solar_system_name: self.solar_system_name,
            security_class: parse_security(&self.security_class)?,
            material_reduction_percent: self.material_reduction_percent,
            time_reduction_percent: self.time_reduction_percent,
            job_cost_reduction_percent: self.job_cost_reduction_percent,
            facility_tax_percent: self.facility_tax_percent,
            scc_surcharge_percent: self.scc_surcharge_percent,
            alliance_surcharge_percent: self.alliance_surcharge_percent,
            fixed_supplemental_cost: Money(self.fixed_supplemental_cost),
            manual_system_cost_index: self.manual_system_cost_index,
            notes: self.notes,
            rigs,
            archived_at: self.archived_at,
            revision: as_u64(self.revision)?,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct RigRow {
    slot_number: i16,
    type_id: i64,
    captured_name: String,
    material_reduction_percent: Decimal,
    time_reduction_percent: Decimal,
    /// Resolved from the active SDE by `rig_select_query`; never stored.
    applicability: sqlx::types::Json<RigApplicability>,
}

impl RigRow {
    fn into_rig(self) -> Result<FacilityRig, FacilityError> {
        Ok(FacilityRig {
            slot_number: u8::try_from(self.slot_number)
                .map_err(|_| FacilityError::ArithmeticOverflow)?,
            type_id: self.type_id,
            type_name: self.captured_name,
            material_reduction_percent: self.material_reduction_percent,
            time_reduction_percent: self.time_reduction_percent,
            applicability: self.applicability.0,
        })
    }
}

/// Rig SELECT for `load_profile`. Applicability is not persisted: it is
/// resolved on every load from the active SDE -- joined against the
/// manufacturing or reaction rig-modifier table per the profile's `role` --
/// so an SDE re-import reaches existing profiles without a re-save. `role_table` is one of two
/// fixed literals, never user input.
///
/// A rig activity may reference several target filters (a multi-scope rig);
/// the resolved applicability is the *union* of those filters' category/group
/// sets. `restricted` is emitted iff the rig declares at least one filter
/// id, even when none resolve -- an unresolvable id must not widen the rig
/// back to "unrestricted".
fn rig_select_query(role_table: &str) -> String {
    // Union subquery for one activity dimension: `array_col` is
    // `material_filter_ids` / `time_filter_ids` on the rig-modifier row.
    let dimension = |alias: &str, array_col: &str| {
        format!(
            "LEFT JOIN LATERAL (
               SELECT
                 COALESCE(CARDINALITY(rm.{array_col}), 0) AS filter_count,
                 COALESCE((SELECT ARRAY_AGG(DISTINCT v ORDER BY v)
                    FROM sde_industry_target_filters f
                    CROSS JOIN LATERAL UNNEST(f.category_ids) AS v
                    WHERE f.import_id = imp.id AND f.filter_id = ANY(rm.{array_col})), '{{}}'::bigint[]) AS category_ids,
                 COALESCE((SELECT ARRAY_AGG(DISTINCT v ORDER BY v)
                    FROM sde_industry_target_filters f
                    CROSS JOIN LATERAL UNNEST(f.group_ids) AS v
                    WHERE f.import_id = imp.id AND f.filter_id = ANY(rm.{array_col})), '{{}}'::bigint[]) AS group_ids
             ) {alias} ON true"
        )
    };
    let filter_json = |alias: &str| {
        format!(
            "CASE WHEN rm.type_id IS NULL OR COALESCE({alias}.filter_count, 0) = 0 \
                  THEN '\"unrestricted\"'::jsonb \
             ELSE jsonb_build_object('restricted', jsonb_build_object( \
               'categoryIds', to_jsonb({alias}.category_ids), \
               'groupIds', to_jsonb({alias}.group_ids))) END"
        )
    };
    format!(
        r#"SELECT r.slot_number, r.type_id, r.captured_name,
                  r.material_reduction_percent, r.time_reduction_percent,
                  jsonb_build_object(
                    'material', {material_json},
                    'time', {time_json}) AS applicability
           FROM industry_facility_profile_rigs r
           LEFT JOIN sde_imports imp ON imp.active = true
           LEFT JOIN {role_table} rm ON rm.import_id = imp.id AND rm.type_id = r.type_id
           {material_dimension}
           {time_dimension}
           WHERE r.facility_profile_id = $1
           ORDER BY r.slot_number"#,
        material_json = filter_json("mf"),
        time_json = filter_json("tf"),
        material_dimension = dimension("mf", "material_filter_ids"),
        time_dimension = dimension("tf", "time_filter_ids"),
    )
}

fn kind_db(value: FacilityKind) -> &'static str {
    match value {
        FacilityKind::NpcStation => "npc_station",
        FacilityKind::UpwellStructure => "upwell_structure",
        FacilityKind::Manual => "manual",
    }
}
fn role_db(value: FacilityRole) -> &'static str {
    match value {
        FacilityRole::Manufacturing => "manufacturing",
        FacilityRole::Reaction => "reaction",
    }
}
fn security_db(value: SecurityClass) -> &'static str {
    match value {
        SecurityClass::HighSec => "high_sec",
        SecurityClass::LowSec => "low_sec",
        SecurityClass::NullSec => "null_sec",
        SecurityClass::Wormhole => "wormhole",
        SecurityClass::Unknown => "unknown",
    }
}
fn parse_kind(value: &str) -> Result<FacilityKind, FacilityError> {
    match value {
        "npc_station" => Ok(FacilityKind::NpcStation),
        "upwell_structure" => Ok(FacilityKind::UpwellStructure),
        "manual" => Ok(FacilityKind::Manual),
        _ => Err(FacilityError::Persistence("invalid facility kind".into())),
    }
}
fn parse_role(value: &str) -> Result<FacilityRole, FacilityError> {
    match value {
        "manufacturing" => Ok(FacilityRole::Manufacturing),
        "reaction" => Ok(FacilityRole::Reaction),
        _ => Err(FacilityError::Persistence("invalid facility role".into())),
    }
}
fn parse_security(value: &str) -> Result<SecurityClass, FacilityError> {
    match value {
        "high_sec" => Ok(SecurityClass::HighSec),
        "low_sec" => Ok(SecurityClass::LowSec),
        "null_sec" => Ok(SecurityClass::NullSec),
        "wormhole" => Ok(SecurityClass::Wormhole),
        "unknown" => Ok(SecurityClass::Unknown),
        _ => Err(FacilityError::Persistence("invalid security class".into())),
    }
}
fn as_i64(value: u64) -> Result<i64, FacilityError> {
    i64::try_from(value).map_err(|_| FacilityError::ArithmeticOverflow)
}
fn as_u64(value: i64) -> Result<u64, FacilityError> {
    u64::try_from(value).map_err(|_| FacilityError::ArithmeticOverflow)
}
fn map_error(error: sqlx::Error) -> FacilityError {
    FacilityError::Persistence(error.to_string())
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    /// A workspace plus one active SDE import containing a real k-space
    /// lowsec system that carries EVE's "shattered wormhole" environmental
    /// effect (`wormhole_class_id = 8`) -- the exact system shape that
    /// triggered the `wormhole_class_id IS NOT NULL` classification bug
    /// (verified live against the dev DB's imported SDE: Erstet,
    /// `solar_system_id 30_003_425`, `security_status 0.4494`). Proves
    /// `search_known_structures`/`get_known_structure` share the same
    /// corrected `SECURITY_CLASS_CASE_SQL` as the SDE-search helpers in
    /// `sde_read.rs`, not a separate, possibly-still-buggy copy.
    async fn fixture(pool: &PgPool) -> WorkspaceId {
        let workspace_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let import_id = Uuid::new_v4();
        let now = crate::db_now();
        // `workspaces.owner_id` and `owners.workspace_id` are mutually
        // FK-referencing and DEFERRABLE INITIALLY DEFERRED -- both inserts
        // must land in one transaction so neither constraint is checked
        // until commit.
        let mut tx = pool.begin().await.unwrap();
        sqlx::query(
            "INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Facility Security Test',$2,$3,$3)",
        )
        .bind(workspace_id)
        .bind(owner_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Facility Security Test',true,$3,$3)",
        )
        .bind(owner_id)
        .bind(workspace_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        sqlx::query(
            "INSERT INTO sde_imports (id,source_version,source_label,source_checksum,status,active,started_at,completed_at) VALUES ($1,'test','fixture','facility-security-fixture','active',true,$2,$2)",
        )
        .bind(import_id)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO sde_solar_systems (import_id,solar_system_id,name_en,region_id,security_status,wormhole_class_id) VALUES ($1,30003425,'Erstet',10000042,0.4494,8)",
        )
        .bind(import_id)
        .execute(pool)
        .await
        .unwrap();
        WorkspaceId(workspace_id)
    }

    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn search_known_structures_does_not_mislabel_a_kspace_shattered_wormhole_system(
        pool: PgPool,
    ) {
        let workspace_id = fixture(&pool).await;
        let now = crate::db_now();
        sqlx::query(
            r#"
            INSERT INTO market_location_names
              (workspace_id,location_id,location_name,owner_id,solar_system_id,structure_type_id,resolved_by_connection_id,resolved_at,updated_at)
            VALUES ($1,1053384697058,'Erstet - Fixture Citadel',1,30003425,35832,NULL,$2,$2)
            "#,
        )
        .bind(workspace_id.0)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let repository = PgFacilityRepository::new(pool.clone());

        let results = repository
            .search_known_structures(workspace_id, "Erstet", 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].security_class, "lowSec");

        let single = repository
            .get_known_structure(workspace_id, 1_053_384_697_058)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(single.security_class, "lowSec");
    }

    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn load_profile_resolves_rig_applicability_from_the_active_sde(pool: PgPool) {
        let workspace_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let import_id = Uuid::new_v4();
        let now = crate::db_now();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Rig Heal Test',$2,$3,$3)")
            .bind(workspace_id).bind(owner_id).bind(now).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Rig Heal Test',true,$3,$3)")
            .bind(owner_id).bind(workspace_id).bind(now).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        sqlx::query("INSERT INTO sde_imports (id,source_version,source_label,source_checksum,status,active,started_at,completed_at) VALUES ($1,'test','fixture','rig-heal-fixture','active',true,$2,$2)")
            .bind(import_id).bind(now).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO sde_industry_target_filters (import_id,filter_id,name_en,category_ids,group_ids) VALUES ($1,3,'Ships','{6,32}','{}')")
            .bind(import_id).execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO sde_structure_rig_modifiers (import_id,type_id,material_reduction_percent,time_reduction_percent,high_sec_multiplier,low_sec_multiplier,null_sec_multiplier,compatible_structure_group_ids,rig_size,material_filter_ids,time_filter_ids) \
             VALUES ($1,43920,2,0,1,1,1,'{}',NULL,'{3}','{}')",
        ).bind(import_id).execute(&pool).await.unwrap();

        let repository = PgFacilityRepository::new(pool.clone());
        let command = iskworks_core::CreateFacilityProfileCommand {
            name: "Raitaru".into(),
            kind: iskworks_core::FacilityKind::UpwellStructure,
            role: FacilityRole::Manufacturing,
            structure_id: Some(1_234_567_890),
            structure_type_id: Some(35_825),
            structure_type_name: "Raitaru".into(),
            solar_system_id: Some(30_000_142),
            solar_system_name: "Jita".into(),
            security_class: SecurityClass::HighSec,
            material_reduction_percent: "0".into(),
            time_reduction_percent: "0".into(),
            job_cost_reduction_percent: "0".into(),
            facility_tax_percent: "0".into(),
            scc_surcharge_percent: "0".into(),
            alliance_surcharge_percent: "0".into(),
            fixed_supplemental_cost: String::new(),
            manual_system_cost_index: None,
            notes: String::new(),
            rigs: vec![iskworks_core::FacilityRigInput {
                slot_number: 1,
                type_id: 43_920,
                type_name: "Standup M-Set Basic Material Efficiency I".into(),
                material_reduction_percent: "2".into(),
                time_reduction_percent: "0".into(),
            }],
        };
        let created = repository
            .create(iskworks_core::parse_profile(WorkspaceId(workspace_id), command).unwrap())
            .await
            .unwrap();
        assert_eq!(
            created.rigs[0].applicability.material,
            iskworks_core::RigTargetFilter::Restricted {
                category_ids: [6, 32].into_iter().collect(),
                group_ids: std::collections::BTreeSet::new(),
            },
            "load_profile should resolve applicability from the active SDE",
        );
        assert_eq!(
            created.rigs[0].applicability.time,
            iskworks_core::RigTargetFilter::Unrestricted
        );
    }

    /// A reaction rig that references several target filters (the real
    /// "Standup L-Set Reactor Efficiency I", type 46496, covers filters 16 +
    /// 17 + 18 = every reaction) resolves to the *union* of their group sets on
    /// both the material and time bonus.
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn load_profile_resolves_a_multi_scope_reaction_rig_to_the_filter_union(pool: PgPool) {
        let workspace_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let import_id = Uuid::new_v4();
        let now = crate::db_now();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Reactor Rig Union',$2,$3,$3)")
            .bind(workspace_id).bind(owner_id).bind(now).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Reactor Rig Union',true,$3,$3)")
            .bind(owner_id).bind(workspace_id).bind(now).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        sqlx::query("INSERT INTO sde_imports (id,source_version,source_label,source_checksum,status,active,started_at,completed_at) VALUES ($1,'test','fixture','reactor-union-fixture','active',true,$2,$2)")
            .bind(import_id).bind(now).execute(&pool).await.unwrap();
        for (filter_id, name, groups) in [
            (16_i64, "Hybrid Reactions", "{974}"),
            (17, "Biochemical Reactions", "{712,4096}"),
            (18, "Composite Reactions", "{428,429,4932}"),
        ] {
            sqlx::query("INSERT INTO sde_industry_target_filters (import_id,filter_id,name_en,category_ids,group_ids) VALUES ($1,$2,$3,'{}',$4::bigint[])")
                .bind(import_id).bind(filter_id).bind(name).bind(groups).execute(&pool).await.unwrap();
        }
        sqlx::query(
            "INSERT INTO sde_reaction_rig_modifiers (import_id,type_id,material_reduction_percent,time_reduction_percent,high_sec_multiplier,low_sec_multiplier,null_sec_multiplier,compatible_structure_group_ids,rig_size,material_filter_ids,time_filter_ids) \
             VALUES ($1,46496,2,20,1,1,1,'{1406}',3,'{16,18,17}','{16,18,17}')",
        ).bind(import_id).execute(&pool).await.unwrap();

        let repository = PgFacilityRepository::new(pool.clone());
        let command = iskworks_core::CreateFacilityProfileCommand {
            name: "0-VG7A Reactions".into(),
            kind: iskworks_core::FacilityKind::UpwellStructure,
            role: FacilityRole::Reaction,
            structure_id: Some(1_234_567_891),
            structure_type_id: Some(35_836),
            structure_type_name: "Tatara".into(),
            solar_system_id: Some(30_000_142),
            solar_system_name: "Jita".into(),
            security_class: SecurityClass::NullSec,
            material_reduction_percent: "0".into(),
            time_reduction_percent: "0".into(),
            job_cost_reduction_percent: "0".into(),
            facility_tax_percent: "0".into(),
            scc_surcharge_percent: "0".into(),
            alliance_surcharge_percent: "0".into(),
            fixed_supplemental_cost: String::new(),
            manual_system_cost_index: None,
            notes: String::new(),
            rigs: vec![iskworks_core::FacilityRigInput {
                slot_number: 1,
                type_id: 46_496,
                type_name: "Standup L-Set Reactor Efficiency I".into(),
                material_reduction_percent: "2".into(),
                time_reduction_percent: "20".into(),
            }],
        };
        let created = repository
            .create(iskworks_core::parse_profile(WorkspaceId(workspace_id), command).unwrap())
            .await
            .unwrap();

        let union = iskworks_core::RigTargetFilter::Restricted {
            category_ids: std::collections::BTreeSet::new(),
            group_ids: [428, 429, 712, 974, 4096, 4932].into_iter().collect(),
        };
        assert_eq!(created.rigs[0].applicability.material, union);
        assert_eq!(created.rigs[0].applicability.time, union);
    }

    /// Rig applicability is never persisted: whatever the caller's domain
    /// object carries (here a stale Hybrid-only resolution, as if resolved
    /// against an SDE imported before the multi-filter fix) is discarded,
    /// and every load resolves from the *active* SDE. A later SDE re-import
    /// therefore reaches existing profiles without a re-save.
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn load_profile_resolves_applicability_from_the_active_sde_not_the_saved_profile(
        pool: PgPool,
    ) {
        let workspace_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let import_id = Uuid::new_v4();
        let now = crate::db_now();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Stale Rig',$2,$3,$3)")
            .bind(workspace_id).bind(owner_id).bind(now).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Stale Rig',true,$3,$3)")
            .bind(owner_id).bind(workspace_id).bind(now).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        sqlx::query("INSERT INTO sde_imports (id,source_version,source_label,source_checksum,status,active,started_at,completed_at) VALUES ($1,'test','fixture','stale-rig-fixture','active',true,$2,$2)")
            .bind(import_id).bind(now).execute(&pool).await.unwrap();
        for (filter_id, name, groups) in [
            (16_i64, "Hybrid Reactions", "{974}"),
            (17, "Biochemical Reactions", "{712,4096}"),
            (18, "Composite Reactions", "{428,429,4932}"),
        ] {
            sqlx::query("INSERT INTO sde_industry_target_filters (import_id,filter_id,name_en,category_ids,group_ids) VALUES ($1,$2,$3,'{}',$4::bigint[])")
                .bind(import_id).bind(filter_id).bind(name).bind(groups).execute(&pool).await.unwrap();
        }
        sqlx::query(
            "INSERT INTO sde_reaction_rig_modifiers (import_id,type_id,material_reduction_percent,time_reduction_percent,high_sec_multiplier,low_sec_multiplier,null_sec_multiplier,compatible_structure_group_ids,rig_size,material_filter_ids,time_filter_ids) \
             VALUES ($1,46496,2,20,1,1,1,'{1406}',3,'{16,18,17}','{16,18,17}')",
        ).bind(import_id).execute(&pool).await.unwrap();

        let repository = PgFacilityRepository::new(pool.clone());
        let command = iskworks_core::CreateFacilityProfileCommand {
            name: "0-VG7A Reactions".into(),
            kind: iskworks_core::FacilityKind::UpwellStructure,
            role: FacilityRole::Reaction,
            structure_id: Some(1_234_567_892),
            structure_type_id: Some(35_836),
            structure_type_name: "Tatara".into(),
            solar_system_id: Some(30_000_142),
            solar_system_name: "Jita".into(),
            security_class: SecurityClass::NullSec,
            material_reduction_percent: "0".into(),
            time_reduction_percent: "0".into(),
            job_cost_reduction_percent: "0".into(),
            facility_tax_percent: "0".into(),
            scc_surcharge_percent: "0".into(),
            alliance_surcharge_percent: "0".into(),
            fixed_supplemental_cost: String::new(),
            manual_system_cost_index: None,
            notes: String::new(),
            rigs: vec![iskworks_core::FacilityRigInput {
                slot_number: 1,
                type_id: 46_496,
                type_name: "Standup L-Set Reactor Efficiency I".into(),
                material_reduction_percent: "2".into(),
                time_reduction_percent: "20".into(),
            }],
        };
        let mut profile = iskworks_core::parse_profile(WorkspaceId(workspace_id), command).unwrap();
        let stale = iskworks_core::RigTargetFilter::Restricted {
            category_ids: std::collections::BTreeSet::new(),
            group_ids: [974].into_iter().collect(),
        };
        profile.rigs[0].applicability = iskworks_core::RigApplicability {
            material: stale.clone(),
            time: stale,
        };
        let created = repository.create(profile).await.unwrap();

        let union = iskworks_core::RigTargetFilter::Restricted {
            category_ids: std::collections::BTreeSet::new(),
            group_ids: [428, 429, 712, 974, 4096, 4932].into_iter().collect(),
        };
        assert_eq!(created.rigs[0].applicability.material, union);
        assert_eq!(created.rigs[0].applicability.time, union);
    }

    fn manual_command(name: &str) -> iskworks_core::CreateFacilityProfileCommand {
        iskworks_core::CreateFacilityProfileCommand {
            name: name.into(),
            kind: FacilityKind::Manual,
            role: FacilityRole::Manufacturing,
            structure_id: None,
            structure_type_id: None,
            structure_type_name: String::new(),
            solar_system_id: Some(30_003_425),
            solar_system_name: "Erstet".into(),
            security_class: SecurityClass::LowSec,
            material_reduction_percent: "1".into(),
            time_reduction_percent: "0".into(),
            job_cost_reduction_percent: "0".into(),
            facility_tax_percent: "0".into(),
            scc_surcharge_percent: "0".into(),
            alliance_surcharge_percent: "0".into(),
            fixed_supplemental_cost: "0".into(),
            manual_system_cost_index: None,
            notes: String::new(),
            rigs: Vec::new(),
        }
    }

    fn import_action(
        kind: iskworks_core::FacilityImportActionKind,
        item: iskworks_core::CreateFacilityProfileCommand,
        existing: Option<(FacilityProfileId, u64)>,
    ) -> iskworks_core::FacilityImportAction {
        iskworks_core::FacilityImportAction {
            kind,
            item,
            existing_id: existing.map(|(id, _)| id),
            expected_revision: existing.map(|(_, revision)| revision),
        }
    }

    fn statuses(outcomes: &[iskworks_core::FacilityImportItemOutcome]) -> Vec<String> {
        outcomes
            .iter()
            .map(|outcome| match &outcome.result {
                Ok(status) => status.as_str().to_string(),
                Err(error) => format!("failed: {error}"),
            })
            .collect()
    }

    /// An item whose write fails in the database aborts the whole import:
    /// the replace and create applied by earlier items are rolled back.
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn import_rolls_back_every_item_when_a_write_fails(pool: PgPool) {
        use iskworks_core::FacilityImportActionKind::{Create, Replace};
        let workspace_id = fixture(&pool).await;
        let repository = PgFacilityRepository::new(pool.clone());
        let existing = repository
            .create(iskworks_core::parse_profile(workspace_id, manual_command("Home")).unwrap())
            .await
            .unwrap();
        // A test-only constraint makes one valid item fail at write time.
        sqlx::query(
            "ALTER TABLE industry_facility_profiles ADD CONSTRAINT test_reject_poison CHECK (display_name <> 'Poison')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut renamed = manual_command("home");
        renamed.notes = "replaced by import".into();

        let error = repository
            .import(
                workspace_id,
                vec![
                    import_action(Replace, renamed, Some((existing.id, existing.revision))),
                    import_action(Create, manual_command("Fresh"), None),
                    import_action(Create, manual_command("Poison"), None),
                ],
            )
            .await
            .expect_err("the failing write aborts the import");
        assert!(matches!(error, FacilityError::Persistence(_)), "{error:?}");

        let profiles = repository.list(workspace_id).await.unwrap();
        assert_eq!(profiles.len(), 1, "no created profile persists");
        assert_eq!(profiles[0].id, existing.id);
        assert_eq!(profiles[0].name, "Home");
        assert_eq!(profiles[0].revision, existing.revision);
        assert_eq!(profiles[0].notes, "");
    }

    /// A successful mixed import keeps the per-item semantics: each item sees
    /// the writes of the items before it, and rejected items write nothing
    /// without aborting the others.
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn import_applies_mixed_creates_and_replaces_in_order(pool: PgPool) {
        use iskworks_core::FacilityImportActionKind::{Create, Replace, Skip, Unknown};
        let workspace_id = fixture(&pool).await;
        let repository = PgFacilityRepository::new(pool.clone());
        let existing = repository
            .create(iskworks_core::parse_profile(workspace_id, manual_command("Home")).unwrap())
            .await
            .unwrap();
        let mut first_replace = manual_command("Home");
        first_replace.notes = "first".into();
        let mut second_replace = manual_command("Home");
        second_replace.notes = "second".into();

        let outcomes = repository
            .import(
                workspace_id,
                vec![
                    import_action(Skip, manual_command("Ignored"), None),
                    import_action(Create, manual_command("Fresh"), None),
                    // Matches the profile created one item earlier.
                    import_action(Create, manual_command("fresh"), None),
                    import_action(Replace, first_replace, Some((existing.id, 1))),
                    // Sees the revision bumped by the previous replace.
                    import_action(Replace, second_replace.clone(), Some((existing.id, 1))),
                    import_action(Replace, second_replace, Some((existing.id, 2))),
                    import_action(Replace, manual_command("Nowhere"), Some((existing.id, 3))),
                    import_action(Replace, manual_command("Home"), None),
                    import_action(Create, manual_command(""), None),
                    import_action(Unknown, manual_command("Other"), None),
                ],
            )
            .await
            .unwrap();

        assert_eq!(
            outcomes.iter().map(|o| o.name.as_str()).collect::<Vec<_>>(),
            vec![
                "Ignored", "Fresh", "fresh", "Home", "Home", "Home", "Nowhere", "Home", "", "Other"
            ]
        );
        assert_eq!(
            statuses(&outcomes),
            vec![
                "skipped".to_string(),
                "created".into(),
                "failed: validation failed: A matching facility already exists.".into(),
                "replaced".into(),
                "failed: facility profile changed since it was loaded".into(),
                "replaced".into(),
                "failed: facility profile changed since it was loaded".into(),
                "failed: validation failed: Replacement facility ID is required.".into(),
                "failed: validation failed: Manual facilities require a name.".into(),
                "failed: validation failed: Unknown import action.".into(),
            ]
        );

        let profiles = repository.list(workspace_id).await.unwrap();
        assert_eq!(
            profiles
                .iter()
                .map(|p| (p.name.as_str(), p.revision, p.notes.as_str()))
                .collect::<Vec<_>>(),
            vec![("Fresh", 1, ""), ("Home", 3, "second")]
        );
        assert_eq!(profiles[1].id, existing.id);
    }

    /// Two active profiles sharing one EVE location: the first match follows
    /// list order (`display_name`), so renaming the first one in a replace
    /// makes the next item match the other -- as a per-item re-list would.
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn import_rematches_duplicates_in_list_order_after_a_rename(pool: PgPool) {
        use iskworks_core::FacilityImportActionKind::Replace;
        let workspace_id = fixture(&pool).await;
        let repository = PgFacilityRepository::new(pool.clone());
        let station = |name: &str| {
            let mut command = manual_command(name);
            command.kind = FacilityKind::NpcStation;
            command.structure_id = Some(60_003_760);
            command
        };
        let alpha = repository
            .create(iskworks_core::parse_profile(workspace_id, station("Alpha")).unwrap())
            .await
            .unwrap();
        let beta = repository
            .create(iskworks_core::parse_profile(workspace_id, station("Beta")).unwrap())
            .await
            .unwrap();

        let outcomes = repository
            .import(
                workspace_id,
                vec![
                    import_action(Replace, station("Zeta"), Some((alpha.id, 1))),
                    import_action(Replace, station("Beta 2"), Some((beta.id, 1))),
                ],
            )
            .await
            .unwrap();

        assert_eq!(statuses(&outcomes), vec!["replaced", "replaced"]);
        let names = repository
            .list(workspace_id)
            .await
            .unwrap()
            .into_iter()
            .map(|profile| profile.name)
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["Beta 2", "Zeta"]);
    }
}
