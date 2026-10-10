use super::WorkerConfig;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[test]
fn defaults_keep_market_polling_separate_from_freshness() {
    let config = WorkerConfig::default();

    assert_eq!(config.market_poll_interval, Duration::from_secs(5));
    assert_eq!(config.market_freshness, Duration::from_secs(15 * 60));
    assert!(config.market_poll_interval < config.market_freshness);
}

#[test]
fn auth_gc_defaults_to_hourly_and_is_configurable() {
    assert_eq!(
        WorkerConfig::default().auth_gc_poll_interval,
        Duration::from_secs(3600)
    );
    let config = WorkerConfig::from_lookup(|name| match name {
        "ISKWORKS_WORKER_AUTH_GC_POLL_SECONDS" => Some("600".to_string()),
        _ => None,
    })
    .unwrap();
    assert_eq!(config.auth_gc_poll_interval, Duration::from_secs(600));
}

#[test]
fn market_gc_settings_default_to_hourly_and_are_configurable() {
    let defaults = WorkerConfig::default();
    assert_eq!(defaults.market_gc_poll_interval, Duration::from_secs(3600));
    assert_eq!(defaults.market_gc_grace, Duration::from_secs(3600));

    let config = WorkerConfig::from_lookup(|name| match name {
        "ISKWORKS_WORKER_MARKET_GC_POLL_SECONDS" => Some("900".to_string()),
        "ISKWORKS_WORKER_MARKET_GC_GRACE_SECONDS" => Some("120".to_string()),
        _ => None,
    })
    .unwrap();
    assert_eq!(config.market_gc_poll_interval, Duration::from_secs(900));
    assert_eq!(config.market_gc_grace, Duration::from_secs(120));
    assert_eq!(config.market_poll_interval, Duration::from_secs(5));
}

#[test]
fn esi_gc_defaults_to_hourly_and_is_configurable() {
    assert_eq!(
        WorkerConfig::default().esi_gc_poll_interval,
        Duration::from_secs(3600)
    );
    let config = WorkerConfig::from_lookup(|name| match name {
        "ISKWORKS_WORKER_ESI_GC_POLL_SECONDS" => Some("600".to_string()),
        _ => None,
    })
    .unwrap();
    assert_eq!(config.esi_gc_poll_interval, Duration::from_secs(600));
}

#[test]
fn configuration_can_change_market_polling_without_changing_freshness() {
    let config = WorkerConfig::from_lookup(|name| match name {
        "ISKWORKS_WORKER_MARKET_POLL_SECONDS" => Some("2".to_string()),
        _ => None,
    })
    .unwrap();

    assert_eq!(config.market_poll_interval, Duration::from_secs(2));
    assert_eq!(config.market_freshness, Duration::from_secs(15 * 60));
}

#[test]
fn bounded_batch_and_shutdown_settings_are_configurable() {
    let config = WorkerConfig::from_lookup(|name| match name {
        "ISKWORKS_WORKER_SOURCE_BATCH_SIZE" => Some("3".to_string()),
        "ISKWORKS_WORKER_MARKET_ITEM_BATCH_SIZE" => Some("17".to_string()),
        "ISKWORKS_WORKER_SYSTEM_BATCH_SIZE" => Some("9".to_string()),
        "ISKWORKS_WORKER_SHUTDOWN_TIMEOUT_SECONDS" => Some("45".to_string()),
        _ => None,
    })
    .unwrap();

    assert_eq!(config.source_batch_size, 3);
    assert_eq!(config.market_item_batch_size, 17);
    assert_eq!(config.system_batch_size, 9);
    assert_eq!(config.shutdown_timeout, Duration::from_secs(45));
}

#[test]
fn character_sync_settings_are_configurable_and_default_separately_from_market() {
    let config = WorkerConfig::from_lookup(|name| match name {
        "ISKWORKS_WORKER_CHARACTER_SYNC_POLL_SECONDS" => Some("10".to_string()),
        "ISKWORKS_WORKER_CHARACTER_SYNC_BATCH_SIZE" => Some("25".to_string()),
        "ISKWORKS_WORKER_CHARACTER_SYNC_CONCURRENCY" => Some("2".to_string()),
        _ => None,
    })
    .unwrap();

    assert_eq!(config.character_sync_poll_interval, Duration::from_secs(10));
    assert_eq!(config.character_sync_batch_size, 25);
    assert_eq!(config.character_sync_concurrency, 2);
    assert_eq!(config.market_poll_interval, Duration::from_secs(5));
}

// --- Cooperative-shutdown loop skeleton ------------------------------
//
// `poll_loop` is the shared tick/cancel wait every worker loop runs on;
// `drain_with_timeout` is the last-resort bound `run` applies after
// cancelling. These exercise both with `start_paused` virtual time: a
// "prompt" exit is one that needs *no* `tokio::time::advance` between
// the cancel and the loop task finishing.

async fn drain(count: usize) {
    for _ in 0..count {
        tokio::task::yield_now().await;
    }
}

/// A loop parked on its interval with nothing running exits the
/// instant the token is cancelled -- the virtual clock never advances
/// toward the (1h away) next tick.
#[tokio::test(start_paused = true)]
async fn shutdown_while_idle_exits_promptly() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let cancel = CancellationToken::new();
    let passes = Arc::new(AtomicUsize::new(0));
    let handle = {
        let passes = Arc::clone(&passes);
        tokio::spawn(super::poll_loop(
            "test",
            Duration::from_secs(3600),
            cancel.clone(),
            super::PassMode::Cooperative,
            move |_cancel| {
                let passes = Arc::clone(&passes);
                async move {
                    passes.fetch_add(1, Ordering::SeqCst);
                }
            },
        ))
    };

    drain(8).await;
    // `tokio::time::interval`'s first tick is immediate: one pass ran,
    // then the loop parked on the 1h tick.
    assert_eq!(passes.load(Ordering::SeqCst), 1);
    assert!(!handle.is_finished());

    cancel.cancel();
    drain(8).await;

    assert!(
        handle.is_finished(),
        "cancelled idle loop must exit without the clock advancing"
    );
    handle.await.unwrap();
    assert_eq!(
        passes.load(Ordering::SeqCst),
        1,
        "no extra pass after cancel"
    );
}

/// A loop parked mid-interval (clock advanced partway to the next
/// tick) also exits at once on cancel, with no further pass.
#[tokio::test(start_paused = true)]
async fn shutdown_while_waiting_on_interval_exits_promptly() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let cancel = CancellationToken::new();
    let passes = Arc::new(AtomicUsize::new(0));
    let handle = {
        let passes = Arc::clone(&passes);
        tokio::spawn(super::poll_loop(
            "test",
            Duration::from_secs(60),
            cancel.clone(),
            super::PassMode::Cooperative,
            move |_cancel| {
                let passes = Arc::clone(&passes);
                async move {
                    passes.fetch_add(1, Ordering::SeqCst);
                }
            },
        ))
    };

    drain(8).await;
    assert_eq!(passes.load(Ordering::SeqCst), 1, "immediate first tick");
    tokio::time::advance(Duration::from_secs(30)).await; // halfway to tick 2
    drain(8).await;
    assert_eq!(
        passes.load(Ordering::SeqCst),
        1,
        "still parked on the interval, no second pass"
    );
    assert!(!handle.is_finished());

    cancel.cancel();
    drain(8).await;

    assert!(handle.is_finished(), "cancel interrupts the interval wait");
    handle.await.unwrap();
    assert_eq!(
        passes.load(Ordering::SeqCst),
        1,
        "no pass fired after cancel"
    );
}

/// A pass that never yields to cancellation (a hung ESI future with
/// no HTTP timeout) keeps its loop task alive -- but `drain_with_timeout`
/// still bounds shutdown at `shutdown_timeout`.
#[tokio::test(start_paused = true)]
async fn main_drain_has_bounded_upper_limit() {
    // A cooperative drain completes immediately, far under the bound.
    assert_eq!(
        super::drain_with_timeout(async {}, Duration::from_secs(30)).await,
        super::DrainResult::Completed
    );

    let cancel = CancellationToken::new();
    let handle = tokio::spawn(super::poll_loop(
        "hung",
        Duration::from_secs(3600),
        cancel.clone(),
        super::PassMode::Cooperative,
        |_cancel| std::future::pending::<()>(),
    ));
    drain(8).await;
    cancel.cancel();
    drain(8).await;
    assert!(
        !handle.is_finished(),
        "a pass ignoring cancellation keeps its loop task alive"
    );

    let drained = super::drain_with_timeout(
        async {
            let _ = handle.await;
        },
        Duration::from_secs(30),
    );
    let advancer = async {
        tokio::time::advance(Duration::from_secs(30)).await;
        drain(8).await;
    };
    let (result, ()) = tokio::join!(drained, advancer);
    assert_eq!(
        result,
        super::DrainResult::TimedOut,
        "the 30s drain bound is what unblocks shutdown"
    );
}

#[derive(Clone)]
struct PanicTransport;

#[async_trait::async_trait]
impl iskworks_esi::EsiTransport for PanicTransport {
    async fn exchange_code(
        &self,
        _code: &str,
        _verifier: &iskworks_esi::PkceVerifier,
    ) -> Result<iskworks_esi::AuthenticatedToken, iskworks_esi::EsiError> {
        panic!("empty worker poll attempted ESI")
    }

    async fn refresh(
        &self,
        _refresh_token: &str,
    ) -> Result<iskworks_esi::RefreshedToken, iskworks_esi::EsiError> {
        panic!("empty worker poll attempted ESI")
    }

    async fn assets(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<iskworks_esi::EsiResponse<iskworks_esi::AssetObservation>, iskworks_esi::EsiError>
    {
        panic!("empty worker poll attempted ESI")
    }

    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        _from_id: Option<i64>,
        _etag: Option<&str>,
    ) -> Result<
        iskworks_esi::EsiResponse<iskworks_esi::WalletTransactionObservation>,
        iskworks_esi::EsiError,
    > {
        panic!("empty worker poll attempted ESI")
    }

    async fn structure(
        &self,
        _access_token: &str,
        _structure_id: i64,
    ) -> Result<iskworks_esi::StructureInformation, iskworks_esi::EsiError> {
        panic!("empty worker poll attempted ESI")
    }

    async fn industry_systems(
        &self,
    ) -> Result<
        iskworks_esi::EsiResponse<iskworks_esi::IndustrySystemCostIndex>,
        iskworks_esi::EsiError,
    > {
        panic!("empty worker poll attempted ESI")
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn empty_market_and_system_polls_make_no_esi_request(pool: sqlx::PgPool) {
    let now = chrono::Utc::now();
    let esi_repository = std::sync::Arc::new(iskworks_storage::PgEsiRepository::new(pool.clone()));
    esi_repository
        .register_adjusted_price_refresh(now)
        .await
        .unwrap();
    let claim = esi_repository
        .begin_adjusted_price_refresh(now, now + chrono::Duration::minutes(2))
        .await
        .unwrap()
        .unwrap();
    esi_repository
        .complete_adjusted_price_refresh(
            claim,
            &[iskworks_esi::AdjustedPrice {
                type_id: 34,
                adjusted_price: rust_decimal::Decimal::ONE,
            }],
            now,
            now + chrono::Duration::hours(6),
            None,
            Some("fixture"),
        )
        .await
        .unwrap();
    let worker = super::EvidenceWorker::new(
        WorkerConfig::default(),
        esi_repository,
        std::sync::Arc::new(iskworks_storage::PgMarketRepository::new(pool)),
        std::sync::Arc::new(PanicTransport),
    );

    assert_eq!(
        worker
            .refresh_due_market(chrono::Utc::now(), &CancellationToken::new())
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        worker
            .refresh_due_system_indices(chrono::Utc::now())
            .await
            .unwrap(),
        0
    );
    assert!(!worker.refresh_due_adjusted_prices(now).await.unwrap());
}

/// Serves one regional Tritanium book and counts calls; every other ESI
/// route panics.
#[derive(Default)]
struct RegionalBookTransport {
    calls: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl iskworks_esi::EsiTransport for RegionalBookTransport {
    async fn exchange_code(
        &self,
        _code: &str,
        _verifier: &iskworks_esi::PkceVerifier,
    ) -> Result<iskworks_esi::AuthenticatedToken, iskworks_esi::EsiError> {
        panic!("public market refresh only calls the regional endpoint")
    }

    async fn refresh(
        &self,
        _refresh_token: &str,
    ) -> Result<iskworks_esi::RefreshedToken, iskworks_esi::EsiError> {
        panic!("public market refresh only calls the regional endpoint")
    }

    async fn assets(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<iskworks_esi::EsiResponse<iskworks_esi::AssetObservation>, iskworks_esi::EsiError>
    {
        panic!("public market refresh only calls the regional endpoint")
    }

    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        _from_id: Option<i64>,
        _etag: Option<&str>,
    ) -> Result<
        iskworks_esi::EsiResponse<iskworks_esi::WalletTransactionObservation>,
        iskworks_esi::EsiError,
    > {
        panic!("public market refresh only calls the regional endpoint")
    }

    async fn structure(
        &self,
        _access_token: &str,
        _structure_id: i64,
    ) -> Result<iskworks_esi::StructureInformation, iskworks_esi::EsiError> {
        panic!("public market refresh only calls the regional endpoint")
    }

    async fn industry_systems(
        &self,
    ) -> Result<
        iskworks_esi::EsiResponse<iskworks_esi::IndustrySystemCostIndex>,
        iskworks_esi::EsiError,
    > {
        panic!("public market refresh only calls the regional endpoint")
    }

    async fn regional_market_orders(
        &self,
        _region_id: i64,
        type_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<
        iskworks_esi::EsiResponse<iskworks_esi::MarketOrderObservation>,
        iskworks_esi::EsiError,
    > {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(iskworks_esi::EsiResponse {
            records: vec![iskworks_esi::MarketOrderObservation {
                order_id: 9_001,
                type_id,
                location_id: 60_003_760,
                system_id: 30_000_142,
                is_buy_order: false,
                price: rust_decimal::Decimal::new(425, 2),
                volume_remain: 100,
                volume_total: 100,
                min_volume: 1,
                order_range: "station".to_string(),
                issued_at: chrono::Utc::now(),
                duration_days: 90,
            }],
            not_modified: false,
            metadata: iskworks_esi::EsiResponseMetadata {
                pages: Some(1),
                etag: Some("forge-34".to_string()),
                ..iskworks_esi::EsiResponseMetadata::default()
            },
        })
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn the_market_pass_refreshes_due_public_coverage_app_wide(pool: sqlx::PgPool) {
    let market = std::sync::Arc::new(iskworks_storage::PgMarketRepository::new(pool.clone()));
    market
        .register_public_market_demand(
            10_000_002,
            vec![iskworks_core::MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }],
            true,
            chrono::Utc::now(),
        )
        .await
        .unwrap();
    let transport = std::sync::Arc::new(RegionalBookTransport::default());
    let worker = super::EvidenceWorker::new(
        WorkerConfig::default(),
        std::sync::Arc::new(iskworks_storage::PgEsiRepository::new(pool.clone())),
        market,
        std::sync::Arc::clone(&transport),
    );

    let refreshed = worker
        .refresh_due_market(chrono::Utc::now(), &CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(refreshed, 1);
    assert_eq!(
        transport.calls.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "one regional call for the one due (region, type)"
    );
    let (state, app_wide, orders): (String, bool, i64) = sqlx::query_as(
        r#"SELECT c.refresh_state, b.workspace_id IS NULL,
                      (SELECT count(*) FROM market_order_observations o
                        WHERE o.observation_batch_id = b.id)
               FROM public_market_coverage c
               JOIN market_observation_batches b ON b.id = c.last_completed_batch_id
               WHERE c.region_id = 10000002 AND c.type_id = 34"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "current");
    assert!(app_wide, "stored app-wide");
    assert_eq!(orders, 1);
}

#[derive(Default, Clone)]
struct FakeCharacterTransport;

#[async_trait::async_trait]
impl iskworks_esi::EsiTransport for FakeCharacterTransport {
    async fn exchange_code(
        &self,
        _code: &str,
        _verifier: &iskworks_esi::PkceVerifier,
    ) -> Result<iskworks_esi::AuthenticatedToken, iskworks_esi::EsiError> {
        Err(iskworks_esi::EsiError::PermanentFailure)
    }

    async fn refresh(
        &self,
        _refresh_token: &str,
    ) -> Result<iskworks_esi::RefreshedToken, iskworks_esi::EsiError> {
        Err(iskworks_esi::EsiError::PermanentFailure)
    }

    async fn assets(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<iskworks_esi::EsiResponse<iskworks_esi::AssetObservation>, iskworks_esi::EsiError>
    {
        Err(iskworks_esi::EsiError::PermanentFailure)
    }

    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        _from_id: Option<i64>,
        _etag: Option<&str>,
    ) -> Result<
        iskworks_esi::EsiResponse<iskworks_esi::WalletTransactionObservation>,
        iskworks_esi::EsiError,
    > {
        Err(iskworks_esi::EsiError::PermanentFailure)
    }

    async fn wallet_balance(
        &self,
        _access_token: &str,
        _character_id: i64,
    ) -> Result<
        iskworks_esi::EsiResponse<iskworks_esi::WalletBalanceObservation>,
        iskworks_esi::EsiError,
    > {
        Ok(iskworks_esi::EsiResponse {
            records: vec![iskworks_esi::WalletBalanceObservation {
                balance: "1000000.0000".parse().unwrap(),
            }],
            not_modified: false,
            metadata: iskworks_esi::EsiResponseMetadata::default(),
        })
    }

    async fn structure(
        &self,
        _access_token: &str,
        _structure_id: i64,
    ) -> Result<iskworks_esi::StructureInformation, iskworks_esi::EsiError> {
        Err(iskworks_esi::EsiError::PermanentFailure)
    }

    async fn industry_systems(
        &self,
    ) -> Result<
        iskworks_esi::EsiResponse<iskworks_esi::IndustrySystemCostIndex>,
        iskworks_esi::EsiError,
    > {
        Err(iskworks_esi::EsiError::PermanentFailure)
    }

    async fn character_public_info(
        &self,
        character_id: i64,
    ) -> Result<iskworks_esi::EsiResponse<iskworks_esi::CharacterPublicInfo>, iskworks_esi::EsiError>
    {
        Ok(iskworks_esi::EsiResponse {
            records: vec![iskworks_esi::CharacterPublicInfo {
                character_id,
                name: "Worker Test Pilot".to_string(),
                corporation_id: 98_000_001,
                security_status: None,
            }],
            not_modified: false,
            metadata: iskworks_esi::EsiResponseMetadata::default(),
        })
    }
}

struct FakeTokenProvider {
    connection: iskworks_core::ConnectedCharacter,
}

#[async_trait::async_trait]
impl iskworks_app::AccessTokenProvider for FakeTokenProvider {
    async fn valid_access_token(
        &self,
        _connection_id: iskworks_core::ConnectedCharacterId,
    ) -> Result<(iskworks_core::ConnectedCharacter, String), iskworks_app::EsiApplicationError>
    {
        Ok((self.connection.clone(), "fake-access-token".to_string()))
    }
}

struct FakeEsiSyncDispatcher;

#[async_trait::async_trait]
impl iskworks_app::EsiSyncDispatcher for FakeEsiSyncDispatcher {
    async fn sync_assets(
        &self,
        connection: &iskworks_core::ConnectedCharacter,
        _token: &str,
    ) -> Result<iskworks_core::EsiSyncRun, iskworks_app::EsiApplicationError> {
        Ok(fake_sync_run(
            connection,
            iskworks_core::EsiSyncKind::Assets,
        ))
    }

    async fn sync_wallet_transactions(
        &self,
        connection: &iskworks_core::ConnectedCharacter,
        _token: &str,
    ) -> Result<iskworks_core::EsiSyncRun, iskworks_app::EsiApplicationError> {
        Ok(fake_sync_run(
            connection,
            iskworks_core::EsiSyncKind::WalletTransactions,
        ))
    }
}

fn fake_sync_run(
    connection: &iskworks_core::ConnectedCharacter,
    kind: iskworks_core::EsiSyncKind,
) -> iskworks_core::EsiSyncRun {
    iskworks_core::EsiSyncRun {
        id: iskworks_core::EsiSyncRunId::new(),
        connection_id: connection.id,
        requested_kind: kind,
        status: iskworks_core::EsiSyncStatus::Succeeded,
        phase: "Complete".to_string(),
        started_at: chrono::Utc::now(),
        completed_at: Some(chrono::Utc::now()),
        cache_expires_at: None,
        imported_count: 0,
        unchanged_count: 0,
        skipped_count: 0,
        error_count: 0,
        error_code: None,
        summary: "fixture sync".to_string(),
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn worker_syncs_every_due_character_connection_exactly_once(pool: sqlx::PgPool) {
    use iskworks_core::{ConnectedCharacterId, MarketRefreshState, OwnerId, WorkspaceId};

    let now = chrono::Utc::now();
    let workspace_id = WorkspaceId::new();
    let owner_id = OwnerId::new();
    // workspaces.owner_id and owners.workspace_id are mutually
    // referencing FKs, deferred within one transaction -- an
    // autocommitted first INSERT would fail the constraint immediately.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, display_name, owner_id, created_at, updated_at) \
             VALUES ($1, 'Worker Test', $2, $3, $3)",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO owners (id, workspace_id, owner_kind, display_name, hidden, created_at, updated_at) \
             VALUES ($1, $2, 'manual', 'Worker Test', false, $3, $3)",
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
             VALUES ($1,$2,$3,$4,$5,'connected',$7,$6,$6,1)",
    )
    .bind(connection_id.0)
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(2_119_555_001_i64)
    .bind("Worker Test Pilot")
    .bind(now)
    .bind([iskworks_esi::WALLET_SCOPE])
    .execute(&pool)
    .await
    .unwrap();

    let esi_repository = std::sync::Arc::new(iskworks_storage::PgEsiRepository::new(pool.clone()));
    esi_repository
        .register_character_sources(connection_id)
        .await
        .unwrap();
    let connection = esi_repository.get_connection(connection_id).await.unwrap();

    let character_sync = iskworks_app::CharacterSyncService::new(
        esi_repository.clone(),
        std::sync::Arc::new(FakeCharacterTransport),
        std::sync::Arc::new(FakeTokenProvider { connection }),
        std::sync::Arc::new(FakeEsiSyncDispatcher),
    );
    let market_repository = std::sync::Arc::new(iskworks_storage::PgMarketRepository::new(pool));
    let worker = super::EvidenceWorker::new(
        WorkerConfig::default(),
        esi_repository.clone(),
        market_repository,
        std::sync::Arc::new(PanicTransport),
    )
    .with_character_sync(Some(character_sync));

    let count = worker
        .refresh_due_character_sources(chrono::Utc::now(), &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(count, 1, "one distinct due connection should be synced");

    let state = esi_repository
        .character_source_state(connection_id)
        .await
        .unwrap();
    let info_row = state
        .iter()
        .find(|row| row.source_kind == iskworks_core::CharacterSourceKind::CharacterInfo)
        .expect("character_info row should exist");
    assert_eq!(info_row.refresh_state, MarketRefreshState::Current);
    let wallet_row = state
        .iter()
        .find(|row| row.source_kind == iskworks_core::CharacterSourceKind::Wallet)
        .expect("wallet row should exist");
    assert_eq!(wallet_row.refresh_state, MarketRefreshState::Current);

    // No granted scopes means location/skills/industry-jobs correctly
    // failed rather than being silently skipped -- proves the worker
    // pass reaches every source, not just the ones with no scope gate.
    let location_row = state
        .iter()
        .find(|row| row.source_kind == iskworks_core::CharacterSourceKind::Location)
        .expect("location row should exist");
    assert_eq!(location_row.refresh_state, MarketRefreshState::Failed);
}

/// With shutdown already in effect, the character-sync fan-out
/// dispatches nothing -- no per-connection task is spawned, so a due
/// connection is left completely untouched (not synced, not failed) for
/// the next post-restart pass.
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn character_sync_fan_out_stops_queued_connections_on_cancel(pool: sqlx::PgPool) {
    use iskworks_core::{ConnectedCharacterId, MarketRefreshState, OwnerId, WorkspaceId};

    let now = chrono::Utc::now();
    let workspace_id = WorkspaceId::new();
    let owner_id = OwnerId::new();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, display_name, owner_id, created_at, updated_at) \
             VALUES ($1, 'Worker Test', $2, $3, $3)",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO owners (id, workspace_id, owner_kind, display_name, hidden, created_at, updated_at) \
             VALUES ($1, $2, 'manual', 'Worker Test', false, $3, $3)",
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
             VALUES ($1,$2,$3,$4,$5,'connected',$7,$6,$6,1)",
    )
    .bind(connection_id.0)
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(2_119_555_002_i64)
    .bind("Worker Test Pilot")
    .bind(now)
    .bind([iskworks_esi::WALLET_SCOPE])
    .execute(&pool)
    .await
    .unwrap();

    let esi_repository = std::sync::Arc::new(iskworks_storage::PgEsiRepository::new(pool.clone()));
    esi_repository
        .register_character_sources(connection_id)
        .await
        .unwrap();
    let connection = esi_repository.get_connection(connection_id).await.unwrap();

    let character_sync = iskworks_app::CharacterSyncService::new(
        esi_repository.clone(),
        std::sync::Arc::new(FakeCharacterTransport),
        std::sync::Arc::new(FakeTokenProvider { connection }),
        std::sync::Arc::new(FakeEsiSyncDispatcher),
    );
    let market_repository = std::sync::Arc::new(iskworks_storage::PgMarketRepository::new(pool));
    let worker = super::EvidenceWorker::new(
        WorkerConfig::default(),
        esi_repository.clone(),
        market_repository,
        std::sync::Arc::new(PanicTransport),
    )
    .with_character_sync(Some(character_sync));

    let cancel = CancellationToken::new();
    cancel.cancel();
    let count = worker
        .refresh_due_character_sources(chrono::Utc::now(), &cancel)
        .await
        .unwrap();
    assert_eq!(count, 0, "a pre-cancelled pass dispatches no connections");

    // Every source row is still in its freshly-registered state: the
    // cancelled pass neither completed nor failed any of them.
    let state = esi_repository
        .character_source_state(connection_id)
        .await
        .unwrap();
    assert!(!state.is_empty(), "sources were registered");
    for row in &state {
        assert_ne!(
            row.refresh_state,
            MarketRefreshState::Current,
            "{:?} must not have synced under a cancelled token",
            row.source_kind
        );
        assert_ne!(
            row.refresh_state,
            MarketRefreshState::Failed,
            "{:?} must not be marked failed by shutdown",
            row.source_kind
        );
    }
}

#[test]
fn metrics_listener_is_off_unless_an_address_is_configured() {
    assert_eq!(WorkerConfig::default().metrics_addr, None);
    let config = WorkerConfig::from_lookup(|name| match name {
        "ISKWORKS_METRICS_ADDR" => Some("0.0.0.0:9100".to_string()),
        _ => None,
    })
    .unwrap();
    assert_eq!(config.metrics_addr, Some("0.0.0.0:9100".parse().unwrap()));

    let blank = WorkerConfig::from_lookup(|name| match name {
        "ISKWORKS_METRICS_ADDR" => Some("  ".to_string()),
        _ => None,
    })
    .unwrap();
    assert_eq!(blank.metrics_addr, None);

    assert!(WorkerConfig::from_lookup(|name| match name {
        "ISKWORKS_METRICS_ADDR" => Some("not-an-address".to_string()),
        _ => None,
    })
    .is_err());
}
