use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::esi_service::{MarketAccessResolution, MarketAccessResolver};
use async_trait::async_trait;
use chrono::Utc;
use iskworks_core::{
    ConnectedCharacterId, EsiMarketObservationBatch, MarketCoverageItem,
    MarketCoverageRegistration, MarketLocationClassification, MarketRefreshState, MarketScope,
    PriceSourceId, WorkspaceId,
};
use iskworks_esi::{
    AdjustedPrice, AssetObservation, AuthenticatedToken, BlueprintAssetObservation, EsiError,
    EsiResponse, EsiResponseMetadata, EsiTransport, IndustrySystemCostIndex,
    MarketOrderObservation, PkceVerifier, RefreshedToken, StructureInformation,
    WalletTransactionObservation,
};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::fmt::MakeWriter;

use super::{MarketRefreshOutcome, MarketRefreshRepository, PublicMarketService};

/// A process-lifetime token that is never cancelled -- for tests
/// exercising the normal (non-shutdown) path. `&'static` so it can be
/// passed straight into `tokio::spawn`ed / joined futures.
fn no_cancel() -> &'static CancellationToken {
    static TOKEN: std::sync::OnceLock<CancellationToken> = std::sync::OnceLock::new();
    TOKEN.get_or_init(CancellationToken::new)
}

const JITA_REGION_ID: i64 = 10_000_002;
const JITA_LOCATION_ID: i64 = 60_003_760;
const JITA_SCOPE: MarketScope = MarketScope {
    region_id: JITA_REGION_ID,
    location_id: Some(JITA_LOCATION_ID),
};
const RENS_REGION_ID: i64 = 10_000_030;
const RENS_LOCATION_ID: i64 = 60_004_588;
const RENS_SCOPE: MarketScope = MarketScope {
    region_id: RENS_REGION_ID,
    location_id: Some(RENS_LOCATION_ID),
};

#[derive(Clone, Default)]
struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

struct CapturedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for CapturedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'writer> MakeWriter<'writer> for CapturedLogs {
    type Writer = CapturedWriter;

    fn make_writer(&'writer self) -> Self::Writer {
        CapturedWriter(self.0.clone())
    }
}

impl CapturedLogs {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

struct PageTransport {
    pages: Mutex<VecDeque<Result<EsiResponse<MarketOrderObservation>, EsiError>>>,
    calls: AtomicUsize,
    requested_regions: Mutex<Vec<i64>>,
    /// `If-None-Match` value seen on each `regional_market_orders` call
    /// (in call order), so a conditional-refresh test can assert what
    /// was (or was not) sent.
    requested_etags: Mutex<Vec<Option<String>>>,
    structure_pages: Mutex<VecDeque<Result<EsiResponse<MarketOrderObservation>, EsiError>>>,
    structure_calls: AtomicUsize,
    requested_structures: Mutex<Vec<(i64, i64, u32)>>,
}

impl PageTransport {
    fn new(pages: Vec<Result<EsiResponse<MarketOrderObservation>, EsiError>>) -> Arc<Self> {
        Arc::new(Self {
            pages: Mutex::new(pages.into()),
            calls: AtomicUsize::new(0),
            requested_regions: Mutex::new(Vec::new()),
            requested_etags: Mutex::new(Vec::new()),
            structure_pages: Mutex::new(VecDeque::new()),
            structure_calls: AtomicUsize::new(0),
            requested_structures: Mutex::new(Vec::new()),
        })
    }

    fn requested_etags(&self) -> Vec<Option<String>> {
        self.requested_etags.lock().unwrap().clone()
    }

    /// A `PageTransport` whose `structure_market_orders` responses (used
    /// only for pages *beyond* the first -- page 1 always comes from the
    /// resolver's `first_page`, never re-fetched) are `pages`.
    fn with_structure_pages(
        pages: Vec<Result<EsiResponse<MarketOrderObservation>, EsiError>>,
    ) -> Arc<Self> {
        let transport = Self::new(Vec::new());
        *transport.structure_pages.lock().unwrap() = pages.into();
        transport
    }
}

#[async_trait]
impl EsiTransport for PageTransport {
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

    async fn market_prices(&self) -> Result<EsiResponse<AdjustedPrice>, EsiError> {
        Err(EsiError::PermanentFailure)
    }

    async fn regional_market_orders(
        &self,
        region_id: i64,
        _type_id: i64,
        _page: u32,
        etag: Option<&str>,
    ) -> Result<EsiResponse<MarketOrderObservation>, EsiError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.requested_regions.lock().unwrap().push(region_id);
        self.requested_etags
            .lock()
            .unwrap()
            .push(etag.map(str::to_string));
        self.pages.lock().unwrap().pop_front().unwrap()
    }

    async fn structure_market_orders(
        &self,
        _access_token: &str,
        structure_id: i64,
        solar_system_id: i64,
        page: u32,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<MarketOrderObservation>, EsiError> {
        self.structure_calls.fetch_add(1, Ordering::SeqCst);
        self.requested_structures
            .lock()
            .unwrap()
            .push((structure_id, solar_system_id, page));
        self.structure_pages.lock().unwrap().pop_front().unwrap()
    }
}

/// `(type_id, claim, next_refresh_at)` for one `revalidate` call.
type RevalidateCall = (
    i64,
    chrono::DateTime<Utc>,
    chrono::DateTime<Utc>,
    Option<chrono::DateTime<Utc>>,
);

struct RecordingRepository {
    // Per-`type_id` leasing: a type is fetched at most once until its
    // lease is released, but distinct types lease independently -- so a
    // multi-candidate concurrency test actually fans out. (A single
    // shared flag would only ever let one candidate through.)
    leased: Mutex<std::collections::HashSet<i64>>,
    // `(type_id, lease_expires_at)` for every successful `begin` -- a
    // cancelled refresh leaves the row `refreshing`, so a test asserts
    // the lease it holds carries a finite deadline (hence recoverable,
    // never permanently stuck).
    lease_deadlines: Mutex<Vec<(i64, chrono::DateTime<Utc>)>>,
    candidates: Mutex<Vec<MarketCoverageItem>>,
    completed: Mutex<Vec<EsiMarketObservationBatch>>,
    failures: Mutex<Vec<String>>,
    // `(attempted_at, next_refresh_at)` recorded on every `fail` call.
    fail_schedule: Mutex<Vec<(chrono::DateTime<Utc>, chrono::DateTime<Utc>)>>,
    // `(type_id, claim, next_refresh_at)` recorded on every `revalidate` call.
    revalidations: Mutex<Vec<RevalidateCall>>,
    revalidate_should_fail: AtomicBool,
    complete_should_fail: AtomicBool,
    scope: MarketScope,
    prioritize_calls: Mutex<Vec<Vec<i64>>>,
    // Extra rows `register` appends to its returned coverage beyond the
    // caller's own `items` -- unused by default (the real Postgres
    // repository never returns *fewer* than this, only more, which is
    // exactly the shape `register_and_refresh`'s own filtering needs to
    // be tested against).
    register_extra: Mutex<Vec<MarketCoverageItem>>,
    // App-wide lifecycle (`public_market_coverage`), keyed by
    // `(region_id, type_id)`.
    public_leased: Mutex<std::collections::HashSet<(i64, i64)>>,
    public_completed: Mutex<Vec<iskworks_core::PublicMarketObservationBatch>>,
    public_revalidations: Mutex<Vec<(i64, i64)>>,
    public_failures: Mutex<Vec<(i64, i64, String)>>,
    // `(region_id, type_ids, prioritize)` per `register_public_demand`.
    public_demand: Mutex<Vec<(i64, Vec<i64>, bool)>>,
    public_demand_should_fail: AtomicBool,
    public_registered: Mutex<Vec<MarketCoverageRegistration>>,
    classification: MarketLocationClassification,
}

impl Default for RecordingRepository {
    fn default() -> Self {
        Self {
            leased: Mutex::default(),
            lease_deadlines: Mutex::default(),
            candidates: Mutex::default(),
            completed: Mutex::default(),
            failures: Mutex::default(),
            fail_schedule: Mutex::default(),
            revalidations: Mutex::default(),
            revalidate_should_fail: AtomicBool::default(),
            complete_should_fail: AtomicBool::default(),
            scope: JITA_SCOPE,
            prioritize_calls: Mutex::default(),
            register_extra: Mutex::default(),
            public_leased: Mutex::default(),
            public_completed: Mutex::default(),
            public_revalidations: Mutex::default(),
            public_failures: Mutex::default(),
            public_demand: Mutex::default(),
            public_demand_should_fail: AtomicBool::default(),
            public_registered: Mutex::default(),
            classification: MarketLocationClassification::NpcStation,
        }
    }
}

#[async_trait]
impl MarketRefreshRepository for RecordingRepository {
    async fn scope(
        &self,
        _workspace_id: WorkspaceId,
        _source_id: PriceSourceId,
    ) -> Result<MarketScope, iskworks_core::MarketError> {
        Ok(self.scope)
    }

    async fn register(
        &self,
        _workspace_id: WorkspaceId,
        _source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<Vec<MarketCoverageItem>, iskworks_core::MarketError> {
        Ok(items
            .into_iter()
            .map(coverage)
            .chain(self.register_extra.lock().unwrap().iter().cloned())
            .collect())
    }

    async fn candidates(
        &self,
        _workspace_id: WorkspaceId,
        _source_id: PriceSourceId,
        _now: chrono::DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<MarketCoverageItem>, iskworks_core::MarketError> {
        Ok(self
            .candidates
            .lock()
            .unwrap()
            .iter()
            .take(limit.max(0) as usize)
            .cloned()
            .collect())
    }

    async fn candidates_for_type_ids(
        &self,
        _workspace_id: WorkspaceId,
        _source_id: PriceSourceId,
        type_ids: &[i64],
        _now: chrono::DateTime<Utc>,
    ) -> Result<Vec<MarketCoverageItem>, iskworks_core::MarketError> {
        Ok(self
            .candidates
            .lock()
            .unwrap()
            .iter()
            .filter(|item| type_ids.contains(&item.type_id))
            .cloned()
            .collect())
    }

    async fn prioritize(
        &self,
        _workspace_id: WorkspaceId,
        _source_id: PriceSourceId,
        type_ids: &[i64],
        _now: chrono::DateTime<Utc>,
    ) -> Result<bool, iskworks_core::MarketError> {
        self.prioritize_calls
            .lock()
            .unwrap()
            .push(type_ids.to_vec());
        Ok(true)
    }

    async fn begin(
        &self,
        _workspace_id: WorkspaceId,
        _source_id: PriceSourceId,
        type_id: i64,
        attempted_at: chrono::DateTime<Utc>,
        lease_expires_at: chrono::DateTime<Utc>,
    ) -> Result<Option<chrono::DateTime<Utc>>, iskworks_core::MarketError> {
        let claimed = self.leased.lock().unwrap().insert(type_id);
        if claimed {
            self.lease_deadlines
                .lock()
                .unwrap()
                .push((type_id, lease_expires_at));
        }
        Ok(claimed.then_some(attempted_at))
    }

    async fn complete(
        &self,
        _workspace_id: WorkspaceId,
        _claim: chrono::DateTime<Utc>,
        _next_refresh_at: chrono::DateTime<Utc>,
        batch: EsiMarketObservationBatch,
    ) -> Result<bool, iskworks_core::MarketError> {
        if self.complete_should_fail.load(Ordering::SeqCst) {
            return Err(iskworks_core::MarketError::Persistence(
                "duration_days check constraint violated".to_string(),
            ));
        }
        self.completed.lock().unwrap().push(batch);
        Ok(true)
    }

    async fn fail(
        &self,
        _workspace_id: WorkspaceId,
        _source_id: PriceSourceId,
        failure: iskworks_core::MarketRefreshFailure,
    ) -> Result<bool, iskworks_core::MarketError> {
        let iskworks_core::MarketRefreshFailure {
            attempted_at,
            next_refresh_at,
            error_message,
            ..
        } = failure;
        self.failures.lock().unwrap().push(error_message);
        self.fail_schedule
            .lock()
            .unwrap()
            .push((attempted_at, next_refresh_at));
        Ok(true)
    }

    async fn revalidate(
        &self,
        _workspace_id: WorkspaceId,
        _source_id: PriceSourceId,
        type_id: i64,
        claim: chrono::DateTime<Utc>,
        next_refresh_at: chrono::DateTime<Utc>,
        cache_expires_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<bool, iskworks_core::MarketError> {
        self.revalidations.lock().unwrap().push((
            type_id,
            claim,
            next_refresh_at,
            cache_expires_at,
        ));
        if self.revalidate_should_fail.load(Ordering::SeqCst) {
            return Err(iskworks_core::MarketError::Persistence(
                "revalidation update failed".to_string(),
            ));
        }
        Ok(true)
    }

    // These tests all exercise the public/NPC-station path
    // (`JITA_SCOPE` is a real NPC station), so `NpcStation` here keeps
    // `dispatch_refresh` routing them through `refresh_via_public_path`.
    async fn classify_location(
        &self,
        _workspace_id: WorkspaceId,
        _location_id: i64,
    ) -> Result<MarketLocationClassification, iskworks_core::MarketError> {
        Ok(self.classification)
    }

    async fn market_access_connection(
        &self,
        _workspace_id: WorkspaceId,
        _location_id: i64,
    ) -> Result<Option<ConnectedCharacterId>, iskworks_core::MarketError> {
        Ok(None)
    }

    async fn remember_market_access(
        &self,
        _workspace_id: WorkspaceId,
        _location_id: i64,
        _connection_id: ConnectedCharacterId,
        _checked_at: chrono::DateTime<Utc>,
    ) -> Result<(), iskworks_core::MarketError> {
        Ok(())
    }

    async fn clear_market_access(
        &self,
        _workspace_id: WorkspaceId,
        _location_id: i64,
        _checked_at: chrono::DateTime<Utc>,
    ) -> Result<(), iskworks_core::MarketError> {
        Ok(())
    }

    async fn register_public_demand(
        &self,
        region_id: i64,
        items: Vec<MarketCoverageRegistration>,
        prioritize: bool,
        _now: chrono::DateTime<Utc>,
    ) -> Result<(), iskworks_core::MarketError> {
        if self.public_demand_should_fail.load(Ordering::SeqCst) {
            return Err(iskworks_core::MarketError::Persistence(
                "public coverage unavailable".to_string(),
            ));
        }
        self.public_demand.lock().unwrap().push((
            region_id,
            items.iter().map(|item| item.type_id).collect(),
            prioritize,
        ));
        self.public_registered.lock().unwrap().extend(items);
        Ok(())
    }

    async fn public_coverage(
        &self,
        _region_id: i64,
        type_ids: &[i64],
    ) -> Result<Vec<MarketCoverageItem>, iskworks_core::MarketError> {
        Ok(self
            .public_registered
            .lock()
            .unwrap()
            .iter()
            .filter(|item| type_ids.contains(&item.type_id))
            .cloned()
            .map(coverage)
            .collect())
    }

    async fn begin_public(
        &self,
        region_id: i64,
        type_id: i64,
        attempted_at: chrono::DateTime<Utc>,
        _lease_expires_at: chrono::DateTime<Utc>,
    ) -> Result<Option<chrono::DateTime<Utc>>, iskworks_core::MarketError> {
        let claimed = self
            .public_leased
            .lock()
            .unwrap()
            .insert((region_id, type_id));
        Ok(claimed.then_some(attempted_at))
    }

    async fn complete_public(
        &self,
        _claim: chrono::DateTime<Utc>,
        _next_refresh_at: chrono::DateTime<Utc>,
        batch: iskworks_core::PublicMarketObservationBatch,
    ) -> Result<bool, iskworks_core::MarketError> {
        self.public_completed.lock().unwrap().push(batch);
        Ok(true)
    }

    async fn revalidate_public(
        &self,
        region_id: i64,
        type_id: i64,
        _claim: chrono::DateTime<Utc>,
        _next_refresh_at: chrono::DateTime<Utc>,
        _cache_expires_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<bool, iskworks_core::MarketError> {
        self.public_revalidations
            .lock()
            .unwrap()
            .push((region_id, type_id));
        Ok(true)
    }

    async fn fail_public(
        &self,
        region_id: i64,
        type_id: i64,
        _claim: chrono::DateTime<Utc>,
        _attempted_at: chrono::DateTime<Utc>,
        _next_refresh_at: chrono::DateTime<Utc>,
        error_message: String,
    ) -> Result<bool, iskworks_core::MarketError> {
        self.public_failures
            .lock()
            .unwrap()
            .push((region_id, type_id, error_message));
        Ok(true)
    }
}

fn coverage(item: MarketCoverageRegistration) -> MarketCoverageItem {
    MarketCoverageItem {
        type_id: item.type_id,
        type_name: item.type_name,
        refresh_state: MarketRefreshState::Missing,
        observed_at: None,
        last_attempted_at: None,
        next_refresh_at: None,
        last_error: None,
        prior_etag: None,
        revalidated_at: None,
        order_count: 0,
        buy_order_count: 0,
        sell_order_count: 0,
    }
}

fn response(
    page_count: u32,
    order_id: i64,
    location_id: i64,
) -> EsiResponse<MarketOrderObservation> {
    EsiResponse {
        records: vec![MarketOrderObservation {
            order_id,
            type_id: 34,
            location_id,
            system_id: 30_000_142,
            is_buy_order: order_id % 2 == 0,
            price: rust_decimal::Decimal::new(425, 2),
            volume_remain: 100,
            volume_total: 100,
            min_volume: 1,
            order_range: "station".to_string(),
            issued_at: Utc::now(),
            duration_days: 90,
        }],
        not_modified: false,
        metadata: EsiResponseMetadata {
            pages: Some(page_count),
            etag: Some(format!("page-{order_id}")),
            ..EsiResponseMetadata::default()
        },
    }
}

/// A `304 Not Modified` regional-market response with the given
/// `X-Pages` value (`None` = header absent, as it may be on a real 304).
fn not_modified_page(pages: Option<u32>) -> EsiResponse<MarketOrderObservation> {
    EsiResponse {
        records: Vec::new(),
        not_modified: true,
        metadata: EsiResponseMetadata {
            pages,
            expires: Some("Sat, 25 Jul 2026 12:05:00 GMT".to_string()),
            ..EsiResponseMetadata::default()
        },
    }
}

const STRUCTURE_LOCATION_ID: i64 = 1_050_487_654_321;
const STRUCTURE_SOLAR_SYSTEM_ID: i64 = 30_000_505;
const STRUCTURE_SCOPE: MarketScope = MarketScope {
    region_id: 10_000_058,
    location_id: Some(STRUCTURE_LOCATION_ID),
};

fn structure_order(order_id: i64, type_id: i64) -> MarketOrderObservation {
    MarketOrderObservation {
        order_id,
        type_id,
        location_id: STRUCTURE_LOCATION_ID,
        system_id: STRUCTURE_SOLAR_SYSTEM_ID,
        is_buy_order: false,
        price: rust_decimal::Decimal::new(500, 2),
        volume_remain: 10,
        volume_total: 10,
        min_volume: 1,
        order_range: "station".to_string(),
        issued_at: Utc::now(),
        duration_days: 90,
    }
}

fn structure_page(
    page_count: u32,
    orders: Vec<MarketOrderObservation>,
) -> EsiResponse<MarketOrderObservation> {
    EsiResponse {
        records: orders,
        not_modified: false,
        metadata: EsiResponseMetadata {
            pages: Some(page_count),
            ..EsiResponseMetadata::default()
        },
    }
}

/// Per-`type_id` leasing (unlike `RecordingRepository`'s single shared
/// lease flag, purpose-built for testing one item's concurrent-lease
/// behavior) plus the structure-market bookkeeping `RecordingRepository`
/// stubs out -- a real `MarketLocationClassification`, a configurable
/// remembered connection, and recorded remember/clear calls, so
/// structure-dispatch tests can assert on them directly.
struct StructureRepository {
    candidates: Mutex<Vec<MarketCoverageItem>>,
    leased: Mutex<std::collections::HashSet<i64>>,
    completed: Mutex<Vec<EsiMarketObservationBatch>>,
    failures: Mutex<Vec<(i64, String)>>,
    classification: MarketLocationClassification,
    preferred_connection: Mutex<Option<ConnectedCharacterId>>,
    remembered: Mutex<Vec<ConnectedCharacterId>>,
    cleared: AtomicUsize,
    scope: MarketScope,
}

impl StructureRepository {
    fn new(
        classification: MarketLocationClassification,
        candidates: Vec<MarketCoverageItem>,
    ) -> Self {
        Self {
            candidates: Mutex::new(candidates),
            leased: Mutex::default(),
            completed: Mutex::default(),
            failures: Mutex::default(),
            classification,
            preferred_connection: Mutex::default(),
            remembered: Mutex::default(),
            cleared: AtomicUsize::new(0),
            scope: STRUCTURE_SCOPE,
        }
    }
}

#[async_trait]
impl MarketRefreshRepository for StructureRepository {
    async fn scope(
        &self,
        _workspace_id: WorkspaceId,
        _source_id: PriceSourceId,
    ) -> Result<MarketScope, iskworks_core::MarketError> {
        Ok(self.scope)
    }

    async fn register(
        &self,
        _workspace_id: WorkspaceId,
        _source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<Vec<MarketCoverageItem>, iskworks_core::MarketError> {
        Ok(items.into_iter().map(coverage).collect())
    }

    async fn candidates(
        &self,
        _workspace_id: WorkspaceId,
        _source_id: PriceSourceId,
        _now: chrono::DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<MarketCoverageItem>, iskworks_core::MarketError> {
        Ok(self
            .candidates
            .lock()
            .unwrap()
            .iter()
            .take(limit.max(0) as usize)
            .cloned()
            .collect())
    }

    async fn candidates_for_type_ids(
        &self,
        _workspace_id: WorkspaceId,
        _source_id: PriceSourceId,
        type_ids: &[i64],
        _now: chrono::DateTime<Utc>,
    ) -> Result<Vec<MarketCoverageItem>, iskworks_core::MarketError> {
        Ok(self
            .candidates
            .lock()
            .unwrap()
            .iter()
            .filter(|item| type_ids.contains(&item.type_id))
            .cloned()
            .collect())
    }

    async fn prioritize(
        &self,
        _workspace_id: WorkspaceId,
        _source_id: PriceSourceId,
        _type_ids: &[i64],
        _now: chrono::DateTime<Utc>,
    ) -> Result<bool, iskworks_core::MarketError> {
        Ok(true)
    }

    async fn begin(
        &self,
        _workspace_id: WorkspaceId,
        _source_id: PriceSourceId,
        type_id: i64,
        attempted_at: chrono::DateTime<Utc>,
        _lease_expires_at: chrono::DateTime<Utc>,
    ) -> Result<Option<chrono::DateTime<Utc>>, iskworks_core::MarketError> {
        Ok(self
            .leased
            .lock()
            .unwrap()
            .insert(type_id)
            .then_some(attempted_at))
    }

    async fn complete(
        &self,
        _workspace_id: WorkspaceId,
        _claim: chrono::DateTime<Utc>,
        _next_refresh_at: chrono::DateTime<Utc>,
        batch: EsiMarketObservationBatch,
    ) -> Result<bool, iskworks_core::MarketError> {
        self.completed.lock().unwrap().push(batch);
        Ok(true)
    }

    async fn fail(
        &self,
        _workspace_id: WorkspaceId,
        _source_id: PriceSourceId,
        failure: iskworks_core::MarketRefreshFailure,
    ) -> Result<bool, iskworks_core::MarketError> {
        let iskworks_core::MarketRefreshFailure {
            type_id,
            error_message,
            ..
        } = failure;
        self.failures.lock().unwrap().push((type_id, error_message));
        Ok(true)
    }

    async fn revalidate(
        &self,
        _workspace_id: WorkspaceId,
        _source_id: PriceSourceId,
        _type_id: i64,
        _claim: chrono::DateTime<Utc>,
        _next_refresh_at: chrono::DateTime<Utc>,
        _cache_expires_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<bool, iskworks_core::MarketError> {
        // Structure-market refreshes never take the conditional path.
        Ok(true)
    }

    async fn classify_location(
        &self,
        _workspace_id: WorkspaceId,
        _location_id: i64,
    ) -> Result<MarketLocationClassification, iskworks_core::MarketError> {
        Ok(self.classification)
    }

    async fn market_access_connection(
        &self,
        _workspace_id: WorkspaceId,
        _location_id: i64,
    ) -> Result<Option<ConnectedCharacterId>, iskworks_core::MarketError> {
        Ok(*self.preferred_connection.lock().unwrap())
    }

    async fn remember_market_access(
        &self,
        _workspace_id: WorkspaceId,
        _location_id: i64,
        connection_id: ConnectedCharacterId,
        _checked_at: chrono::DateTime<Utc>,
    ) -> Result<(), iskworks_core::MarketError> {
        *self.preferred_connection.lock().unwrap() = Some(connection_id);
        self.remembered.lock().unwrap().push(connection_id);
        Ok(())
    }

    async fn clear_market_access(
        &self,
        _workspace_id: WorkspaceId,
        _location_id: i64,
        _checked_at: chrono::DateTime<Utc>,
    ) -> Result<(), iskworks_core::MarketError> {
        *self.preferred_connection.lock().unwrap() = None;
        self.cleared.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn register_public_demand(
        &self,
        _region_id: i64,
        _items: Vec<MarketCoverageRegistration>,
        _prioritize: bool,
        _now: chrono::DateTime<Utc>,
    ) -> Result<(), iskworks_core::MarketError> {
        Ok(())
    }

    async fn public_coverage(
        &self,
        _region_id: i64,
        _type_ids: &[i64],
    ) -> Result<Vec<MarketCoverageItem>, iskworks_core::MarketError> {
        Ok(Vec::new())
    }

    async fn begin_public(
        &self,
        _region_id: i64,
        _type_id: i64,
        _attempted_at: chrono::DateTime<Utc>,
        _lease_expires_at: chrono::DateTime<Utc>,
    ) -> Result<Option<chrono::DateTime<Utc>>, iskworks_core::MarketError> {
        unreachable!("structure tests never refresh public coverage")
    }

    async fn complete_public(
        &self,
        _claim: chrono::DateTime<Utc>,
        _next_refresh_at: chrono::DateTime<Utc>,
        _batch: iskworks_core::PublicMarketObservationBatch,
    ) -> Result<bool, iskworks_core::MarketError> {
        unreachable!("structure tests never refresh public coverage")
    }

    async fn revalidate_public(
        &self,
        _region_id: i64,
        _type_id: i64,
        _claim: chrono::DateTime<Utc>,
        _next_refresh_at: chrono::DateTime<Utc>,
        _cache_expires_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<bool, iskworks_core::MarketError> {
        unreachable!("structure tests never refresh public coverage")
    }

    async fn fail_public(
        &self,
        _region_id: i64,
        _type_id: i64,
        _claim: chrono::DateTime<Utc>,
        _attempted_at: chrono::DateTime<Utc>,
        _next_refresh_at: chrono::DateTime<Utc>,
        _error_message: String,
    ) -> Result<bool, iskworks_core::MarketError> {
        unreachable!("structure tests never refresh public coverage")
    }
}

/// Fake `MarketAccessResolver` -- one canned `MarketAccessResolution`
/// per call, popped in order, so tests can script exactly the
/// preferred-succeeds / preferred-denied-then-fallback-succeeds /
/// nothing-eligible / everything-denied sequences amendment 3 calls for
/// without depending on `EsiApplicationService` or real HTTP.
struct FakeMarketAccess {
    resolutions: Mutex<VecDeque<crate::esi_service::EsiApplicationError>>,
    confirmed: Mutex<VecDeque<MarketAccessResolution>>,
    calls: AtomicUsize,
    seen_preferred: Mutex<Vec<Option<ConnectedCharacterId>>>,
}

impl FakeMarketAccess {
    fn confirmed(resolutions: Vec<MarketAccessResolution>) -> Arc<Self> {
        Arc::new(Self {
            resolutions: Mutex::new(VecDeque::new()),
            confirmed: Mutex::new(resolutions.into()),
            calls: AtomicUsize::new(0),
            seen_preferred: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl MarketAccessResolver for FakeMarketAccess {
    async fn resolve_market_access(
        &self,
        _workspace_id: WorkspaceId,
        _structure_id: i64,
        _solar_system_id: i64,
        preferred_connection_id: Option<ConnectedCharacterId>,
    ) -> Result<MarketAccessResolution, crate::esi_service::EsiApplicationError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.seen_preferred
            .lock()
            .unwrap()
            .push(preferred_connection_id);
        if let Some(error) = self.resolutions.lock().unwrap().pop_front() {
            return Err(error);
        }
        Ok(self
            .confirmed
            .lock()
            .unwrap()
            .pop_front()
            .expect("FakeMarketAccess ran out of scripted resolutions"))
    }
}

mod app_wide_refresh;
mod conditional_refresh;
mod public_scope_demand;
mod rate_limit;
mod regional_refresh;
mod registration;
mod shutdown;
mod structure_scope;
mod timeouts;

// ---------------------------------------------------------------------------
// Rate-limit probe transport
// ---------------------------------------------------------------------------

/// Rate-limit probe transport for the shared-cooldown test.
///
/// The first `park_first` calls to `regional_market_orders` record their
/// (virtual-clock) instant and then block on `gate` until the test
/// releases them -- so all four can be genuinely in flight at once
/// before any 429 is returned, which the single-threaded test runtime
/// would otherwise never allow. Once released, call index 0 returns a
/// 429 carrying `Retry-After`; every other call returns a one-page Jita
/// book for the requested `type_id`.
struct RateLimitProbeTransport {
    retry_after_seconds: u64,
    park_first: usize,
    calls: Mutex<Vec<tokio::time::Instant>>,
    parked: AtomicUsize,
    gate: tokio::sync::Semaphore,
}

impl RateLimitProbeTransport {
    fn new(retry_after_seconds: u64, park_first: usize) -> Arc<Self> {
        Arc::new(Self {
            retry_after_seconds,
            park_first,
            calls: Mutex::new(Vec::new()),
            parked: AtomicUsize::new(0),
            gate: tokio::sync::Semaphore::new(0),
        })
    }

    fn call_instants(&self) -> Vec<tokio::time::Instant> {
        self.calls.lock().unwrap().clone()
    }

    fn parked_count(&self) -> usize {
        self.parked.load(Ordering::SeqCst)
    }

    fn release_parked(&self) {
        self.gate.add_permits(self.park_first);
    }
}

#[async_trait]
impl EsiTransport for RateLimitProbeTransport {
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

    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        _from_id: Option<i64>,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<WalletTransactionObservation>, EsiError> {
        Err(EsiError::PermanentFailure)
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

    async fn regional_market_orders(
        &self,
        _region_id: i64,
        type_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<MarketOrderObservation>, EsiError> {
        let index = {
            let mut calls = self.calls.lock().unwrap();
            calls.push(tokio::time::Instant::now());
            calls.len() - 1
        };
        if index < self.park_first {
            self.parked.fetch_add(1, Ordering::SeqCst);
            let _permit = self.gate.acquire().await;
        }
        if index == 0 {
            return Err(EsiError::RateLimited {
                retry_after_seconds: Some(self.retry_after_seconds),
            });
        }
        Ok(EsiResponse {
            records: vec![probe_order(1_000 + index as i64, type_id)],
            not_modified: false,
            metadata: EsiResponseMetadata {
                pages: Some(1),
                ..EsiResponseMetadata::default()
            },
        })
    }
}

/// Yields to the current-thread runtime `count` times so `tokio::join!`
/// siblings, spawned tasks, and any task woken by `tokio::time::advance`
/// all get polled through to their next suspension point.
async fn drain(count: usize) {
    for _ in 0..count {
        tokio::task::yield_now().await;
    }
}

fn probe_order(order_id: i64, type_id: i64) -> MarketOrderObservation {
    MarketOrderObservation {
        order_id,
        type_id,
        location_id: JITA_LOCATION_ID,
        system_id: 30_000_142,
        is_buy_order: false,
        price: rust_decimal::Decimal::new(425, 2),
        volume_remain: 100,
        volume_total: 100,
        min_volume: 1,
        order_range: "station".to_string(),
        issued_at: Utc::now(),
        duration_days: 90,
    }
}

// ---------------------------------------------------------------------------
// Structure-scope repository
// ---------------------------------------------------------------------------

/// A `RecordingRepository` whose scope is a resolved structure, so
/// `register_and_refresh` takes the per-workspace coverage path.
fn structure_recording_repository() -> RecordingRepository {
    RecordingRepository {
        scope: STRUCTURE_SCOPE,
        classification: MarketLocationClassification::Structure {
            solar_system_id: STRUCTURE_SOLAR_SYSTEM_ID,
        },
        ..RecordingRepository::default()
    }
}
