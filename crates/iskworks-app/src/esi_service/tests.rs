use super::authorization::{authorization_scopes, require_requested_scopes};
use super::sync::{lookup_entity_names, NAME_LOOKUP_ERROR_BUDGET};
use super::*;

#[test]
fn structure_candidates_exclude_nested_asset_containers() {
    let structure_id = 1_049_854_533_347;
    let container_id = 1_100_000_000_001;
    let records = vec![
        asset(container_id, 60_003_760, "station"),
        asset(1_100_000_000_002, container_id, "item"),
        asset(1_100_000_000_003, structure_id, "item"),
    ];

    assert_eq!(
        asset_structure_candidates(&records),
        BTreeSet::from([structure_id])
    );
}

fn asset(item_id: i64, location_id: i64, location_type: &str) -> AssetObservation {
    AssetObservation {
        item_id,
        type_id: 34,
        quantity: 1,
        location_id,
        location_type: location_type.to_string(),
        location_flag: "Hangar".to_string(),
        is_singleton: false,
        is_blueprint_copy: None,
        raw: json!({}),
    }
}

#[tokio::test]
async fn fixture_transport_serves_planets_with_details() {
    let transport = FixtureEsiTransport;
    let planets = transport
        .character_planets("fixture-token", 1)
        .await
        .unwrap();
    assert_eq!(planets.records.len(), 3);
    for planet in planets.records {
        let detail = transport
            .character_planet_detail("fixture-token", 1, planet.planet_id)
            .await
            .unwrap();
        assert!(!detail.records[0].pins.is_empty());
    }
    assert!(fixture_identity().scopes.contains(PLANETS_SCOPE));
}

#[test]
fn planets_scope_is_requested_but_not_required() {
    assert!(authorization_scopes().contains(&PLANETS_SCOPE.to_string()));
    let identity = Identity {
        character_id: 1,
        character_name: "No PI".to_string(),
        scopes: REQUESTED_SCOPES.iter().map(ToString::to_string).collect(),
    };
    assert!(require_requested_scopes(&identity).is_ok());
}

#[tokio::test]
async fn fixture_transport_returns_a_character_source_for_every_new_scope() {
    let transport = FixtureEsiTransport;
    let character_id = fixture_identity().character_id;

    let info = transport.character_public_info(character_id).await.unwrap();
    assert_eq!(info.records.len(), 1);
    assert_eq!(info.records[0].character_id, character_id);

    let location = transport
        .character_location("fixture-token", character_id)
        .await
        .unwrap();
    assert_eq!(location.records.len(), 1);
    assert!(location.records[0].solar_system_id > 0);

    let skills = transport
        .character_skills("fixture-token", character_id)
        .await
        .unwrap();
    assert_eq!(skills.records.len(), 1);
    assert!(skills.records[0].total_sp > 0);
    assert!(!skills.records[0].skills.is_empty());

    let queue = transport
        .character_skill_queue("fixture-token", character_id)
        .await
        .unwrap();
    assert_eq!(queue.records.len(), 1);
    assert!(queue.records[0].finish_date > queue.records[0].start_date);

    let jobs = transport
        .character_industry_jobs("fixture-token", character_id)
        .await
        .unwrap();
    // The fixture returns one job per activity family (manufacturing,
    // reaction, research) so the Industry tab has something to render in
    // ISKWORKS_ESI_MOCK mode.
    assert_eq!(jobs.records.len(), 3);
    assert!(jobs
        .records
        .iter()
        .all(|job| job.end_date > job.start_date && job.runs >= 1));
    assert!(jobs.records.iter().any(|job| job.activity_id == 1));
    assert!(jobs.records.iter().any(|job| job.activity_id == 9));
}

/// A fixture transport whose `refresh` maps a known refresh-token
/// plaintext to a fixed character_id (so `access-{character_id}` access
/// tokens are predictable), and whose `structure_market_orders` grants
/// or denies access per character_id -- controllable per test, plus a
/// call log so tests can assert exactly which access tokens were
/// actually tried (not just the final outcome).
struct MarketAccessFakeTransport {
    tokens: std::collections::BTreeMap<&'static str, i64>,
    granted: std::collections::BTreeSet<i64>,
    market_calls: std::sync::Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl EsiTransport for MarketAccessFakeTransport {
    async fn exchange_code(
        &self,
        _code: &str,
        _verifier: &PkceVerifier,
    ) -> Result<AuthenticatedToken, EsiError> {
        unimplemented!("test never exchanges an OAuth code")
    }

    async fn refresh(&self, refresh_token: &str) -> Result<RefreshedToken, EsiError> {
        let character_id = *self
            .tokens
            .get(refresh_token)
            .expect("unknown fixture refresh token");
        Ok(RefreshedToken {
            access_token: format!("access-{character_id}"),
            rotated_refresh_token: None,
            expires_at: Utc::now() + Duration::minutes(20),
            identity: Identity {
                character_id,
                character_name: format!("Character {character_id}"),
                scopes: BTreeSet::from([
                    ASSET_SCOPE.to_string(),
                    BLUEPRINT_SCOPE.to_string(),
                    WALLET_SCOPE.to_string(),
                ]),
            },
        })
    }

    async fn assets(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<AssetObservation>, EsiError> {
        unimplemented!("test never reads ESI resources")
    }

    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        _from_id: Option<i64>,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<WalletTransactionObservation>, EsiError> {
        unimplemented!("test never reads ESI resources")
    }

    async fn structure(
        &self,
        _access_token: &str,
        _structure_id: i64,
    ) -> Result<StructureInformation, EsiError> {
        unimplemented!("test never reads ESI resources")
    }

    async fn industry_systems(
        &self,
    ) -> Result<EsiResponse<iskworks_esi::IndustrySystemCostIndex>, EsiError> {
        unimplemented!("test never reads ESI resources")
    }

    async fn structure_market_orders(
        &self,
        access_token: &str,
        _structure_id: i64,
        _solar_system_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<MarketOrderObservation>, EsiError> {
        self.market_calls
            .lock()
            .unwrap()
            .push(access_token.to_string());
        let character_id: i64 = access_token
            .strip_prefix("access-")
            .and_then(|value| value.parse().ok())
            .expect("fixture access token");
        if self.granted.contains(&character_id) {
            Ok(EsiResponse {
                records: Vec::new(),
                not_modified: false,
                metadata: EsiResponseMetadata {
                    pages: Some(1),
                    ..Default::default()
                },
            })
        } else {
            Err(EsiError::AccessDenied)
        }
    }
}

async fn market_access_fixture(
    pool: &sqlx::PgPool,
) -> (WorkspaceId, Arc<iskworks_storage::PgEsiRepository>) {
    let workspace_repository = Arc::new(iskworks_storage::PgWorkspaceRepository::new(pool.clone()));
    let workspace_service = iskworks_core::WorkspaceService::new(workspace_repository);
    let workspace_state = workspace_service
        .create_workspace(iskworks_core::CreateWorkspaceCommand {
            name: "Market Access Test".to_string(),
        })
        .await
        .unwrap();
    let workspace = workspace_state.workspace.unwrap();
    let esi_repository = Arc::new(iskworks_storage::PgEsiRepository::new(pool.clone()));
    (workspace.id, esi_repository)
}

/// Inserts a connected character directly (bypassing the OAuth dance
/// entirely, matching this repo's convention for connection fixtures)
/// with a distinguishable plaintext refresh token and the given granted
/// scopes, returning its `ConnectedCharacterId`.
async fn market_access_connection(
    pool: &sqlx::PgPool,
    workspace_id: WorkspaceId,
    refresh_token_plaintext: &str,
    character_id: i64,
    market_scope_granted: bool,
) -> ConnectedCharacterId {
    let owner_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM owners WHERE workspace_id=$1")
        .bind(workspace_id.0)
        .fetch_one(pool)
        .await
        .unwrap();
    let cipher = SecretCipher::for_tests();
    let repository = iskworks_storage::PgEsiRepository::new(pool.clone());
    let mut scopes = vec![ASSET_SCOPE.to_string(), WALLET_SCOPE.to_string()];
    if market_scope_granted {
        scopes.push(MARKET_STRUCTURE_SCOPE.to_string());
    }
    let pending = PendingAuthorization {
        workspace_id,
        owner_id: OwnerId(owner_id),
        verifier: cipher.encrypt("unused").unwrap(),
        requested_scopes: vec![],
        return_path: "/settings/eve".to_string(),
    };
    let connection = repository
        .complete_connection(
            &pending,
            character_id,
            &format!("Character {character_id}"),
            &scopes,
            Utc::now() + Duration::minutes(20),
            cipher.encrypt(refresh_token_plaintext).unwrap(),
        )
        .await
        .unwrap();
    connection.id
}

/// Answers `structure` lookups -- `GRANTED_STRUCTURE` resolves, anything
/// else is 403 -- and records every structure id it was asked about.
/// Never refreshes: these tests must not need a token.
struct StructureLookupTransport {
    lookups: std::sync::Mutex<Vec<i64>>,
}

const GRANTED_STRUCTURE: i64 = 1_050_000_000_001;
const DENIED_STRUCTURE: i64 = 1_050_000_000_002;

#[async_trait::async_trait]
impl EsiTransport for StructureLookupTransport {
    async fn exchange_code(
        &self,
        _code: &str,
        _verifier: &PkceVerifier,
    ) -> Result<AuthenticatedToken, EsiError> {
        unimplemented!("test never exchanges an OAuth code")
    }

    async fn refresh(&self, _refresh_token: &str) -> Result<RefreshedToken, EsiError> {
        panic!("nothing left to look up, so no token is needed")
    }

    async fn assets(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<AssetObservation>, EsiError> {
        unimplemented!("test never syncs assets")
    }

    async fn blueprints(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
    ) -> Result<EsiResponse<BlueprintAssetObservation>, EsiError> {
        unimplemented!("test never syncs blueprints")
    }

    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        _from_id: Option<i64>,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<WalletTransactionObservation>, EsiError> {
        unimplemented!("test never syncs wallets")
    }

    async fn structure(
        &self,
        _access_token: &str,
        structure_id: i64,
    ) -> Result<StructureInformation, EsiError> {
        self.lookups.lock().unwrap().push(structure_id);
        if structure_id == GRANTED_STRUCTURE {
            Ok(StructureInformation {
                structure_id,
                name: "Granted Fortizar".to_string(),
                owner_id: 98_000_001,
                solar_system_id: 30_000_142,
                type_id: Some(35_833),
            })
        } else {
            Err(EsiError::AccessDenied)
        }
    }

    async fn industry_systems(
        &self,
    ) -> Result<EsiResponse<iskworks_esi::IndustrySystemCostIndex>, EsiError> {
        unimplemented!("test never reads cost indices")
    }
}

/// `/universe/names` as ESI behaves: any invalid id rejects the whole
/// batch with 404; `UNNAMED_ID` is valid but omitted from the answer.
struct NamesTransport {
    invalid: BTreeSet<i64>,
    requests: std::sync::Mutex<Vec<usize>>,
    fail_with: Option<EsiError>,
}

const UNNAMED_ID: i64 = 90_000_002;

#[async_trait::async_trait]
impl EsiTransport for NamesTransport {
    async fn exchange_code(
        &self,
        _code: &str,
        _verifier: &PkceVerifier,
    ) -> Result<AuthenticatedToken, EsiError> {
        unimplemented!()
    }
    async fn refresh(&self, _refresh_token: &str) -> Result<RefreshedToken, EsiError> {
        unimplemented!()
    }
    async fn assets(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<AssetObservation>, EsiError> {
        unimplemented!()
    }
    async fn blueprints(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
    ) -> Result<EsiResponse<BlueprintAssetObservation>, EsiError> {
        unimplemented!()
    }
    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        _from_id: Option<i64>,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<WalletTransactionObservation>, EsiError> {
        unimplemented!()
    }
    async fn industry_systems(
        &self,
    ) -> Result<EsiResponse<iskworks_esi::IndustrySystemCostIndex>, EsiError> {
        unimplemented!()
    }
    async fn structure(
        &self,
        _access_token: &str,
        _structure_id: i64,
    ) -> Result<StructureInformation, EsiError> {
        unimplemented!()
    }
    async fn universe_names(
        &self,
        ids: &[i64],
    ) -> Result<Vec<iskworks_esi::EveEntityName>, EsiError> {
        self.requests.lock().unwrap().push(ids.len());
        if let Some(error) = &self.fail_with {
            return Err(error.clone());
        }
        if ids.iter().any(|id| self.invalid.contains(id)) {
            return Err(EsiError::PermanentFailure);
        }
        Ok(ids
            .iter()
            .filter(|id| **id != UNNAMED_ID)
            .map(|&id| iskworks_esi::EveEntityName {
                id,
                name: format!("Entity {id}"),
                category: "character".to_string(),
            })
            .collect())
    }
}

#[tokio::test]
async fn one_invalid_id_does_not_block_naming_the_rest() {
    let ids = (90_000_001..=90_001_000).collect::<Vec<i64>>();
    let bad = 90_000_777;
    let transport = NamesTransport {
        invalid: BTreeSet::from([bad]),
        requests: std::sync::Mutex::new(Vec::new()),
        fail_with: None,
    };

    let lookup = lookup_entity_names(&transport, &ids).await;

    assert_eq!(lookup.names.len(), 998);
    assert_eq!(lookup.misses, vec![UNNAMED_ID, bad]);
    assert!(lookup.stopped_by.is_none());
    let requests = transport.requests.lock().unwrap();
    let rejected = requests.len() / 2;
    assert!(
        rejected <= NAME_LOOKUP_ERROR_BUDGET,
        "{} requests for one bad id",
        requests.len()
    );
}

#[tokio::test]
async fn name_lookup_gives_up_after_its_error_budget_or_when_esi_pushes_back() {
    // Every id invalid: splitting stops once the budget is spent and
    // the rest become misses, retried next week.
    let ids = (1..=64).collect::<Vec<i64>>();
    let transport = NamesTransport {
        invalid: ids.iter().copied().collect(),
        requests: std::sync::Mutex::new(Vec::new()),
        fail_with: None,
    };
    let lookup = lookup_entity_names(&transport, &ids).await;
    assert!(lookup.names.is_empty());
    assert_eq!(lookup.misses.len(), 64);
    let failed_requests = transport.requests.lock().unwrap().len();
    assert!(failed_requests <= 2 * NAME_LOOKUP_ERROR_BUDGET + 1);

    // A 429 stops at once; nothing is recorded as a miss.
    let transport = NamesTransport {
        invalid: BTreeSet::new(),
        requests: std::sync::Mutex::new(Vec::new()),
        fail_with: Some(EsiError::RateLimited {
            retry_after_seconds: Some(30),
        }),
    };
    let lookup = lookup_entity_names(&transport, &(1..=2_500).collect::<Vec<_>>()).await;
    assert!(lookup.misses.is_empty());
    assert!(lookup.stopped_by.is_some());
    assert_eq!(transport.requests.lock().unwrap().len(), 1);
}

/// A structure the character can't access is asked about once, not on
/// every sync, and a freshly resolved name isn't looked up again.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn inaccessible_structures_are_not_looked_up_again(pool: sqlx::PgPool) {
    let (workspace_id, esi_repository) = market_access_fixture(&pool).await;
    let connection_id =
        market_access_connection(&pool, workspace_id, "refresh-token", 9501, false).await;
    sqlx::query("UPDATE eve_connections SET granted_scopes = granted_scopes || $2 WHERE id=$1")
        .bind(connection_id.0)
        .bind(vec![STRUCTURE_SCOPE.to_string()])
        .execute(&pool)
        .await
        .unwrap();
    let connection = esi_repository.get_connection(connection_id).await.unwrap();
    let transport = Arc::new(StructureLookupTransport {
        lookups: std::sync::Mutex::new(Vec::new()),
    });
    let service = EsiApplicationService::new_for_tests(esi_repository, transport.clone());

    for _ in 0..2 {
        service
            .resolve_and_cache_structures(
                &connection,
                "access",
                [GRANTED_STRUCTURE, DENIED_STRUCTURE],
            )
            .await
            .unwrap();
    }
    assert_eq!(
        *transport.lookups.lock().unwrap(),
        vec![GRANTED_STRUCTURE, DENIED_STRUCTURE],
        "the second sync asks ESI nothing"
    );

    // The resolve route skips the denied structure without even
    // refreshing the character's token.
    let resolution = service
        .resolve_structures(workspace_id, &[DENIED_STRUCTURE])
        .await
        .unwrap();
    assert_eq!(resolution.unresolved_structure_ids, vec![DENIED_STRUCTURE]);
    assert_eq!(transport.lookups.lock().unwrap().len(), 2);
}

/// A transport whose `refresh` returns one canned outcome, for testing
/// how `EsiApplicationService::refresh` reacts to token failures.
struct CannedRefreshTransport {
    outcome: std::sync::Mutex<Option<Result<RefreshedToken, EsiError>>>,
}

#[async_trait::async_trait]
impl EsiTransport for CannedRefreshTransport {
    async fn exchange_code(
        &self,
        _code: &str,
        _verifier: &PkceVerifier,
    ) -> Result<AuthenticatedToken, EsiError> {
        unimplemented!("test never exchanges an OAuth code")
    }

    async fn refresh(&self, _refresh_token: &str) -> Result<RefreshedToken, EsiError> {
        self.outcome
            .lock()
            .unwrap()
            .take()
            .expect("test refreshes once")
    }

    async fn assets(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<AssetObservation>, EsiError> {
        unimplemented!("test never reads ESI resources")
    }

    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        _from_id: Option<i64>,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<WalletTransactionObservation>, EsiError> {
        unimplemented!("test never reads ESI resources")
    }

    async fn structure(
        &self,
        _access_token: &str,
        _structure_id: i64,
    ) -> Result<StructureInformation, EsiError> {
        unimplemented!("test never reads ESI resources")
    }

    async fn industry_systems(
        &self,
    ) -> Result<EsiResponse<iskworks_esi::IndustrySystemCostIndex>, EsiError> {
        unimplemented!("test never reads ESI resources")
    }
}

/// Records which refresh tokens were revoked at EVE SSO; optionally
/// fails the revocation.
struct RevocationRecordingTransport {
    revoked: std::sync::Mutex<Vec<String>>,
    fail: bool,
}

#[async_trait::async_trait]
impl EsiTransport for RevocationRecordingTransport {
    async fn exchange_code(
        &self,
        _code: &str,
        _verifier: &PkceVerifier,
    ) -> Result<AuthenticatedToken, EsiError> {
        unimplemented!("test never exchanges an OAuth code")
    }

    async fn refresh(&self, _refresh_token: &str) -> Result<RefreshedToken, EsiError> {
        unimplemented!("test never refreshes")
    }

    async fn revoke_refresh_token(&self, refresh_token: &str) -> Result<(), EsiError> {
        self.revoked.lock().unwrap().push(refresh_token.to_string());
        if self.fail {
            Err(EsiError::TemporaryFailure)
        } else {
            Ok(())
        }
    }

    async fn assets(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<AssetObservation>, EsiError> {
        unimplemented!("test never reads ESI resources")
    }

    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        _from_id: Option<i64>,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<WalletTransactionObservation>, EsiError> {
        unimplemented!("test never reads ESI resources")
    }

    async fn structure(
        &self,
        _access_token: &str,
        _structure_id: i64,
    ) -> Result<StructureInformation, EsiError> {
        unimplemented!("test never reads ESI resources")
    }

    async fn industry_systems(
        &self,
    ) -> Result<EsiResponse<iskworks_esi::IndustrySystemCostIndex>, EsiError> {
        unimplemented!("test never reads ESI resources")
    }
}

/// Disconnecting deletes the token locally *and* revokes it at EVE, so
/// the grant stops working everywhere. Revocation is best effort: if EVE
/// can't be reached, the local disconnect still stands.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn disconnect_revokes_the_refresh_token_at_eve(pool: sqlx::PgPool) {
    let (workspace_id, esi_repository) = market_access_fixture(&pool).await;
    for (index, fail) in [false, true].into_iter().enumerate() {
        let refresh_token = format!("refresh-token-{index}");
        let connection_id = market_access_connection(
            &pool,
            workspace_id,
            &refresh_token,
            9401 + i64::try_from(index).unwrap(),
            false,
        )
        .await;
        let transport = Arc::new(RevocationRecordingTransport {
            revoked: std::sync::Mutex::new(Vec::new()),
            fail,
        });
        let service =
            EsiApplicationService::new_for_tests(esi_repository.clone(), transport.clone());

        let disconnected = service.disconnect(connection_id).await.unwrap();

        assert_eq!(disconnected.status, ConnectionStatus::Disconnected);
        assert_eq!(*transport.revoked.lock().unwrap(), vec![refresh_token]);
        assert!(esi_repository
            .load_refresh_token(connection_id)
            .await
            .is_err());
    }
}

/// A refresh EVE rejects for good (revoked token, scopes no longer
/// granted) flags the connection so the worker stops retrying it and the
/// UI asks for a reconnect. Transient failures -- and our own
/// misconfiguration (`PermanentFailure`) -- must leave it `connected`.
/// A manual sync started moments ago -- even one that then failed --
/// blocks another of the same data for that character for a minute.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn back_to_back_manual_syncs_are_throttled(pool: sqlx::PgPool) {
    let (workspace_id, esi_repository) = market_access_fixture(&pool).await;
    let connection_id =
        market_access_connection(&pool, workspace_id, "refresh-token", 9301, false).await;
    let service = EsiApplicationService::new_for_tests(
        esi_repository,
        Arc::new(CannedRefreshTransport {
            outcome: std::sync::Mutex::new(Some(Err(EsiError::TemporaryFailure))),
        }),
    );

    assert!(matches!(
        service.sync(connection_id, EsiSyncKind::Assets).await,
        Err(EsiApplicationError::Protocol(EsiError::TemporaryFailure))
    ));
    assert!(matches!(
        service.sync(connection_id, EsiSyncKind::Assets).await,
        Err(EsiApplicationError::SyncTooSoon {
            retry_after_seconds: 1..=60
        })
    ));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_permanently_rejected_refresh_flags_the_connection(pool: sqlx::PgPool) {
    let unscoped_identity = || RefreshedToken {
        access_token: "access".to_string(),
        rotated_refresh_token: None,
        expires_at: Utc::now() + Duration::minutes(20),
        identity: Identity {
            character_id: 9101,
            character_name: "Character 9101".to_string(),
            scopes: BTreeSet::new(),
        },
    };
    let cases: Vec<(Result<RefreshedToken, EsiError>, ConnectionStatus)> = vec![
        (
            Err(EsiError::AuthorizationRequired),
            ConnectionStatus::NeedsReconnection,
        ),
        (Ok(unscoped_identity()), ConnectionStatus::MissingScope),
        (Err(EsiError::TemporaryFailure), ConnectionStatus::Connected),
        (
            Err(EsiError::RateLimited {
                retry_after_seconds: None,
            }),
            ConnectionStatus::Connected,
        ),
        (Err(EsiError::PermanentFailure), ConnectionStatus::Connected),
    ];
    let (workspace_id, esi_repository) = market_access_fixture(&pool).await;
    for (index, (outcome, expected_status)) in cases.into_iter().enumerate() {
        let connection_id = market_access_connection(
            &pool,
            workspace_id,
            "refresh-token",
            9101 + i64::try_from(index).unwrap() * 1000,
            false,
        )
        .await;
        let service = EsiApplicationService::new_for_tests(
            esi_repository.clone(),
            Arc::new(CannedRefreshTransport {
                outcome: std::sync::Mutex::new(Some(outcome)),
            }),
        );

        assert!(service.refresh(connection_id).await.is_err());
        let connection = esi_repository.get_connection(connection_id).await.unwrap();
        assert_eq!(connection.status, expected_status, "case {index}");
        if expected_status != ConnectionStatus::Connected {
            // Only a reconnect can fix a flagged connection, so another
            // refresh fails without asking EVE SSO again (the canned
            // transport panics if it is asked twice).
            assert!(
                service.refresh(connection_id).await.is_err(),
                "case {index}"
            );
        }
    }
}

fn refreshed_token(access_token: &str, expires_at: chrono::DateTime<Utc>) -> RefreshedToken {
    RefreshedToken {
        access_token: access_token.to_string(),
        rotated_refresh_token: None,
        expires_at,
        identity: Identity {
            character_id: 9101,
            character_name: "Character 9101".to_string(),
            scopes: BTreeSet::from([
                ASSET_SCOPE.to_string(),
                BLUEPRINT_SCOPE.to_string(),
                WALLET_SCOPE.to_string(),
            ]),
        },
    }
}

/// EVE SSO is only asked for a new access token once the stored one is
/// about to expire -- not on every ESI call.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_live_access_token_is_reused_instead_of_refreshed(pool: sqlx::PgPool) {
    let (workspace_id, esi_repository) = market_access_fixture(&pool).await;
    let connection_id =
        market_access_connection(&pool, workspace_id, "refresh-token", 9101, false).await;
    let refresh_once = |token: RefreshedToken| {
        EsiApplicationService::new_for_tests(
            esi_repository.clone(),
            Arc::new(CannedRefreshTransport {
                outcome: std::sync::Mutex::new(Some(Ok(token))),
            }),
        )
    };

    let service = refresh_once(refreshed_token(
        "access-1",
        Utc::now() + Duration::minutes(20),
    ));
    let (_, first) = service.refresh(connection_id).await.unwrap();
    // `CannedRefreshTransport` panics if asked to refresh a second time.
    let (_, second) = service.refresh(connection_id).await.unwrap();
    assert_eq!(first, "access-1");
    assert_eq!(second, "access-1", "the live token is reused");

    // A token about to expire is refreshed rather than handed out.
    sqlx::query("UPDATE eve_connections SET access_token_expires_at=now() + interval '2 minutes' WHERE id=$1")
        .bind(connection_id.0)
        .execute(&pool)
        .await
        .unwrap();
    let service = refresh_once(refreshed_token(
        "access-2",
        Utc::now() + Duration::minutes(20),
    ));
    assert_eq!(service.refresh(connection_id).await.unwrap().1, "access-2");
    assert_eq!(service.refresh(connection_id).await.unwrap().1, "access-2");

    // A connection that needs reconnecting never hands out its old token.
    esi_repository
        .mark_connection_unrefreshable(
            connection_id,
            ConnectionStatus::NeedsReconnection,
            "authorization_required",
        )
        .await
        .unwrap();
    let service = EsiApplicationService::new_for_tests(
        esi_repository.clone(),
        Arc::new(CannedRefreshTransport {
            outcome: std::sync::Mutex::new(Some(Err(EsiError::AuthorizationRequired))),
        }),
    );
    assert!(service.refresh(connection_id).await.is_err());
}

/// A remembered/preferred connection that succeeds must be the *only*
/// one tried -- no fallback iteration over other eligible connections,
/// even though one exists and is also granted.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn preferred_connection_success_skips_iterating_other_eligible_connections(
    pool: sqlx::PgPool,
) {
    let (workspace_id, esi_repository) = market_access_fixture(&pool).await;
    let preferred_id =
        market_access_connection(&pool, workspace_id, "refresh-preferred", 9001, true).await;
    let _other_eligible =
        market_access_connection(&pool, workspace_id, "refresh-other", 9002, true).await;
    let transport = Arc::new(MarketAccessFakeTransport {
        tokens: std::collections::BTreeMap::from([
            ("refresh-preferred", 9001),
            ("refresh-other", 9002),
        ]),
        granted: std::collections::BTreeSet::from([9001, 9002]),
        market_calls: std::sync::Mutex::new(Vec::new()),
    });
    let service = EsiApplicationService::new_for_tests(esi_repository, transport.clone());

    let resolution = service
        .resolve_market_access(
            workspace_id,
            1_050_487_654_321,
            30_000_505,
            Some(preferred_id),
        )
        .await
        .unwrap();

    match resolution {
        MarketAccessResolution::Confirmed { connection_id, .. } => {
            assert_eq!(connection_id, preferred_id);
        }
        other => panic!("expected Confirmed, got {other:?}"),
    }
    assert_eq!(
        transport.market_calls.lock().unwrap().as_slice(),
        &["access-9001".to_string()],
        "the other eligible connection must never have been tried",
    );
}

/// A preferred connection that 403s is cleared (by the caller, not this
/// method -- resolution itself just doesn't return it) and the
/// remaining eligible connections are tried; a later one succeeding is
/// remembered as the new winner.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn preferred_connection_denial_falls_back_to_the_next_eligible_connection(
    pool: sqlx::PgPool,
) {
    let (workspace_id, esi_repository) = market_access_fixture(&pool).await;
    let preferred_id =
        market_access_connection(&pool, workspace_id, "refresh-preferred", 9001, true).await;
    let second_id =
        market_access_connection(&pool, workspace_id, "refresh-second", 9002, true).await;
    let transport = Arc::new(MarketAccessFakeTransport {
        tokens: std::collections::BTreeMap::from([
            ("refresh-preferred", 9001),
            ("refresh-second", 9002),
        ]),
        // Only the second character actually has market access.
        granted: std::collections::BTreeSet::from([9002]),
        market_calls: std::sync::Mutex::new(Vec::new()),
    });
    let service = EsiApplicationService::new_for_tests(esi_repository, transport.clone());

    let resolution = service
        .resolve_market_access(
            workspace_id,
            1_050_487_654_321,
            30_000_505,
            Some(preferred_id),
        )
        .await
        .unwrap();

    match resolution {
        MarketAccessResolution::Confirmed { connection_id, .. } => {
            assert_eq!(connection_id, second_id);
        }
        other => panic!("expected Confirmed with the second connection, got {other:?}"),
    }
    assert_eq!(
        transport.market_calls.lock().unwrap().as_slice(),
        &["access-9001".to_string(), "access-9002".to_string()],
        "the preferred connection must be tried first, then the fallback",
    );
}

/// No connected character has the market-structure scope granted at
/// all -- distinct from `AllDenied`, since nobody is even tried, and
/// zero market calls are made.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn no_eligible_character_makes_zero_market_calls(pool: sqlx::PgPool) {
    let (workspace_id, esi_repository) = market_access_fixture(&pool).await;
    let _ungranted =
        market_access_connection(&pool, workspace_id, "refresh-ungranted", 9001, false).await;
    let transport = Arc::new(MarketAccessFakeTransport {
        tokens: std::collections::BTreeMap::from([("refresh-ungranted", 9001)]),
        granted: std::collections::BTreeSet::new(),
        market_calls: std::sync::Mutex::new(Vec::new()),
    });
    let service = EsiApplicationService::new_for_tests(esi_repository, transport.clone());

    let resolution = service
        .resolve_market_access(workspace_id, 1_050_487_654_321, 30_000_505, None)
        .await
        .unwrap();

    assert!(matches!(
        resolution,
        MarketAccessResolution::NoEligibleCharacter
    ));
    assert!(transport.market_calls.lock().unwrap().is_empty());
}

/// Every eligible character is tried and every one is denied.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn all_eligible_characters_denied_reports_all_denied(pool: sqlx::PgPool) {
    let (workspace_id, esi_repository) = market_access_fixture(&pool).await;
    let _first = market_access_connection(&pool, workspace_id, "refresh-first", 9001, true).await;
    let _second = market_access_connection(&pool, workspace_id, "refresh-second", 9002, true).await;
    let transport = Arc::new(MarketAccessFakeTransport {
        tokens: std::collections::BTreeMap::from([
            ("refresh-first", 9001),
            ("refresh-second", 9002),
        ]),
        granted: std::collections::BTreeSet::new(),
        market_calls: std::sync::Mutex::new(Vec::new()),
    });
    let service = EsiApplicationService::new_for_tests(esi_repository, transport.clone());

    let resolution = service
        .resolve_market_access(workspace_id, 1_050_487_654_321, 30_000_505, None)
        .await
        .unwrap();

    assert!(matches!(resolution, MarketAccessResolution::AllDenied));
    assert_eq!(transport.market_calls.lock().unwrap().len(), 2);
}

/// Wallet-sync fixture: no new transactions, a configurable journal.
struct JournalFakeTransport {
    /// `Some(entries)` is served as one page; `pages` reports the page count.
    pages: Vec<Result<Vec<iskworks_esi::WalletJournalObservation>, EsiError>>,
    journal_calls: std::sync::Mutex<Vec<u32>>,
}

fn journal_entry(ref_id: i64, ref_type: &str) -> iskworks_esi::WalletJournalObservation {
    iskworks_esi::WalletJournalObservation {
        ref_id,
        date: "2026-09-28T10:00:00Z".parse().unwrap(),
        ref_type: ref_type.to_string(),
        amount: Decimal::new(-1234, 2),
        balance: None,
        first_party_id: None,
        second_party_id: None,
        context_id: None,
        context_id_type: None,
        description: None,
        reason: None,
        tax: None,
        tax_receiver_id: None,
        raw: json!({"id": ref_id}),
    }
}

#[async_trait::async_trait]
impl EsiTransport for JournalFakeTransport {
    async fn exchange_code(
        &self,
        _code: &str,
        _verifier: &PkceVerifier,
    ) -> Result<AuthenticatedToken, EsiError> {
        unimplemented!()
    }

    async fn refresh(&self, _refresh_token: &str) -> Result<RefreshedToken, EsiError> {
        Ok(RefreshedToken {
            access_token: "access-9001".to_string(),
            rotated_refresh_token: None,
            expires_at: Utc::now() + Duration::minutes(20),
            identity: Identity {
                character_id: 9001,
                character_name: "Character 9001".to_string(),
                scopes: BTreeSet::from([
                    ASSET_SCOPE.to_string(),
                    BLUEPRINT_SCOPE.to_string(),
                    WALLET_SCOPE.to_string(),
                ]),
            },
        })
    }

    async fn assets(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<AssetObservation>, EsiError> {
        unimplemented!()
    }

    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        _from_id: Option<i64>,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<WalletTransactionObservation>, EsiError> {
        Ok(EsiResponse {
            records: Vec::new(),
            not_modified: false,
            metadata: EsiResponseMetadata::default(),
        })
    }

    async fn wallet_journal(
        &self,
        _access_token: &str,
        _character_id: i64,
        page: u32,
    ) -> Result<EsiResponse<iskworks_esi::WalletJournalObservation>, EsiError> {
        self.journal_calls.lock().unwrap().push(page);
        let result = self.pages[(page - 1) as usize].clone();
        result.map(|records| EsiResponse {
            records,
            not_modified: false,
            metadata: EsiResponseMetadata {
                pages: Some(self.pages.len() as u32),
                ..Default::default()
            },
        })
    }

    async fn structure(
        &self,
        _access_token: &str,
        _structure_id: i64,
    ) -> Result<StructureInformation, EsiError> {
        unimplemented!()
    }

    async fn industry_systems(
        &self,
    ) -> Result<EsiResponse<iskworks_esi::IndustrySystemCostIndex>, EsiError> {
        unimplemented!()
    }
}

async fn journal_sync(
    pool: sqlx::PgPool,
    pages: Vec<Result<Vec<iskworks_esi::WalletJournalObservation>, EsiError>>,
) -> (i64, Vec<u32>, Result<Vec<EsiSyncRun>, EsiApplicationError>) {
    let (workspace_id, esi_repository) = market_access_fixture(&pool).await;
    let connection_id = market_access_connection(&pool, workspace_id, "refresh", 9001, false).await;
    let transport = Arc::new(JournalFakeTransport {
        pages,
        journal_calls: std::sync::Mutex::new(Vec::new()),
    });
    let service = EsiApplicationService::new_for_tests(esi_repository, transport.clone());
    let outcome = service
        .sync(connection_id, EsiSyncKind::WalletTransactions)
        .await;
    let stored: i64 =
        sqlx::query_scalar("SELECT count(*) FROM esi_wallet_journal WHERE connection_id=$1")
            .bind(connection_id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    let calls = transport.journal_calls.lock().unwrap().clone();
    (stored, calls, outcome)
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn wallet_sync_ingests_every_journal_page(pool: sqlx::PgPool) {
    let (stored, calls, outcome) = journal_sync(
        pool,
        vec![
            Ok(vec![
                journal_entry(1, "brokers_fee"),
                journal_entry(2, "transaction_tax"),
            ]),
            Ok(vec![journal_entry(3, "brokers_fee")]),
        ],
    )
    .await;
    assert!(outcome.is_ok());
    assert_eq!(stored, 3);
    assert_eq!(calls, vec![1, 2]);
}

/// The journal is best-effort: a failing page keeps what was read and the
/// transaction sync still succeeds.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_failing_journal_page_keeps_earlier_pages_and_does_not_fail_the_sync(pool: sqlx::PgPool) {
    let (stored, calls, outcome) = journal_sync(
        pool,
        vec![
            Ok(vec![journal_entry(1, "brokers_fee")]),
            Err(EsiError::PermanentFailure),
        ],
    )
    .await;
    assert!(outcome.is_ok());
    assert_eq!(stored, 1);
    assert_eq!(calls, vec![1, 2]);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_journal_that_always_fails_leaves_the_sync_successful(pool: sqlx::PgPool) {
    let (stored, _, outcome) = journal_sync(pool, vec![Err(EsiError::AccessDenied)]).await;
    assert!(outcome.is_ok());
    assert_eq!(stored, 0);
}

/// Fails every asset read; the test never reaches anything else.
struct FailingAssetsTransport;

#[async_trait::async_trait]
impl EsiTransport for FailingAssetsTransport {
    async fn exchange_code(
        &self,
        _code: &str,
        _verifier: &PkceVerifier,
    ) -> Result<AuthenticatedToken, EsiError> {
        unimplemented!("test never exchanges an OAuth code")
    }

    async fn refresh(&self, _refresh_token: &str) -> Result<RefreshedToken, EsiError> {
        unimplemented!("test never refreshes a token")
    }

    async fn assets(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<AssetObservation>, EsiError> {
        Err(EsiError::TemporaryFailure)
    }

    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        _from_id: Option<i64>,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<WalletTransactionObservation>, EsiError> {
        unimplemented!("test never reads wallet transactions")
    }

    async fn structure(
        &self,
        _access_token: &str,
        _structure_id: i64,
    ) -> Result<StructureInformation, EsiError> {
        unimplemented!("test never resolves structures")
    }

    async fn industry_systems(
        &self,
    ) -> Result<EsiResponse<iskworks_esi::IndustrySystemCostIndex>, EsiError> {
        unimplemented!("test never reads cost indices")
    }
}

/// The worker syncs through `EsiSyncDispatcher`; its `sync_assets` must run
/// the service's own asset sync rather than resolve back to itself.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn the_sync_dispatcher_runs_the_services_asset_sync(pool: sqlx::PgPool) {
    let (workspace_id, esi_repository) = market_access_fixture(&pool).await;
    let connection_id =
        market_access_connection(&pool, workspace_id, "refresh-token", 9401, false).await;
    let connection = esi_repository.get_connection(connection_id).await.unwrap();
    let service =
        EsiApplicationService::new_for_tests(esi_repository, Arc::new(FailingAssetsTransport));

    let result = EsiSyncDispatcher::sync_assets(&service, &connection, "access-token").await;

    assert!(matches!(
        result,
        Err(EsiApplicationError::Protocol(EsiError::TemporaryFailure))
    ));
}

fn blueprint(
    item_id: i64,
    material_efficiency: i16,
    time_efficiency: i16,
) -> BlueprintAssetObservation {
    BlueprintAssetObservation {
        item_id,
        type_id: 691,
        location_id: 60_003_760,
        location_flag: "Hangar".to_string(),
        material_efficiency,
        time_efficiency,
        runs: -1,
        quantity: -1,
        raw: json!({}),
    }
}

/// One page of assets (always changed) and a configurable blueprint page.
struct BlueprintPageTransport {
    blueprints: Vec<BlueprintAssetObservation>,
}

#[async_trait::async_trait]
impl EsiTransport for BlueprintPageTransport {
    async fn exchange_code(
        &self,
        _code: &str,
        _verifier: &PkceVerifier,
    ) -> Result<AuthenticatedToken, EsiError> {
        unimplemented!("test never exchanges an OAuth code")
    }

    async fn refresh(&self, _refresh_token: &str) -> Result<RefreshedToken, EsiError> {
        unimplemented!("test passes its own token")
    }

    async fn assets(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<AssetObservation>, EsiError> {
        Ok(EsiResponse {
            records: vec![asset(1_100_000_000_001, 60_003_760, "station")],
            not_modified: false,
            metadata: EsiResponseMetadata {
                pages: Some(1),
                ..EsiResponseMetadata::default()
            },
        })
    }

    async fn blueprints(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
    ) -> Result<EsiResponse<BlueprintAssetObservation>, EsiError> {
        Ok(EsiResponse {
            records: self.blueprints.clone(),
            not_modified: false,
            metadata: EsiResponseMetadata {
                pages: Some(1),
                ..EsiResponseMetadata::default()
            },
        })
    }

    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        _from_id: Option<i64>,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<WalletTransactionObservation>, EsiError> {
        unimplemented!("test never reads wallet transactions")
    }

    async fn structure(
        &self,
        _access_token: &str,
        _structure_id: i64,
    ) -> Result<StructureInformation, EsiError> {
        unimplemented!("test never resolves structures")
    }

    async fn industry_systems(
        &self,
    ) -> Result<EsiResponse<iskworks_esi::IndustrySystemCostIndex>, EsiError> {
        unimplemented!("test never reads cost indices")
    }
}

/// `(eve_item_id, material_efficiency, time_efficiency)` of every stored blueprint.
async fn stored_blueprints(pool: &sqlx::PgPool) -> Vec<(i64, i16, i16)> {
    sqlx::query_as(
        "SELECT eve_item_id, material_efficiency, time_efficiency \
         FROM blueprint_observations ORDER BY eve_item_id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn sync_blueprints(
    pool: &sqlx::PgPool,
    blueprints: Vec<BlueprintAssetObservation>,
) -> Result<EsiSyncRun, EsiApplicationError> {
    let (workspace_id, esi_repository) = market_access_fixture(pool).await;
    let connection_id =
        market_access_connection(pool, workspace_id, "refresh-token", 9402, false).await;
    let connection = esi_repository.get_connection(connection_id).await.unwrap();
    let service = EsiApplicationService::new_for_tests(
        esi_repository,
        Arc::new(BlueprintPageTransport { blueprints }),
    );
    EsiSyncDispatcher::sync_assets(&service, &connection, "access-token").await
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn an_out_of_range_blueprint_is_skipped_without_failing_the_sync(pool: sqlx::PgPool) {
    let run = sync_blueprints(
        &pool,
        vec![
            blueprint(1, 10, 20),
            blueprint(2, 11, 0),
            blueprint(3, -1, 0),
            blueprint(4, 0, 21),
        ],
    )
    .await
    .unwrap();

    assert_eq!(run.status, iskworks_core::EsiSyncStatus::Succeeded);
    assert_eq!(run.imported_count, 1, "the asset snapshot is imported");
    assert_eq!(run.skipped_count, 3);
    assert!(
        run.summary.contains("3 blueprints skipped"),
        "summary: {}",
        run.summary
    );
    assert_eq!(stored_blueprints(&pool).await, vec![(1, 10, 20)]);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_blueprint_turning_invalid_keeps_its_last_good_row(pool: sqlx::PgPool) {
    let (workspace_id, esi_repository) = market_access_fixture(&pool).await;
    let connection_id =
        market_access_connection(&pool, workspace_id, "refresh-token", 9403, false).await;
    let connection = esi_repository.get_connection(connection_id).await.unwrap();
    let sync = |blueprints| {
        let service = EsiApplicationService::new_for_tests(
            esi_repository.clone(),
            Arc::new(BlueprintPageTransport { blueprints }),
        );
        let connection = connection.clone();
        async move { EsiSyncDispatcher::sync_assets(&service, &connection, "access-token").await }
    };
    sync(vec![blueprint(1, 5, 10), blueprint(2, 8, 16)])
        .await
        .unwrap();

    let run = sync(vec![blueprint(1, 11, 10)]).await.unwrap();

    assert_eq!(run.status, iskworks_core::EsiSyncStatus::Succeeded);
    assert_eq!(run.skipped_count, 1);
    // Item 1 keeps its last good values; item 2 is gone for real.
    assert_eq!(stored_blueprints(&pool).await, vec![(1, 5, 10)]);
}
