//! Facility profile domain/data types: the persisted profile, its rig entries,
//! the create/update commands, the preview result DTOs, and the shared error.
//! Behaviour lives in the sibling modules (`rig_applicability`, `parse`,
//! `preview`).

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{Money, WorkspaceId};

/// The canonical formula identifier stamped into every manufacturing preview
/// DTO's `formula_version` field. Bump only alongside a deliberate change to
/// the manufacturing math.
pub const FACILITY_FORMULA_VERSION: &str = "eve-manufacturing-facility-v1";
/// The reaction counterpart of [`FACILITY_FORMULA_VERSION`].
pub const REACTION_FACILITY_FORMULA_VERSION: &str = "eve-reaction-facility-v1";

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct EvidenceRefreshOverlay {
    pub pending: bool,
    pub last_error: Option<String>,
}

#[async_trait]
pub trait AdjustedPriceRepository: Send + Sync {
    async fn latest_adjusted_prices(
        &self,
        type_ids: &[i64],
        as_of: DateTime<Utc>,
    ) -> Result<BTreeMap<i64, Decimal>, crate::InventoryError>;

    async fn latest_adjusted_price_observed_at(
        &self,
        _type_ids: &[i64],
    ) -> Result<Option<DateTime<Utc>>, crate::InventoryError> {
        Ok(None)
    }

    async fn adjusted_price_refresh_overlay(
        &self,
    ) -> Result<EvidenceRefreshOverlay, crate::InventoryError> {
        Ok(EvidenceRefreshOverlay::default())
    }

    async fn register_adjusted_price_refresh(
        &self,
        _now: DateTime<Utc>,
    ) -> Result<(), crate::InventoryError> {
        Ok(())
    }

    async fn register_system_cost_index(
        &self,
        _solar_system_id: i64,
        _now: DateTime<Utc>,
    ) -> Result<(), crate::InventoryError> {
        Ok(())
    }

    async fn latest_system_cost_index(
        &self,
        _solar_system_id: i64,
    ) -> Result<Option<(Decimal, DateTime<Utc>)>, crate::InventoryError> {
        Ok(None)
    }

    async fn system_cost_index_refresh_overlay(
        &self,
        _solar_system_id: i64,
    ) -> Result<EvidenceRefreshOverlay, crate::InventoryError> {
        Ok(EvidenceRefreshOverlay::default())
    }

    async fn prioritize_adjusted_price_refresh(
        &self,
        _now: DateTime<Utc>,
    ) -> Result<bool, crate::InventoryError> {
        Ok(false)
    }

    async fn prioritize_system_cost_index_refresh(
        &self,
        _solar_system_id: i64,
        _now: DateTime<Utc>,
    ) -> Result<bool, crate::InventoryError> {
        Ok(false)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FacilityProfileId(pub Uuid);

impl FacilityProfileId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for FacilityProfileId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FacilityKind {
    NpcStation,
    UpwellStructure,
    Manual,
}

/// Which production activity a facility profile can run. Manufacturing and
/// reaction facilities are genuinely different structure lines in EVE with
/// non-overlapping bonuses (Engineering Complexes vs. Refineries) and rig
/// slots, so a profile is unambiguous about which recipe kind it applies to
/// rather than letting `FacilityKind::UpwellStructure` cover both.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FacilityRole {
    Manufacturing,
    Reaction,
}

impl Default for FacilityRole {
    /// Every facility profile that existed before reactions were modeled was
    /// a manufacturing facility; this keeps historical profiles valid.
    fn default() -> Self {
        Self::Manufacturing
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SecurityClass {
    HighSec,
    LowSec,
    NullSec,
    Wormhole,
    Unknown,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FacilityRig {
    pub slot_number: u8,
    pub type_id: i64,
    pub type_name: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub material_reduction_percent: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub time_reduction_percent: Decimal,
    /// What this rig's bonuses are allowed to affect, resolved from the SDE
    /// for the owning profile's role at save time. Defaults to
    /// `Unrestricted`/`Unrestricted` so a profile persisted before rig
    /// applicability existed keeps its exact current behavior until re-saved.
    #[serde(default)]
    pub applicability: super::rig_applicability::RigApplicability,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndustryFacilityProfile {
    pub id: FacilityProfileId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub kind: FacilityKind,
    #[serde(default)]
    pub role: FacilityRole,
    pub structure_id: Option<i64>,
    pub structure_type_id: Option<i64>,
    pub structure_type_name: String,
    pub solar_system_id: Option<i64>,
    pub solar_system_name: String,
    pub security_class: SecurityClass,
    #[serde(with = "rust_decimal::serde::str")]
    pub material_reduction_percent: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub time_reduction_percent: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub job_cost_reduction_percent: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub facility_tax_percent: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub scc_surcharge_percent: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub alliance_surcharge_percent: Decimal,
    pub fixed_supplemental_cost: Money,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub manual_system_cost_index: Option<Decimal>,
    pub notes: String,
    pub rigs: Vec<FacilityRig>,
    pub archived_at: Option<DateTime<Utc>>,
    pub revision: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FacilityRigInput {
    pub slot_number: u8,
    pub type_id: i64,
    pub type_name: String,
    #[serde(default)]
    pub material_reduction_percent: String,
    #[serde(default)]
    pub time_reduction_percent: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFacilityProfileCommand {
    pub name: String,
    pub kind: FacilityKind,
    #[serde(default)]
    pub role: FacilityRole,
    pub structure_id: Option<i64>,
    pub structure_type_id: Option<i64>,
    #[serde(default)]
    pub structure_type_name: String,
    pub solar_system_id: Option<i64>,
    #[serde(default)]
    pub solar_system_name: String,
    pub security_class: SecurityClass,
    #[serde(default)]
    pub material_reduction_percent: String,
    #[serde(default)]
    pub time_reduction_percent: String,
    #[serde(default)]
    pub job_cost_reduction_percent: String,
    #[serde(default)]
    pub facility_tax_percent: String,
    #[serde(default)]
    pub scc_surcharge_percent: String,
    #[serde(default)]
    pub alliance_surcharge_percent: String,
    #[serde(default)]
    pub fixed_supplemental_cost: String,
    pub manual_system_cost_index: Option<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub rigs: Vec<FacilityRigInput>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum FacilityIdentity {
    EveLocation {
        role: FacilityRole,
        location_id: i64,
    },
    Manual {
        role: FacilityRole,
        solar_system_id: i64,
        normalized_name: String,
    },
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FacilityIdentityBasis {
    EveLocation,
    ManualName,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFacilityProfileCommand {
    pub expected_revision: u64,
    #[serde(flatten)]
    pub profile: CreateFacilityProfileCommand,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveMaterialRequirement {
    pub type_id: i64,
    pub type_name: String,
    pub sort_order: u32,
    pub base_quantity_per_run: u64,
    pub runs: u64,
    pub base_extended_quantity: u64,
    pub blueprint_me: Option<u8>,
    /// The combined **facility** material multiplier actually applied to
    /// `base_extended_quantity` -- `(1 - structure_material_reduction%/100)`
    /// times the product of every *applicable* rig's
    /// `(1 - rig_material_reduction%/100)`. Exactly `1` when the plan used no
    /// facility. Blueprint ME is **not** folded in here (it stays on
    /// `blueprint_me`); this is the piece the `calculation_trace` string
    /// otherwise only exposes as text. `#[serde(default)]` so a revision
    /// persisted before this field still deserializes as `1`.
    #[serde(default = "decimal_one", with = "rust_decimal::serde::str")]
    pub facility_material_factor: Decimal,
    /// Summed over every job of the operation's `JobSplit`, each job rounded
    /// on its own (see `job_count`).
    pub final_required_quantity: u64,
    /// How many industry jobs `runs` splits into -- more than one when a
    /// blueprint copy licenses fewer runs than requested. `#[serde(default)]`
    /// so a revision persisted before this field reads as one job.
    #[serde(default = "one_job")]
    pub job_count: u64,
    pub calculation_trace: String,
    pub formula_version: String,
    pub recipe_fingerprint: String,
}

pub(crate) fn decimal_one() -> Decimal {
    Decimal::ONE
}

fn one_job() -> u64 {
    1
}

/// The reaction counterpart of `EffectiveMaterialRequirement`. No
/// `blueprint_me` field at all (rather than defaulting it to 0) so callers
/// can't silently treat a reaction line as ME-adjusted -- reaction formulas
/// have no material efficiency concept in EVE.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveReactionMaterialRequirement {
    pub type_id: i64,
    pub type_name: String,
    pub sort_order: u32,
    pub base_quantity_per_run: u64,
    pub runs: u64,
    pub base_extended_quantity: u64,
    /// The combined rig material multiplier actually applied (reactions get
    /// no structure discount) -- `1` with no facility or no applicable
    /// material rig. See [`EffectiveMaterialRequirement::facility_material_factor`].
    #[serde(default = "decimal_one", with = "rust_decimal::serde::str")]
    pub facility_material_factor: Decimal,
    pub final_required_quantity: u64,
    pub calculation_trace: String,
    pub formula_version: String,
    pub recipe_fingerprint: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationCostBreakdown {
    pub complete: bool,
    pub estimated_item_value: Option<Money>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub system_cost_index: Option<Decimal>,
    pub unmodified_system_index_cost: Option<Money>,
    #[serde(with = "rust_decimal::serde::str")]
    pub job_cost_reduction_percent: Decimal,
    pub system_index_cost: Option<Money>,
    pub facility_tax: Option<Money>,
    pub scc_surcharge: Option<Money>,
    pub alliance_surcharge: Option<Money>,
    pub fixed_supplemental_cost: Money,
    pub total: Option<Money>,
    pub warnings: Vec<String>,
    pub formula_version: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DurationCalculationStep {
    pub label: String,
    pub detail: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub multiplier: Decimal,
    pub running_duration_seconds: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FacilityPlanPreview {
    pub profile: IndustryFacilityProfile,
    pub blueprint_me: Option<u8>,
    pub blueprint_te: Option<u8>,
    pub requirements: Vec<EffectiveMaterialRequirement>,
    /// The product of every *applicable* rig's `(1 - material_reduction%/100)`
    /// -- the rig share of each requirement's `facility_material_factor`.
    /// A fitted rig skipped as non-applicable to the product is not folded
    /// in. `#[serde(default)]` so a preview persisted before this field
    /// still deserializes as `1`.
    #[serde(default = "decimal_one", with = "rust_decimal::serde::str")]
    pub rig_material_factor: Decimal,
    pub planned_duration_seconds: Option<u64>,
    pub duration_steps: Vec<DurationCalculationStep>,
    pub installation_cost: InstallationCostBreakdown,
    pub warnings: Vec<String>,
    pub formula_version: String,
}

/// The reaction counterpart of `FacilityPlanPreview`. No `blueprint_me`/
/// `blueprint_te` fields -- reaction formulas have neither in EVE, so
/// there's no blueprint-level input to preview alongside the facility rig
/// bonuses.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionFacilityPlanPreview {
    pub profile: IndustryFacilityProfile,
    pub requirements: Vec<EffectiveReactionMaterialRequirement>,
    /// The product of every *applicable* rig's `(1 - material_reduction%/100)`
    /// -- the rig share of each requirement's `facility_material_factor`.
    /// A fitted rig skipped as non-applicable to the product is not folded
    /// in. `#[serde(default)]` so a preview persisted before this field
    /// still deserializes as `1`.
    #[serde(default = "decimal_one", with = "rust_decimal::serde::str")]
    pub rig_material_factor: Decimal,
    pub planned_duration_seconds: Option<u64>,
    pub duration_steps: Vec<DurationCalculationStep>,
    pub installation_cost: InstallationCostBreakdown,
    pub warnings: Vec<String>,
    pub formula_version: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjustedPriceEiv {
    pub value: Option<Money>,
    pub missing_type_ids: Vec<i64>,
}

/// A live Build's manufacturing facility selection. The Build always
/// calculates against the profile's *current* settings, resolved by
/// `facility_profile_id` -- there is no recorded revision to consult (a
/// `FacilityProfile` is mutable; only editing it uses optimistic
/// concurrency). Old persisted drafts / requests may still carry an
/// `expectedProfileRevision` key; serde ignores it.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FacilityPreviewCommand {
    pub facility_profile_id: FacilityProfileId,
    #[serde(default)]
    pub blueprint_me: u8,
    #[serde(default)]
    pub blueprint_te: u8,
    pub estimated_item_value: Option<String>,
}

/// The reaction counterpart of `FacilityPreviewCommand`. No blueprint
/// ME/TE fields -- reaction formulas have neither in EVE, matching
/// `preview_reaction_facility`'s own signature.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionFacilityPreviewCommand {
    pub facility_profile_id: FacilityProfileId,
    pub estimated_item_value: Option<String>,
}

#[derive(Debug, Error)]
pub enum FacilityError {
    #[error("facility profile was not found")]
    NotFound,
    #[error("facility profile is archived")]
    Archived,
    #[error("facility profile changed since it was loaded")]
    RevisionConflict,
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("facility calculation overflowed")]
    ArithmeticOverflow,
    #[error("persistence failed: {0}")]
    Persistence(String),
}

impl From<EffectiveReactionMaterialRequirement> for EffectiveMaterialRequirement {
    fn from(value: EffectiveReactionMaterialRequirement) -> Self {
        Self {
            type_id: value.type_id,
            type_name: value.type_name,
            sort_order: value.sort_order,
            base_quantity_per_run: value.base_quantity_per_run,
            runs: value.runs,
            base_extended_quantity: value.base_extended_quantity,
            blueprint_me: None,
            facility_material_factor: value.facility_material_factor,
            final_required_quantity: value.final_required_quantity,
            job_count: 1,
            calculation_trace: value.calculation_trace,
            formula_version: value.formula_version,
            recipe_fingerprint: value.recipe_fingerprint,
        }
    }
}

impl From<ReactionFacilityPlanPreview> for FacilityPlanPreview {
    fn from(value: ReactionFacilityPlanPreview) -> Self {
        Self {
            profile: value.profile,
            blueprint_me: None,
            blueprint_te: None,
            requirements: value.requirements.into_iter().map(Into::into).collect(),
            rig_material_factor: value.rig_material_factor,
            planned_duration_seconds: value.planned_duration_seconds,
            duration_steps: value.duration_steps,
            installation_cost: value.installation_cost,
            warnings: value.warnings,
            formula_version: value.formula_version,
        }
    }
}
