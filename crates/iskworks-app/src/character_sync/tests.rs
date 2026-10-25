use tokio_util::sync::CancellationToken;

fn no_cancel() -> &'static CancellationToken {
    static TOKEN: std::sync::OnceLock<CancellationToken> = std::sync::OnceLock::new();
    TOKEN.get_or_init(CancellationToken::new)
}
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use iskworks_core::{
    ConnectionStatus, EsiSyncKind, EsiSyncRun, EsiSyncRunId, EsiSyncStatus, OwnerId, WorkspaceId,
};
use iskworks_esi::{
    AssetObservation, AuthenticatedToken, BlueprintAssetObservation,
    CalendarEventDetailObservation, CalendarEventObservation, CharacterAttributesObservation,
    CharacterIndustryJobObservation, CharacterLocationObservation, CharacterOrderObservation,
    CharacterPlanetDetailObservation, CharacterPlanetObservation, CharacterPublicInfo,
    CharacterSkillEntry, CharacterSkillQueueEntry, CharacterSkillsObservation, EsiError,
    EsiResponse, EsiResponseMetadata, EveEntityName, IndustrySystemCostIndex, PkceVerifier,
    PlanetPinContentObservation, PlanetPinObservation, RefreshedToken, StructureInformation,
    WalletBalanceObservation, WalletTransactionObservation,
};

use super::*;

fn connection(granted_scopes: Vec<String>) -> ConnectedCharacter {
    ConnectedCharacter {
        id: ConnectedCharacterId::new(),
        workspace_id: WorkspaceId::new(),
        owner_id: OwnerId::new(),
        eve_character_id: 2_119_000_001,
        character_name: "Aeva Stark".to_string(),
        status: ConnectionStatus::Connected,
        granted_scopes,
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

fn all_scopes() -> Vec<String> {
    vec![
        LOCATION_SCOPE.to_string(),
        SKILLS_SCOPE.to_string(),
        SKILL_QUEUE_SCOPE.to_string(),
        WALLET_SCOPE.to_string(),
        INDUSTRY_JOBS_SCOPE.to_string(),
        ASSET_SCOPE.to_string(),
        PLANETS_SCOPE.to_string(),
        CALENDAR_SCOPE.to_string(),
        MARKET_ORDERS_SCOPE.to_string(),
    ]
}

#[derive(Default)]
struct RecordingRepository {
    registered: Mutex<Vec<ConnectedCharacterId>>,
    completed: Mutex<Vec<(ConnectedCharacterId, CharacterSourceKind, Value)>>,
    completed_next_refresh_ats:
        Mutex<Vec<(ConnectedCharacterId, CharacterSourceKind, DateTime<Utc>)>>,
    failed: Mutex<Vec<(ConnectedCharacterId, CharacterSourceKind, String)>>,
    failed_next_refresh_ats: Mutex<Vec<(ConnectedCharacterId, CharacterSourceKind, DateTime<Utc>)>>,
    unclaimable: Mutex<HashSet<CharacterSourceKind>>,
    cached_names: Mutex<HashMap<i64, String>>,
    entity_names_calls: Mutex<Vec<Vec<i64>>>,
    persisted_sources: Mutex<HashMap<ConnectedCharacterId, Vec<CharacterSourceSyncState>>>,
    deferred: Mutex<Vec<(ConnectedCharacterId, DateTime<Utc>)>>,
    calendar_copies: Mutex<Vec<CharacterCalendarEvent>>,
    stored_calendar: Mutex<Vec<(ConnectedCharacterId, Vec<CharacterCalendarEvent>)>>,
    recorded_jobs: Mutex<Vec<(ConnectedCharacterId, Vec<CharacterIndustryJobObservation>)>>,
    stored_market_orders: Mutex<Vec<(ConnectedCharacterId, Vec<CharacterMarketOrder>)>>,
}

#[async_trait]
impl CharacterSyncRepository for RecordingRepository {
    async fn record_industry_jobs(
        &self,
        connection_id: ConnectedCharacterId,
        jobs: &[CharacterIndustryJobObservation],
        _observed_at: DateTime<Utc>,
    ) -> Result<(), InventoryError> {
        self.recorded_jobs
            .lock()
            .unwrap()
            .push((connection_id, jobs.to_vec()));
        Ok(())
    }

    async fn register_character_sources(
        &self,
        connection_id: ConnectedCharacterId,
    ) -> Result<(), InventoryError> {
        self.registered.lock().unwrap().push(connection_id);
        Ok(())
    }

    async fn begin_character_source_refresh(
        &self,
        _connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        attempted_at: DateTime<Utc>,
        _lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, InventoryError> {
        if self.unclaimable.lock().unwrap().contains(&source_kind) {
            Ok(None)
        } else {
            Ok(Some(attempted_at))
        }
    }

    async fn complete_character_source_refresh(
        &self,
        connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        _claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        summary: Value,
        _observed_at: DateTime<Utc>,
    ) -> Result<bool, InventoryError> {
        self.completed
            .lock()
            .unwrap()
            .push((connection_id, source_kind, summary));
        self.completed_next_refresh_ats.lock().unwrap().push((
            connection_id,
            source_kind,
            next_refresh_at,
        ));
        Ok(true)
    }

    async fn fail_character_source_refresh(
        &self,
        connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        _claim: DateTime<Utc>,
        _attempted_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        error_message: String,
    ) -> Result<bool, InventoryError> {
        self.failed
            .lock()
            .unwrap()
            .push((connection_id, source_kind, error_message));
        self.failed_next_refresh_ats.lock().unwrap().push((
            connection_id,
            source_kind,
            next_refresh_at,
        ));
        Ok(true)
    }

    async fn defer_character_sources(
        &self,
        connection_id: ConnectedCharacterId,
        until: DateTime<Utc>,
    ) -> Result<(), InventoryError> {
        self.deferred.lock().unwrap().push((connection_id, until));
        Ok(())
    }

    async fn entity_names(&self, ids: &[i64]) -> Result<HashMap<i64, String>, InventoryError> {
        self.entity_names_calls.lock().unwrap().push(ids.to_vec());
        let cache = self.cached_names.lock().unwrap();
        Ok(ids
            .iter()
            .filter_map(|id| cache.get(id).map(|name| (*id, name.clone())))
            .collect())
    }

    async fn cache_entity_names(
        &self,
        names: &[iskworks_esi::EveEntityName],
    ) -> Result<(), InventoryError> {
        let mut cache = self.cached_names.lock().unwrap();
        for name in names {
            cache.insert(name.id, name.name.clone());
        }
        Ok(())
    }

    async fn character_source_state(
        &self,
        connection_id: ConnectedCharacterId,
    ) -> Result<Vec<CharacterSourceSyncState>, InventoryError> {
        Ok(self
            .persisted_sources
            .lock()
            .unwrap()
            .get(&connection_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn calendar_event_copies(
        &self,
        _workspace_id: WorkspaceId,
        event_ids: &[i64],
    ) -> Result<Vec<CharacterCalendarEvent>, InventoryError> {
        Ok(self
            .calendar_copies
            .lock()
            .unwrap()
            .iter()
            .filter(|copy| event_ids.contains(&copy.event_id))
            .cloned()
            .collect())
    }

    async fn store_calendar_events(
        &self,
        connection_id: ConnectedCharacterId,
        events: &[CharacterCalendarEvent],
    ) -> Result<(), InventoryError> {
        self.stored_calendar
            .lock()
            .unwrap()
            .push((connection_id, events.to_vec()));
        Ok(())
    }

    async fn store_market_orders(
        &self,
        connection_id: ConnectedCharacterId,
        orders: &[CharacterMarketOrder],
        _observed_at: DateTime<Utc>,
    ) -> Result<(), InventoryError> {
        self.stored_market_orders
            .lock()
            .unwrap()
            .push((connection_id, orders.to_vec()));
        Ok(())
    }
}

#[derive(Default)]
struct FakeTransport {
    fail: Mutex<HashMap<&'static str, EsiError>>,
    universe_names_calls: Mutex<Vec<Vec<i64>>>,
    skill_queue_override: Mutex<Option<Vec<CharacterSkillQueueEntry>>>,
    /// Every upcoming event the fake calendar knows, served 50 at a time
    /// like ESI; `None` serves one event.
    calendar_override: Mutex<Option<Vec<CalendarEventObservation>>>,
    calendar_calls: Mutex<Vec<Option<i64>>>,
    calendar_detail_calls: Mutex<Vec<i64>>,
    /// `None` serves one personal sell order.
    market_orders_override: Mutex<Option<Vec<CharacterOrderObservation>>>,
}

impl FakeTransport {
    fn failing(endpoint: &'static str, error: EsiError) -> Self {
        let mut fail = HashMap::new();
        fail.insert(endpoint, error);
        Self {
            fail: Mutex::new(fail),
            ..Self::default()
        }
    }

    fn with_skill_queue(entries: Vec<CharacterSkillQueueEntry>) -> Self {
        Self {
            skill_queue_override: Mutex::new(Some(entries)),
            ..Self::default()
        }
    }

    fn with_calendar(events: Vec<CalendarEventObservation>) -> Self {
        Self {
            calendar_override: Mutex::new(Some(events)),
            ..Self::default()
        }
    }

    fn check(&self, endpoint: &'static str) -> Result<(), EsiError> {
        match self.fail.lock().unwrap().get(endpoint) {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }
}

#[async_trait]
impl EsiTransport for FakeTransport {
    async fn exchange_code(
        &self,
        _code: &str,
        _verifier: &PkceVerifier,
    ) -> Result<AuthenticatedToken, EsiError> {
        Err(EsiError::PermanentFailure)
    }

    async fn refresh(&self, _refresh_token: &str) -> Result<RefreshedToken, EsiError> {
        Err(EsiError::PermanentFailure)
    }

    async fn assets(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<AssetObservation>, EsiError> {
        Err(EsiError::PermanentFailure)
    }

    async fn blueprints(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
    ) -> Result<EsiResponse<BlueprintAssetObservation>, EsiError> {
        Err(EsiError::PermanentFailure)
    }

    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        _from_id: Option<i64>,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<WalletTransactionObservation>, EsiError> {
        Err(EsiError::PermanentFailure)
    }

    async fn wallet_balance(
        &self,
        _access_token: &str,
        _character_id: i64,
    ) -> Result<EsiResponse<WalletBalanceObservation>, EsiError> {
        self.check("wallet_balance")?;
        Ok(EsiResponse {
            records: vec![WalletBalanceObservation {
                balance: "125000000.0000".parse().unwrap(),
            }],
            not_modified: false,
            metadata: EsiResponseMetadata::default(),
        })
    }

    async fn universe_names(&self, ids: &[i64]) -> Result<Vec<EveEntityName>, EsiError> {
        self.check("universe_names")?;
        self.universe_names_calls.lock().unwrap().push(ids.to_vec());
        Ok(ids
            .iter()
            .filter_map(|&id| match id {
                98_000_001 => Some(EveEntityName {
                    id,
                    name: "Perimeter Industrial Holdings".to_string(),
                    category: "corporation".to_string(),
                }),
                30_000_142 => Some(EveEntityName {
                    id,
                    name: "Jita".to_string(),
                    category: "solar_system".to_string(),
                }),
                _ => None,
            })
            .collect())
    }

    async fn structure(
        &self,
        _access_token: &str,
        _structure_id: i64,
    ) -> Result<StructureInformation, EsiError> {
        Err(EsiError::PermanentFailure)
    }

    async fn industry_systems(&self) -> Result<EsiResponse<IndustrySystemCostIndex>, EsiError> {
        Err(EsiError::PermanentFailure)
    }

    async fn character_public_info(
        &self,
        character_id: i64,
    ) -> Result<EsiResponse<CharacterPublicInfo>, EsiError> {
        self.check("character_public_info")?;
        Ok(EsiResponse {
            records: vec![CharacterPublicInfo {
                character_id,
                name: "Aeva Stark".to_string(),
                corporation_id: 98_000_001,
                security_status: Some("5.00000".parse().unwrap()),
            }],
            not_modified: false,
            metadata: EsiResponseMetadata::default(),
        })
    }

    async fn character_location(
        &self,
        _access_token: &str,
        _character_id: i64,
    ) -> Result<EsiResponse<CharacterLocationObservation>, EsiError> {
        self.check("character_location")?;
        Ok(EsiResponse {
            records: vec![CharacterLocationObservation {
                solar_system_id: 30_000_142,
                station_id: Some(60_003_760),
                structure_id: None,
            }],
            not_modified: false,
            metadata: EsiResponseMetadata::default(),
        })
    }

    async fn character_skills(
        &self,
        _access_token: &str,
        _character_id: i64,
    ) -> Result<EsiResponse<CharacterSkillsObservation>, EsiError> {
        self.check("character_skills")?;
        Ok(EsiResponse {
            records: vec![CharacterSkillsObservation {
                total_sp: 61_200_000,
                unallocated_sp: Some(0),
                skills: vec![CharacterSkillEntry {
                    skill_id: 3380,
                    active_skill_level: 5,
                    trained_skill_level: 5,
                    skillpoints_in_skill: 1_280_000,
                }],
            }],
            not_modified: false,
            metadata: EsiResponseMetadata::default(),
        })
    }

    async fn character_attributes(
        &self,
        _access_token: &str,
        _character_id: i64,
    ) -> Result<EsiResponse<CharacterAttributesObservation>, EsiError> {
        self.check("character_attributes")?;
        Ok(EsiResponse {
            records: vec![CharacterAttributesObservation {
                charisma: 19,
                intelligence: 20,
                memory: 27,
                perception: 27,
                willpower: 17,
                bonus_remaps: Some(1),
                last_remap_date: None,
                accrued_remap_cooldown_date: None,
            }],
            not_modified: false,
            metadata: EsiResponseMetadata::default(),
        })
    }

    async fn character_skill_queue(
        &self,
        _access_token: &str,
        _character_id: i64,
    ) -> Result<EsiResponse<CharacterSkillQueueEntry>, EsiError> {
        self.check("character_skill_queue")?;
        let records = match self.skill_queue_override.lock().unwrap().clone() {
            Some(entries) => entries,
            None => vec![CharacterSkillQueueEntry {
                skill_id: 3327,
                finished_level: 5,
                queue_position: 0,
                start_date: Some(Utc::now()),
                finish_date: Some(Utc::now() + Duration::days(6)),
                training_start_sp: Some(1_280_000),
                level_start_sp: Some(1_280_000),
                level_end_sp: Some(1_612_800),
            }],
        };
        Ok(EsiResponse {
            records,
            not_modified: false,
            metadata: EsiResponseMetadata::default(),
        })
    }

    async fn character_industry_jobs(
        &self,
        _access_token: &str,
        _character_id: i64,
    ) -> Result<EsiResponse<CharacterIndustryJobObservation>, EsiError> {
        self.check("character_industry_jobs")?;
        Ok(EsiResponse {
            records: vec![
                CharacterIndustryJobObservation {
                    job_id: 500_001,
                    activity_id: 1,
                    blueprint_id: Some(1_040_000_000_001),
                    blueprint_type_id: 691,
                    product_type_id: Some(626),
                    facility_id: 1_050_474_463_169,
                    station_id: None,
                    runs: 2,
                    licensed_runs: Some(100),
                    cost: None,
                    probability: None,
                    duration_seconds: Some(14_400),
                    status: "active".to_string(),
                    start_date: Utc::now() - Duration::hours(1),
                    end_date: Utc::now() + Duration::hours(3),
                    pause_date: None,
                    completed_date: None,
                    completed_character_id: None,
                    successful_runs: None,
                },
                // Finished jobs come back too (`include_completed=true`).
                CharacterIndustryJobObservation {
                    job_id: 500_000,
                    activity_id: 1,
                    blueprint_id: Some(1_040_000_000_001),
                    blueprint_type_id: 691,
                    product_type_id: Some(626),
                    facility_id: 1_050_474_463_169,
                    station_id: None,
                    runs: 1,
                    licensed_runs: Some(100),
                    cost: None,
                    probability: None,
                    duration_seconds: Some(7_200),
                    status: "delivered".to_string(),
                    start_date: Utc::now() - Duration::days(2),
                    end_date: Utc::now() - Duration::days(1),
                    pause_date: None,
                    completed_date: Some(Utc::now() - Duration::days(1)),
                    completed_character_id: Some(2_119_000_001),
                    successful_runs: Some(1),
                },
            ],
            not_modified: false,
            metadata: EsiResponseMetadata::default(),
        })
    }

    async fn character_planets(
        &self,
        _access_token: &str,
        _character_id: i64,
    ) -> Result<EsiResponse<CharacterPlanetObservation>, EsiError> {
        self.check("character_planets")?;
        Ok(EsiResponse {
            records: vec![CharacterPlanetObservation {
                planet_id: 40_050_361,
                planet_type: "barren".to_string(),
                solar_system_id: 30_000_797,
                upgrade_level: 5,
                num_pins: 2,
                last_update: "2026-10-01T06:23:47Z".parse().unwrap(),
            }],
            not_modified: false,
            metadata: EsiResponseMetadata::default(),
        })
    }

    async fn character_planet_detail(
        &self,
        _access_token: &str,
        _character_id: i64,
        _planet_id: i64,
    ) -> Result<EsiResponse<CharacterPlanetDetailObservation>, EsiError> {
        self.check("character_planet_detail")?;
        Ok(EsiResponse {
            records: vec![CharacterPlanetDetailObservation {
                pins: vec![PlanetPinObservation {
                    pin_id: 1,
                    type_id: 2_544,
                    schematic_id: None,
                    contents: vec![PlanetPinContentObservation {
                        type_id: 2_398,
                        amount: 100,
                    }],
                    install_time: None,
                    expiry_time: None,
                    last_cycle_start: None,
                    extractor: None,
                }],
            }],
            not_modified: false,
            metadata: EsiResponseMetadata::default(),
        })
    }

    async fn character_orders(
        &self,
        _access_token: &str,
        _character_id: i64,
    ) -> Result<EsiResponse<CharacterOrderObservation>, EsiError> {
        self.check("character_orders")?;
        Ok(EsiResponse {
            records: self
                .market_orders_override
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| vec![esi_order(7, false, "region")]),
            not_modified: false,
            metadata: EsiResponseMetadata::default(),
        })
    }

    async fn character_calendar(
        &self,
        _access_token: &str,
        _character_id: i64,
        from_event: Option<i64>,
    ) -> Result<EsiResponse<CalendarEventObservation>, EsiError> {
        self.check("character_calendar")?;
        self.calendar_calls.lock().unwrap().push(from_event);
        let events = self
            .calendar_override
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| vec![calendar_summary(1, "Fleet op")]);
        let start = from_event.map_or(0, |from| {
            events
                .iter()
                .position(|event| event.event_id == from)
                .map_or(events.len(), |index| index + 1)
        });
        Ok(EsiResponse {
            records: events.into_iter().skip(start).take(50).collect(),
            not_modified: false,
            metadata: EsiResponseMetadata::default(),
        })
    }

    async fn character_calendar_event(
        &self,
        _access_token: &str,
        _character_id: i64,
        event_id: i64,
    ) -> Result<EsiResponse<CalendarEventDetailObservation>, EsiError> {
        self.check("character_calendar_event")?;
        self.calendar_detail_calls.lock().unwrap().push(event_id);
        Ok(EsiResponse {
            records: vec![CalendarEventDetailObservation {
                event_id,
                date: calendar_date(),
                title: "ignored: the summary's title wins".to_string(),
                importance: 0,
                duration_minutes: 90,
                owner_id: Some(98_000_001),
                owner_name: Some("Test Corp".to_string()),
                owner_type: Some("corporation".to_string()),
                text: format!("<b>Details for {event_id}</b>"),
            }],
            not_modified: false,
            metadata: EsiResponseMetadata::default(),
        })
    }
}

fn calendar_date() -> DateTime<Utc> {
    "2026-10-20T18:00:00Z".parse().unwrap()
}

fn esi_order(order_id: i64, is_buy_order: bool, range: &str) -> CharacterOrderObservation {
    CharacterOrderObservation {
        order_id,
        type_id: 12_005,
        region_id: 10_000_002,
        location_id: 60_003_760,
        is_buy_order,
        is_corporation: false,
        price: "312400000.00".parse().unwrap(),
        volume_remain: 4,
        volume_total: 5,
        min_volume: 1,
        order_range: range.to_string(),
        issued_at: calendar_date(),
        duration_days: 90,
        escrow: is_buy_order.then(|| "1562000000.00".parse().unwrap()),
    }
}

fn calendar_summary(event_id: i64, title: &str) -> CalendarEventObservation {
    CalendarEventObservation {
        event_id,
        event_date: calendar_date(),
        title: title.to_string(),
        importance: 0,
        response: iskworks_esi::CalendarEventResponse::Tentative,
    }
}

fn stored_calendar_event(
    connection_id: ConnectedCharacterId,
    event_id: i64,
    title: &str,
) -> CharacterCalendarEvent {
    CharacterCalendarEvent {
        connection_id,
        event_id,
        event_date: calendar_date(),
        title: title.to_string(),
        importance: 0,
        response: CalendarEventResponse::Accepted,
        duration_minutes: 45,
        owner_id: Some(99_000_001),
        owner_name: Some("Test Alliance".to_string()),
        owner_type: Some("alliance".to_string()),
        text: "stored text".to_string(),
        observed_at: "2026-10-15T00:00:00Z".parse().unwrap(),
    }
}

#[derive(Default)]
struct FakeEsiSyncDispatcher {
    fail: Mutex<HashSet<&'static str>>,
}

impl FakeEsiSyncDispatcher {
    fn failing(endpoint: &'static str) -> Self {
        let mut fail = HashSet::new();
        fail.insert(endpoint);
        Self {
            fail: Mutex::new(fail),
        }
    }
}

fn fake_sync_run(connection: &ConnectedCharacter, kind: EsiSyncKind) -> EsiSyncRun {
    EsiSyncRun {
        id: EsiSyncRunId::new(),
        connection_id: connection.id,
        requested_kind: kind,
        status: EsiSyncStatus::Succeeded,
        phase: "Complete".to_string(),
        started_at: Utc::now(),
        completed_at: Some(Utc::now()),
        cache_expires_at: None,
        imported_count: 0,
        unchanged_count: 0,
        skipped_count: 0,
        error_count: 0,
        error_code: None,
        summary: "fixture sync".to_string(),
    }
}

#[async_trait]
impl EsiSyncDispatcher for FakeEsiSyncDispatcher {
    async fn sync_assets(
        &self,
        connection: &ConnectedCharacter,
        _token: &str,
    ) -> Result<EsiSyncRun, EsiApplicationError> {
        if self.fail.lock().unwrap().contains("assets") {
            return Err(EsiApplicationError::Configuration(
                "assets sync failed".to_string(),
            ));
        }
        Ok(fake_sync_run(connection, EsiSyncKind::Assets))
    }

    async fn sync_wallet_transactions(
        &self,
        connection: &ConnectedCharacter,
        _token: &str,
    ) -> Result<EsiSyncRun, EsiApplicationError> {
        if self.fail.lock().unwrap().contains("wallet_transactions") {
            return Err(EsiApplicationError::Configuration(
                "wallet transactions sync failed".to_string(),
            ));
        }
        Ok(fake_sync_run(connection, EsiSyncKind::WalletTransactions))
    }
}

struct FakeTokenProvider {
    connection: ConnectedCharacter,
    fail: bool,
}

#[async_trait]
impl AccessTokenProvider for FakeTokenProvider {
    async fn valid_access_token(
        &self,
        _connection_id: ConnectedCharacterId,
    ) -> Result<(ConnectedCharacter, String), EsiApplicationError> {
        if self.fail {
            Err(EsiApplicationError::Configuration(
                "refresh token is invalid".to_string(),
            ))
        } else {
            Ok((self.connection.clone(), "fake-access-token".to_string()))
        }
    }
}

fn service(
    repository: RecordingRepository,
    transport: FakeTransport,
    connection: ConnectedCharacter,
) -> (Arc<RecordingRepository>, CharacterSyncService) {
    let repository = Arc::new(repository);
    let token_provider = Arc::new(FakeTokenProvider {
        connection,
        fail: false,
    });
    let service = CharacterSyncService::new(
        repository.clone(),
        Arc::new(transport),
        token_provider,
        Arc::new(FakeEsiSyncDispatcher::default()),
    );
    (repository, service)
}

#[tokio::test]
async fn successful_sync_persists_a_summary_for_every_granted_source() {
    let (repository, service) = service(
        RecordingRepository::default(),
        FakeTransport::default(),
        connection(all_scopes()),
    );

    let outcomes = service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    assert_eq!(outcomes.len(), 10);
    assert!(outcomes
        .iter()
        .all(|(_, outcome)| *outcome == CharacterSyncOutcome::Succeeded));
    assert_eq!(repository.completed.lock().unwrap().len(), 10);
    assert!(repository.failed.lock().unwrap().is_empty());
    assert_eq!(repository.registered.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn industry_jobs_records_every_job_but_summarises_only_running_ones() {
    let (repository, service) = service(
        RecordingRepository::default(),
        FakeTransport::default(),
        connection(all_scopes()),
    );
    let connection_id = ConnectedCharacterId::new();

    service.sync_connection(connection_id, no_cancel()).await;

    let recorded = repository.recorded_jobs.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].0, connection_id);
    let ids: Vec<i64> = recorded[0].1.iter().map(|job| job.job_id).collect();
    assert_eq!(ids, vec![500_001, 500_000]);

    // Roster, calendar and ticket linking read the summary: it must keep
    // exactly what it held before completed jobs were requested.
    let completed = repository.completed.lock().unwrap();
    let (_, _, summary) = completed
        .iter()
        .find(|(_, kind, _)| *kind == CharacterSourceKind::IndustryJobs)
        .expect("jobs summary persisted");
    let summary_ids: Vec<i64> = summary["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|job| job["job_id"].as_i64().unwrap())
        .collect();
    assert_eq!(summary_ids, vec![500_001]);
    assert_eq!(summary["activeCount"], 1);
}

#[tokio::test]
async fn assets_and_wallet_transactions_are_dispatched_through_the_esi_sync_service() {
    let (repository, service) = service(
        RecordingRepository::default(),
        FakeTransport::default(),
        connection(all_scopes()),
    );

    let outcomes = service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let assets_outcome = outcomes
        .iter()
        .find(|(kind, _)| *kind == CharacterSourceKind::Assets)
        .map(|(_, outcome)| *outcome);
    assert_eq!(assets_outcome, Some(CharacterSyncOutcome::Succeeded));
    let wallet_tx_outcome = outcomes
        .iter()
        .find(|(kind, _)| *kind == CharacterSourceKind::WalletTransactions)
        .map(|(_, outcome)| *outcome);
    assert_eq!(wallet_tx_outcome, Some(CharacterSyncOutcome::Succeeded));

    let completed = repository.completed.lock().unwrap();
    let assets_summary = completed
        .iter()
        .find(|(_, kind, _)| *kind == CharacterSourceKind::Assets)
        .map(|(_, _, summary)| summary)
        .expect("assets summary persisted");
    assert_eq!(assets_summary["requestedKind"], "assets");
    assert_eq!(assets_summary["status"], "succeeded");
}

#[tokio::test]
async fn missing_asset_scope_fails_only_assets() {
    let mut scopes = all_scopes();
    scopes.retain(|scope| scope != ASSET_SCOPE);
    let (repository, service) = service(
        RecordingRepository::default(),
        FakeTransport::default(),
        connection(scopes),
    );

    let outcomes = service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let assets_outcome = outcomes
        .iter()
        .find(|(kind, _)| *kind == CharacterSourceKind::Assets)
        .map(|(_, outcome)| *outcome);
    assert_eq!(assets_outcome, Some(CharacterSyncOutcome::Failed));
    let succeeded = outcomes
        .iter()
        .filter(|(_, outcome)| *outcome == CharacterSyncOutcome::Succeeded)
        .count();
    assert_eq!(succeeded, 9);
    let failed = repository.failed.lock().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].1, CharacterSourceKind::Assets);
    assert!(failed[0].2.contains(ASSET_SCOPE));
}

#[tokio::test]
async fn an_esi_sync_dispatcher_failure_fails_only_that_source() {
    let repository = Arc::new(RecordingRepository::default());
    let token_provider = Arc::new(FakeTokenProvider {
        connection: connection(all_scopes()),
        fail: false,
    });
    let service = CharacterSyncService::new(
        repository.clone(),
        Arc::new(FakeTransport::default()),
        token_provider,
        Arc::new(FakeEsiSyncDispatcher::failing("assets")),
    );

    let outcomes = service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let assets_outcome = outcomes
        .iter()
        .find(|(kind, _)| *kind == CharacterSourceKind::Assets)
        .map(|(_, outcome)| *outcome);
    assert_eq!(assets_outcome, Some(CharacterSyncOutcome::Failed));
    let wallet_tx_outcome = outcomes
        .iter()
        .find(|(kind, _)| *kind == CharacterSourceKind::WalletTransactions)
        .map(|(_, outcome)| *outcome);
    assert_eq!(
        wallet_tx_outcome,
        Some(CharacterSyncOutcome::Succeeded),
        "an assets sync failure must not affect wallet transactions"
    );
    let failed = repository.failed.lock().unwrap();
    assert_eq!(failed.len(), 1);
    assert!(failed[0].2.contains("assets sync failed"));
}

#[tokio::test]
async fn missing_scope_fails_only_that_source_and_leaves_siblings_unaffected() {
    let mut scopes = all_scopes();
    scopes.retain(|scope| scope != INDUSTRY_JOBS_SCOPE);
    let (repository, service) = service(
        RecordingRepository::default(),
        FakeTransport::default(),
        connection(scopes),
    );

    let outcomes = service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let jobs_outcome = outcomes
        .iter()
        .find(|(kind, _)| *kind == CharacterSourceKind::IndustryJobs)
        .map(|(_, outcome)| *outcome);
    assert_eq!(jobs_outcome, Some(CharacterSyncOutcome::Failed));
    let succeeded = outcomes
        .iter()
        .filter(|(_, outcome)| *outcome == CharacterSyncOutcome::Succeeded)
        .count();
    assert_eq!(succeeded, 9);
    let failed = repository.failed.lock().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].1, CharacterSourceKind::IndustryJobs);
    assert!(failed[0].2.contains(INDUSTRY_JOBS_SCOPE));
}

#[tokio::test]
async fn planets_summary_flattens_each_colony_header_with_its_pins() {
    let (repository, service) = service(
        RecordingRepository::default(),
        FakeTransport::default(),
        connection(all_scopes()),
    );

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let completed = repository.completed.lock().unwrap();
    let (_, _, summary) = completed
        .iter()
        .find(|(_, kind, _)| *kind == CharacterSourceKind::Planets)
        .expect("planets source completed");
    let planet = &summary["planets"][0];
    assert_eq!(planet["planet_id"], 40_050_361);
    assert_eq!(planet["planet_type"], "barren");
    assert_eq!(planet["pins"][0]["contents"][0]["type_id"], 2_398);
}

#[tokio::test]
async fn missing_planets_scope_fails_only_planets() {
    let mut scopes = all_scopes();
    scopes.retain(|scope| scope != PLANETS_SCOPE);
    let (repository, service) = service(
        RecordingRepository::default(),
        FakeTransport::default(),
        connection(scopes),
    );

    let outcomes = service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let failed = repository.failed.lock().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].1, CharacterSourceKind::Planets);
    assert!(failed[0].2.starts_with("missing scope: "));
    assert_eq!(
        outcomes
            .iter()
            .filter(|(_, outcome)| *outcome == CharacterSyncOutcome::Succeeded)
            .count(),
        9
    );
}

#[tokio::test]
async fn a_failed_planet_layout_fails_the_planets_source() {
    let (repository, service) = service(
        RecordingRepository::default(),
        FakeTransport::failing("character_planet_detail", EsiError::TemporaryFailure),
        connection(all_scopes()),
    );

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let failed = repository.failed.lock().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].1, CharacterSourceKind::Planets);
}

#[tokio::test]
async fn esi_failure_on_one_source_does_not_abort_the_others() {
    let (repository, service) = service(
        RecordingRepository::default(),
        FakeTransport::failing("character_location", EsiError::TemporaryFailure),
        connection(all_scopes()),
    );

    let outcomes = service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let location_outcome = outcomes
        .iter()
        .find(|(kind, _)| *kind == CharacterSourceKind::Location)
        .map(|(_, outcome)| *outcome);
    assert_eq!(location_outcome, Some(CharacterSyncOutcome::Failed));
    let succeeded = outcomes
        .iter()
        .filter(|(_, outcome)| *outcome == CharacterSyncOutcome::Succeeded)
        .count();
    assert_eq!(succeeded, 9);
    assert_eq!(repository.completed.lock().unwrap().len(), 9);
    assert_eq!(repository.failed.lock().unwrap().len(), 1);
}

/// An ESI HTTP timeout reaches character sync as `EsiError::TemporaryFailure`
/// (see `iskworks-esi` transport tests). It must keep the existing
/// single-attempt-per-pass behaviour: the source is `Failed`, rescheduled
/// ~5 minutes out via `retry_interval()`, every later source still runs,
/// and no in-pass retry is introduced.
#[tokio::test]
async fn esi_timeout_on_one_source_fails_it_and_reschedules_in_five_minutes() {
    let before = Utc::now();
    let (repository, service) = service(
        RecordingRepository::default(),
        // Location is the 2nd of 7 stages; Skills/Wallet/IndustryJobs/
        // Assets/WalletTransactions all come after it.
        FakeTransport::failing("character_location", EsiError::TemporaryFailure),
        connection(all_scopes()),
    );

    let outcomes = service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;
    let after = Utc::now();

    assert_eq!(
        outcomes
            .iter()
            .find(|(kind, _)| *kind == CharacterSourceKind::Location)
            .map(|(_, outcome)| *outcome),
        Some(CharacterSyncOutcome::Failed)
    );
    // Every stage after the timed-out one still ran.
    assert_eq!(
        outcomes
            .iter()
            .filter(|(_, outcome)| *outcome == CharacterSyncOutcome::Succeeded)
            .count(),
        9
    );
    // Exactly one attempt was made for Location -- no in-pass retry.
    let location_failures = repository
        .failed
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, kind, _)| *kind == CharacterSourceKind::Location)
        .count();
    assert_eq!(location_failures, 1);
    // Rescheduled ~now + 5 minutes.
    let (_, _, next_refresh_at) = repository
        .failed_next_refresh_ats
        .lock()
        .unwrap()
        .iter()
        .find(|(_, kind, _)| *kind == CharacterSourceKind::Location)
        .copied()
        .expect("a failed source records its next_refresh_at");
    assert!(
        next_refresh_at >= before + Duration::minutes(5)
            && next_refresh_at <= after + Duration::minutes(5),
        "expected next_refresh_at ~= now + 5 min, got {next_refresh_at}"
    );
}

#[tokio::test]
async fn token_refresh_failure_skips_every_source_without_writing_any_state() {
    let repository = Arc::new(RecordingRepository::default());
    let token_provider = Arc::new(FakeTokenProvider {
        connection: connection(all_scopes()),
        fail: true,
    });
    let service = CharacterSyncService::new(
        repository.clone(),
        Arc::new(FakeTransport::default()),
        token_provider,
        Arc::new(FakeEsiSyncDispatcher::default()),
    );

    let connection_id = ConnectedCharacterId::new();
    let before = Utc::now();
    let outcomes = service.sync_connection(connection_id, no_cancel()).await;
    let after = Utc::now();

    assert_eq!(outcomes.len(), CharacterSourceKind::all().len());
    assert!(outcomes
        .iter()
        .all(|(_, outcome)| *outcome == CharacterSyncOutcome::Skipped));
    assert!(repository.registered.lock().unwrap().is_empty());
    assert!(repository.completed.lock().unwrap().is_empty());
    assert!(repository.failed.lock().unwrap().is_empty());
    // A transient token failure backs the whole connection off, so its
    // sources don't sit at the head of the due queue every pass.
    let deferred = repository.deferred.lock().unwrap().clone();
    assert_eq!(deferred.len(), 1);
    let (deferred_id, until) = deferred[0];
    assert_eq!(deferred_id, connection_id);
    assert!(
        until >= before + TOKEN_FAILURE_BACKOFF && until <= after + TOKEN_FAILURE_BACKOFF,
        "expected deferral ~= now + backoff, got {until}"
    );
}

/// A refresh EVE rejected for good already flagged the connection
/// (`EsiApplicationService::refresh`), which takes it out of the due
/// queue -- no deferral, so a reconnect syncs immediately.
#[tokio::test]
async fn rejected_token_refresh_does_not_defer_sources() {
    struct RejectingTokenProvider;

    #[async_trait]
    impl AccessTokenProvider for RejectingTokenProvider {
        async fn valid_access_token(
            &self,
            _connection_id: ConnectedCharacterId,
        ) -> Result<(ConnectedCharacter, String), EsiApplicationError> {
            Err(EsiError::AuthorizationRequired.into())
        }
    }

    let repository = Arc::new(RecordingRepository::default());
    let service = CharacterSyncService::new(
        repository.clone(),
        Arc::new(FakeTransport::default()),
        Arc::new(RejectingTokenProvider),
        Arc::new(FakeEsiSyncDispatcher::default()),
    );

    let outcomes = service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    assert!(outcomes
        .iter()
        .all(|(_, outcome)| *outcome == CharacterSyncOutcome::Skipped));
    assert!(repository.deferred.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_source_not_yet_due_is_skipped_but_others_still_sync() {
    let mut repository = RecordingRepository::default();
    repository
        .unclaimable
        .get_mut()
        .unwrap()
        .insert(CharacterSourceKind::Wallet);
    let (repository, service) = service(
        repository,
        FakeTransport::default(),
        connection(all_scopes()),
    );

    let outcomes = service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let wallet_outcome = outcomes
        .iter()
        .find(|(kind, _)| *kind == CharacterSourceKind::Wallet)
        .map(|(_, outcome)| *outcome);
    assert_eq!(wallet_outcome, Some(CharacterSyncOutcome::Skipped));
    let succeeded = outcomes
        .iter()
        .filter(|(_, outcome)| *outcome == CharacterSyncOutcome::Succeeded)
        .count();
    assert_eq!(succeeded, 9);
    assert_eq!(repository.completed.lock().unwrap().len(), 9);
    assert!(repository
        .failed
        .lock()
        .unwrap()
        .iter()
        .all(|(_, kind, _)| *kind != CharacterSourceKind::Wallet));
}

#[tokio::test]
async fn skills_summary_includes_the_training_queue_when_the_scope_is_granted() {
    let (repository, service) = service(
        RecordingRepository::default(),
        FakeTransport::default(),
        connection(all_scopes()),
    );

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let completed = repository.completed.lock().unwrap();
    let (_, _, summary) = completed
        .iter()
        .find(|(_, kind, _)| *kind == CharacterSourceKind::Skills)
        .expect("skills should have completed");
    assert!(summary.get("skillQueue").is_some());
}

#[tokio::test]
async fn skills_summary_omits_the_training_queue_without_the_scope() {
    let mut scopes = all_scopes();
    scopes.retain(|scope| scope != SKILL_QUEUE_SCOPE);
    let (repository, service) = service(
        RecordingRepository::default(),
        FakeTransport::default(),
        connection(scopes),
    );

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let completed = repository.completed.lock().unwrap();
    let (_, _, summary) = completed
        .iter()
        .find(|(_, kind, _)| *kind == CharacterSourceKind::Skills)
        .expect("skills should have completed even without the skill-queue scope");
    assert!(summary.get("skillQueue").is_none());
}

#[tokio::test]
async fn skills_summary_includes_the_character_attributes() {
    let (repository, service) = service(
        RecordingRepository::default(),
        FakeTransport::default(),
        connection(all_scopes()),
    );

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let completed = repository.completed.lock().unwrap();
    let (_, _, summary) = completed
        .iter()
        .find(|(_, kind, _)| *kind == CharacterSourceKind::Skills)
        .expect("skills should have completed");
    let attributes = summary.get("attributes").expect("attributes stored");
    assert_eq!(attributes["memory"], 27);
    assert_eq!(attributes["perception"], 27);
    assert_eq!(attributes["willpower"], 17);
    assert_eq!(attributes["bonus_remaps"], 1);
}

#[tokio::test]
async fn skills_still_complete_without_attributes_when_that_call_fails() {
    let (repository, service) = service(
        RecordingRepository::default(),
        FakeTransport::failing("character_attributes", EsiError::TemporaryFailure),
        connection(all_scopes()),
    );

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let completed = repository.completed.lock().unwrap();
    let (_, _, summary) = completed
        .iter()
        .find(|(_, kind, _)| *kind == CharacterSourceKind::Skills)
        .expect("skills should complete even when attributes fail");
    assert!(summary.get("attributes").is_none());
    assert!(summary.get("skillQueue").is_some());
}

fn next_refresh_at_for(
    repository: &RecordingRepository,
    kind: CharacterSourceKind,
) -> DateTime<Utc> {
    repository
        .completed_next_refresh_ats
        .lock()
        .unwrap()
        .iter()
        .find(|(_, k, _)| *k == kind)
        .map(|(_, _, next_refresh_at)| *next_refresh_at)
        .expect("skills source should have completed")
}

#[tokio::test]
async fn skills_sync_schedules_next_refresh_near_the_active_entrys_finish_date() {
    let t0 = Utc::now();
    let transport = FakeTransport::with_skill_queue(vec![CharacterSkillQueueEntry {
        skill_id: 3327,
        finished_level: 5,
        queue_position: 0,
        start_date: Some(t0 - Duration::hours(1)),
        finish_date: Some(t0 + Duration::seconds(30)),
        training_start_sp: None,
        level_start_sp: None,
        level_end_sp: None,
    }]);
    let (repository, service) = service(
        RecordingRepository::default(),
        transport,
        connection(all_scopes()),
    );

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let next_refresh_at = next_refresh_at_for(&repository, CharacterSourceKind::Skills);
    // finish_date (t0+30s) + 60s buffer = ~t0+90s, well short of the
    // flat 60-minute ceiling.
    assert!(
        next_refresh_at > t0 + Duration::seconds(80)
            && next_refresh_at < t0 + Duration::seconds(100),
        "expected next_refresh_at near t0+90s, got {next_refresh_at} (t0={t0})"
    );
    assert!(next_refresh_at < t0 + Duration::minutes(30));
}

#[tokio::test]
async fn skills_sync_falls_back_to_flat_interval_when_paused() {
    let t0 = Utc::now();
    let transport = FakeTransport::with_skill_queue(vec![CharacterSkillQueueEntry {
        skill_id: 3327,
        finished_level: 5,
        queue_position: 0,
        start_date: None,
        finish_date: None,
        training_start_sp: None,
        level_start_sp: None,
        level_end_sp: None,
    }]);
    let (repository, service) = service(
        RecordingRepository::default(),
        transport,
        connection(all_scopes()),
    );

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let next_refresh_at = next_refresh_at_for(&repository, CharacterSourceKind::Skills);
    assert!(
        next_refresh_at > t0 + Duration::minutes(55)
            && next_refresh_at < t0 + Duration::minutes(65),
        "expected next_refresh_at near the flat 60-minute ceiling, got {next_refresh_at} (t0={t0})"
    );
}

#[tokio::test]
async fn skills_sync_schedules_immediate_refresh_when_cached_queue_is_already_expired() {
    // Every cached entry's finish_date is already in the past by the
    // time this very sync completed -- refinement #5: this must not
    // silently fall back to the flat 60-minute ceiling, since we
    // already know the data is stale.
    let t0 = Utc::now();
    let transport = FakeTransport::with_skill_queue(vec![CharacterSkillQueueEntry {
        skill_id: 3327,
        finished_level: 5,
        queue_position: 0,
        start_date: Some(t0 - Duration::hours(2)),
        finish_date: Some(t0 - Duration::hours(1)),
        training_start_sp: None,
        level_start_sp: None,
        level_end_sp: None,
    }]);
    let (repository, service) = service(
        RecordingRepository::default(),
        transport,
        connection(all_scopes()),
    );

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let next_refresh_at = next_refresh_at_for(&repository, CharacterSourceKind::Skills);
    assert!(
        next_refresh_at <= t0 + Duration::seconds(5),
        "expected an immediately-due refresh, got {next_refresh_at} (t0={t0})"
    );
    assert!(next_refresh_at < t0 + Duration::minutes(30));
}

#[tokio::test]
async fn a_successful_sync_resolves_and_caches_the_corp_and_system_names() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = Arc::new(FakeTransport::default());
    let token_provider = Arc::new(FakeTokenProvider {
        connection: connection(all_scopes()),
        fail: false,
    });
    let service = CharacterSyncService::new(
        repository.clone(),
        Arc::clone(&transport) as Arc<dyn EsiTransport>,
        token_provider,
        Arc::new(FakeEsiSyncDispatcher::default()),
    );

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let cached = repository.cached_names.lock().unwrap();
    assert_eq!(
        cached.get(&98_000_001).map(String::as_str),
        Some("Perimeter Industrial Holdings")
    );
    assert_eq!(cached.get(&30_000_142).map(String::as_str), Some("Jita"));
    // One bulk call, not one per ID.
    assert_eq!(transport.universe_names_calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn already_cached_names_do_not_trigger_a_universe_names_call() {
    let repository = RecordingRepository::default();
    repository
        .cached_names
        .lock()
        .unwrap()
        .insert(98_000_001, "Perimeter Industrial Holdings".to_string());
    repository
        .cached_names
        .lock()
        .unwrap()
        .insert(30_000_142, "Jita".to_string());
    let repository = Arc::new(repository);
    let transport = Arc::new(FakeTransport::default());
    let token_provider = Arc::new(FakeTokenProvider {
        connection: connection(all_scopes()),
        fail: false,
    });
    let service = CharacterSyncService::new(
        repository,
        Arc::clone(&transport) as Arc<dyn EsiTransport>,
        token_provider,
        Arc::new(FakeEsiSyncDispatcher::default()),
    );

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    assert!(transport.universe_names_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_failed_name_resolution_does_not_affect_the_sync_outcome() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = FakeTransport::failing("universe_names", EsiError::PermanentFailure);
    let token_provider = Arc::new(FakeTokenProvider {
        connection: connection(all_scopes()),
        fail: false,
    });
    let service = CharacterSyncService::new(
        repository.clone(),
        Arc::new(transport),
        token_provider,
        Arc::new(FakeEsiSyncDispatcher::default()),
    );

    let outcomes = service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    assert!(outcomes
        .iter()
        .all(|(_, outcome)| *outcome == CharacterSyncOutcome::Succeeded));
    assert!(repository.cached_names.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_corporation_id_already_persisted_before_this_pass_still_gets_resolved() {
    let connection_id = ConnectedCharacterId::new();
    let repository = RecordingRepository::default();
    repository.unclaimable.lock().unwrap().extend([
        CharacterSourceKind::CharacterInfo,
        CharacterSourceKind::Location,
    ]);
    repository.persisted_sources.lock().unwrap().insert(
        connection_id,
        vec![CharacterSourceSyncState {
            connection_id,
            source_kind: CharacterSourceKind::CharacterInfo,
            refresh_state: iskworks_core::MarketRefreshState::Current,
            summary: Some(serde_json::json!({ "corporation_id": 98_000_001 })),
            observed_at: None,
            last_attempted_at: None,
            next_refresh_at: None,
            last_error: None,
        }],
    );
    let repository = Arc::new(repository);
    let transport = Arc::new(FakeTransport::default());
    let token_provider = Arc::new(FakeTokenProvider {
        connection: connection(all_scopes()),
        fail: false,
    });
    let service = CharacterSyncService::new(
        repository.clone(),
        Arc::clone(&transport) as Arc<dyn EsiTransport>,
        token_provider,
        Arc::new(FakeEsiSyncDispatcher::default()),
    );

    let outcomes = service.sync_connection(connection_id, no_cancel()).await;

    let (_, character_info_outcome) = outcomes
        .iter()
        .find(|(kind, _)| *kind == CharacterSourceKind::CharacterInfo)
        .unwrap();
    assert_eq!(*character_info_outcome, CharacterSyncOutcome::Skipped);
    assert_eq!(
        repository
            .cached_names
            .lock()
            .unwrap()
            .get(&98_000_001)
            .map(String::as_str),
        Some("Perimeter Industrial Holdings")
    );
}

/// Stage level: a token already cancelled when `sync_connection`
/// starts reports every source as `Skipped` and touches nothing -- no
/// `begin`/`complete`/`fail` for any stage, no name resolution.
/// Shutdown is control flow, never a per-source failure.
#[tokio::test]
async fn cancelled_token_skips_every_source_without_persisting() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = Arc::new(FakeTransport::default());
    let token_provider = Arc::new(FakeTokenProvider {
        connection: connection(all_scopes()),
        fail: false,
    });
    let service = CharacterSyncService::new(
        repository.clone(),
        Arc::clone(&transport) as Arc<dyn EsiTransport>,
        token_provider,
        Arc::new(FakeEsiSyncDispatcher::default()),
    );

    let cancel = CancellationToken::new();
    cancel.cancel();
    let outcomes = service
        .sync_connection(ConnectedCharacterId::new(), &cancel)
        .await;

    assert_eq!(
        outcomes.len(),
        CharacterSourceKind::all().len(),
        "every source kind is still accounted for"
    );
    assert!(
        outcomes
            .iter()
            .all(|(_, outcome)| *outcome == CharacterSyncOutcome::Skipped),
        "no source runs under a cancelled token: {outcomes:?}"
    );
    assert!(
        repository.completed.lock().unwrap().is_empty(),
        "nothing completed"
    );
    assert!(
        repository.failed.lock().unwrap().is_empty(),
        "shutdown must not persist a source failure"
    );
    assert!(
        repository.entity_names_calls.lock().unwrap().is_empty(),
        "the name-resolution tail is skipped too"
    );
}

#[test]
fn a_failed_source_waits_at_least_as_long_as_esi_asked() {
    let now = Utc::now();
    let rate_limited = SourceFailure::from(EsiError::RateLimited {
        retry_after_seconds: Some(1_800),
    });
    assert_eq!(rate_limited.retry_floor(now), now + Duration::minutes(30));
    let error_limited =
        SourceFailure::from(EsiApplicationError::Protocol(EsiError::EsiErrorLimit {
            reset_seconds: Some(10),
        }));
    assert_eq!(
        error_limited.retry_floor(now),
        now + retry_interval(),
        "never sooner than the first backoff step"
    );
    let forbidden = SourceFailure::from(EsiError::AccessDenied);
    assert_eq!(forbidden.retry_floor(now), now + retry_interval());
}

#[tokio::test]
async fn every_source_refresh_is_recorded_by_kind_and_result() {
    use crate::sync_metrics::test_support::Recorded;
    use metrics_util::debugging::DebuggingRecorder;

    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let scopes = all_scopes()
        .into_iter()
        .filter(|scope| scope != LOCATION_SCOPE)
        .collect();
    let (_, service) = service(
        RecordingRepository::default(),
        FakeTransport::default(),
        connection(scopes),
    );

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let recorded = Recorded::take(&snapshotter);
    let runs = |kind: &str, result: &str| {
        recorded.counter(
            "iskworks_esi_sync_runs_total",
            &[("kind", kind), ("result", result)],
        )
    };
    assert_eq!(runs("location", "failed"), 1);
    assert_eq!(runs("skills", "success"), 1);
    assert_eq!(runs("assets", "success"), 1);
    assert_eq!(recorded.counter("iskworks_esi_sync_runs_total", &[]), 10);
    assert_eq!(
        recorded.histogram_count("iskworks_esi_sync_duration_seconds", &[]),
        10
    );
    let last_success = "iskworks_esi_sync_last_success_timestamp_seconds";
    assert!(recorded.has(last_success, &[("kind", "skills")]));
    assert!(!recorded.has(last_success, &[("kind", "location")]));
}

#[derive(Default)]
struct RecordingJobsSynced {
    workspaces: Mutex<Vec<WorkspaceId>>,
}

#[async_trait]
impl crate::IndustryJobsSynced for RecordingJobsSynced {
    async fn industry_jobs_synced(&self, workspace_id: WorkspaceId) {
        self.workspaces.lock().unwrap().push(workspace_id);
    }
}

#[tokio::test]
async fn a_successful_jobs_refresh_tells_the_hook_for_the_characters_workspace() {
    let character = connection(all_scopes());
    let workspace_id = character.workspace_id;
    let hook = Arc::new(RecordingJobsSynced::default());
    let (_, service) = service(
        RecordingRepository::default(),
        FakeTransport::default(),
        character,
    );
    let service = service.with_industry_jobs_synced(hook.clone());

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    assert_eq!(*hook.workspaces.lock().unwrap(), vec![workspace_id]);
}

#[tokio::test]
async fn a_failed_jobs_refresh_does_not_tell_the_hook() {
    let hook = Arc::new(RecordingJobsSynced::default());
    let (_, service) = service(
        RecordingRepository::default(),
        FakeTransport::failing("character_industry_jobs", EsiError::TemporaryFailure),
        connection(all_scopes()),
    );
    let service = service.with_industry_jobs_synced(hook.clone());

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    assert!(hook.workspaces.lock().unwrap().is_empty());
}

#[tokio::test]
async fn calendar_sync_pages_past_fifty_events_and_stores_every_one() {
    let events: Vec<_> = (1..=120)
        .map(|id| calendar_summary(id, &format!("Op {id}")))
        .collect();
    let transport = Arc::new(FakeTransport::with_calendar(events));
    let repository = Arc::new(RecordingRepository::default());
    let connection = connection(all_scopes());
    let connection_id = connection.id;
    let service = CharacterSyncService::new(
        repository.clone(),
        transport.clone(),
        Arc::new(FakeTokenProvider {
            connection,
            fail: false,
        }),
        Arc::new(FakeEsiSyncDispatcher::default()),
    );

    service.sync_connection(connection_id, no_cancel()).await;

    assert_eq!(
        *transport.calendar_calls.lock().unwrap(),
        vec![None, Some(50), Some(100)]
    );
    let stored = repository.stored_calendar.lock().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].0, connection_id);
    assert_eq!(stored[0].1.len(), 120);
    assert_eq!(transport.calendar_detail_calls.lock().unwrap().len(), 120);
    let completed = repository.completed.lock().unwrap();
    let (_, _, summary) = completed
        .iter()
        .find(|(_, kind, _)| *kind == CharacterSourceKind::Calendar)
        .unwrap();
    assert_eq!(summary["eventCount"], 120);
    assert_eq!(summary["pages"], 3);
}

#[tokio::test]
async fn calendar_sync_takes_summary_fields_and_details_from_esi() {
    let transport = Arc::new(FakeTransport::default());
    let repository = Arc::new(RecordingRepository::default());
    let connection = connection(all_scopes());
    let connection_id = connection.id;
    let service = CharacterSyncService::new(
        repository.clone(),
        transport.clone(),
        Arc::new(FakeTokenProvider {
            connection,
            fail: false,
        }),
        Arc::new(FakeEsiSyncDispatcher::default()),
    );

    service.sync_connection(connection_id, no_cancel()).await;

    let stored = repository.stored_calendar.lock().unwrap();
    let event = &stored[0].1[0];
    assert_eq!(event.event_id, 1);
    assert_eq!(event.title, "Fleet op");
    assert_eq!(event.response, CalendarEventResponse::Tentative);
    assert_eq!(event.duration_minutes, 90);
    assert_eq!(event.owner_name.as_deref(), Some("Test Corp"));
    assert_eq!(event.text, "<b>Details for 1</b>");
}

#[tokio::test]
async fn calendar_sync_reuses_current_details_and_refetches_changed_events() {
    let transport = Arc::new(FakeTransport::with_calendar(vec![
        calendar_summary(1, "Shared op"),
        calendar_summary(2, "Renamed op"),
        calendar_summary(3, "New op"),
    ]));
    let repository = Arc::new(RecordingRepository::default());
    let connection = connection(all_scopes());
    let connection_id = connection.id;
    // Another character in the workspace already stored event 1 as it is
    // now, and event 2 under its old title.
    let corpmate = ConnectedCharacterId::new();
    repository.calendar_copies.lock().unwrap().extend([
        stored_calendar_event(corpmate, 1, "Shared op"),
        stored_calendar_event(corpmate, 2, "Old title"),
    ]);
    let service = CharacterSyncService::new(
        repository.clone(),
        transport.clone(),
        Arc::new(FakeTokenProvider {
            connection,
            fail: false,
        }),
        Arc::new(FakeEsiSyncDispatcher::default()),
    );

    service.sync_connection(connection_id, no_cancel()).await;

    assert_eq!(*transport.calendar_detail_calls.lock().unwrap(), vec![2, 3]);
    let stored = repository.stored_calendar.lock().unwrap();
    let shared = &stored[0].1[0];
    assert_eq!(shared.connection_id, connection_id);
    assert_eq!(shared.text, "stored text");
    assert_eq!(shared.owner_name.as_deref(), Some("Test Alliance"));
    // The response is this character's own, from the summary.
    assert_eq!(shared.response, CalendarEventResponse::Tentative);
    assert_eq!(stored[0].1[1].title, "Renamed op");
    assert_eq!(stored[0].1[1].text, "<b>Details for 2</b>");
}

#[tokio::test]
async fn missing_calendar_scope_fails_only_calendar_and_stores_nothing() {
    let mut scopes = all_scopes();
    scopes.retain(|scope| scope != CALENDAR_SCOPE);
    let (repository, service) = service(
        RecordingRepository::default(),
        FakeTransport::default(),
        connection(scopes),
    );

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let failed = repository.failed.lock().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].1, CharacterSourceKind::Calendar);
    assert_eq!(
        failed[0].2,
        "missing scope: esi-calendar.read_calendar_events.v1"
    );
    assert!(repository.stored_calendar.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_failed_event_detail_fails_the_calendar_source_without_storing() {
    let (repository, service) = service(
        RecordingRepository::default(),
        FakeTransport::failing("character_calendar_event", EsiError::TemporaryFailure),
        connection(all_scopes()),
    );

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let failed = repository.failed.lock().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].1, CharacterSourceKind::Calendar);
    assert!(repository.stored_calendar.lock().unwrap().is_empty());
}

#[tokio::test]
async fn market_orders_sync_stores_every_order_and_counts_open_buys_and_sells() {
    let transport = FakeTransport {
        market_orders_override: Mutex::new(Some(vec![
            esi_order(1, false, "region"),
            esi_order(2, true, "5"),
            CharacterOrderObservation {
                is_corporation: true,
                ..esi_order(3, true, "station")
            },
        ])),
        ..FakeTransport::default()
    };
    let connection = connection(all_scopes());
    let connection_id = connection.id;
    let (repository, service) = service(RecordingRepository::default(), transport, connection);

    service.sync_connection(connection_id, no_cancel()).await;

    let stored = repository.stored_market_orders.lock().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].0, connection_id);
    let orders = &stored[0].1;
    assert_eq!(orders.len(), 3);
    assert_eq!(orders[0].side, MarketOrderSide::Sell);
    assert_eq!(orders[0].range, OrderRange::Region);
    assert_eq!(orders[0].volume_remain, 4);
    assert_eq!(orders[0].escrow, None);
    assert_eq!(orders[1].side, MarketOrderSide::Buy);
    assert_eq!(orders[1].range, OrderRange::Jumps(5));
    assert_eq!(
        orders[1].escrow,
        Some("1562000000.00".parse::<rust_decimal::Decimal>().unwrap())
    );
    assert!(orders[2].is_corporation);
    let completed = repository.completed.lock().unwrap();
    let (_, _, summary) = completed
        .iter()
        .find(|(_, kind, _)| *kind == CharacterSourceKind::MarketOrders)
        .unwrap();
    // Corporation orders are stored but not counted as the character's own.
    assert_eq!(summary["openSellOrders"], 1);
    assert_eq!(summary["openBuyOrders"], 1);
    assert_eq!(summary["corporationOrders"], 1);
}

#[tokio::test]
async fn missing_market_orders_scope_fails_only_market_orders_and_stores_nothing() {
    let mut scopes = all_scopes();
    scopes.retain(|scope| scope != MARKET_ORDERS_SCOPE);
    let (repository, service) = service(
        RecordingRepository::default(),
        FakeTransport::default(),
        connection(scopes),
    );

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let failed = repository.failed.lock().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].1, CharacterSourceKind::MarketOrders);
    assert_eq!(
        failed[0].2,
        "missing scope: esi-markets.read_character_orders.v1"
    );
    assert!(repository.stored_market_orders.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_failed_orders_fetch_keeps_the_stored_orders() {
    let (repository, service) = service(
        RecordingRepository::default(),
        FakeTransport::failing("character_orders", EsiError::TemporaryFailure),
        connection(all_scopes()),
    );

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let failed = repository.failed.lock().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].1, CharacterSourceKind::MarketOrders);
    // Storing an empty set would close every open order.
    assert!(repository.stored_market_orders.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_unknown_order_range_fails_the_source_without_storing() {
    let transport = FakeTransport {
        market_orders_override: Mutex::new(Some(vec![esi_order(1, true, "constellation")])),
        ..FakeTransport::default()
    };
    let (repository, service) = service(
        RecordingRepository::default(),
        transport,
        connection(all_scopes()),
    );

    service
        .sync_connection(ConnectedCharacterId::new(), no_cancel())
        .await;

    let failed = repository.failed.lock().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].1, CharacterSourceKind::MarketOrders);
    assert!(repository.stored_market_orders.lock().unwrap().is_empty());
}
