//! Skill plans: ordered (skill, level) entries a character trains through,
//! the way the client's skill plans list them (each level its own entry,
//! prerequisites first).
//!
//! Pure: callers pass the SDE skill definitions.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use iskworks_sde::SkillDefinition;
use serde::{Deserialize, Serialize};

use crate::skills::{
    sp_for_level, training_rate, CharacterSkillsSnapshot, CloneState, MAX_SKILL_LEVEL,
};
use crate::training::{derive_training_state, TrainingState};

/// One level of one skill.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanEntry {
    pub skill_type_id: i64,
    pub level: i16,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SkillPlanError {
    UnknownSkill(i64),
    InvalidLevel(PlanEntry),
    /// `entry` needs `requires`, which the plan only has further down.
    OrderViolation {
        entry: PlanEntry,
        requires: PlanEntry,
    },
    /// The prerequisite data loops back on this skill (never in real SDE
    /// data, but refused rather than recursed forever).
    CyclicPrerequisites(i64),
}

/// The skill definitions, indexed for prerequisite lookups.
pub struct SkillGraph<'a> {
    skills: HashMap<i64, &'a SkillDefinition>,
}

impl<'a> SkillGraph<'a> {
    #[must_use]
    pub fn new(definitions: &'a [SkillDefinition]) -> Self {
        Self {
            skills: definitions
                .iter()
                .map(|definition| (definition.type_id, definition))
                .collect(),
        }
    }

    #[must_use]
    pub fn skill(&self, type_id: i64) -> Option<&'a SkillDefinition> {
        self.skills.get(&type_id).copied()
    }
}

fn validate<'a>(
    graph: &SkillGraph<'a>,
    entry: PlanEntry,
) -> Result<&'a SkillDefinition, SkillPlanError> {
    let skill = graph
        .skill(entry.skill_type_id)
        .ok_or(SkillPlanError::UnknownSkill(entry.skill_type_id))?;
    if !(1..=MAX_SKILL_LEVEL).contains(&entry.level) {
        return Err(SkillPlanError::InvalidLevel(entry));
    }
    Ok(skill)
}

/// Appends `skill` levels `1..=level` to `out`, each after the
/// prerequisites they need, skipping anything already in `seen`.
fn require(
    graph: &SkillGraph<'_>,
    skill_type_id: i64,
    level: i16,
    seen: &mut HashSet<PlanEntry>,
    visiting: &mut HashSet<i64>,
    out: &mut Vec<PlanEntry>,
) -> Result<(), SkillPlanError> {
    if (1..=level).all(|level| {
        seen.contains(&PlanEntry {
            skill_type_id,
            level,
        })
    }) {
        return Ok(());
    }
    let skill = graph
        .skill(skill_type_id)
        .ok_or(SkillPlanError::UnknownSkill(skill_type_id))?;
    if !visiting.insert(skill_type_id) {
        return Err(SkillPlanError::CyclicPrerequisites(skill_type_id));
    }
    for prerequisite in &skill.prerequisites {
        require(
            graph,
            prerequisite.skill_type_id,
            prerequisite.level,
            seen,
            visiting,
            out,
        )?;
    }
    visiting.remove(&skill_type_id);
    for level in 1..=level {
        let entry = PlanEntry {
            skill_type_id,
            level,
        };
        if seen.insert(entry) {
            out.push(entry);
        }
    }
    Ok(())
}

/// Everything that must be trained before `entry`, in training order:
/// prerequisites deepest first (in the client's prerequisite order), then
/// the skill's own lower levels. Excludes `entry` itself.
pub fn prerequisite_closure(
    graph: &SkillGraph<'_>,
    entry: PlanEntry,
) -> Result<Vec<PlanEntry>, SkillPlanError> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    closure_into(graph, entry, &mut seen, &mut out)?;
    Ok(out)
}

fn closure_into(
    graph: &SkillGraph<'_>,
    entry: PlanEntry,
    seen: &mut HashSet<PlanEntry>,
    out: &mut Vec<PlanEntry>,
) -> Result<(), SkillPlanError> {
    let skill = validate(graph, entry)?;
    let mut visiting = HashSet::from([entry.skill_type_id]);
    for prerequisite in &skill.prerequisites {
        require(
            graph,
            prerequisite.skill_type_id,
            prerequisite.level,
            seen,
            &mut visiting,
            out,
        )?;
    }
    for level in 1..entry.level {
        let lower = PlanEntry {
            skill_type_id: entry.skill_type_id,
            level,
        };
        if seen.insert(lower) {
            out.push(lower);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NormalizedPlan {
    pub entries: Vec<PlanEntry>,
    /// Prerequisites and lower levels that weren't in the plan, in the order
    /// they were added.
    pub inserted: Vec<PlanEntry>,
}

/// Makes a plan trainable top to bottom: inserts each entry's missing
/// prerequisites and lower levels directly before it and drops duplicates.
/// A prerequisite that the plan does have, but only further down, is an
/// [`SkillPlanError::OrderViolation`] rather than a silent reorder, so a
/// drag that breaks the order is refused instead of undone.
pub fn normalize_plan(
    entries: &[PlanEntry],
    graph: &SkillGraph<'_>,
) -> Result<NormalizedPlan, SkillPlanError> {
    for entry in entries {
        validate(graph, *entry)?;
    }
    let planned = entries.iter().copied().collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(entries.len());
    let mut inserted = Vec::new();
    for entry in entries {
        if seen.contains(entry) {
            continue;
        }
        let mut needed = Vec::new();
        closure_into(graph, *entry, &mut seen.clone(), &mut needed)?;
        if let Some(requires) = needed.iter().find(|needed| planned.contains(needed)) {
            return Err(SkillPlanError::OrderViolation {
                entry: *entry,
                requires: *requires,
            });
        }
        for needed in needed {
            seen.insert(needed);
            out.push(needed);
            inserted.push(needed);
        }
        seen.insert(*entry);
        out.push(*entry);
    }
    Ok(NormalizedPlan {
        entries: out,
        inserted,
    })
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanEntryStatus {
    /// Trained.
    Done,
    /// The next level to train, with some SP already in it (or training).
    Partial,
    Remaining,
    /// An Omega-only skill on an Alpha clone.
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanEntryProgress {
    pub skill_type_id: i64,
    pub level: i16,
    pub status: PlanEntryStatus,
    /// SP this level takes from the previous one.
    pub sp_total: i64,
    pub sp_remaining: i64,
    /// `None` when blocked or the training rate is unknown.
    pub seconds: Option<i64>,
    pub estimated: bool,
}

/// One character's progress through a plan.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanEvaluation {
    pub entries: Vec<PlanEntryProgress>,
    pub total_sp: i64,
    pub remaining_sp: i64,
    /// By SP, 0–100.
    pub percent_complete: f64,
    /// To finish every entry that isn't blocked; `None` when the training
    /// rate is unknown.
    pub remaining_seconds: Option<i64>,
    /// Skills in the plan the character hasn't injected, in plan order.
    pub missing_skillbooks: Vec<i64>,
    /// Some entry is blocked (Alpha, Omega-only skill).
    pub blocked: bool,
    pub estimated: bool,
}

/// Where `character` stands on each entry of `entries`. Entries for skills
/// the SDE no longer has are left out. The entry training right now takes
/// the queue's own finish time, as the catalogue does.
#[must_use]
pub fn evaluate_plan(
    entries: &[PlanEntry],
    definitions: &[SkillDefinition],
    character: &CharacterSkillsSnapshot,
    now: DateTime<Utc>,
) -> PlanEvaluation {
    let graph = SkillGraph::new(definitions);
    let rate = training_rate(character, definitions, now);
    let training = match derive_training_state(&character.queue, now) {
        TrainingState::Active { entry } => Some(entry),
        _ => None,
    };
    let mut progress = Vec::with_capacity(entries.len());
    let mut missing_skillbooks = Vec::new();
    for entry in entries {
        let Some(skill) = graph.skill(entry.skill_type_id) else {
            continue;
        };
        let owned = character.skill(entry.skill_type_id);
        let trained_level = owned.map_or(0, |owned| owned.trained_level);
        let skillpoints = owned.map_or(0, |owned| owned.skillpoints);
        let level_start = sp_for_level(skill.rank, entry.level - 1);
        let level_end = sp_for_level(skill.rank, entry.level);
        let sp_total = level_end - level_start;
        if trained_level >= entry.level {
            progress.push(PlanEntryProgress {
                skill_type_id: entry.skill_type_id,
                level: entry.level,
                status: PlanEntryStatus::Done,
                sp_total,
                sp_remaining: 0,
                seconds: Some(0),
                estimated: false,
            });
            continue;
        }
        if owned.is_none() && !missing_skillbooks.contains(&entry.skill_type_id) {
            missing_skillbooks.push(entry.skill_type_id);
        }
        let sp_remaining = level_end - skillpoints.clamp(level_start, level_end);
        let in_training = training.as_ref().filter(|queued| {
            queued.skill_id == entry.skill_type_id
                && queued.finished_level == i64::from(entry.level)
        });
        let blocked = skill.omega_only && rate.clone == CloneState::Alpha;
        let status = if blocked {
            PlanEntryStatus::Blocked
        } else if in_training.is_some() || sp_remaining < sp_total {
            PlanEntryStatus::Partial
        } else {
            PlanEntryStatus::Remaining
        };
        let (seconds, estimated) = if blocked {
            (None, false)
        } else if let Some(finish) = in_training.and_then(|queued| queued.finish_date) {
            (Some((finish - now).num_seconds().max(0)), false)
        } else {
            let seconds = rate
                .sp_per_minute(character.attributes.as_ref(), skill)
                .filter(|per_minute| *per_minute > 0.0)
                .map(|per_minute| (sp_remaining as f64 / per_minute * 60.0).ceil() as i64);
            (seconds, seconds.is_some() && rate.estimated())
        };
        progress.push(PlanEntryProgress {
            skill_type_id: entry.skill_type_id,
            level: entry.level,
            status,
            sp_total,
            sp_remaining,
            seconds,
            estimated,
        });
    }
    let total_sp = progress.iter().map(|entry| entry.sp_total).sum::<i64>();
    let remaining_sp = progress.iter().map(|entry| entry.sp_remaining).sum::<i64>();
    let open = || {
        progress
            .iter()
            .filter(|entry| entry.status != PlanEntryStatus::Blocked)
    };
    PlanEvaluation {
        total_sp,
        remaining_sp,
        percent_complete: if total_sp == 0 {
            100.0
        } else {
            (total_sp - remaining_sp) as f64 / total_sp as f64 * 100.0
        },
        remaining_seconds: open().map(|entry| entry.seconds).sum(),
        missing_skillbooks,
        blocked: progress
            .iter()
            .any(|entry| entry.status == PlanEntryStatus::Blocked),
        estimated: open().any(|entry| entry.estimated),
        entries: progress,
    }
}

#[cfg(test)]
mod tests;
