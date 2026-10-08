use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_core::{ConnectedCharacterId, IndustryActivity, InventoryError, WorkspaceId};
use serde::Serialize;
use thiserror::Error;

use crate::character_roster::{CharacterDetail, CharacterRosterService};
use crate::planetary::{PlanetaryService, PlanetaryTimer, PlanetaryTimerEvent};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct CalendarRange {
    from: DateTime<Utc>,
    to: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Eq, Error, PartialEq)]
#[error("calendar range must have from before to")]
pub struct CalendarRangeError;

impl CalendarRange {
    pub fn new(from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Self, CalendarRangeError> {
        (from < to)
            .then_some(Self { from, to })
            .ok_or(CalendarRangeError)
    }

    fn contains(self, instant: DateTime<Utc>) -> bool {
        instant >= self.from && instant < self.to
    }

    #[must_use]
    pub fn from(self) -> DateTime<Utc> {
        self.from
    }

    #[must_use]
    pub fn to(self) -> DateTime<Utc> {
        self.to
    }
}

#[async_trait]
pub trait CalendarCharacterSource: Send + Sync {
    async fn calendar_details(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<CharacterDetail>, InventoryError>;
}

#[async_trait]
impl CalendarCharacterSource for CharacterRosterService {
    async fn calendar_details(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<CharacterDetail>, InventoryError> {
        let roster = self.list(workspace_id).await?;
        let mut details = Vec::with_capacity(roster.len());
        for character in roster {
            details.push(self.detail(character.connection_id).await?);
        }
        Ok(details)
    }
}

#[async_trait]
pub trait CalendarPlanetarySource: Send + Sync {
    async fn planetary_timers(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<PlanetaryTimer>, InventoryError>;
}

#[async_trait]
impl CalendarPlanetarySource for PlanetaryService {
    async fn planetary_timers(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<PlanetaryTimer>, InventoryError> {
        self.timers(workspace_id).await
    }
}

#[derive(Clone)]
pub struct CalendarService {
    source: Arc<dyn CalendarCharacterSource>,
    planetary: Option<Arc<dyn CalendarPlanetarySource>>,
}

impl CalendarService {
    #[must_use]
    pub fn new(source: Arc<dyn CalendarCharacterSource>) -> Self {
        Self {
            source,
            planetary: None,
        }
    }

    #[must_use]
    pub fn with_planetary(mut self, planetary: Arc<dyn CalendarPlanetarySource>) -> Self {
        self.planetary = Some(planetary);
        self
    }

    pub async fn range(
        &self,
        workspace_id: WorkspaceId,
        range: CalendarRange,
    ) -> Result<Vec<CalendarMilestone>, InventoryError> {
        let details = self.source.calendar_details(workspace_id).await?;
        let mut milestones = project_calendar_milestones(&details, range);
        if let Some(planetary) = &self.planetary {
            // PI is an optional layer: a failure here must not blank the
            // industry/skill calendar.
            match planetary.planetary_timers(workspace_id).await {
                Ok(timers) => {
                    milestones.extend(project_planetary_milestones(&timers, range));
                    sort_milestones(&mut milestones);
                }
                Err(error) => tracing::warn!(%error, "calendar planetary timers unavailable"),
            }
        }
        Ok(milestones)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarMilestoneCommon {
    pub id: String,
    pub connection_id: ConnectedCharacterId,
    pub eve_character_id: i64,
    pub character_name: String,
    pub title: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarIndustryMilestone {
    #[serde(flatten)]
    pub common: CalendarMilestoneCommon,
    pub occurs_at: DateTime<Utc>,
    pub job_id: i64,
    pub activity: IndustryActivity,
    pub activity_id: i64,
    pub status: String,
    pub type_id: i64,
    pub type_name: Option<String>,
    pub blueprint_type_id: i64,
    pub blueprint_name: Option<String>,
    pub product_type_id: Option<i64>,
    pub product_name: Option<String>,
    pub runs: i64,
    pub facility_id: i64,
    pub facility_name: Option<String>,
    pub solar_system_name: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarSkillMilestone {
    #[serde(flatten)]
    pub common: CalendarMilestoneCommon,
    pub occurs_at: DateTime<Utc>,
    pub skill_type_id: i64,
    pub skill_name: Option<String>,
    pub target_level: i64,
    pub queue_position: i64,
    pub next_skill_type_id: Option<i64>,
    pub next_skill_name: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarPlanetaryMilestone {
    #[serde(flatten)]
    pub common: CalendarMilestoneCommon,
    pub occurs_at: DateTime<Utc>,
    pub planet_id: i64,
    pub planet_name: String,
    pub planet_type: String,
    pub solar_system_name: Option<String>,
    /// Projected from the in-game snapshot rather than reported by ESI.
    pub estimated: bool,
    #[serde(flatten)]
    pub event: PlanetaryTimerEvent,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CalendarMilestone {
    Industry {
        #[serde(flatten)]
        milestone: CalendarIndustryMilestone,
    },
    Skill {
        #[serde(flatten)]
        milestone: CalendarSkillMilestone,
    },
    Planetary {
        #[serde(flatten)]
        milestone: CalendarPlanetaryMilestone,
    },
}

impl CalendarMilestone {
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Industry { milestone } => &milestone.common.id,
            Self::Skill { milestone } => &milestone.common.id,
            Self::Planetary { milestone } => &milestone.common.id,
        }
    }

    fn occurs_at(&self) -> DateTime<Utc> {
        match self {
            Self::Industry { milestone } => milestone.occurs_at,
            Self::Skill { milestone } => milestone.occurs_at,
            Self::Planetary { milestone } => milestone.occurs_at,
        }
    }
}

#[must_use]
pub fn project_calendar_milestones(
    details: &[CharacterDetail],
    range: CalendarRange,
) -> Vec<CalendarMilestone> {
    let mut milestones = Vec::new();
    for detail in details {
        let connection_id = detail.entry.connection_id;
        let common = |id: String, title: String| CalendarMilestoneCommon {
            id,
            connection_id,
            eve_character_id: detail.entry.eve_character_id,
            character_name: detail.entry.character_name.clone(),
            title,
        };

        for job in &detail.industry_jobs {
            let Some(occurs_at) = job.end_date else {
                continue;
            };
            if !range.contains(occurs_at) {
                continue;
            }
            let type_id = job.product_type_id.unwrap_or(job.blueprint_type_id);
            let type_name = job
                .product_name
                .clone()
                .or_else(|| job.blueprint_name.clone());
            let title = type_name
                .clone()
                .unwrap_or_else(|| format!("Type {type_id}"));
            milestones.push(CalendarMilestone::Industry {
                milestone: CalendarIndustryMilestone {
                    common: common(
                        format!("industry:{}:{}", connection_id.0, job.job_id),
                        title,
                    ),
                    occurs_at,
                    job_id: job.job_id,
                    activity: job.activity,
                    activity_id: job.activity_id,
                    status: job.status.clone(),
                    type_id,
                    type_name,
                    blueprint_type_id: job.blueprint_type_id,
                    blueprint_name: job.blueprint_name.clone(),
                    product_type_id: job.product_type_id,
                    product_name: job.product_name.clone(),
                    runs: job.runs,
                    facility_id: job.facility_id,
                    facility_name: job.facility_name.clone(),
                    solar_system_name: job.solar_system_name.clone(),
                },
            });
        }

        let mut queue: Vec<_> = detail.entry.training_queue.iter().collect();
        queue.sort_by_key(|entry| entry.queue_position);
        for (index, entry) in queue.iter().enumerate() {
            let Some(occurs_at) = entry.finish_date else {
                continue;
            };
            if !range.contains(occurs_at) {
                continue;
            }
            let skill_name = entry.skill_name.clone();
            let base_title = skill_name
                .clone()
                .unwrap_or_else(|| format!("Skill {}", entry.skill_id));
            let title = format!("{base_title} {}", roman_level(entry.finished_level));
            let next = queue.get(index + 1).copied();
            milestones.push(CalendarMilestone::Skill {
                milestone: CalendarSkillMilestone {
                    common: common(
                        format!(
                            "skill:{}:{}:{}",
                            connection_id.0, entry.skill_id, entry.finished_level
                        ),
                        title,
                    ),
                    occurs_at,
                    skill_type_id: entry.skill_id,
                    skill_name,
                    target_level: entry.finished_level,
                    queue_position: entry.queue_position,
                    next_skill_type_id: next.map(|next| next.skill_id),
                    next_skill_name: next.and_then(|next| next.skill_name.clone()),
                },
            });
        }
    }
    sort_milestones(&mut milestones);
    milestones
}

fn sort_milestones(milestones: &mut [CalendarMilestone]) {
    milestones.sort_by(|left, right| {
        left.occurs_at()
            .cmp(&right.occurs_at())
            .then_with(|| left.id().cmp(right.id()))
    });
}

#[must_use]
pub fn project_planetary_milestones(
    timers: &[PlanetaryTimer],
    range: CalendarRange,
) -> Vec<CalendarMilestone> {
    timers
        .iter()
        .filter(|timer| range.contains(timer.occurs_at))
        .map(|timer| {
            let (id, title, estimated) = match &timer.event {
                PlanetaryTimerEvent::ExtractorExpiry { .. } => (
                    format!(
                        "planetary:extractor:{}:{}:{}",
                        timer.connection_id.0,
                        timer.planet_id,
                        timer.occurs_at.timestamp()
                    ),
                    format!("{} extractors", timer.planet_name),
                    false,
                ),
                PlanetaryTimerEvent::ImportDepleted {
                    type_id, type_name, ..
                } => (
                    format!(
                        "planetary:import:{}:{}:{type_id}",
                        timer.connection_id.0, timer.planet_id
                    ),
                    format!("{} out of {type_name}", timer.planet_name),
                    true,
                ),
            };
            CalendarMilestone::Planetary {
                milestone: CalendarPlanetaryMilestone {
                    common: CalendarMilestoneCommon {
                        id,
                        connection_id: timer.connection_id,
                        eve_character_id: timer.eve_character_id,
                        character_name: timer.character_name.clone(),
                        title,
                    },
                    occurs_at: timer.occurs_at,
                    planet_id: timer.planet_id,
                    planet_name: timer.planet_name.clone(),
                    planet_type: timer.planet_type.clone(),
                    solar_system_name: timer.solar_system_name.clone(),
                    estimated,
                    event: timer.event.clone(),
                },
            }
        })
        .collect()
}

fn roman_level(level: i64) -> String {
    match level {
        1 => "I".to_string(),
        2 => "II".to_string(),
        3 => "III".to_string(),
        4 => "IV".to_string(),
        5 => "V".to_string(),
        value => value.to_string(),
    }
}

#[cfg(test)]
mod tests;
