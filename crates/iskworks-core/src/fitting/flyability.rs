//! Who can fly a fit: its skill requirements from the SDE, checked against a
//! character's synced skills.
//!
//! A character meets a requirement by its **active** level, not its trained
//! one: an Alpha clone caps trained skills, and a capped level can't fly
//! anything. Training times come from the skill plan evaluation over the
//! requirements' prerequisite closure.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use iskworks_sde::{RequiredSkill, SkillDefinition};
use serde::Serialize;

use super::{FitInput, SlotKind};
use crate::skill_plans::{
    evaluate_plan, normalize_plan, PlanEntry, PlanEntryStatus, SkillGraph, SkillPlanError,
};
use crate::skills::CharacterSkillsSnapshot;

/// A fit's direct skill requirements: what the SDE lists on each type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FitRequirements {
    /// The hull's, by skill id.
    pub hull: Vec<PlanEntry>,
    /// The hull's and every fitted item's and charge's (cargo carries no
    /// requirement), at the highest level any of them needs, by skill id.
    pub all: Vec<PlanEntry>,
    /// Each item's (with its charge's), by item index; items that need
    /// nothing are left out.
    pub by_item: BTreeMap<usize, Vec<PlanEntry>>,
}

/// The direct requirements of `fit`, given each type's requirements.
#[must_use]
pub fn fit_requirements(
    fit: &FitInput,
    required_by_type: &BTreeMap<i64, Vec<RequiredSkill>>,
) -> FitRequirements {
    let of = |type_id: i64| required_by_type.get(&type_id).into_iter().flatten();
    let hull = highest(of(fit.ship_type_id));
    let mut by_item = BTreeMap::new();
    for (index, item) in fit.items.iter().enumerate() {
        if item.slot_kind == SlotKind::Cargo {
            continue;
        }
        let needs = highest(of(item.type_id).chain(item.charge_type_id.into_iter().flat_map(of)));
        if !needs.is_empty() {
            by_item.insert(index, needs);
        }
    }
    let all = highest(
        hull.iter()
            .chain(by_item.values().flatten())
            .map(|entry| RequiredSkill {
                skill_type_id: entry.skill_type_id,
                level: entry.level,
            })
            .collect::<Vec<_>>()
            .iter(),
    );
    FitRequirements { hull, all, by_item }
}

/// One entry per skill at the highest level asked for, by skill id.
fn highest<'a>(requirements: impl Iterator<Item = &'a RequiredSkill>) -> Vec<PlanEntry> {
    let mut levels = BTreeMap::new();
    for requirement in requirements {
        let level = levels.entry(requirement.skill_type_id).or_insert(0);
        *level = requirement.level.max(*level);
    }
    levels
        .into_iter()
        .map(|(skill_type_id, level)| PlanEntry {
            skill_type_id,
            level,
        })
        .collect()
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FlyabilityStatus {
    CanUseAll,
    /// Flies the hull but can't use every fitted item.
    CanFlyHull,
    Cannot,
}

/// A skill a character doesn't have at the level the fit needs.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingSkill {
    pub skill_type_id: i64,
    pub name: String,
    /// What the character can use (an Alpha's cap applied).
    pub active_level: i16,
    pub trained_level: i16,
    pub required_level: i16,
    /// To train from `trained_level` to `required_level`; `None` when it
    /// needs an Omega clone or the training rate is unknown.
    pub seconds: Option<i64>,
    /// Trained high enough, but an Alpha clone caps it below what's needed.
    pub omega_required: bool,
}

/// Where one character stands on a fit.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PilotFlyability {
    pub status: FlyabilityStatus,
    /// Per skill, prerequisites first, in training order.
    pub missing: Vec<MissingSkill>,
    /// To train everything missing that training can fix; `None` when the
    /// training rate is unknown.
    pub remaining_seconds: Option<i64>,
    /// Skills the character hasn't injected, in training order.
    pub missing_skillbooks: Vec<i64>,
    /// Some requirement needs an Omega clone.
    pub omega_required: bool,
    pub estimated: bool,
    /// Indexes of fitted items the character can't use.
    pub blocked_items: Vec<usize>,
}

/// Whether `character` can fly the hull and use every item, and what it is
/// missing (over the requirements' whole prerequisite closure).
#[must_use]
pub fn evaluate_flyability(
    requirements: &FitRequirements,
    definitions: &[SkillDefinition],
    character: &CharacterSkillsSnapshot,
    now: DateTime<Utc>,
) -> PilotFlyability {
    let level_of = |skill_type_id: i64| {
        character
            .skill(skill_type_id)
            .map_or((0, 0), |skill| (skill.active_level, skill.trained_level))
    };
    let meets = |entries: &[PlanEntry]| {
        entries
            .iter()
            .all(|entry| level_of(entry.skill_type_id).0 >= entry.level)
    };
    let status = if !meets(&requirements.hull) {
        FlyabilityStatus::Cannot
    } else if meets(&requirements.all) {
        FlyabilityStatus::CanUseAll
    } else {
        FlyabilityStatus::CanFlyHull
    };
    let blocked_items = requirements
        .by_item
        .iter()
        .filter(|(_, needs)| !meets(needs))
        .map(|(index, _)| *index)
        .collect();

    // Training times for the closure; a level that is trained counts as
    // done here, which is right for training and why the Omega case is
    // handled apart below.
    let graph = SkillGraph::new(definitions);
    let closure = closure(requirements, definitions).unwrap_or_default();
    let evaluation = evaluate_plan(&closure, definitions, character, now);
    let mut missing: Vec<MissingSkill> = Vec::new();
    let mut remaining_seconds = Some(0_i64);
    for entry in &evaluation.entries {
        let (active_level, trained_level) = level_of(entry.skill_type_id);
        if active_level >= entry.level {
            continue;
        }
        let omega_required = trained_level >= entry.level;
        let seconds = if omega_required {
            None
        } else if entry.status == PlanEntryStatus::Blocked {
            // An Omega-only skill on an Alpha.
            None
        } else {
            entry.seconds
        };
        if !omega_required && entry.status != PlanEntryStatus::Blocked {
            remaining_seconds = remaining_seconds
                .zip(entry.seconds)
                .map(|(sum, add)| sum + add);
        }
        let omega_required = omega_required || entry.status == PlanEntryStatus::Blocked;
        match missing
            .iter_mut()
            .find(|missing| missing.skill_type_id == entry.skill_type_id)
        {
            Some(skill) => {
                skill.required_level = skill.required_level.max(entry.level);
                skill.seconds = skill.seconds.zip(seconds).map(|(sum, add)| sum + add);
                skill.omega_required |= omega_required;
            }
            None => missing.push(MissingSkill {
                skill_type_id: entry.skill_type_id,
                name: graph
                    .skill(entry.skill_type_id)
                    .map_or_else(String::new, |skill| skill.name.clone()),
                active_level,
                trained_level,
                required_level: entry.level,
                seconds,
                omega_required,
            }),
        }
    }
    PilotFlyability {
        status,
        omega_required: missing.iter().any(|missing| missing.omega_required),
        missing,
        remaining_seconds,
        missing_skillbooks: evaluation.missing_skillbooks,
        estimated: evaluation.estimated,
        blocked_items,
    }
}

/// Every level of every required skill and its prerequisites, in training
/// order.
fn closure(
    requirements: &FitRequirements,
    definitions: &[SkillDefinition],
) -> Result<Vec<PlanEntry>, SkillPlanError> {
    let graph = SkillGraph::new(definitions);
    // Skills the SDE no longer lists can't be planned; leave them out
    // rather than fail the whole fit.
    let known: Vec<PlanEntry> = requirements
        .all
        .iter()
        .copied()
        .filter(|entry| graph.skill(entry.skill_type_id).is_some())
        .collect();
    Ok(normalize_plan(&known, &graph)?.entries)
}

/// A skill plan for the fit: the whole closure, or, for `character`, only
/// the levels it hasn't trained yet.
pub fn requirement_plan(
    requirements: &FitRequirements,
    definitions: &[SkillDefinition],
    character: Option<&CharacterSkillsSnapshot>,
) -> Result<Vec<PlanEntry>, SkillPlanError> {
    let closure = closure(requirements, definitions)?;
    Ok(match character {
        Some(character) => closure
            .into_iter()
            .filter(|entry| {
                character
                    .skill(entry.skill_type_id)
                    .map_or(0, |skill| skill.trained_level)
                    < entry.level
            })
            .collect(),
        None => closure,
    })
}

#[cfg(test)]
mod tests;
