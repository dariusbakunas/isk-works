//! Skill plans: ordered (skill, level) entries a character trains through,
//! the way the client's skill plans list them (each level its own entry,
//! prerequisites first).
//!
//! Pure: callers pass the SDE skill definitions.

use std::collections::{HashMap, HashSet};

use iskworks_sde::SkillDefinition;
use serde::{Deserialize, Serialize};

use crate::skills::MAX_SKILL_LEVEL;

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

#[cfg(test)]
mod tests;
