use chrono::TimeZone;
use uuid::Uuid;

use super::*;

const CONNECTION: ConnectedCharacterId = ConnectedCharacterId(Uuid::from_u128(1));

fn at(month: u32, day: u32, hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, month, day, hour, 0, 0).unwrap()
}

fn date(month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, month, day).unwrap()
}

fn finished(
    job_id: i64,
    activity_id: i64,
    status: &str,
    runs: i64,
    successful: Option<i64>,
    cost: &str,
    completed: DateTime<Utc>,
) -> StoredIndustryJob {
    StoredIndustryJob {
        connection_id: CONNECTION,
        job_id,
        activity_id,
        blueprint_type_id: 691,
        product_type_id: Some(626),
        facility_id: 60_003_760,
        runs,
        successful_runs: successful,
        cost: Some(Money::parse(cost).unwrap()),
        status: status.into(),
        start_date: completed - Duration::hours(4),
        end_date: completed - Duration::hours(1),
        completed_date: Some(completed),
    }
}

#[test]
fn an_active_job_past_its_end_is_shown_ready() {
    let now = at(10, 20, 12);
    assert_eq!(
        job_status("active", at(10, 20, 13), now),
        Some(JobStatus::Active)
    );
    assert_eq!(
        job_status("active", at(10, 20, 12), now),
        Some(JobStatus::Ready)
    );
    assert_eq!(
        job_status("paused", at(10, 19, 0), now),
        Some(JobStatus::Paused)
    );
    assert_eq!(
        job_status("delivered", at(10, 19, 0), now),
        Some(JobStatus::Delivered)
    );
    assert_eq!(
        job_status("cancelled", at(10, 19, 0), now),
        Some(JobStatus::Cancelled)
    );
    assert_eq!(
        job_status("reverted", at(10, 19, 0), now),
        Some(JobStatus::Reverted)
    );
    assert_eq!(job_status("mystery", at(10, 19, 0), now), None);
}

#[test]
fn running_summary_counts_ready_and_ending_within_a_day() {
    let now = at(10, 20, 12);
    let summary = summarize_running(
        [
            (JobStatus::Active, at(10, 20, 18)),
            (JobStatus::Active, at(10, 21, 12)),
            (JobStatus::Active, at(10, 22, 0)),
            (JobStatus::Ready, at(10, 20, 0)),
            (JobStatus::Paused, at(10, 20, 13)),
            (JobStatus::Delivered, at(10, 19, 0)),
        ],
        now,
    );
    assert_eq!(
        summary,
        RunningJobSummary {
            running: 5,
            ready: 1,
            ending_within_day: 2
        }
    );
}

#[test]
fn history_kpis_count_delivered_runs_and_invention_success() {
    let jobs = vec![
        finished(1, 1, "delivered", 2, Some(2), "12400000", at(10, 13, 9)),
        finished(2, 8, "delivered", 10, Some(4), "840000", at(10, 14, 9)),
        finished(3, 8, "delivered", 20, Some(9), "1800000", at(10, 15, 9)),
        finished(4, 9, "cancelled", 40, None, "1000", at(10, 16, 9)),
        finished(5, 4, "delivered", 5, None, "3100000", at(10, 16, 10)),
    ];
    let (kpis, _) = job_history(&jobs, date(10, 1), date(10, 20));
    assert_eq!(kpis.jobs_completed, 5);
    // 2 manufactured + 4 + 9 invented + 5 ME levels; the cancelled job adds none.
    assert_eq!(kpis.runs_delivered, 20);
    assert_eq!(kpis.install_cost, Money::parse("18141000").unwrap());
    assert_eq!(
        (
            kpis.invention_successful_runs,
            kpis.invention_attempted_runs
        ),
        (13, 30)
    );
}

#[test]
fn history_buckets_every_week_of_the_window_by_activity() {
    let jobs = vec![
        finished(1, 1, "delivered", 1, None, "1", at(10, 13, 9)),
        finished(2, 9, "delivered", 1, None, "1", at(10, 14, 9)),
        finished(3, 1, "delivered", 1, None, "1", at(10, 18, 23)),
        // Outside the window.
        finished(4, 1, "delivered", 1, None, "1", at(9, 20, 9)),
    ];
    let (kpis, weekly) = job_history(&jobs, date(10, 1), date(10, 20));
    assert_eq!(kpis.jobs_completed, 3);
    let starts: Vec<NaiveDate> = weekly.iter().map(|week| week.week_start).collect();
    assert_eq!(
        starts,
        vec![date(9, 28), date(10, 5), date(10, 12), date(10, 19)]
    );
    assert!(weekly[0].by_activity.is_empty());
    assert_eq!(
        weekly[2].by_activity,
        vec![
            ActivityCount {
                activity: IndustryActivity::Manufacturing,
                count: 2
            },
            ActivityCount {
                activity: IndustryActivity::Reaction,
                count: 1
            },
        ]
    );
}

#[test]
fn statuses_serialize_camel_case() {
    assert_eq!(serde_json::to_value(JobStatus::Ready).unwrap(), "ready");
    let (kpis, weekly) = job_history(&[], date(10, 19), date(10, 20));
    let json = serde_json::to_value(&kpis).unwrap();
    assert_eq!(json["installCost"], "0.0000");
    assert_eq!(json["inventionAttemptedRuns"], 0);
    assert_eq!(
        serde_json::to_value(&weekly).unwrap()[0]["weekStart"],
        "2026-10-19"
    );
}
