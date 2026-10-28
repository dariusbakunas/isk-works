//! A ship's bonuses as the client's Traits tab shows them: per-skill
//! groups, then the role and misc bonuses.

use std::collections::BTreeMap;

use iskworks_sde::{TypeBonusLine, TypeBonusSource};
use serde::Serialize;

use super::{format_bonus, parse_rich_text, RichSegment};

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraitGroup {
    pub heading: String,
    /// The skill whose level scales the group's bonuses.
    pub skill_type_id: Option<i64>,
    pub bonuses: Vec<TraitBonus>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraitBonus {
    /// `4%`, `+5`; `None` for a bonus with no number ("Can fit …").
    pub amount: Option<String>,
    pub text: Vec<RichSegment>,
}

/// Groups bonus lines (already in bonus order) under their headings.
#[must_use]
pub fn trait_groups(
    lines: &[TypeBonusLine],
    skill_names: &BTreeMap<i64, String>,
) -> Vec<TraitGroup> {
    let mut groups: Vec<(TypeBonusSource, TraitGroup)> = Vec::new();
    for line in lines {
        let bonus = TraitBonus {
            amount: line
                .bonus
                .map(|bonus| format_bonus(bonus, line.unit_id, line.unit_display_name.as_deref())),
            text: parse_rich_text(&line.text),
        };
        match groups.last_mut() {
            Some((source, group)) if *source == line.source => group.bonuses.push(bonus),
            _ => groups.push((
                line.source,
                TraitGroup {
                    heading: heading(line.source, skill_names),
                    skill_type_id: line.source.skill_type_id(),
                    bonuses: vec![bonus],
                },
            )),
        }
    }
    groups.into_iter().map(|(_, group)| group).collect()
}

fn heading(source: TypeBonusSource, skill_names: &BTreeMap<i64, String>) -> String {
    match source {
        TypeBonusSource::Skill(skill_type_id) => format!(
            "{} bonuses (per skill level)",
            skill_names
                .get(&skill_type_id)
                .map_or("Skill", String::as_str)
        ),
        TypeBonusSource::Role => "Role Bonus".to_string(),
        TypeBonusSource::Misc => "Misc Bonus".to_string(),
    }
}

#[cfg(test)]
mod tests;
