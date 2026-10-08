//! Industry facility profiles.
//!
//! Split into focused submodules:
//! - [`types`] — the persisted profile, rig entries, create/update commands,
//!   preview result DTOs, [`FacilityError`], and the [`AdjustedPriceRepository`]
//!   port.
//! - [`rig_applicability`] — which produced items a fitted rig's bonuses may
//!   affect, and the rig-selection helpers previews funnel through.
//! - [`parse`] — deriving/matching facility identity and parsing a command
//!   into a persisted profile (and the inverse export).
//! - [`import`] — the per-item decision of a facility profile import.
//! - [`preview`] — adjusted-price EIV plus the manufacturing/reaction preview
//!   calculators and installation-cost breakdown.
//!
//! This file is the public surface: it re-exports every symbol callers used
//! before the split, so existing `use iskworks_core::{...}` paths are
//! unchanged.

mod import;
mod parse;
mod preview;
mod rig_applicability;
mod types;

#[cfg(test)]
mod tests_common;

pub use import::{
    decide_facility_import, FacilityImportAction, FacilityImportActionKind, FacilityImportDecision,
    FacilityImportItemOutcome, FacilityImportStatus,
};
pub use parse::{export_command, facility_identity, find_active_facility_match, parse_profile};
pub use preview::{
    calculate_adjusted_price_eiv, preview_blueprint_effects, preview_facility,
    preview_reaction_effects, preview_reaction_facility,
};
pub use rig_applicability::{ProductClassification, RigApplicability, RigTargetFilter};
pub use types::{
    AdjustedPriceEiv, AdjustedPriceRepository, CreateFacilityProfileCommand,
    DurationCalculationStep, EffectiveMaterialRequirement, EvidenceRefreshOverlay, FacilityError,
    FacilityIdentityBasis, FacilityKind, FacilityPlanPreview, FacilityPreviewCommand,
    FacilityProfileId, FacilityRig, FacilityRigInput, FacilityRole, IndustryFacilityProfile,
    InstallationCostBreakdown, ReactionFacilityPreviewCommand, SecurityClass,
    UpdateFacilityProfileCommand,
};

// Crate-internal calculation helpers reached via `crate::facility::...` from
// `plan_calculation` and `component_expansion`.
pub(crate) use preview::{decimal_to_ceiling_u64, installation_cost};
