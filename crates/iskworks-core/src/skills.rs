//! Skill training rules for the Skills page: skill points per level,
//! training rates, and reading the `skills` character source summary.
//!
//! Pure: callers pass the SDE skill definitions and the synced summary.

use chrono::{DateTime, Utc};
use iskworks_sde::{SkillAttribute, SkillDefinition};
use serde::Serialize;
use serde_json::Value;

use crate::training::{derive_training_state, SkillQueueEntry, TrainingState};

pub const MAX_SKILL_LEVEL: i16 = 5;

/// Cumulative skill points to reach `level` (0 for level 0):
/// `ceil(250 × rank × 2^(2.5 × (level − 1)))`. Matches the client's own
/// `level_start_sp` / `level_end_sp`, which round up (rank 5 level II is
/// 7,072, not 7,071).
#[must_use]
pub fn sp_for_level(rank: i32, level: i16) -> i64 {
    if level <= 0 {
        return 0;
    }
    let exact = 250.0 * f64::from(rank) * 2f64.powf(2.5 * f64::from(level - 1));
    // Levels I, III and V are exact integers; guard them against float noise.
    let rounded = exact.round();
    if (exact - rounded).abs() < 1e-6 {
        rounded as i64
    } else {
        exact.ceil() as i64
    }
}

/// The five training attributes as ESI reports them (implants included).
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterAttributes {
    pub charisma: i64,
    pub intelligence: i64,
    pub memory: i64,
    pub perception: i64,
    pub willpower: i64,
}

impl CharacterAttributes {
    #[must_use]
    pub fn value(&self, attribute: SkillAttribute) -> i64 {
        match attribute {
            SkillAttribute::Charisma => self.charisma,
            SkillAttribute::Intelligence => self.intelligence,
            SkillAttribute::Memory => self.memory,
            SkillAttribute::Perception => self.perception,
            SkillAttribute::Willpower => self.willpower,
        }
    }
}

/// Alpha clones train at half speed and can't train Omega-only skills.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CloneState {
    Alpha,
    Omega,
}

/// Skill points per minute for a skill training from `primary` and
/// `secondary`: `primary + secondary / 2`, halved for an Alpha.
#[must_use]
pub fn sp_per_minute(
    attributes: &CharacterAttributes,
    primary: SkillAttribute,
    secondary: SkillAttribute,
    clone: CloneState,
) -> f64 {
    let omega = attributes.value(primary) as f64 + attributes.value(secondary) as f64 / 2.0;
    match clone {
        CloneState::Omega => omega,
        CloneState::Alpha => omega / 2.0,
    }
}

/// One skill from `/characters/{id}/skills/`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CharacterSkill {
    pub skill_id: i64,
    pub trained_level: i16,
    /// Lower than `trained_level` when an Alpha clone caps the skill.
    pub active_level: i16,
    pub skillpoints: i64,
}

/// The `skills` character source summary, parsed.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterSkillsSnapshot {
    pub total_sp: i64,
    pub unallocated_sp: Option<i64>,
    pub skills: Vec<CharacterSkill>,
    /// Sorted by `queue_position`, as synced.
    pub queue: Vec<SkillQueueEntry>,
    pub attributes: Option<CharacterAttributes>,
}

impl CharacterSkillsSnapshot {
    #[must_use]
    pub fn skill(&self, skill_id: i64) -> Option<&CharacterSkill> {
        self.skills.iter().find(|skill| skill.skill_id == skill_id)
    }
}

/// Reads the summary the skills source stores: the ESI skills record plus
/// optional `attributes` and `skillQueue` keys. `None` when the core skills
/// record itself is unreadable.
#[must_use]
pub fn parse_skills_summary(summary: &Value) -> Option<CharacterSkillsSnapshot> {
    let int = |value: &Value, key: &str| value.get(key).and_then(Value::as_i64);
    let level =
        |value: &Value, key: &str| int(value, key).and_then(|level| i16::try_from(level).ok());
    let skills = summary
        .get("skills")?
        .as_array()?
        .iter()
        .map(|skill| {
            Some(CharacterSkill {
                skill_id: int(skill, "skill_id")?,
                trained_level: level(skill, "trained_skill_level")?,
                active_level: level(skill, "active_skill_level")?,
                skillpoints: int(skill, "skillpoints_in_skill")?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let attributes = summary.get("attributes").and_then(|value| {
        Some(CharacterAttributes {
            charisma: int(value, "charisma")?,
            intelligence: int(value, "intelligence")?,
            memory: int(value, "memory")?,
            perception: int(value, "perception")?,
            willpower: int(value, "willpower")?,
        })
    });
    let date = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
            .map(|date| date.with_timezone(&Utc))
    };
    let mut queue = summary
        .get("skillQueue")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    Some(SkillQueueEntry {
                        skill_id: int(entry, "skill_id")?,
                        finished_level: int(entry, "finished_level")?,
                        queue_position: int(entry, "queue_position")?,
                        start_date: date(entry, "start_date"),
                        finish_date: date(entry, "finish_date"),
                        training_start_sp: int(entry, "training_start_sp"),
                        level_end_sp: int(entry, "level_end_sp"),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    queue.sort_by_key(|entry| entry.queue_position);
    Some(CharacterSkillsSnapshot {
        total_sp: int(summary, "total_sp")?,
        unallocated_sp: int(summary, "unallocated_sp"),
        skills,
        queue,
        attributes,
    })
}

/// ESI has no clone-state field; an Alpha shows as skills whose active
/// level is capped below the trained one. An Omega with nothing capped
/// can't be told apart, which errs towards the faster rate.
#[must_use]
pub fn detect_clone_state(snapshot: &CharacterSkillsSnapshot) -> CloneState {
    if snapshot
        .skills
        .iter()
        .any(|skill| skill.active_level < skill.trained_level)
    {
        CloneState::Alpha
    } else {
        CloneState::Omega
    }
}

/// Where training times come from.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RateBasis {
    /// Synced attributes, per skill attribute pair.
    Attributes,
    /// No attributes: the skill in training's real rate, applied to every
    /// skill as an estimate.
    ObservedQueue {
        #[serde(rename = "spPerMinute")]
        sp_per_minute: f64,
    },
    /// Neither: no training times.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainingRate {
    pub basis: RateBasis,
    pub clone: CloneState,
    /// The skill in training is more than 1% faster than the attributes
    /// explain (an accelerator, or implants ESI doesn't report), so times
    /// computed from attributes are upper bounds.
    pub accelerated: bool,
}

impl TrainingRate {
    /// SP per minute for one skill, or `None` when the rate is unknown.
    #[must_use]
    pub fn sp_per_minute(
        &self,
        attributes: Option<&CharacterAttributes>,
        definition: &SkillDefinition,
    ) -> Option<f64> {
        match self.basis {
            RateBasis::Attributes => attributes.map(|attributes| {
                sp_per_minute(
                    attributes,
                    definition.primary_attribute,
                    definition.secondary_attribute,
                    self.clone,
                )
            }),
            RateBasis::ObservedQueue { sp_per_minute } => Some(sp_per_minute),
            RateBasis::Unknown => None,
        }
    }

    /// Whether times from this rate are estimates rather than exact.
    #[must_use]
    pub fn estimated(&self) -> bool {
        self.accelerated || !matches!(self.basis, RateBasis::Attributes)
    }
}

/// The real SP/min of the entry training right now, from the queue.
fn observed_sp_per_minute(
    snapshot: &CharacterSkillsSnapshot,
    now: DateTime<Utc>,
) -> Option<(i64, f64)> {
    let TrainingState::Active { entry } = derive_training_state(&snapshot.queue, now) else {
        return None;
    };
    let minutes = (entry.finish_date? - entry.start_date?).num_seconds() as f64 / 60.0;
    let sp = (entry.level_end_sp? - entry.training_start_sp?) as f64;
    (minutes > 0.0 && sp > 0.0).then(|| (entry.skill_id, sp / minutes))
}

#[must_use]
pub fn training_rate(
    snapshot: &CharacterSkillsSnapshot,
    definitions: &[SkillDefinition],
    now: DateTime<Utc>,
) -> TrainingRate {
    let clone = detect_clone_state(snapshot);
    let observed = observed_sp_per_minute(snapshot, now);
    match (&snapshot.attributes, observed) {
        (Some(attributes), observed) => {
            let accelerated = observed.is_some_and(|(skill_id, observed)| {
                definitions
                    .iter()
                    .find(|definition| definition.type_id == skill_id)
                    .is_some_and(|definition| {
                        let expected = sp_per_minute(
                            attributes,
                            definition.primary_attribute,
                            definition.secondary_attribute,
                            clone,
                        );
                        observed > expected * 1.01
                    })
            });
            TrainingRate {
                basis: RateBasis::Attributes,
                clone,
                accelerated,
            }
        }
        (None, Some((_, sp_per_minute))) => TrainingRate {
            basis: RateBasis::ObservedQueue { sp_per_minute },
            clone,
            accelerated: false,
        },
        (None, None) => TrainingRate {
            basis: RateBasis::Unknown,
            clone,
            accelerated: false,
        },
    }
}

#[cfg(test)]
mod tests;
