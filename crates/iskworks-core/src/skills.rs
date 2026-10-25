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

/// What a character can do with one skill right now.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillState {
    /// Trained to V (an Alpha's cap doesn't change that).
    Max,
    /// The queue entry training right now is this skill's next level.
    Training,
    /// In the queue, not training yet.
    Queued,
    /// Injected, below V, not queued.
    CanTrain,
    /// Not injected; every prerequisite is met.
    CanInject,
    /// Not injected; a prerequisite is missing.
    Locked,
    /// An Alpha clone and an Omega-only skill below V.
    OmegaOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillProgress {
    pub trained_level: i16,
    pub active_level: i16,
    pub skillpoints: i64,
    pub state: SkillState,
    /// Highest level this skill is queued to, when above the trained one.
    pub queued_to_level: Option<i16>,
    /// To finish the next level. `None` when there's nothing to train,
    /// the skill isn't injected, or the rate is unknown.
    pub seconds_to_next_level: Option<i64>,
    /// The time is an estimate (see [`TrainingRate::estimated`]).
    pub estimated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueSkill {
    #[serde(flatten)]
    pub definition: SkillDefinition,
    /// Cumulative SP for levels I..V.
    pub sp_per_level: [i64; 5],
    pub progress: Option<SkillProgress>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueGroup {
    pub group_id: i64,
    pub name: String,
    pub skill_count: usize,
    /// SP to train every skill in the group to V.
    pub max_sp: i64,
    pub injected_count: Option<usize>,
    pub trained_sp: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCatalogue {
    /// In catalogue order (the definitions' group order).
    pub groups: Vec<CatalogueGroup>,
    pub skills: Vec<CatalogueSkill>,
    /// `None` without a character.
    pub rate: Option<TrainingRate>,
}

/// Overlays a character's skills on the catalogue. `definitions` come in
/// catalogue order (grouped); groups keep that order.
#[must_use]
pub fn build_catalogue(
    definitions: &[SkillDefinition],
    character: Option<&CharacterSkillsSnapshot>,
    now: DateTime<Utc>,
) -> SkillCatalogue {
    let rate = character.map(|character| training_rate(character, definitions, now));
    let training =
        character.and_then(
            |character| match derive_training_state(&character.queue, now) {
                TrainingState::Active { entry } => Some(entry),
                _ => None,
            },
        );
    let mut groups: Vec<CatalogueGroup> = Vec::new();
    let mut skills = Vec::with_capacity(definitions.len());
    for definition in definitions {
        let progress = character.zip(rate).map(|(character, rate)| {
            skill_progress(definition, character, &rate, training.as_ref(), now)
        });
        let max_sp = sp_for_level(definition.rank, MAX_SKILL_LEVEL);
        let owned = character
            .and_then(|character| character.skill(definition.type_id))
            .map(|skill| skill.skillpoints);
        let group = match groups.last_mut() {
            Some(group) if group.group_id == definition.group_id => group,
            _ => {
                groups.push(CatalogueGroup {
                    group_id: definition.group_id,
                    name: definition.group_name.clone(),
                    skill_count: 0,
                    max_sp: 0,
                    injected_count: character.map(|_| 0),
                    trained_sp: character.map(|_| 0),
                });
                groups.last_mut().expect("just pushed")
            }
        };
        group.skill_count += 1;
        group.max_sp += max_sp;
        if let Some(skillpoints) = owned {
            group.injected_count = group.injected_count.map(|count| count + 1);
            group.trained_sp = group.trained_sp.map(|sp| sp + skillpoints);
        }
        skills.push(CatalogueSkill {
            sp_per_level: [1, 2, 3, 4, 5].map(|level| sp_for_level(definition.rank, level)),
            definition: definition.clone(),
            progress,
        });
    }
    SkillCatalogue {
        groups,
        skills,
        rate,
    }
}

fn skill_progress(
    definition: &SkillDefinition,
    character: &CharacterSkillsSnapshot,
    rate: &TrainingRate,
    training: Option<&SkillQueueEntry>,
    now: DateTime<Utc>,
) -> SkillProgress {
    let owned = character.skill(definition.type_id);
    let trained_level = owned.map_or(0, |skill| skill.trained_level);
    let active_level = owned.map_or(0, |skill| skill.active_level);
    let skillpoints = owned.map_or(0, |skill| skill.skillpoints);
    let queued_to_level = character
        .queue
        .iter()
        .filter(|entry| entry.skill_id == definition.type_id)
        .filter_map(|entry| i16::try_from(entry.finished_level).ok())
        .filter(|level| *level > trained_level)
        .max();
    let training = training.filter(|entry| {
        entry.skill_id == definition.type_id && entry.finished_level == i64::from(trained_level) + 1
    });
    let prerequisites_met = definition.prerequisites.iter().all(|required| {
        character
            .skill(required.skill_type_id)
            .is_some_and(|skill| skill.active_level >= required.level)
    });
    let state = if trained_level >= MAX_SKILL_LEVEL {
        SkillState::Max
    } else if definition.omega_only && rate.clone == CloneState::Alpha {
        SkillState::OmegaOnly
    } else if training.is_some() {
        SkillState::Training
    } else if queued_to_level.is_some() {
        SkillState::Queued
    } else if owned.is_some() {
        SkillState::CanTrain
    } else if prerequisites_met {
        SkillState::CanInject
    } else {
        SkillState::Locked
    };
    let trainable = matches!(
        state,
        SkillState::Training | SkillState::Queued | SkillState::CanTrain
    );
    let (seconds_to_next_level, estimated) = match training.and_then(|entry| entry.finish_date) {
        // The queue's own clock, so the catalogue agrees with the queue.
        Some(finish) => (Some((finish - now).num_seconds().max(0)), false),
        None if trainable => {
            let remaining =
                (sp_for_level(definition.rank, trained_level + 1) - skillpoints).max(0) as f64;
            let seconds = rate
                .sp_per_minute(character.attributes.as_ref(), definition)
                .filter(|per_minute| *per_minute > 0.0)
                .map(|per_minute| (remaining / per_minute * 60.0).ceil() as i64);
            (seconds, seconds.is_some() && rate.estimated())
        }
        None => (None, false),
    };
    SkillProgress {
        trained_level,
        active_level,
        skillpoints,
        state,
        queued_to_level,
        seconds_to_next_level,
        estimated,
    }
}

#[cfg(test)]
mod tests;
