//! A type's required skills as the client's Requirements tab shows them:
//! each requirement with its prerequisites nested under it, checked
//! against a pilot.

use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};
use iskworks_sde::{RequiredSkill, SkillDefinition};
use serde::Serialize;

use crate::skill_plans::{evaluate_plan, prerequisite_closure, PlanEntry, SkillGraph};
use crate::skills::CharacterSkillsSnapshot;

/// Whose skills the tree is checked against.
#[derive(Debug, Clone, Copy)]
pub enum RequirementPilot<'a> {
    AllV,
    AllZero,
    /// Active levels, so an Alpha's cap counts.
    Character(&'a CharacterSkillsSnapshot),
}

impl RequirementPilot<'_> {
    fn level(&self, skill_type_id: i64) -> i16 {
        match self {
            Self::AllV => 5,
            Self::AllZero => 0,
            Self::Character(snapshot) => snapshot
                .skill(skill_type_id)
                .map_or(0, |skill| skill.active_level),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillNode {
    pub skill_type_id: i64,
    /// `None` for a skill the import doesn't list.
    pub name: Option<String>,
    pub level: i16,
    pub trained_level: i16,
    pub children: Vec<SkillNode>,
}

impl SkillNode {
    #[must_use]
    pub fn met(&self) -> bool {
        self.trained_level >= self.level
    }
}

/// What a character still has to train for a type.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementSummary {
    /// Distinct skills below the level the tree needs.
    pub missing_skills: usize,
    /// To train them all; `None` when the training rate is unknown.
    pub remaining_seconds: Option<i64>,
}

/// The requirement tree, in the type's own requirement order, with
/// prerequisites in the order the client lists them.
#[must_use]
pub fn requirement_tree(
    required: &[RequiredSkill],
    definitions: &[SkillDefinition],
    pilot: RequirementPilot<'_>,
) -> Vec<SkillNode> {
    let skills: BTreeMap<i64, &SkillDefinition> = definitions
        .iter()
        .map(|definition| (definition.type_id, definition))
        .collect();
    let mut path = HashSet::new();
    required
        .iter()
        .map(|requirement| node(requirement, &skills, pilot, &mut path))
        .collect()
}

/// `path` holds the skills above this one, so a cyclic prerequisite (bad
/// data) ends the branch instead of recursing forever.
fn node(
    requirement: &RequiredSkill,
    skills: &BTreeMap<i64, &SkillDefinition>,
    pilot: RequirementPilot<'_>,
    path: &mut HashSet<i64>,
) -> SkillNode {
    let definition = skills.get(&requirement.skill_type_id);
    let mut children = Vec::new();
    if path.insert(requirement.skill_type_id) {
        for prerequisite in definition.map_or(&[][..], |definition| &definition.prerequisites) {
            if !path.contains(&prerequisite.skill_type_id) {
                children.push(node(prerequisite, skills, pilot, path));
            }
        }
        path.remove(&requirement.skill_type_id);
    }
    SkillNode {
        skill_type_id: requirement.skill_type_id,
        name: definition.map(|definition| definition.name.clone()),
        level: requirement.level,
        trained_level: pilot.level(requirement.skill_type_id),
        children,
    }
}

/// What `character` lacks across the whole tree.
#[must_use]
pub fn requirement_summary(
    required: &[RequiredSkill],
    definitions: &[SkillDefinition],
    character: &CharacterSkillsSnapshot,
    now: DateTime<Utc>,
) -> RequirementSummary {
    let graph = SkillGraph::new(definitions);
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for requirement in required {
        let entry = PlanEntry {
            skill_type_id: requirement.skill_type_id,
            level: requirement.level,
        };
        // A skill the import doesn't list can't be planned; leave it out.
        let Ok(closure) = prerequisite_closure(&graph, entry) else {
            continue;
        };
        for needed in closure.into_iter().chain([entry]) {
            if seen.insert(needed) {
                entries.push(needed);
            }
        }
    }
    let pilot = RequirementPilot::Character(character);
    let missing_skills = entries
        .iter()
        .filter(|entry| pilot.level(entry.skill_type_id) < entry.level)
        .map(|entry| entry.skill_type_id)
        .collect::<HashSet<_>>()
        .len();
    let evaluation = evaluate_plan(&entries, definitions, character, now);
    RequirementSummary {
        missing_skills,
        remaining_seconds: evaluation.remaining_seconds,
    }
}

#[cfg(test)]
mod tests;
