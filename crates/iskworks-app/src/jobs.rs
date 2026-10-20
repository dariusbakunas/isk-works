//! The Jobs page read model: running jobs per character (from each
//! character's last jobs sync, the same data as the roster's slot counts) and
//! finished jobs from the stored history, labelled and linked to their Board
//! tickets.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use iskworks_core::{
    job_history, job_status, summarize_running, CharacterSourceKind, ConnectedCharacterId,
    IndustryActivity, IndustryJobRepository, InventoryError, JobHistoryKpis, JobStatus,
    JobTicketLink, MarketRefreshState, Money, RunningJobSummary, StoredIndustryJob,
    WeeklyCompletions, WorkspaceId, MAX_JOB_HISTORY_DAYS,
};
use serde::Serialize;

use crate::character_roster::{CharacterRosterService, CharacterSourceDetail, IndustryJobDto};

/// A character's jobs counts as stale once its last jobs sync is this old
/// (two refresh intervals).
const JOBS_STALE_AFTER_MINUTES: i64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JobsSyncState {
    Ok,
    Stale,
    MissingScope,
    Failed,
    NeverSynced,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotUsage {
    pub used: u64,
    /// `None` until the character's skills have synced.
    pub max: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSlots {
    pub manufacturing: SlotUsage,
    pub reaction: SlotUsage,
    pub research: SlotUsage,
}

/// One connected character as the Jobs page needs it.
#[derive(Debug, Clone)]
pub struct JobCharacterSnapshot {
    pub connection_id: ConnectedCharacterId,
    pub eve_character_id: i64,
    pub character_name: String,
    pub slots: JobSlots,
    pub sync_state: JobsSyncState,
    pub synced_at: Option<DateTime<Utc>>,
    /// Labelled, slot-occupying jobs.
    pub running: Vec<IndustryJobDto>,
}

#[async_trait]
pub trait JobsCharacterSource: Send + Sync {
    async fn job_characters(
        &self,
        workspace_id: WorkspaceId,
        now: DateTime<Utc>,
    ) -> Result<Vec<JobCharacterSnapshot>, InventoryError>;
    /// Fills in blueprint, product and facility names, best-effort.
    async fn label_jobs(
        &self,
        workspace_id: WorkspaceId,
        jobs: &mut [IndustryJobDto],
    ) -> Result<(), InventoryError>;
}

/// How current a character's jobs are, from its `industry_jobs` source.
fn sync_state(
    source: Option<&CharacterSourceDetail>,
    now: DateTime<Utc>,
) -> (JobsSyncState, Option<DateTime<Utc>>) {
    let Some(source) = source else {
        return (JobsSyncState::NeverSynced, None);
    };
    let missing_scope = source
        .last_error
        .as_deref()
        .is_some_and(|error| error.starts_with("missing scope"));
    let state = if missing_scope {
        JobsSyncState::MissingScope
    } else if source.refresh_state == MarketRefreshState::Failed {
        JobsSyncState::Failed
    } else if let Some(observed_at) = source.observed_at {
        if observed_at < now - Duration::minutes(JOBS_STALE_AFTER_MINUTES) {
            JobsSyncState::Stale
        } else {
            JobsSyncState::Ok
        }
    } else {
        JobsSyncState::NeverSynced
    };
    (state, source.observed_at)
}

#[async_trait]
impl JobsCharacterSource for CharacterRosterService {
    async fn job_characters(
        &self,
        workspace_id: WorkspaceId,
        now: DateTime<Utc>,
    ) -> Result<Vec<JobCharacterSnapshot>, InventoryError> {
        let roster = self.list(workspace_id).await?;
        let mut characters = Vec::with_capacity(roster.len());
        for character in roster {
            let detail = self.detail(character.connection_id).await?;
            let (sync_state, synced_at) = sync_state(
                detail
                    .sources
                    .iter()
                    .find(|source| source.source_kind == CharacterSourceKind::IndustryJobs),
                now,
            );
            let entry = &detail.entry;
            characters.push(JobCharacterSnapshot {
                connection_id: entry.connection_id,
                eve_character_id: entry.eve_character_id,
                character_name: entry.character_name.clone(),
                slots: JobSlots {
                    manufacturing: SlotUsage {
                        used: entry.manufacturing_active_jobs.unwrap_or(0),
                        max: entry.manufacturing_max_jobs,
                    },
                    reaction: SlotUsage {
                        used: entry.reaction_active_jobs.unwrap_or(0),
                        max: entry.reaction_max_jobs,
                    },
                    research: SlotUsage {
                        used: entry.research_active_jobs.unwrap_or(0),
                        max: entry.research_max_jobs,
                    },
                },
                sync_state,
                synced_at,
                running: detail.industry_jobs,
            });
        }
        Ok(characters)
    }

    async fn label_jobs(
        &self,
        workspace_id: WorkspaceId,
        jobs: &mut [IndustryJobDto],
    ) -> Result<(), InventoryError> {
        self.label_industry_jobs(workspace_id, jobs).await
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobCharacter {
    pub connection_id: ConnectedCharacterId,
    pub eve_character_id: i64,
    pub character_name: String,
    pub slots: JobSlots,
    pub sync_state: JobsSyncState,
    pub synced_at: Option<DateTime<Utc>>,
}

/// One job, running or finished, as the Jobs page shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobView {
    pub connection_id: ConnectedCharacterId,
    pub job_id: i64,
    pub activity: IndustryActivity,
    pub status: JobStatus,
    pub blueprint_type_id: i64,
    pub blueprint_name: Option<String>,
    pub product_type_id: Option<i64>,
    pub product_name: Option<String>,
    pub runs: i64,
    /// Finished jobs only: delivered runs (for invention, the successful ones).
    pub successful_runs: Option<i64>,
    pub facility_id: i64,
    pub facility_name: Option<String>,
    pub solar_system_name: Option<String>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub pause_date: Option<DateTime<Utc>>,
    pub completed_date: Option<DateTime<Utc>>,
    pub cost: Option<Money>,
    pub ticket: Option<JobTicketLink>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveJobs {
    pub characters: Vec<JobCharacter>,
    /// Soonest-ending first.
    pub jobs: Vec<JobView>,
    pub summary: RunningJobSummary,
    /// Across characters whose skills have synced.
    pub slots: JobSlots,
    /// Oldest jobs sync among the characters, so the page can say how fresh
    /// the list is.
    pub synced_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobHistoryView {
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// When ISK Works first stored a job: history before it is unknown.
    pub earliest_observed_at: Option<DateTime<Utc>>,
    pub kpis: JobHistoryKpis,
    pub weekly: Vec<WeeklyCompletions>,
    /// Newest first.
    pub jobs: Vec<JobView>,
    pub characters: Vec<JobCharacter>,
}

#[derive(Clone)]
pub struct JobsService {
    characters: Arc<dyn JobsCharacterSource>,
    jobs: Arc<dyn IndustryJobRepository>,
}

fn add_slots(total: &mut SlotUsage, slots: SlotUsage) {
    total.used += slots.used;
    if let Some(max) = slots.max {
        total.max = Some(total.max.unwrap_or(0) + max);
    }
}

impl JobsService {
    #[must_use]
    pub fn new(
        characters: Arc<dyn JobsCharacterSource>,
        jobs: Arc<dyn IndustryJobRepository>,
    ) -> Self {
        Self { characters, jobs }
    }

    pub async fn active(
        &self,
        workspace_id: WorkspaceId,
        now: DateTime<Utc>,
    ) -> Result<ActiveJobs, InventoryError> {
        let snapshots = self.characters.job_characters(workspace_id, now).await?;
        let job_ids: Vec<i64> = snapshots
            .iter()
            .flat_map(|character| character.running.iter().map(|job| job.job_id))
            .collect();
        let tickets = self.jobs.ticket_links(workspace_id, &job_ids).await?;
        let mut slots = JobSlots::default();
        let mut jobs = Vec::new();
        for character in &snapshots {
            add_slots(&mut slots.manufacturing, character.slots.manufacturing);
            add_slots(&mut slots.reaction, character.slots.reaction);
            add_slots(&mut slots.research, character.slots.research);
            for job in &character.running {
                let end = job.end_date.unwrap_or(now);
                let Some(status) = job_status(&job.status, end, now) else {
                    continue;
                };
                jobs.push(view(
                    character.connection_id,
                    job,
                    status,
                    None,
                    None,
                    &tickets,
                ));
            }
        }
        jobs.sort_by_key(|job| (job.end_date, job.job_id));
        let summary = summarize_running(
            jobs.iter()
                .map(|job| (job.status, job.end_date.unwrap_or(now))),
            now,
        );
        let synced_at = snapshots
            .iter()
            .filter_map(|character| character.synced_at)
            .min();
        Ok(ActiveJobs {
            characters: snapshots.into_iter().map(character).collect(),
            jobs,
            summary,
            slots,
            synced_at,
        })
    }

    pub async fn history(
        &self,
        workspace_id: WorkspaceId,
        from: NaiveDate,
        to: NaiveDate,
        now: DateTime<Utc>,
    ) -> Result<JobHistoryView, InventoryError> {
        if from > to {
            return Err(InventoryError::Validation(
                "from must not be after to".into(),
            ));
        }
        if (to - from).num_days() + 1 > MAX_JOB_HISTORY_DAYS {
            return Err(InventoryError::Validation(format!(
                "range must be at most {MAX_JOB_HISTORY_DAYS} days"
            )));
        }
        let start = from
            .and_hms_opt(0, 0, 0)
            .expect("midnight exists")
            .and_utc();
        let end = (to + Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .expect("midnight exists")
            .and_utc();
        let (stored, earliest, snapshots) = tokio::try_join!(
            self.jobs.completed_jobs(workspace_id, start, end),
            self.jobs.earliest_job_seen(workspace_id),
            self.characters.job_characters(workspace_id, now),
        )?;
        let (kpis, weekly) = job_history(&stored, from, to);
        let job_ids: Vec<i64> = stored.iter().map(|job| job.job_id).collect();
        let tickets = self.jobs.ticket_links(workspace_id, &job_ids).await?;
        let mut dtos: Vec<IndustryJobDto> = stored.iter().map(dto).collect();
        self.characters.label_jobs(workspace_id, &mut dtos).await?;
        let jobs = stored
            .iter()
            .zip(&dtos)
            .filter_map(|(job, dto)| {
                let status = job_status(&job.status, job.end_date, now)?;
                Some(view(
                    job.connection_id,
                    dto,
                    status,
                    job.successful_runs,
                    job.completed_date,
                    &tickets,
                ))
            })
            .collect();
        Ok(JobHistoryView {
            from,
            to,
            earliest_observed_at: earliest,
            kpis,
            weekly,
            jobs,
            characters: snapshots.into_iter().map(character).collect(),
        })
    }
}

fn character(snapshot: JobCharacterSnapshot) -> JobCharacter {
    JobCharacter {
        connection_id: snapshot.connection_id,
        eve_character_id: snapshot.eve_character_id,
        character_name: snapshot.character_name,
        slots: snapshot.slots,
        sync_state: snapshot.sync_state,
        synced_at: snapshot.synced_at,
    }
}

fn dto(job: &StoredIndustryJob) -> IndustryJobDto {
    IndustryJobDto {
        job_id: job.job_id,
        activity: IndustryActivity::from_activity_id(job.activity_id),
        activity_id: job.activity_id,
        status: job.status.clone(),
        blueprint_type_id: job.blueprint_type_id,
        blueprint_name: None,
        product_type_id: job.product_type_id,
        product_name: None,
        runs: job.runs,
        facility_id: job.facility_id,
        facility_name: None,
        solar_system_name: None,
        start_date: Some(job.start_date),
        end_date: Some(job.end_date),
        pause_date: None,
        cost: job.cost,
    }
}

fn view(
    connection_id: ConnectedCharacterId,
    job: &IndustryJobDto,
    status: JobStatus,
    successful_runs: Option<i64>,
    completed_date: Option<DateTime<Utc>>,
    tickets: &HashMap<i64, JobTicketLink>,
) -> JobView {
    JobView {
        connection_id,
        job_id: job.job_id,
        activity: job.activity,
        status,
        blueprint_type_id: job.blueprint_type_id,
        blueprint_name: job.blueprint_name.clone(),
        product_type_id: job.product_type_id,
        product_name: job.product_name.clone(),
        runs: job.runs,
        successful_runs,
        facility_id: job.facility_id,
        facility_name: job.facility_name.clone(),
        solar_system_name: job.solar_system_name.clone(),
        start_date: job.start_date,
        end_date: job.end_date,
        pause_date: job.pause_date,
        completed_date,
        cost: job.cost,
        ticket: tickets.get(&job.job_id).cloned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(
        refresh_state: MarketRefreshState,
        observed_minutes_ago: Option<i64>,
        last_error: Option<&str>,
        now: DateTime<Utc>,
    ) -> CharacterSourceDetail {
        CharacterSourceDetail {
            source_kind: CharacterSourceKind::IndustryJobs,
            refresh_state,
            observed_at: observed_minutes_ago.map(|minutes| now - Duration::minutes(minutes)),
            next_refresh_at: None,
            last_error: last_error.map(str::to_string),
        }
    }

    #[test]
    fn sync_state_reads_the_jobs_source() {
        let now = Utc::now();
        let state = |source: Option<CharacterSourceDetail>| sync_state(source.as_ref(), now).0;
        assert_eq!(state(None), JobsSyncState::NeverSynced);
        assert_eq!(
            state(Some(source(
                MarketRefreshState::Current,
                Some(12),
                None,
                now
            ))),
            JobsSyncState::Ok
        );
        assert_eq!(
            state(Some(source(
                MarketRefreshState::Current,
                Some(61),
                None,
                now
            ))),
            JobsSyncState::Stale
        );
        assert_eq!(
            state(Some(source(
                MarketRefreshState::Failed,
                Some(5),
                Some("missing scope: esi-industry.read_character_jobs.v1"),
                now
            ))),
            JobsSyncState::MissingScope
        );
        assert_eq!(
            state(Some(source(
                MarketRefreshState::Failed,
                Some(5),
                Some("ESI 502"),
                now
            ))),
            JobsSyncState::Failed
        );
        assert_eq!(
            state(Some(source(MarketRefreshState::Missing, None, None, now))),
            JobsSyncState::NeverSynced
        );
    }
}
