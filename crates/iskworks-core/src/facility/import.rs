//! Facility profile import: the per-item decision an import action makes
//! against the workspace's current profiles. Pure -- the storage layer
//! applies every decision of one import inside a single transaction, feeding
//! each item the profiles as they stand after the items before it.

use crate::WorkspaceId;

use super::parse::{find_active_facility_match, parse_profile};
use super::types::{
    CreateFacilityProfileCommand, FacilityError, FacilityProfileId, IndustryFacilityProfile,
};

/// What the user chose to do with one imported item.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum FacilityImportActionKind {
    Skip,
    Create,
    Replace,
    /// Any other wire value; rejected per item as "Unknown import action."
    Unknown,
}

impl FacilityImportActionKind {
    /// Maps the wire `action` string (`skip` / `create` / `replace`).
    #[must_use]
    pub fn from_wire(value: &str) -> Self {
        match value {
            "skip" => Self::Skip,
            "create" => Self::Create,
            "replace" => Self::Replace,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FacilityImportAction {
    pub kind: FacilityImportActionKind,
    pub item: CreateFacilityProfileCommand,
    pub existing_id: Option<FacilityProfileId>,
    pub expected_revision: Option<u64>,
}

/// The successful status of one import item.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum FacilityImportStatus {
    Skipped,
    Created,
    Replaced,
}

impl FacilityImportStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Skipped => "skipped",
            Self::Created => "created",
            Self::Replaced => "replaced",
        }
    }
}

/// One import item's result. `result` is `Err` when the item itself was
/// rejected (validation, duplicate, stale revision); such an item writes
/// nothing and does not abort the rest of the import.
#[derive(Debug)]
pub struct FacilityImportItemOutcome {
    pub name: String,
    pub result: Result<FacilityImportStatus, FacilityError>,
}

/// The write an accepted import item asks for.
#[derive(Debug, Clone)]
pub enum FacilityImportDecision {
    Skip,
    Create(IndustryFacilityProfile),
    Replace {
        existing_id: FacilityProfileId,
        expected_revision: u64,
        replacement: IndustryFacilityProfile,
    },
}

/// Decides one import action against `current` (the workspace's profiles,
/// in list order, including writes made by earlier items of the same
/// import). Check order is part of the contract -- it decides which message
/// an item that is wrong in several ways reports: identity/match first, then
/// profile validation, then the action-specific checks.
pub fn decide_facility_import(
    workspace_id: WorkspaceId,
    action: FacilityImportAction,
    current: &[IndustryFacilityProfile],
) -> Result<FacilityImportDecision, FacilityError> {
    if action.kind == FacilityImportActionKind::Skip {
        return Ok(FacilityImportDecision::Skip);
    }
    let matched = find_active_facility_match(&action.item, current)?
        .map(|(profile, _)| (profile.id, profile.revision));
    let replacement = parse_profile(workspace_id, action.item)?;
    match action.kind {
        FacilityImportActionKind::Create => {
            if matched.is_some() {
                return Err(FacilityError::Validation(
                    "A matching facility already exists.".into(),
                ));
            }
            Ok(FacilityImportDecision::Create(replacement))
        }
        FacilityImportActionKind::Replace => {
            let existing_id = action.existing_id.ok_or_else(|| {
                FacilityError::Validation("Replacement facility ID is required.".into())
            })?;
            let expected_revision = action.expected_revision.ok_or_else(|| {
                FacilityError::Validation("Replacement revision is required.".into())
            })?;
            let Some((matched_id, matched_revision)) = matched else {
                return Err(FacilityError::RevisionConflict);
            };
            if matched_id != existing_id || matched_revision != expected_revision {
                return Err(FacilityError::RevisionConflict);
            }
            Ok(FacilityImportDecision::Replace {
                existing_id,
                expected_revision,
                replacement,
            })
        }
        FacilityImportActionKind::Unknown => {
            Err(FacilityError::Validation("Unknown import action.".into()))
        }
        FacilityImportActionKind::Skip => unreachable!("handled above"),
    }
}
