//! The Jobs page: every industry job across a workspace's characters.
//!
//! Running jobs come from each character's last jobs sync (the same data the
//! roster's slot counts read); finished ones from `character_industry_jobs`,
//! which keeps them after ESI's 90-day window. The rules here are pure:
//! the status a job is shown with, and the history figures.

use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Serialize;

use crate::order::TicketId;
use crate::{
    bucket_start, next_bucket, ConnectedCharacterId, Granularity, IndustryActivity, InventoryError,
    Money, WorkspaceId,
};

/// Longest history window the API accepts.
pub const MAX_JOB_HISTORY_DAYS: i64 = 366;

/// A job's status as shown. ESI reports `active` until its cache next
/// refreshes, so an `active` job whose end has passed is shown `Ready`.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JobStatus {
    Active,
    Paused,
    Ready,
    Delivered,
    Cancelled,
    Reverted,
}

impl JobStatus {
    #[must_use]
    pub fn is_running(self) -> bool {
        matches!(self, Self::Active | Self::Paused | Self::Ready)
    }
}

/// `None` for a status ESI does not document.
#[must_use]
pub fn job_status(raw: &str, end_date: DateTime<Utc>, now: DateTime<Utc>) -> Option<JobStatus> {
    Some(match raw {
        "active" if end_date <= now => JobStatus::Ready,
        "active" => JobStatus::Active,
        "paused" => JobStatus::Paused,
        "ready" => JobStatus::Ready,
        "delivered" => JobStatus::Delivered,
        "cancelled" => JobStatus::Cancelled,
        "reverted" => JobStatus::Reverted,
        _ => return None,
    })
}

/// One stored job (a `character_industry_jobs` row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredIndustryJob {
    pub connection_id: ConnectedCharacterId,
    pub job_id: i64,
    pub activity_id: i64,
    pub blueprint_type_id: i64,
    pub product_type_id: Option<i64>,
    pub facility_id: i64,
    pub runs: i64,
    pub successful_runs: Option<i64>,
    pub cost: Option<Money>,
    pub status: String,
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub completed_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobTicketLink {
    pub ticket_id: TicketId,
    pub display_id: String,
}

#[async_trait]
pub trait IndustryJobRepository: Send + Sync {
    /// Finished jobs with `completed_date` in `[from, to)`, newest first.
    async fn completed_jobs(
        &self,
        workspace_id: WorkspaceId,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<StoredIndustryJob>, InventoryError>;
    /// When ISK Works first stored any job in the workspace.
    async fn earliest_job_seen(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Option<DateTime<Utc>>, InventoryError>;
    /// The Board ticket each of these jobs is linked to, if any.
    async fn ticket_links(
        &self,
        workspace_id: WorkspaceId,
        job_ids: &[i64],
    ) -> Result<HashMap<i64, JobTicketLink>, InventoryError>;
}

/// Counts for the Active tab's summary strip.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningJobSummary {
    pub running: u32,
    pub ready: u32,
    /// Active (not paused, not yet ready) and ending within 24 hours.
    pub ending_within_day: u32,
}

#[must_use]
pub fn summarize_running(
    jobs: impl IntoIterator<Item = (JobStatus, DateTime<Utc>)>,
    now: DateTime<Utc>,
) -> RunningJobSummary {
    let mut summary = RunningJobSummary::default();
    for (status, end_date) in jobs {
        if !status.is_running() {
            continue;
        }
        summary.running += 1;
        match status {
            JobStatus::Ready => summary.ready += 1,
            JobStatus::Active if end_date <= now + Duration::hours(24) => {
                summary.ending_within_day += 1;
            }
            _ => {}
        }
    }
    summary
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobHistoryKpis {
    pub jobs_completed: u32,
    /// Delivered runs; for invention, the successful ones.
    pub runs_delivered: u64,
    /// Installation cost of every finished job, whatever its outcome.
    pub install_cost: Money,
    pub invention_attempted_runs: u64,
    pub invention_successful_runs: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyCompletions {
    /// Monday (UTC).
    pub week_start: NaiveDate,
    /// Non-zero activities only.
    pub by_activity: Vec<ActivityCount>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityCount {
    pub activity: IndustryActivity,
    pub count: u32,
}

/// KPIs and one bucket per week of `[from, to]` (empty weeks included) for
/// jobs completed in that window.
#[must_use]
pub fn job_history(
    jobs: &[StoredIndustryJob],
    from: NaiveDate,
    to: NaiveDate,
) -> (JobHistoryKpis, Vec<WeeklyCompletions>) {
    let mut kpis = JobHistoryKpis {
        jobs_completed: 0,
        runs_delivered: 0,
        install_cost: Money::zero(),
        invention_attempted_runs: 0,
        invention_successful_runs: 0,
    };
    let mut cost = Decimal::ZERO;
    let mut weeks: BTreeMap<NaiveDate, BTreeMap<u8, (IndustryActivity, u32)>> = BTreeMap::new();
    let mut week = bucket_start(from, Granularity::Week);
    while week <= to {
        weeks.insert(week, BTreeMap::new());
        week = next_bucket(week, Granularity::Week);
    }
    for job in jobs {
        let Some(completed) = job.completed_date.map(|date| date.date_naive()) else {
            continue;
        };
        if completed < from || completed > to {
            continue;
        }
        let activity = IndustryActivity::from_activity_id(job.activity_id);
        kpis.jobs_completed += 1;
        cost += job.cost.map_or(Decimal::ZERO, |cost| cost.0);
        let runs = u64::try_from(job.runs).unwrap_or(0);
        let successful = job
            .successful_runs
            .and_then(|runs| u64::try_from(runs).ok());
        if job.status == "delivered" {
            if activity == IndustryActivity::Invention {
                kpis.invention_attempted_runs += runs;
                kpis.invention_successful_runs += successful.unwrap_or(0);
                kpis.runs_delivered += successful.unwrap_or(0);
            } else {
                kpis.runs_delivered += successful.unwrap_or(runs);
            }
        }
        let entry = weeks
            .entry(bucket_start(completed, Granularity::Week))
            .or_default()
            .entry(activity_order(activity))
            .or_insert((activity, 0));
        entry.1 += 1;
    }
    let mut install_cost = cost;
    install_cost.rescale(4);
    kpis.install_cost = Money(install_cost);
    let weekly = weeks
        .into_iter()
        .map(|(week_start, counts)| WeeklyCompletions {
            week_start,
            by_activity: counts
                .into_values()
                .map(|(activity, count)| ActivityCount { activity, count })
                .collect(),
        })
        .collect();
    (kpis, weekly)
}

/// Display and stacking order of activities.
fn activity_order(activity: IndustryActivity) -> u8 {
    match activity {
        IndustryActivity::Manufacturing => 0,
        IndustryActivity::Reaction => 1,
        IndustryActivity::MaterialEfficiencyResearch => 2,
        IndustryActivity::TimeEfficiencyResearch => 3,
        IndustryActivity::Copying => 4,
        IndustryActivity::Invention => 5,
        IndustryActivity::ReverseEngineering => 6,
        IndustryActivity::Other => 7,
    }
}

#[cfg(test)]
mod tests;
