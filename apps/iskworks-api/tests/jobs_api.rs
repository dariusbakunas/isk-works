use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::{DateTime, Duration, Utc};
use iskworks_api::{build_router, AppState};
use iskworks_app::{
    IndustryJobDto, JobCharacterSnapshot, JobSlots, JobsCharacterSource, JobsService,
    JobsSyncState, SlotUsage,
};
use iskworks_core::order::TicketId;
use iskworks_core::{
    ConnectedCharacterId, IndustryActivity, IndustryJobRepository, InventoryError, JobTicketLink,
    Money, StoredIndustryJob, WorkspaceId,
};
use tower::ServiceExt;
use uuid::Uuid;

mod support;
use support::workspace::configured_workspace;

const KIRA: ConnectedCharacterId = ConnectedCharacterId(Uuid::from_u128(1));
const MOX: ConnectedCharacterId = ConnectedCharacterId(Uuid::from_u128(2));
const TICKET: TicketId = TicketId(Uuid::from_u128(9));

fn running(job_id: i64, activity_id: i64, status: &str, end: DateTime<Utc>) -> IndustryJobDto {
    IndustryJobDto {
        job_id,
        activity: IndustryActivity::from_activity_id(activity_id),
        activity_id,
        status: status.into(),
        blueprint_type_id: 691,
        blueprint_name: Some("Ishtar Blueprint".into()),
        product_type_id: Some(12_005),
        product_name: Some("Ishtar".into()),
        runs: 2,
        facility_id: 1_050_474_463_169,
        facility_name: Some("Tatara \"Forge Two\"".into()),
        solar_system_name: Some("Perimeter".into()),
        start_date: Some(end - Duration::hours(10)),
        end_date: Some(end),
        pause_date: None,
        cost: Some(Money::parse("12400000").unwrap()),
    }
}

struct FakeCharacters {
    now: DateTime<Utc>,
}

#[async_trait]
impl JobsCharacterSource for FakeCharacters {
    async fn job_characters(
        &self,
        _workspace_id: WorkspaceId,
        _now: DateTime<Utc>,
    ) -> Result<Vec<JobCharacterSnapshot>, InventoryError> {
        let now = self.now;
        Ok(vec![
            JobCharacterSnapshot {
                connection_id: KIRA,
                eve_character_id: 2_119_000_001,
                character_name: "Kira Vantor".into(),
                slots: JobSlots {
                    manufacturing: SlotUsage {
                        used: 2,
                        max: Some(12),
                    },
                    reaction: SlotUsage {
                        used: 0,
                        max: Some(10),
                    },
                    research: SlotUsage {
                        used: 0,
                        max: Some(20),
                    },
                },
                sync_state: JobsSyncState::Ok,
                synced_at: Some(now - Duration::minutes(12)),
                running: vec![
                    running(11, 1, "active", now + Duration::hours(3)),
                    running(12, 1, "active", now - Duration::minutes(5)),
                ],
            },
            JobCharacterSnapshot {
                connection_id: MOX,
                eve_character_id: 2_119_000_002,
                character_name: "Mox Halden".into(),
                slots: JobSlots {
                    manufacturing: SlotUsage { used: 0, max: None },
                    reaction: SlotUsage::default(),
                    research: SlotUsage { used: 1, max: None },
                },
                sync_state: JobsSyncState::MissingScope,
                synced_at: Some(now - Duration::hours(2)),
                running: vec![running(21, 4, "paused", now + Duration::days(2))],
            },
        ])
    }

    async fn label_jobs(
        &self,
        _workspace_id: WorkspaceId,
        jobs: &mut [IndustryJobDto],
    ) -> Result<(), InventoryError> {
        for job in jobs {
            job.blueprint_name = Some(format!("Blueprint {}", job.blueprint_type_id));
        }
        Ok(())
    }
}

#[derive(Default)]
struct FakeJobs {
    windows: Mutex<Vec<(DateTime<Utc>, DateTime<Utc>)>>,
}

#[async_trait]
impl IndustryJobRepository for FakeJobs {
    async fn completed_jobs(
        &self,
        _workspace_id: WorkspaceId,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<StoredIndustryJob>, InventoryError> {
        self.windows.lock().unwrap().push((from, to));
        let completed = to - Duration::days(2);
        Ok(vec![StoredIndustryJob {
            connection_id: KIRA,
            job_id: 31,
            activity_id: 8,
            blueprint_type_id: 691,
            product_type_id: Some(12_005),
            facility_id: 60_003_760,
            runs: 10,
            successful_runs: Some(4),
            cost: Some(Money::parse("840000").unwrap()),
            status: "delivered".into(),
            start_date: completed - Duration::hours(10),
            end_date: completed - Duration::hours(1),
            completed_date: Some(completed),
        }])
    }

    async fn earliest_job_seen(
        &self,
        _workspace_id: WorkspaceId,
    ) -> Result<Option<DateTime<Utc>>, InventoryError> {
        Ok(Some("2026-10-20T12:00:00Z".parse().unwrap()))
    }

    async fn ticket_links(
        &self,
        _workspace_id: WorkspaceId,
        job_ids: &[i64],
    ) -> Result<HashMap<i64, JobTicketLink>, InventoryError> {
        Ok(job_ids
            .iter()
            .filter(|id| **id == 11 || **id == 31)
            .map(|id| {
                (
                    *id,
                    JobTicketLink {
                        ticket_id: TICKET,
                        display_id: "TK-142".into(),
                    },
                )
            })
            .collect())
    }
}

fn app(jobs: Arc<FakeJobs>) -> axum::Router {
    let service = JobsService::new(Arc::new(FakeCharacters { now: Utc::now() }), jobs);
    build_router(
        AppState::new(Arc::new(configured_workspace("Jobs Test")))
            .with_jobs_service(Arc::new(service)),
    )
}

async fn get(app: axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

#[tokio::test]
async fn active_jobs_list_running_jobs_soonest_first_with_summary_and_slots() {
    let (status, body) = get(app(Arc::default()), "/api/jobs").await;

    assert_eq!(status, StatusCode::OK);
    let jobs = body["jobs"].as_array().unwrap();
    let ids: Vec<i64> = jobs
        .iter()
        .map(|job| job["jobId"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, vec![12, 11, 21]);
    // Past its end but still `active` in ESI: shown ready.
    assert_eq!(jobs[0]["status"], "ready");
    assert_eq!(jobs[1]["status"], "active");
    assert_eq!(jobs[1]["ticket"]["displayId"], "TK-142");
    assert_eq!(jobs[1]["activity"], "manufacturing");
    assert_eq!(jobs[1]["facilityName"], "Tatara \"Forge Two\"");
    assert!(jobs[0]["ticket"].is_null());
    assert_eq!(jobs[2]["status"], "paused");
    assert_eq!(jobs[2]["connectionId"], MOX.0.to_string());

    assert_eq!(body["summary"]["running"], 3);
    assert_eq!(body["summary"]["ready"], 1);
    assert_eq!(body["summary"]["endingWithinDay"], 1);
    assert_eq!(body["slots"]["manufacturing"]["used"], 2);
    assert_eq!(body["slots"]["manufacturing"]["max"], 12);
    assert_eq!(body["slots"]["research"]["used"], 1);
    assert_eq!(body["slots"]["research"]["max"], 20);
    assert_eq!(body["characters"][1]["syncState"], "missingScope");
    assert_eq!(body["characters"][0]["characterName"], "Kira Vantor");
    assert!(body["syncedAt"].is_string());
}

#[tokio::test]
async fn history_defaults_to_the_last_30_days_and_labels_finished_jobs() {
    let jobs = Arc::new(FakeJobs::default());
    let (status, body) = get(app(jobs.clone()), "/api/jobs/history").await;

    assert_eq!(status, StatusCode::OK);
    let today = Utc::now().date_naive();
    assert_eq!(body["to"], today.to_string());
    assert_eq!(body["from"], (today - Duration::days(29)).to_string());
    let (from, to) = jobs.windows.lock().unwrap()[0];
    assert_eq!(from.date_naive(), today - Duration::days(29));
    assert_eq!(to.date_naive(), today + Duration::days(1));

    assert_eq!(body["kpis"]["jobsCompleted"], 1);
    assert_eq!(body["kpis"]["inventionSuccessfulRuns"], 4);
    assert_eq!(body["kpis"]["inventionAttemptedRuns"], 10);
    assert_eq!(body["earliestObservedAt"], "2026-10-20T12:00:00Z");
    let job = &body["jobs"][0];
    assert_eq!(job["status"], "delivered");
    assert_eq!(job["activity"], "invention");
    assert_eq!(job["successfulRuns"], 4);
    assert_eq!(job["blueprintName"], "Blueprint 691");
    assert_eq!(job["ticket"]["displayId"], "TK-142");
    assert!(job["completedDate"].is_string());
    assert!(body["weekly"].as_array().unwrap().len() >= 5);
}

#[tokio::test]
async fn history_rejects_invalid_windows_before_storage_access() {
    let jobs = Arc::new(FakeJobs::default());
    for uri in [
        "/api/jobs/history?from=2026-10-02&to=2026-10-01",
        "/api/jobs/history?from=10/01/2026",
        "/api/jobs/history?from=2024-01-01&to=2026-10-01",
    ] {
        let (status, body) = get(app(jobs.clone()), uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(body["error"]["code"], "validation_failed", "{uri}");
    }
    assert!(jobs.windows.lock().unwrap().is_empty());
}

#[tokio::test]
async fn jobs_without_storage_are_unavailable() {
    let app = build_router(AppState::new(Arc::new(configured_workspace("Jobs Test"))));
    for uri in ["/api/jobs", "/api/jobs/history"] {
        let (status, _) = get(app.clone(), uri).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{uri}");
    }
}
