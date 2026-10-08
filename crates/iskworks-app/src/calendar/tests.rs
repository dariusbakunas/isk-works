use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use iskworks_core::{
    CharacterHealth, ConnectedCharacterId, ConnectionStatus, IndustryActivity, InventoryError,
    WorkspaceId,
};
use uuid::Uuid;

use crate::character_roster::{
    CharacterDetail, CharacterRosterEntry, IndustryJobDto, SkillQueueEntryDto,
};

use super::{
    project_calendar_milestones, CalendarCharacterSource, CalendarMilestone, CalendarRange,
    CalendarService,
};

fn instant(day: u32, hour: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, day, hour, 0, 0)
        .single()
        .unwrap()
}

fn detail(name: &str, connection: u128) -> CharacterDetail {
    CharacterDetail {
        entry: CharacterRosterEntry {
            connection_id: ConnectedCharacterId(Uuid::from_u128(connection)),
            eve_character_id: connection as i64,
            character_name: name.to_string(),
            corporation_id: None,
            corporation_name: None,
            security_status: None,
            solar_system_id: None,
            solar_system_name: None,
            wallet_balance: None,
            total_sp: None,
            unallocated_sp: None,
            training_queue: Vec::new(),
            training_observed_at: None,
            training_queue_scope_missing: false,
            manufacturing_active_jobs: None,
            manufacturing_max_jobs: None,
            reaction_active_jobs: None,
            reaction_max_jobs: None,
            research_active_jobs: None,
            research_max_jobs: None,
            connection_status: ConnectionStatus::Connected,
            health: CharacterHealth::Healthy,
            last_synced_at: None,
        },
        sources: Vec::new(),
        industry_jobs: Vec::new(),
    }
}

fn industry_job(job_id: i64, occurs_at: chrono::DateTime<Utc>) -> IndustryJobDto {
    IndustryJobDto {
        job_id,
        activity: IndustryActivity::Reaction,
        activity_id: 9,
        status: "active".to_string(),
        blueprint_type_id: 1889,
        blueprint_name: Some("Nitrogen Fuel Block Blueprint".to_string()),
        product_type_id: Some(4051),
        product_name: Some("Nitrogen Fuel Block".to_string()),
        runs: 20,
        facility_id: 10_000_000_000_001,
        facility_name: Some("Tatara Prime".to_string()),
        solar_system_name: Some("Perimeter".to_string()),
        start_date: Some(occurs_at - chrono::Duration::hours(2)),
        end_date: Some(occurs_at),
    }
}

fn skill(
    skill_id: i64,
    name: &str,
    level: i64,
    position: i64,
    occurs_at: chrono::DateTime<Utc>,
) -> SkillQueueEntryDto {
    SkillQueueEntryDto {
        skill_id,
        skill_name: Some(name.to_string()),
        finished_level: level,
        queue_position: position,
        start_date: Some(occurs_at - chrono::Duration::hours(4)),
        finish_date: Some(occurs_at),
        training_start_sp: None,
        level_start_sp: None,
        level_end_sp: None,
        current_trained_level: Some(level - 1),
    }
}

fn october_range() -> CalendarRange {
    CalendarRange::new(
        instant(1, 0),
        Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).single().unwrap(),
    )
    .unwrap()
}

#[test]
fn projects_truthful_industry_fields_and_flat_tagged_json() {
    let mut character = detail("Valka", 1);
    character
        .industry_jobs
        .push(industry_job(77, instant(2, 13)));

    let milestones = project_calendar_milestones(&[character], october_range());

    let CalendarMilestone::Industry { milestone } = &milestones[0] else {
        panic!("expected Industry milestone");
    };
    assert_eq!(
        milestone.common.id,
        "industry:00000000-0000-0000-0000-000000000001:77"
    );
    assert_eq!(milestone.common.title, "Nitrogen Fuel Block");
    assert_eq!(milestone.occurs_at, instant(2, 13));
    assert_eq!(milestone.activity, IndustryActivity::Reaction);
    assert_eq!(milestone.runs, 20);
    assert_eq!(milestone.facility_name.as_deref(), Some("Tatara Prime"));

    let json = serde_json::to_value(&milestones[0]).unwrap();
    assert_eq!(json["kind"], "industry");
    assert_eq!(json["connectionId"], "00000000-0000-0000-0000-000000000001");
    assert_eq!(json["occursAt"], "2026-10-02T13:00:00Z");
    assert!(json.get("milestone").is_none(), "DTO must stay flat");
}

#[test]
fn falls_back_to_blueprint_then_type_id_without_inventing_optional_industry_data() {
    let mut character = detail("Aeryn", 2);
    let mut job = industry_job(88, instant(3, 9));
    job.product_type_id = None;
    job.product_name = None;
    job.facility_name = None;
    job.solar_system_name = None;
    character.industry_jobs.push(job);

    let milestones = project_calendar_milestones(&[character], october_range());
    let CalendarMilestone::Industry { milestone } = &milestones[0] else {
        panic!("expected Industry milestone");
    };
    assert_eq!(milestone.common.title, "Nitrogen Fuel Block Blueprint");
    assert_eq!(milestone.type_id, 1889);
    assert_eq!(milestone.facility_name, None);
    assert_eq!(milestone.solar_system_name, None);
}

#[test]
fn projects_skill_level_and_the_next_authoritative_queue_entry() {
    let mut character = detail("Tyran", 3);
    character.entry.training_queue = vec![
        skill(3387, "Advanced Mass Production", 5, 0, instant(8, 9)),
        skill(
            24625,
            "Advanced Laboratory Operation",
            4,
            1,
            instant(12, 15),
        ),
    ];

    let milestones = project_calendar_milestones(&[character], october_range());
    let CalendarMilestone::Skill { milestone } = &milestones[0] else {
        panic!("expected Skill milestone");
    };
    assert_eq!(
        milestone.common.id,
        "skill:00000000-0000-0000-0000-000000000003:3387:5"
    );
    assert_eq!(milestone.common.title, "Advanced Mass Production V");
    assert_eq!(milestone.target_level, 5);
    assert_eq!(milestone.queue_position, 0);
    assert_eq!(milestone.next_skill_type_id, Some(24625));
    assert_eq!(
        milestone.next_skill_name.as_deref(),
        Some("Advanced Laboratory Operation")
    );
}

#[test]
fn stable_identity_does_not_change_when_projected_finish_time_moves() {
    let mut first = detail("Tyran", 4);
    first.entry.training_queue = vec![skill(3387, "Advanced Mass Production", 5, 0, instant(8, 9))];
    let mut moved = detail("Tyran", 4);
    moved.entry.training_queue = vec![skill(
        3387,
        "Advanced Mass Production",
        5,
        0,
        instant(8, 10),
    )];

    let first_milestones = project_calendar_milestones(&[first], october_range());
    let moved_milestones = project_calendar_milestones(&[moved], october_range());
    let first_id = first_milestones[0].id();
    let moved_id = moved_milestones[0].id();

    assert_eq!(first_id, moved_id);
}

#[test]
fn range_is_half_open_and_results_sort_by_occurrence_then_identity_across_characters() {
    let from = instant(2, 0);
    let to = instant(3, 0);
    let range = CalendarRange::new(from, to).unwrap();
    let mut a = detail("Aeryn", 20);
    a.industry_jobs.push(industry_job(2, from));
    a.industry_jobs.push(industry_job(3, to));
    let mut x = detail("Valka", 10);
    x.entry.training_queue = vec![skill(1, "Biology", 4, 0, instant(2, 0))];

    let milestones = project_calendar_milestones(&[a, x], range);

    assert_eq!(milestones.len(), 2);
    assert_eq!(
        milestones[0].id(),
        "industry:00000000-0000-0000-0000-000000000014:2"
    );
    assert_eq!(
        milestones[1].id(),
        "skill:00000000-0000-0000-0000-00000000000a:1:4"
    );
}

#[test]
fn rejects_empty_or_reversed_ranges() {
    assert!(CalendarRange::new(instant(2, 0), instant(2, 0)).is_err());
    assert!(CalendarRange::new(instant(3, 0), instant(2, 0)).is_err());
}

struct FakeCalendarSource {
    details: HashMap<WorkspaceId, Vec<CharacterDetail>>,
    requested: Mutex<Vec<WorkspaceId>>,
}

#[async_trait]
impl CalendarCharacterSource for FakeCalendarSource {
    async fn calendar_details(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<CharacterDetail>, InventoryError> {
        self.requested.lock().unwrap().push(workspace_id);
        Ok(self.details.get(&workspace_id).cloned().unwrap_or_default())
    }
}

#[tokio::test]
async fn service_projects_only_the_authenticated_workspaces_characters() {
    let owned_workspace = WorkspaceId(Uuid::from_u128(100));
    let foreign_workspace = WorkspaceId(Uuid::from_u128(200));
    let mut owned = detail("Owned A", 10);
    owned.industry_jobs.push(industry_job(1, instant(2, 8)));
    let mut second_owned = detail("Owned B", 11);
    second_owned.entry.training_queue = vec![skill(55, "Biology", 3, 0, instant(4, 8))];
    let mut foreign = detail("Foreign", 12);
    foreign.industry_jobs.push(industry_job(2, instant(3, 8)));
    let source = Arc::new(FakeCalendarSource {
        details: HashMap::from([
            (owned_workspace, vec![owned, second_owned]),
            (foreign_workspace, vec![foreign]),
        ]),
        requested: Mutex::new(Vec::new()),
    });
    let service = CalendarService::new(source.clone());

    let milestones = service
        .range(owned_workspace, october_range())
        .await
        .unwrap();

    assert_eq!(milestones.len(), 2);
    assert!(milestones
        .iter()
        .all(|milestone| !milestone.id().contains(":12:")));
    assert_eq!(*source.requested.lock().unwrap(), vec![owned_workspace]);
}

struct FakePlanetary(Result<Vec<crate::planetary::PlanetaryTimer>, ()>);

#[async_trait]
impl super::CalendarPlanetarySource for FakePlanetary {
    async fn planetary_timers(
        &self,
        _: WorkspaceId,
    ) -> Result<Vec<crate::planetary::PlanetaryTimer>, InventoryError> {
        self.0
            .clone()
            .map_err(|()| InventoryError::Persistence("boom".into()))
    }
}

fn timer(
    occurs_at: chrono::DateTime<Utc>,
    event: crate::planetary::PlanetaryTimerEvent,
) -> crate::planetary::PlanetaryTimer {
    crate::planetary::PlanetaryTimer {
        connection_id: ConnectedCharacterId(Uuid::from_u128(10)),
        eve_character_id: 10,
        character_name: "Valka".into(),
        planet_id: 40_050_359,
        planet_name: "EUU-4N II".into(),
        planet_type: "barren".into(),
        solar_system_name: Some("EUU-4N".into()),
        occurs_at,
        event,
    }
}

fn planetary_service(
    timers: Result<Vec<crate::planetary::PlanetaryTimer>, ()>,
) -> (WorkspaceId, CalendarService) {
    let workspace = WorkspaceId(Uuid::from_u128(100));
    let mut owned = detail("Valka", 10);
    owned.industry_jobs.push(industry_job(1, instant(5, 8)));
    let source = Arc::new(FakeCalendarSource {
        details: HashMap::from([(workspace, vec![owned])]),
        requested: Mutex::new(Vec::new()),
    });
    let service = CalendarService::new(source).with_planetary(Arc::new(FakePlanetary(timers)));
    (workspace, service)
}

#[tokio::test]
async fn planetary_timers_join_the_calendar_in_time_order_within_the_range() {
    use crate::planetary::{PlanetaryTimerEvent, TimerProduct};
    let (workspace, service) = planetary_service(Ok(vec![
        timer(
            instant(3, 6),
            PlanetaryTimerEvent::ExtractorExpiry {
                extractor_count: 2,
                products: vec![TimerProduct {
                    type_id: 2_272,
                    name: "Heavy Metals".into(),
                }],
            },
        ),
        timer(
            instant(7, 12),
            PlanetaryTimerEvent::ImportDepleted {
                type_id: 2_398,
                type_name: "Reactive Metals".into(),
                qty_per_hour: "80".into(),
            },
        ),
        // Outside the October range.
        timer(
            Utc.with_ymd_and_hms(2026, 11, 2, 0, 0, 0).single().unwrap(),
            PlanetaryTimerEvent::ExtractorExpiry {
                extractor_count: 1,
                products: vec![],
            },
        ),
    ]));

    let milestones = service.range(workspace, october_range()).await.unwrap();

    let ids: Vec<&str> = milestones.iter().map(CalendarMilestone::id).collect();
    assert_eq!(ids.len(), 3);
    assert!(ids[0].starts_with("planetary:extractor:"));
    assert!(ids[1].starts_with("industry:"));
    assert!(ids[2].starts_with("planetary:import:"));

    let extractor = serde_json::to_value(&milestones[0]).unwrap();
    assert_eq!(extractor["kind"], "planetary");
    assert_eq!(extractor["event"], "extractorExpiry");
    assert_eq!(extractor["title"], "EUU-4N II extractors");
    assert_eq!(extractor["estimated"], false);
    assert_eq!(extractor["extractorCount"], 2);
    assert_eq!(extractor["products"][0]["name"], "Heavy Metals");
    let import = serde_json::to_value(&milestones[2]).unwrap();
    assert_eq!(import["event"], "importDepleted");
    assert_eq!(import["title"], "EUU-4N II out of Reactive Metals");
    assert_eq!(import["estimated"], true);
    assert_eq!(import["typeName"], "Reactive Metals");
}

#[tokio::test]
async fn a_planetary_failure_leaves_the_rest_of_the_calendar_intact() {
    let (workspace, service) = planetary_service(Err(()));

    let milestones = service.range(workspace, october_range()).await.unwrap();

    assert_eq!(milestones.len(), 1);
    assert!(milestones[0].id().starts_with("industry:"));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn persisted_calendar_source_never_crosses_workspace_boundaries(pool: sqlx::PgPool) {
    use iskworks_core::{CharacterSourceKind, OwnerId};
    use serde_json::json;

    let now = instant(1, 0);
    let owned_workspace = WorkspaceId::new();
    let foreign_workspace = WorkspaceId::new();
    let owned_owner = OwnerId::new();
    let foreign_owner = OwnerId::new();
    let mut tx = pool.begin().await.unwrap();
    for (workspace, owner, name) in [
        (owned_workspace, owned_owner, "Owned Calendar"),
        (foreign_workspace, foreign_owner, "Foreign Calendar"),
    ] {
        sqlx::query(
            "INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,$2,$3,$4,$4)",
        )
        .bind(workspace.0)
        .bind(name)
        .bind(owner.0)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual',$3,false,$4,$4)",
        )
        .bind(owner.0)
        .bind(workspace.0)
        .bind(name)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();

    let repository = Arc::new(iskworks_storage::PgEsiRepository::new(pool.clone()));
    for (workspace, owner, connection, character_id, job_id, name) in [
        (
            owned_workspace,
            owned_owner,
            ConnectedCharacterId::new(),
            9001_i64,
            101_i64,
            "Owned Pilot",
        ),
        (
            foreign_workspace,
            foreign_owner,
            ConnectedCharacterId::new(),
            9002_i64,
            202_i64,
            "Foreign Pilot",
        ),
    ] {
        sqlx::query(
            "INSERT INTO eve_connections (id,workspace_id,owner_id,eve_character_id,character_name,status,granted_scopes,connected_at,updated_at,revision) VALUES ($1,$2,$3,$4,$5,'connected',ARRAY[]::text[],$6,$6,1)",
        )
        .bind(connection.0)
        .bind(workspace.0)
        .bind(owner.0)
        .bind(character_id)
        .bind(name)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();
        repository
            .register_character_sources(connection)
            .await
            .unwrap();
        let claim = repository
            .begin_character_source_refresh(
                connection,
                CharacterSourceKind::IndustryJobs,
                now,
                now + chrono::Duration::minutes(2),
            )
            .await
            .unwrap()
            .unwrap();
        repository
            .complete_character_source_refresh(
                connection,
                CharacterSourceKind::IndustryJobs,
                claim,
                now + chrono::Duration::minutes(30),
                json!({
                    "jobs": [{
                        "job_id": job_id,
                        "activity_id": 1,
                        "status": "active",
                        "blueprint_type_id": 12004,
                        "product_type_id": 12005,
                        "facility_id": 60003760,
                        "runs": 3,
                        "start_date": now.to_rfc3339(),
                        "end_date": instant(2, 8).to_rfc3339()
                    }],
                    "activeCount": 1
                }),
                now,
            )
            .await
            .unwrap();
    }

    let roster = Arc::new(crate::CharacterRosterService::new(
        repository,
        Arc::new(iskworks_storage::PgSdeRepository::new(pool)),
    ));
    let service = CalendarService::new(roster);

    let milestones = service
        .range(owned_workspace, october_range())
        .await
        .unwrap();

    assert_eq!(milestones.len(), 1);
    assert!(milestones[0].id().ends_with(":101"));
    assert!(!milestones[0].id().contains(":202"));
}
