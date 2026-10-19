//! Workspace facility defaults per EVE industry target filter.
//!
//! A default maps one of a role's `industryTargetFilters` (or the role's
//! fallback, `target_filter_id = None`) to a facility profile. New Builds take
//! their facility from the most specific target matching their product that
//! has a default, then from the role's fallback.
//!
//! This module is pure policy: target specificity, resolution, and the
//! per-target bonus/coverage/suggestion math the Defaults view shows.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::preview::reduction_factor;
use super::rig_applicability::{rig_target_applies, ProductClassification, RigTargetFilter};
use super::types::{
    FacilityError, FacilityProfileId, FacilityRig, FacilityRole, IndustryFacilityProfile,
};

/// One EVE industry target filter, tagged with the role whose rigs reference
/// it (manufacturing rigs use 1-15, reaction rigs 16-18 in the current SDE).
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TargetFilter {
    pub filter_id: i64,
    pub role: FacilityRole,
    pub name: String,
    pub category_ids: BTreeSet<i64>,
    pub group_ids: BTreeSet<i64>,
}

/// A stored default: `target_filter_id = None` is the role's fallback.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FacilityDefault {
    pub role: FacilityRole,
    pub target_filter_id: Option<i64>,
    pub facility_profile_id: FacilityProfileId,
    /// Whether a rig on the facility covered the target when the default was
    /// written, so the Defaults view warns only when coverage is later lost,
    /// not when an unrigged facility was picked on purpose.
    pub rig_covered_at_assignment: bool,
}

/// One requested change: `facility_profile_id = None` clears the target.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FacilityDefaultAssignment {
    pub role: FacilityRole,
    pub filter_id: Option<i64>,
    pub facility_profile_id: Option<FacilityProfileId>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DefaultSource {
    Target(i64),
    Fallback,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ResolvedDefault {
    pub facility_profile_id: FacilityProfileId,
    pub source: DefaultSource,
}

/// The `role`'s targets admitting `product`, most specific first:
/// a group match beats a category-only match, then fewer listed categories
/// win, then the lower filter id.
pub fn matching_targets(
    role: FacilityRole,
    product: ProductClassification,
    filters: &[TargetFilter],
) -> Vec<&TargetFilter> {
    let mut matches: Vec<(&TargetFilter, bool)> = filters
        .iter()
        .filter(|filter| filter.role == role)
        .filter_map(|filter| {
            let by_group = product
                .group_id
                .is_some_and(|id| filter.group_ids.contains(&id));
            let by_category = product
                .category_id
                .is_some_and(|id| filter.category_ids.contains(&id));
            (by_group || by_category).then_some((filter, by_group))
        })
        .collect();
    matches.sort_by(|(a, a_group), (b, b_group)| {
        b_group
            .cmp(a_group)
            .then(a.category_ids.len().cmp(&b.category_ids.len()))
            .then(a.filter_id.cmp(&b.filter_id))
    });
    matches.into_iter().map(|(filter, _)| filter).collect()
}

/// The default facility for a new `role` job producing `product`: the most
/// specific matching target with a default, else the role's fallback.
pub fn resolve_default(
    role: FacilityRole,
    product: ProductClassification,
    filters: &[TargetFilter],
    defaults: &[FacilityDefault],
) -> Option<ResolvedDefault> {
    let default_for = |target: Option<i64>| {
        defaults
            .iter()
            .find(|default| default.role == role && default.target_filter_id == target)
    };
    matching_targets(role, product, filters)
        .into_iter()
        .find_map(|filter| {
            default_for(Some(filter.filter_id)).map(|default| ResolvedDefault {
                facility_profile_id: default.facility_profile_id,
                source: DefaultSource::Target(filter.filter_id),
            })
        })
        .or_else(|| {
            default_for(None).map(|default| ResolvedDefault {
                facility_profile_id: default.facility_profile_id,
                source: DefaultSource::Fallback,
            })
        })
}

/// The classifications standing in for "everything `filter` targets": each
/// listed group with its category (from `group_categories`), and each listed
/// category on its own. A rig covers the target only if it admits all of them.
pub fn target_probes(
    filter: &TargetFilter,
    group_categories: &BTreeMap<i64, i64>,
) -> Vec<ProductClassification> {
    filter
        .group_ids
        .iter()
        .map(|group_id| ProductClassification {
            category_id: group_categories.get(group_id).copied(),
            group_id: Some(*group_id),
        })
        .chain(
            filter
                .category_ids
                .iter()
                .map(|category_id| ProductClassification {
                    category_id: Some(*category_id),
                    group_id: None,
                }),
        )
        .collect()
}

/// A facility's effective bonus for one target (or the fallback).
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetBonus {
    pub material_bonus_percent: Decimal,
    pub time_bonus_percent: Decimal,
    /// At least one target-restricted rig covers the whole target.
    pub covered: bool,
}

/// `probes = None` is the fallback: only unrestricted bonuses count.
/// Otherwise a rig counts when its filter admits every probe; a rig that
/// covers only part of the target is left out. Reactions have no structure
/// base bonus, matching `preview_reaction_facility`.
pub fn facility_target_bonus(
    profile: &IndustryFacilityProfile,
    probes: Option<&[ProductClassification]>,
) -> Result<TargetBonus, FacilityError> {
    let covers = |filter: &RigTargetFilter| match (filter, probes) {
        (RigTargetFilter::Unrestricted, _) => true,
        (RigTargetFilter::Restricted { .. }, None) => false,
        (RigTargetFilter::Restricted { .. }, Some(probes)) => {
            !probes.is_empty()
                && probes
                    .iter()
                    .all(|probe| rig_target_applies(filter, *probe))
        }
    };
    let restricted_cover = |filter: &RigTargetFilter| {
        matches!(filter, RigTargetFilter::Restricted { .. }) && covers(filter)
    };
    let (material_base, time_base) = match profile.role {
        FacilityRole::Manufacturing => (
            profile.material_reduction_percent,
            profile.time_reduction_percent,
        ),
        FacilityRole::Reaction => (Decimal::ZERO, Decimal::ZERO),
    };
    let rigs_for = |filter_of: fn(&FacilityRig) -> &RigTargetFilter| {
        profile
            .rigs
            .iter()
            .filter(move |rig| covers(filter_of(rig)))
    };
    let material_factor = reduction_factor(
        material_base,
        rigs_for(|rig| &rig.applicability.material).map(|rig| rig.material_reduction_percent),
    )?;
    let time_factor = reduction_factor(
        time_base,
        rigs_for(|rig| &rig.applicability.time).map(|rig| rig.time_reduction_percent),
    )?;
    let covered = profile.rigs.iter().any(|rig| {
        restricted_cover(&rig.applicability.material) || restricted_cover(&rig.applicability.time)
    });
    Ok(TargetBonus {
        material_bonus_percent: as_percent(material_factor),
        time_bonus_percent: as_percent(time_factor),
        covered,
    })
}

fn as_percent(factor: Decimal) -> Decimal {
    ((Decimal::ONE - factor) * Decimal::ONE_HUNDRED).normalize()
}

/// The facility to suggest for a target: among the role's active profiles
/// (with a covering rig, unless this is the fallback), the highest material
/// bonus, then time bonus, then the lowest facility tax, then the name.
pub fn suggest_facility<'a>(
    profiles: &'a [IndustryFacilityProfile],
    role: FacilityRole,
    probes: Option<&[ProductClassification]>,
) -> Result<Option<(&'a IndustryFacilityProfile, TargetBonus)>, FacilityError> {
    let mut best: Option<(&IndustryFacilityProfile, TargetBonus)> = None;
    for profile in profiles
        .iter()
        .filter(|profile| profile.role == role && profile.archived_at.is_none())
    {
        let bonus = facility_target_bonus(profile, probes)?;
        if probes.is_some() && !bonus.covered {
            continue;
        }
        let better = match &best {
            None => true,
            Some((current, current_bonus)) => {
                rank(profile, &bonus, current, current_bonus) == Ordering::Less
            }
        };
        if better {
            best = Some((profile, bonus));
        }
    }
    Ok(best)
}

fn rank(
    a: &IndustryFacilityProfile,
    a_bonus: &TargetBonus,
    b: &IndustryFacilityProfile,
    b_bonus: &TargetBonus,
) -> Ordering {
    b_bonus
        .material_bonus_percent
        .cmp(&a_bonus.material_bonus_percent)
        .then(b_bonus.time_bonus_percent.cmp(&a_bonus.time_bonus_percent))
        .then(a.facility_tax_percent.cmp(&b.facility_tax_percent))
        .then(a.name.cmp(&b.name))
}

/// How a target row reads in the Defaults view.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DefaultStatus {
    /// Has a default.
    Set,
    /// Has a default whose rigs covered the target when assigned but no
    /// longer do.
    CoverageLost,
    /// No default; the role's fallback applies.
    Fallback,
    /// No default and no fallback.
    None,
    /// The stored filter is absent from the active SDE.
    UnknownTarget,
}

/// `covered_now` is the default facility's current coverage of the target
/// (ignored for the fallback row, which has no target to cover).
pub fn default_status(
    default: Option<&FacilityDefault>,
    covered_now: bool,
    has_fallback: bool,
) -> DefaultStatus {
    match default {
        Some(default)
            if default.target_filter_id.is_some()
                && default.rig_covered_at_assignment
                && !covered_now =>
        {
            DefaultStatus::CoverageLost
        }
        Some(_) => DefaultStatus::Set,
        None if has_fallback => DefaultStatus::Fallback,
        None => DefaultStatus::None,
    }
}

#[cfg(test)]
mod tests;
