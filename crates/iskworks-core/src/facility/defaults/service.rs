//! The Defaults view: every target with its default, suggestion and status,
//! plus each facility's bonus per target, and the writes that change it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use iskworks_sde::SdeReadRepository;
use rust_decimal::Decimal;
use serde::Serialize;

use super::{
    default_status, facility_target_bonus, suggest_facility, target_filters_from_sde,
    target_probes, DefaultStatus, FacilityDefault, FacilityDefaultAssignment,
    FacilityDefaultChange, FacilityDefaultsRepository, TargetFilter,
};
use crate::facility::{
    FacilityError, FacilityProfileId, FacilityRole, IndustryFacilityProfile, ProductClassification,
};
use crate::WorkspaceId;

/// A facility shown for a target, with its bonus for that target.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FacilityChoice {
    pub facility_profile_id: FacilityProfileId,
    pub facility_name: String,
    pub material_bonus_percent: Decimal,
    pub time_bonus_percent: Decimal,
}

/// One target row; `filter_id = None` is the role's fallback.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultTargetRow {
    pub role: FacilityRole,
    pub filter_id: Option<i64>,
    pub name: String,
    pub default: Option<FacilityChoice>,
    /// Only when it differs from the default.
    pub suggestion: Option<FacilityChoice>,
    pub status: DefaultStatus,
}

/// One active facility's bonus for one target of its role.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FacilityTargetBonusRow {
    pub facility_profile_id: FacilityProfileId,
    pub role: FacilityRole,
    pub filter_id: Option<i64>,
    pub material_bonus_percent: Decimal,
    pub time_bonus_percent: Decimal,
    pub covered: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FacilityDefaultsView {
    pub targets: Vec<DefaultTargetRow>,
    pub bonuses: Vec<FacilityTargetBonusRow>,
}

pub const FALLBACK_TARGET_NAME: &str = "Everything else";

#[derive(Clone)]
pub struct FacilityDefaultsService {
    repository: Arc<dyn FacilityDefaultsRepository>,
    sde: Arc<dyn SdeReadRepository>,
}

/// Everything one view or write needs, loaded once.
struct Snapshot {
    defaults: Vec<FacilityDefault>,
    profiles: Vec<IndustryFacilityProfile>,
    filters: Vec<TargetFilter>,
    probes: BTreeMap<i64, Vec<ProductClassification>>,
}

impl Snapshot {
    fn probes_for(&self, filter_id: Option<i64>) -> Option<&[ProductClassification]> {
        filter_id.map(|id| self.probes.get(&id).map_or(&[][..], Vec::as_slice))
    }

    fn active_profile(&self, id: FacilityProfileId) -> Option<&IndustryFacilityProfile> {
        self.profiles
            .iter()
            .find(|profile| profile.id == id && profile.archived_at.is_none())
    }
}

const ROLES: [FacilityRole; 2] = [FacilityRole::Manufacturing, FacilityRole::Reaction];

impl FacilityDefaultsService {
    #[must_use]
    pub fn new(
        repository: Arc<dyn FacilityDefaultsRepository>,
        sde: Arc<dyn SdeReadRepository>,
    ) -> Self {
        Self { repository, sde }
    }

    async fn snapshot(&self, workspace_id: WorkspaceId) -> Result<Snapshot, FacilityError> {
        let defaults = self.repository.list_facility_defaults(workspace_id).await?;
        let profiles = self.repository.list_facility_profiles(workspace_id).await?;
        let filters = target_filters_from_sde(
            self.sde
                .industry_target_filters_by_activity()
                .await
                .map_err(|error| FacilityError::Persistence(error.to_string()))?,
        );
        let group_ids: Vec<i64> = filters
            .iter()
            .flat_map(|filter| filter.group_ids.iter().copied())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let group_categories = self
            .sde
            .group_categories(&group_ids)
            .await
            .map_err(|error| FacilityError::Persistence(error.to_string()))?;
        let probes = filters
            .iter()
            .map(|filter| (filter.filter_id, target_probes(filter, &group_categories)))
            .collect();
        Ok(Snapshot {
            defaults,
            profiles,
            filters,
            probes,
        })
    }

    pub async fn view(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<FacilityDefaultsView, FacilityError> {
        let snapshot = self.snapshot(workspace_id).await?;
        build_view(&snapshot)
    }

    /// Applies `assignments` (a `None` facility clears the target) and
    /// returns the refreshed view. Records whether the facility's rigs cover
    /// the target now, so a later loss of coverage can be flagged.
    pub async fn assign(
        &self,
        workspace_id: WorkspaceId,
        assignments: Vec<FacilityDefaultAssignment>,
    ) -> Result<FacilityDefaultsView, FacilityError> {
        let snapshot = self.snapshot(workspace_id).await?;
        let mut changes = Vec::with_capacity(assignments.len());
        for assignment in assignments {
            let Some(facility_profile_id) = assignment.facility_profile_id else {
                // Clearing is allowed for any target, including one the
                // active SDE no longer knows.
                changes.push(FacilityDefaultChange::Clear {
                    role: assignment.role,
                    target_filter_id: assignment.filter_id,
                });
                continue;
            };
            if let Some(filter_id) = assignment.filter_id {
                let known = snapshot
                    .filters
                    .iter()
                    .any(|filter| filter.filter_id == filter_id && filter.role == assignment.role);
                if !known {
                    return Err(FacilityError::Validation(format!(
                        "target {filter_id} is not a {} target",
                        role_label(assignment.role)
                    )));
                }
            }
            let profile = snapshot
                .active_profile(facility_profile_id)
                .ok_or_else(|| {
                    FacilityError::Validation(
                        "default facility was not found or is archived".into(),
                    )
                })?;
            if profile.role != assignment.role {
                return Err(FacilityError::Validation(
                    "default facility has a different role".into(),
                ));
            }
            let covered = match snapshot.probes_for(assignment.filter_id) {
                Some(probes) => facility_target_bonus(profile, Some(probes))?.covered,
                None => false,
            };
            changes.push(FacilityDefaultChange::Set(FacilityDefault {
                role: assignment.role,
                target_filter_id: assignment.filter_id,
                facility_profile_id,
                rig_covered_at_assignment: covered,
            }));
        }
        self.repository
            .apply_facility_default_changes(workspace_id, changes)
            .await?;
        self.view(workspace_id).await
    }
}

fn role_label(role: FacilityRole) -> &'static str {
    match role {
        FacilityRole::Manufacturing => "manufacturing",
        FacilityRole::Reaction => "reaction",
    }
}

fn choice(
    profile: &IndustryFacilityProfile,
    material_bonus_percent: Decimal,
    time_bonus_percent: Decimal,
) -> FacilityChoice {
    FacilityChoice {
        facility_profile_id: profile.id,
        facility_name: profile.name.clone(),
        material_bonus_percent,
        time_bonus_percent,
    }
}

fn build_view(snapshot: &Snapshot) -> Result<FacilityDefaultsView, FacilityError> {
    let mut targets = Vec::new();
    let mut bonuses = Vec::new();
    for role in ROLES {
        let stored = |filter_id: Option<i64>| {
            snapshot
                .defaults
                .iter()
                .find(|default| default.role == role && default.target_filter_id == filter_id)
                .filter(|default| {
                    snapshot
                        .active_profile(default.facility_profile_id)
                        .is_some()
                })
        };
        let has_fallback = stored(None).is_some();
        let role_targets = snapshot
            .filters
            .iter()
            .filter(|filter| filter.role == role)
            .map(|filter| (Some(filter.filter_id), filter.name.clone()))
            .chain(std::iter::once((None, FALLBACK_TARGET_NAME.to_string())));
        for (filter_id, name) in role_targets {
            let probes = snapshot.probes_for(filter_id);
            let default = stored(filter_id);
            let (default_choice, covered_now) = match default {
                Some(default) => {
                    let profile = snapshot
                        .active_profile(default.facility_profile_id)
                        .expect("stored() keeps only active facilities");
                    let bonus = facility_target_bonus(profile, probes)?;
                    (
                        Some(choice(
                            profile,
                            bonus.material_bonus_percent,
                            bonus.time_bonus_percent,
                        )),
                        bonus.covered,
                    )
                }
                None => (None, false),
            };
            // A suggestion is only worth showing when accepting it would
            // change the facility new Builds get: not the target's own
            // default, nor (with no default) the fallback it already uses.
            let current = default
                .map(|default| default.facility_profile_id)
                .or_else(|| {
                    filter_id
                        .and(stored(None))
                        .map(|fallback| fallback.facility_profile_id)
                });
            let suggestion = suggest_facility(&snapshot.profiles, role, probes)?
                .filter(|(profile, _)| current != Some(profile.id))
                .map(|(profile, bonus)| {
                    choice(
                        profile,
                        bonus.material_bonus_percent,
                        bonus.time_bonus_percent,
                    )
                });
            let status = default_status(default, covered_now, has_fallback && filter_id.is_some());
            targets.push(DefaultTargetRow {
                role,
                filter_id,
                name,
                default: default_choice,
                suggestion,
                status,
            });
            for profile in snapshot
                .profiles
                .iter()
                .filter(|profile| profile.role == role && profile.archived_at.is_none())
            {
                let bonus = facility_target_bonus(profile, probes)?;
                bonuses.push(FacilityTargetBonusRow {
                    facility_profile_id: profile.id,
                    role,
                    filter_id,
                    material_bonus_percent: bonus.material_bonus_percent,
                    time_bonus_percent: bonus.time_bonus_percent,
                    covered: bonus.covered,
                });
            }
        }
        // Stored defaults for a filter the active SDE no longer has.
        for default in snapshot.defaults.iter().filter(|default| {
            default.role == role
                && default.target_filter_id.is_some_and(|id| {
                    !snapshot
                        .filters
                        .iter()
                        .any(|filter| filter.filter_id == id && filter.role == role)
                })
        }) {
            let filter_id = default.target_filter_id.expect("filtered to targets");
            targets.push(DefaultTargetRow {
                role,
                filter_id: Some(filter_id),
                name: format!("Unknown target {filter_id}"),
                default: snapshot
                    .active_profile(default.facility_profile_id)
                    .map(|profile| choice(profile, Decimal::ZERO, Decimal::ZERO)),
                suggestion: None,
                status: DefaultStatus::UnknownTarget,
            });
        }
    }
    Ok(FacilityDefaultsView { targets, bonuses })
}
