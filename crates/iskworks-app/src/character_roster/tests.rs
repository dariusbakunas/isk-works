use iskworks_core::{OwnerId, WorkspaceId};
use serde_json::json;

use super::*;

/// Null `SdeReadRepository` for the DB-backed roster test, which
/// exercises only connection/source assembly and never needs skill
/// names resolved. Replaces the API crate's `state::UnavailableSdeRepository`
/// (which stays API-owned) so this test has no `iskworks-api` dependency.
/// Only the trait's four non-defaulted methods need bodies; `type_names`
/// (the one the service actually calls) keeps its empty default.
struct NoopSdeRepository;

#[async_trait::async_trait]
impl SdeReadRepository for NoopSdeRepository {
    async fn active_sde(&self) -> Result<Option<iskworks_sde::ActiveSde>, iskworks_sde::SdeError> {
        Ok(None)
    }

    async fn search_manufacturing_blueprints(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<iskworks_sde::BlueprintSearchResult>, iskworks_sde::SdeError> {
        Ok(Vec::new())
    }

    async fn manufacturing_recipe(
        &self,
        _blueprint_type_id: i64,
    ) -> Result<Option<iskworks_sde::ManufacturingRecipe>, iskworks_sde::SdeError> {
        Ok(None)
    }

    async fn search_types(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<iskworks_sde::TypeSearchResult>, iskworks_sde::SdeError> {
        Ok(Vec::new())
    }
}

fn connection(status: ConnectionStatus) -> ConnectedCharacter {
    ConnectedCharacter {
        id: ConnectedCharacterId::new(),
        workspace_id: WorkspaceId::new(),
        owner_id: OwnerId::new(),
        eve_character_id: 2_119_000_001,
        character_name: "Aeva Stark".to_string(),
        status,
        granted_scopes: Vec::new(),
        access_token_expires_at: None,
        last_refreshed_at: None,
        last_error_code: None,
        last_error_message: None,
        connected_at: Utc::now(),
        updated_at: Utc::now(),
        disconnected_at: None,
        revision: 1,
    }
}

fn source(
    connection_id: ConnectedCharacterId,
    kind: CharacterSourceKind,
    observed_at: Option<DateTime<Utc>>,
    summary: Option<Value>,
) -> CharacterSourceSyncState {
    CharacterSourceSyncState {
        connection_id,
        source_kind: kind,
        refresh_state: if summary.is_some() {
            MarketRefreshState::Current
        } else {
            MarketRefreshState::Missing
        },
        summary,
        observed_at,
        last_attempted_at: None,
        next_refresh_at: None,
        last_error: None,
    }
}

#[test]
fn assemble_entry_extracts_every_field_from_its_source_summary() {
    let connection = connection(ConnectionStatus::Connected);
    let now = Utc::now();
    let sources = vec![
        source(
            connection.id,
            CharacterSourceKind::CharacterInfo,
            Some(now),
            Some(json!({
                "character_id": connection.eve_character_id,
                "name": "Aeva Stark",
                "corporation_id": 98_000_001,
                "security_status": "5.00000",
            })),
        ),
        source(
            connection.id,
            CharacterSourceKind::Location,
            Some(now),
            Some(json!({
                "solar_system_id": 30_000_142,
                "station_id": 60_003_760,
                "structure_id": null,
            })),
        ),
        source(
            connection.id,
            CharacterSourceKind::Skills,
            Some(now),
            Some(json!({
                "total_sp": 61_200_000,
                "unallocated_sp": 0,
                "skills": [
                    { "skill_id": 3387, "active_skill_level": 5, "trained_skill_level": 5, "skillpoints_in_skill": 0 },
                    { "skill_id": 24625, "active_skill_level": 4, "trained_skill_level": 4, "skillpoints_in_skill": 0 },
                    { "skill_id": 45748, "active_skill_level": 3, "trained_skill_level": 3, "skillpoints_in_skill": 0 },
                    { "skill_id": 3327, "active_skill_level": 4, "trained_skill_level": 4, "skillpoints_in_skill": 1_280_000 },
                ],
                "skillQueue": [
                    { "skill_id": 3327, "finished_level": 5, "queue_position": 0,
                      "start_date": now.to_rfc3339(),
                      "finish_date": (now + chrono::Duration::days(6)).to_rfc3339(),
                      "training_start_sp": 1_280_000, "level_start_sp": 1_280_000, "level_end_sp": 1_612_800 },
                    { "skill_id": 3402, "finished_level": 3, "queue_position": 1,
                      "start_date": (now + chrono::Duration::days(6)).to_rfc3339(),
                      "finish_date": (now + chrono::Duration::days(9)).to_rfc3339() }
                ],
            })),
        ),
        source(
            connection.id,
            CharacterSourceKind::Wallet,
            Some(now),
            Some(json!({ "balance": "125000000.0000" })),
        ),
        source(
            connection.id,
            CharacterSourceKind::IndustryJobs,
            Some(now),
            Some(json!({
                "jobs": [
                    { "job_id": 1, "activity_id": 1, "blueprint_type_id": 1, "facility_id": 1, "status": "active", "start_date": now.to_rfc3339(), "end_date": now.to_rfc3339() },
                    { "job_id": 2, "activity_id": 1, "blueprint_type_id": 1, "facility_id": 1, "status": "active", "start_date": now.to_rfc3339(), "end_date": now.to_rfc3339() },
                    { "job_id": 3, "activity_id": 1, "blueprint_type_id": 1, "facility_id": 1, "status": "delivered", "start_date": now.to_rfc3339(), "end_date": now.to_rfc3339() },
                    { "job_id": 4, "activity_id": 9, "blueprint_type_id": 1, "facility_id": 1, "status": "active", "start_date": now.to_rfc3339(), "end_date": now.to_rfc3339() },
                    { "job_id": 5, "activity_id": 4, "blueprint_type_id": 1, "facility_id": 1, "status": "active", "start_date": now.to_rfc3339(), "end_date": now.to_rfc3339() },
                ],
                "activeCount": 4,
            })),
        ),
    ];
    let mut names = HashMap::new();
    names.insert(98_000_001, "Perimeter Industrial Holdings".to_string());
    names.insert(30_000_142, "Jita".to_string());
    let mut skill_names = HashMap::new();
    skill_names.insert(3327, "Caldari Industrial".to_string());
    skill_names.insert(3402, "Mining".to_string());

    let entry = assemble_entry(connection.clone(), &sources, &names, &skill_names, now);

    assert_eq!(entry.corporation_id, Some(98_000_001));
    assert_eq!(
        entry.corporation_name.as_deref(),
        Some("Perimeter Industrial Holdings")
    );
    assert_eq!(
        entry.security_status,
        Some("5.00000".parse::<Decimal>().unwrap())
    );
    assert_eq!(entry.solar_system_id, Some(30_000_142));
    assert_eq!(entry.solar_system_name.as_deref(), Some("Jita"));
    assert_eq!(
        entry.wallet_balance,
        Some("125000000.0000".parse::<Decimal>().unwrap())
    );
    assert_eq!(entry.total_sp, Some(61_200_000));
    assert_eq!(entry.unallocated_sp, Some(0));
    // granted_scopes is empty on this fixture connection.
    assert!(entry.training_queue_scope_missing);
    assert_eq!(entry.training_queue.len(), 2);
    assert_eq!(entry.training_queue[0].skill_id, 3327);
    assert_eq!(
        entry.training_queue[0].skill_name.as_deref(),
        Some("Caldari Industrial")
    );
    assert_eq!(entry.training_queue[0].finished_level, 5);
    assert_eq!(entry.training_queue[0].start_date, Some(now));
    assert!(entry.training_queue[0].finish_date.is_some());
    assert_eq!(entry.training_queue[0].training_start_sp, Some(1_280_000));
    assert_eq!(entry.training_queue[0].level_start_sp, Some(1_280_000));
    assert_eq!(entry.training_queue[0].level_end_sp, Some(1_612_800));
    // 3327 is in the synced skills array at trained level 4.
    assert_eq!(entry.training_queue[0].current_trained_level, Some(4));
    // 3402 is queued but absent from the skills array -> untrained (0),
    // not "unknown".
    assert_eq!(entry.training_queue[1].current_trained_level, Some(0));
    assert_eq!(entry.training_queue[1].skill_id, 3402);
    assert_eq!(
        entry.training_queue[1].skill_name.as_deref(),
        Some("Mining")
    );
    assert_eq!(entry.training_observed_at, Some(now));
    assert_eq!(entry.manufacturing_active_jobs, Some(2));
    assert_eq!(entry.manufacturing_max_jobs, Some(10));
    assert_eq!(entry.reaction_active_jobs, Some(1));
    assert_eq!(entry.reaction_max_jobs, Some(4));
    assert_eq!(entry.research_active_jobs, Some(1));
    assert_eq!(entry.research_max_jobs, Some(1));
    assert_eq!(entry.last_synced_at, Some(now));
    assert_eq!(entry.health, CharacterHealth::Healthy);
}

#[test]
fn assemble_entry_leaves_names_none_when_the_id_is_not_yet_cached() {
    let connection = connection(ConnectionStatus::Connected);
    let now = Utc::now();
    let sources = vec![source(
        connection.id,
        CharacterSourceKind::CharacterInfo,
        Some(now),
        Some(json!({
            "character_id": connection.eve_character_id,
            "name": "Aeva Stark",
            "corporation_id": 98_000_001,
            "security_status": "5.00000",
        })),
    )];

    let entry = assemble_entry(connection, &sources, &HashMap::new(), &HashMap::new(), now);

    assert_eq!(entry.corporation_id, Some(98_000_001));
    assert_eq!(entry.corporation_name, None);
}

#[test]
fn assemble_entry_training_queue_retains_already_completed_entries() {
    // The API layer exposes the raw cached queue as-is, including
    // entries whose finish_date has already passed relative to `now`
    // -- skipping/deriving "what's current" is the frontend's/worker's
    // job (`derive_training_state`), not this read model's.
    let connection = connection(ConnectionStatus::Connected);
    let now = Utc::now();
    let sources = vec![source(
        connection.id,
        CharacterSourceKind::Skills,
        Some(now),
        Some(json!({
            "total_sp": 1,
            "unallocated_sp": 0,
            "skills": [],
            "skillQueue": [
                { "skill_id": 3327, "finished_level": 5, "queue_position": 0,
                  "start_date": (now - chrono::Duration::hours(2)).to_rfc3339(),
                  "finish_date": (now - chrono::Duration::hours(1)).to_rfc3339() },
                { "skill_id": 3402, "finished_level": 4, "queue_position": 1,
                  "start_date": (now - chrono::Duration::hours(1)).to_rfc3339(),
                  "finish_date": (now + chrono::Duration::hours(1)).to_rfc3339() },
            ],
        })),
    )];

    let entry = assemble_entry(connection, &sources, &HashMap::new(), &HashMap::new(), now);

    assert_eq!(entry.training_queue.len(), 2);
    assert_eq!(entry.training_queue[0].skill_id, 3327);
    assert_eq!(entry.training_queue[1].skill_id, 3402);
}

#[test]
fn assemble_entry_leaves_training_fields_none_without_a_skill_queue() {
    let connection = connection(ConnectionStatus::Connected);
    let now = Utc::now();
    let sources = vec![source(
        connection.id,
        CharacterSourceKind::Skills,
        Some(now),
        Some(json!({ "total_sp": 1_000_000, "unallocated_sp": 0, "skills": [] })),
    )];

    let entry = assemble_entry(connection, &sources, &HashMap::new(), &HashMap::new(), now);

    assert_eq!(entry.total_sp, Some(1_000_000));
    assert!(entry.training_queue.is_empty());
    // Skills synced but no relevant skill trained yet -> a known base
    // slot (1), not "unknown".
    assert_eq!(entry.manufacturing_max_jobs, Some(1));
    assert_eq!(entry.reaction_max_jobs, Some(1));
    assert_eq!(entry.research_max_jobs, Some(1));
    // Industry jobs never synced at all -> genuinely unknown.
    assert_eq!(entry.manufacturing_active_jobs, None);
    assert_eq!(entry.reaction_active_jobs, None);
    assert_eq!(entry.research_active_jobs, None);
}

#[test]
fn assemble_entry_reports_training_queue_scope_present_when_granted() {
    let mut connection = connection(ConnectionStatus::Connected);
    connection.granted_scopes = vec![SKILL_QUEUE_SCOPE.to_string()];
    let now = Utc::now();
    let sources = vec![source(
        connection.id,
        CharacterSourceKind::Skills,
        Some(now),
        Some(json!({ "total_sp": 1_000_000, "unallocated_sp": 25_000, "skills": [] })),
    )];

    let entry = assemble_entry(connection, &sources, &HashMap::new(), &HashMap::new(), now);

    assert!(!entry.training_queue_scope_missing);
    assert_eq!(entry.unallocated_sp, Some(25_000));
}

#[test]
fn current_trained_level_is_none_only_when_the_skills_array_is_absent() {
    let now = Utc::now();
    // skills array present, skill listed
    let present = json!({ "skills": [
        { "skill_id": 42, "trained_skill_level": 3, "active_skill_level": 3 },
    ] });
    assert_eq!(current_trained_level(Some(&present), 42), Some(3));
    // skills array present, skill omitted (ESI omits untrained skills)
    assert_eq!(current_trained_level(Some(&present), 99), Some(0));
    // skills array absent entirely
    let no_array = json!({ "total_sp": 1 });
    assert_eq!(current_trained_level(Some(&no_array), 42), None);
    assert_eq!(current_trained_level(None, 42), None);

    let sources = vec![source(
        ConnectedCharacterId::new(),
        CharacterSourceKind::Skills,
        Some(now),
        Some(json!({
            "total_sp": 10_000_000,
            "unallocated_sp": 0,
            "skills": [
                { "skill_id": 3327, "trained_skill_level": 2, "active_skill_level": 2, "skillpoints_in_skill": 45_255 },
            ],
            "skillQueue": [
                { "skill_id": 3327, "finished_level": 3, "queue_position": 0,
                  "start_date": now.to_rfc3339(),
                  "finish_date": (now + chrono::Duration::hours(4)).to_rfc3339(),
                  "training_start_sp": 45_255, "level_start_sp": 40_000, "level_end_sp": 226_275 },
            ],
        })),
    )];
    let entry = assemble_entry(
        connection(ConnectionStatus::Connected),
        &sources,
        &HashMap::new(),
        &HashMap::new(),
        now,
    );
    assert_eq!(entry.training_queue[0].current_trained_level, Some(2));
}

#[test]
fn collect_entity_ids_only_looks_at_character_info_and_location() {
    let connection_id = ConnectedCharacterId::new();
    let sources = vec![
        source(
            connection_id,
            CharacterSourceKind::CharacterInfo,
            None,
            Some(json!({ "corporation_id": 98_000_001 })),
        ),
        source(
            connection_id,
            CharacterSourceKind::Location,
            None,
            Some(json!({ "solar_system_id": 30_000_142 })),
        ),
        source(
            connection_id,
            CharacterSourceKind::Wallet,
            None,
            Some(json!({ "balance": "1.0000" })),
        ),
    ];
    let mut ids = HashSet::new();
    collect_entity_ids(&sources, &mut ids);
    assert_eq!(ids, HashSet::from([98_000_001, 30_000_142]));
}

#[test]
fn collect_training_skill_ids_reads_every_entry_in_the_skill_queue() {
    let connection_id = ConnectedCharacterId::new();
    let now = Utc::now();
    let sources = vec![source(
        connection_id,
        CharacterSourceKind::Skills,
        None,
        Some(json!({
            "total_sp": 1,
            "unallocated_sp": 0,
            "skills": [],
            "skillQueue": [
                { "skill_id": 3327, "finished_level": 5, "queue_position": 0,
                  "start_date": now.to_rfc3339(), "finish_date": now.to_rfc3339() },
                { "skill_id": 3402, "finished_level": 4, "queue_position": 1,
                  "start_date": now.to_rfc3339(), "finish_date": now.to_rfc3339() },
            ],
        })),
    )];
    let mut ids = HashSet::new();
    collect_training_skill_ids(&sources, &mut ids);
    assert_eq!(ids, HashSet::from([3327, 3402]));
}

#[allow(clippy::too_many_arguments)]
fn job(
    job_id: i64,
    activity_id: i64,
    status: &str,
    blueprint_type_id: i64,
    product_type_id: Option<i64>,
    facility_id: i64,
    runs: i64,
    ends_in: chrono::Duration,
    now: DateTime<Utc>,
) -> Value {
    let mut value = json!({
        "job_id": job_id,
        "activity_id": activity_id,
        "status": status,
        "blueprint_type_id": blueprint_type_id,
        "facility_id": facility_id,
        "runs": runs,
        "start_date": (now - chrono::Duration::hours(1)).to_rfc3339(),
        "end_date": (now + ends_in).to_rfc3339(),
    });
    if let Some(product) = product_type_id {
        value["product_type_id"] = json!(product);
    }
    value
}

#[test]
fn active_job_counts_counts_paused_and_ready_but_not_delivered() {
    let now = Utc::now();
    let summary = json!({
        "jobs": [
            job(1, 1, "active", 100, None, 60_003_760, 1, chrono::Duration::hours(2), now),
            job(2, 1, "paused", 100, None, 60_003_760, 1, chrono::Duration::hours(2), now),
            job(3, 1, "ready", 100, None, 60_003_760, 1, chrono::Duration::hours(-1), now),
            job(4, 1, "delivered", 100, None, 60_003_760, 1, chrono::Duration::hours(-2), now),
            job(5, 9, "active", 200, None, 60_003_760, 1, chrono::Duration::hours(3), now),
            job(6, 8, "active", 300, None, 60_003_760, 1, chrono::Duration::hours(4), now),
        ],
    });
    let counts = active_job_counts(&summary);
    // active + paused + ready manufacturing, delivered excluded.
    assert_eq!(counts.manufacturing, 3);
    assert_eq!(counts.reaction, 1);
    // Invention (activity 8) draws on the shared research pool.
    assert_eq!(counts.research, 1);
}

#[test]
fn parse_industry_jobs_classifies_resolves_names_and_sorts_by_finish() {
    let now = Utc::now();
    let summary = json!({
        "jobs": [
            job(10, 4, "active", 691, Some(691), 1_050_000_000_001_i64, 1, chrono::Duration::hours(9), now),
            job(11, 1, "active", 12_004, Some(12_005), 1_050_000_000_001_i64, 2, chrono::Duration::hours(1), now),
            job(12, 9, "active", 46_178, Some(60_714), 60_003_760, 400, chrono::Duration::hours(4), now),
            job(13, 1, "delivered", 12_004, Some(12_005), 1_050_000_000_001_i64, 1, chrono::Duration::hours(-3), now),
        ],
    });
    let mut type_names = HashMap::new();
    type_names.insert(12_005, "Ishtar".to_string());
    type_names.insert(691, "Merlin Blueprint".to_string());
    type_names.insert(60_714, "Nanoelectrical Microprocessor".to_string());
    let mut facilities = HashMap::new();
    facilities.insert(
        1_050_000_000_001_i64,
        FacilityLabel {
            name: "Perimeter - ISK Works Factory".to_string(),
            solar_system_name: Some("Perimeter".to_string()),
        },
    );

    let jobs = parse_industry_jobs(Some(&summary), &type_names, &facilities);

    // delivered dropped; soonest-finishing first (job 11 @ +1h, 12 @ +4h, 10 @ +9h).
    assert_eq!(
        jobs.iter().map(|j| j.job_id).collect::<Vec<_>>(),
        vec![11, 12, 10]
    );
    let manufacturing = &jobs[0];
    assert_eq!(manufacturing.activity, IndustryActivity::Manufacturing);
    assert_eq!(manufacturing.product_name.as_deref(), Some("Ishtar"));
    assert_eq!(manufacturing.runs, 2);
    assert_eq!(
        manufacturing.facility_name.as_deref(),
        Some("Perimeter - ISK Works Factory")
    );
    assert_eq!(
        manufacturing.solar_system_name.as_deref(),
        Some("Perimeter")
    );

    let reaction = &jobs[1];
    assert_eq!(reaction.activity, IndustryActivity::Reaction);
    assert_eq!(
        reaction.product_name.as_deref(),
        Some("Nanoelectrical Microprocessor")
    );
    // Facility id not in the cache -> no name, rendered as unknown structure.
    assert_eq!(reaction.facility_name, None);

    let research = &jobs[2];
    assert_eq!(
        research.activity,
        IndustryActivity::MaterialEfficiencyResearch
    );
    assert_eq!(research.blueprint_name.as_deref(), Some("Merlin Blueprint"));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn list_and_detail_assemble_a_registered_connections_sources(pool: sqlx::PgPool) {
    let now = Utc::now();
    let workspace_id = WorkspaceId::new();
    let owner_id = OwnerId::new();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, display_name, owner_id, created_at, updated_at) \
             VALUES ($1, 'Roster Test', $2, $3, $3)",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO owners (id, workspace_id, owner_kind, display_name, hidden, created_at, updated_at) \
             VALUES ($1, $2, 'manual', 'Roster Test', false, $3, $3)",
    )
    .bind(owner_id.0)
    .bind(workspace_id.0)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let connection_id = ConnectedCharacterId::new();
    sqlx::query(
        "INSERT INTO eve_connections (id, workspace_id, owner_id, eve_character_id, character_name, \
             status, granted_scopes, connected_at, updated_at, revision) \
             VALUES ($1,$2,$3,$4,$5,'connected',ARRAY[]::text[],$6,$6,1)",
    )
    .bind(connection_id.0)
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(2_119_777_001_i64)
    .bind("Roster Test Pilot")
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    // Resolved player-structure name for the industry-jobs assertion
    // below (inserted before `pool` is moved into the repository).
    let facility_id = 1_050_000_000_777_i64;
    sqlx::query(
        "INSERT INTO market_location_names \
             (workspace_id, location_id, location_name, owner_id, solar_system_id, resolved_at, updated_at) \
             VALUES ($1, $2, 'Perimeter - Test Factory', 98000001, 30000144, $3, $3)",
    )
    .bind(workspace_id.0)
    .bind(facility_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let repository = std::sync::Arc::new(iskworks_storage::PgEsiRepository::new(pool));
    repository
        .register_character_sources(connection_id)
        .await
        .unwrap();
    let claim = repository
        .begin_character_source_refresh(
            connection_id,
            CharacterSourceKind::Wallet,
            now,
            now + chrono::Duration::seconds(120),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_character_source_refresh(
            connection_id,
            CharacterSourceKind::Wallet,
            claim,
            now + chrono::Duration::minutes(15),
            json!({ "balance": "42.0000" }),
            now,
        )
        .await
        .unwrap();

    // A running-jobs summary so `detail()` exercises the real
    // jobs -> DTO + facility-label path (structure name inserted above).
    let jobs_claim = repository
        .begin_character_source_refresh(
            connection_id,
            CharacterSourceKind::IndustryJobs,
            now,
            now + chrono::Duration::seconds(120),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_character_source_refresh(
            connection_id,
            CharacterSourceKind::IndustryJobs,
            jobs_claim,
            now + chrono::Duration::minutes(30),
            json!({
                "jobs": [
                    { "job_id": 20, "activity_id": 4, "status": "active", "blueprint_type_id": 691,
                      "product_type_id": 691, "facility_id": facility_id, "runs": 1,
                      "start_date": now.to_rfc3339(),
                      "end_date": (now + chrono::Duration::hours(9)).to_rfc3339() },
                    { "job_id": 21, "activity_id": 1, "status": "active", "blueprint_type_id": 12004,
                      "product_type_id": 12005, "facility_id": facility_id, "runs": 3,
                      "start_date": now.to_rfc3339(),
                      "end_date": (now + chrono::Duration::hours(2)).to_rfc3339() },
                    { "job_id": 22, "activity_id": 1, "status": "delivered", "blueprint_type_id": 12004,
                      "product_type_id": 12005, "facility_id": facility_id, "runs": 1,
                      "start_date": now.to_rfc3339(), "end_date": now.to_rfc3339() }
                ],
                "activeCount": 2,
            }),
            now,
        )
        .await
        .unwrap();

    let service = CharacterRosterService::new(repository, std::sync::Arc::new(NoopSdeRepository));

    let roster = service.list(workspace_id).await.unwrap();
    assert_eq!(roster.len(), 1);
    assert_eq!(roster[0].connection_id, connection_id);
    assert_eq!(
        roster[0].wallet_balance,
        Some("42.0000".parse::<Decimal>().unwrap())
    );

    let detail = service.detail(connection_id).await.unwrap();
    assert_eq!(detail.entry.connection_id, connection_id);
    assert_eq!(
        detail
            .sources
            .iter()
            .map(|source| source.source_kind)
            .collect::<std::collections::HashSet<_>>(),
        CharacterSourceKind::all()
            .into_iter()
            .collect::<std::collections::HashSet<_>>(),
    );
    let wallet_source = detail
        .sources
        .iter()
        .find(|source| source.source_kind == CharacterSourceKind::Wallet)
        .unwrap();
    assert_eq!(wallet_source.refresh_state, MarketRefreshState::Current);

    // Slot-occupying jobs only (delivered dropped), soonest-finishing
    // first, classified, with the player-structure name resolved from
    // market_location_names.
    assert_eq!(
        detail
            .industry_jobs
            .iter()
            .map(|job| job.job_id)
            .collect::<Vec<_>>(),
        vec![21, 20]
    );
    let manufacturing = &detail.industry_jobs[0];
    assert_eq!(manufacturing.activity, IndustryActivity::Manufacturing);
    assert_eq!(manufacturing.runs, 3);
    assert_eq!(
        manufacturing.facility_name.as_deref(),
        Some("Perimeter - Test Factory")
    );
    assert_eq!(
        detail.industry_jobs[1].activity,
        IndustryActivity::MaterialEfficiencyResearch
    );
    // Counts on the flattened entry match the rendered job list.
    assert_eq!(detail.entry.manufacturing_active_jobs, Some(1));
    assert_eq!(detail.entry.research_active_jobs, Some(1));
}
