use std::sync::Mutex;

use async_trait::async_trait;
use iskworks_core::{AdjustedPrice, StoreError};
use iskworks_esi::{
    AssetObservation, AuthenticatedToken, EsiResponse, EsiResponseMetadata,
    IndustrySystemCostIndex, PkceVerifier, RefreshedToken, StructureInformation,
    WalletTransactionObservation,
};
use rust_decimal::Decimal;

use super::*;

/// One write the service made to the evidence store.
#[derive(Debug, Clone, PartialEq)]
enum Write {
    Registered,
    PricesCompleted {
        type_ids: Vec<i64>,
        next_refresh_at: DateTime<Utc>,
    },
    PricesFailed {
        next_refresh_at: DateTime<Utc>,
    },
    IndexCompleted {
        system_id: i64,
        cost_index: Decimal,
        next_refresh_at: DateTime<Utc>,
    },
    IndexFailed {
        system_id: i64,
        next_refresh_at: DateTime<Utc>,
        error: String,
    },
}

/// The evidence store: whether adjusted prices are due, which systems are
/// candidates and which of them this worker wins, and every write.
#[derive(Default)]
struct FakeEvidence {
    prices_due: bool,
    candidates: Vec<i64>,
    /// Candidates another worker claims first.
    lost_claims: Vec<i64>,
    writes: Mutex<Vec<Write>>,
}

impl FakeEvidence {
    fn writes(&self) -> Vec<Write> {
        self.writes.lock().unwrap().clone()
    }

    fn record(&self, write: Write) {
        self.writes.lock().unwrap().push(write);
    }
}

fn claim_at() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-10T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

#[async_trait]
impl AdjustedPriceRepository for FakeEvidence {
    async fn latest_adjusted_prices(
        &self,
        _type_ids: &[i64],
        _as_of: DateTime<Utc>,
    ) -> Result<BTreeMap<i64, Decimal>, StoreError> {
        unimplemented!("refresh never reads prices")
    }

    async fn register_adjusted_price_refresh(&self, _now: DateTime<Utc>) -> Result<(), StoreError> {
        self.record(Write::Registered);
        Ok(())
    }
}

#[async_trait]
impl EvidenceRefreshRepository for FakeEvidence {
    async fn adjusted_price_refresh_due(&self, _now: DateTime<Utc>) -> Result<bool, StoreError> {
        Ok(self.prices_due)
    }

    async fn begin_adjusted_price_refresh(
        &self,
        _attempted_at: DateTime<Utc>,
        _lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, StoreError> {
        Ok(Some(claim_at()))
    }

    async fn complete_adjusted_price_refresh(
        &self,
        claim: DateTime<Utc>,
        prices: &[AdjustedPrice],
        _observed_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        _cache_expires_at: Option<DateTime<Utc>>,
        _etag: Option<&str>,
    ) -> Result<bool, StoreError> {
        assert_eq!(claim, claim_at());
        self.record(Write::PricesCompleted {
            type_ids: prices.iter().map(|price| price.type_id).collect(),
            next_refresh_at,
        });
        Ok(true)
    }

    async fn fail_adjusted_price_refresh(
        &self,
        claim: DateTime<Utc>,
        _attempted_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        _error_message: &str,
    ) -> Result<bool, StoreError> {
        assert_eq!(claim, claim_at());
        self.record(Write::PricesFailed { next_refresh_at });
        Ok(true)
    }

    async fn save_adjusted_prices(
        &self,
        _prices: &[AdjustedPrice],
        _observed_at: DateTime<Utc>,
        _expires_at: DateTime<Utc>,
        _etag: Option<&str>,
    ) -> Result<(), StoreError> {
        unimplemented!("refresh completes its claim instead")
    }

    async fn system_cost_index_refresh_candidates(
        &self,
        _now: DateTime<Utc>,
        _limit: i64,
    ) -> Result<Vec<i64>, StoreError> {
        Ok(self.candidates.clone())
    }

    async fn begin_system_cost_index_refresh(
        &self,
        solar_system_id: i64,
        _attempted_at: DateTime<Utc>,
        _lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, StoreError> {
        Ok((!self.lost_claims.contains(&solar_system_id)).then(claim_at))
    }

    async fn complete_system_cost_index_refresh(
        &self,
        solar_system_id: i64,
        claim: DateTime<Utc>,
        cost_index: Decimal,
        _observed_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        _cache_expires_at: Option<DateTime<Utc>>,
        _etag: Option<&str>,
    ) -> Result<bool, StoreError> {
        assert_eq!(claim, claim_at());
        self.record(Write::IndexCompleted {
            system_id: solar_system_id,
            cost_index,
            next_refresh_at,
        });
        Ok(true)
    }

    async fn fail_system_cost_index_refresh(
        &self,
        solar_system_id: i64,
        claim: DateTime<Utc>,
        _attempted_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        error_message: &str,
    ) -> Result<bool, StoreError> {
        assert_eq!(claim, claim_at());
        self.record(Write::IndexFailed {
            system_id: solar_system_id,
            next_refresh_at,
            error: error_message.to_string(),
        });
        Ok(true)
    }
}

/// Answers `/markets/prices/` and `/industry/systems/` with canned results
/// and counts the requests; everything else panics.
struct EvidenceTransport {
    prices: Result<Vec<AdjustedPrice>, EsiError>,
    systems: Result<Vec<IndustrySystemCostIndex>, EsiError>,
    requests: Mutex<usize>,
}

impl EvidenceTransport {
    fn new(
        prices: Result<Vec<AdjustedPrice>, EsiError>,
        systems: Result<Vec<IndustrySystemCostIndex>, EsiError>,
    ) -> Self {
        Self {
            prices,
            systems,
            requests: Mutex::new(0),
        }
    }

    fn requests(&self) -> usize {
        *self.requests.lock().unwrap()
    }
}

fn response<T>(records: Vec<T>) -> EsiResponse<T> {
    EsiResponse {
        records,
        not_modified: false,
        metadata: EsiResponseMetadata::default(),
    }
}

#[async_trait]
impl EsiTransport for EvidenceTransport {
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
    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        _from_id: Option<i64>,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<WalletTransactionObservation>, EsiError> {
        unimplemented!()
    }
    async fn structure(
        &self,
        _access_token: &str,
        _structure_id: i64,
    ) -> Result<StructureInformation, EsiError> {
        unimplemented!()
    }
    async fn industry_systems(&self) -> Result<EsiResponse<IndustrySystemCostIndex>, EsiError> {
        *self.requests.lock().unwrap() += 1;
        self.systems.clone().map(response)
    }
    async fn market_prices(&self) -> Result<EsiResponse<AdjustedPrice>, EsiError> {
        *self.requests.lock().unwrap() += 1;
        self.prices.clone().map(response)
    }
}

fn price(type_id: i64) -> AdjustedPrice {
    AdjustedPrice {
        type_id,
        adjusted_price: Decimal::ONE,
    }
}

fn system(solar_system_id: i64, manufacturing: i64) -> IndustrySystemCostIndex {
    IndustrySystemCostIndex {
        solar_system_id,
        manufacturing: Decimal::new(manufacturing, 3),
        reaction: Decimal::ZERO,
    }
}

fn service(
    store: &Arc<FakeEvidence>,
    transport: &Arc<EvidenceTransport>,
) -> EvidenceRefreshService {
    EvidenceRefreshService::new(store.clone(), transport.clone())
}

#[tokio::test]
async fn adjusted_prices_that_are_not_due_make_no_esi_request() {
    let store = Arc::new(FakeEvidence::default());
    let transport = Arc::new(EvidenceTransport::new(Ok(vec![price(34)]), Ok(vec![])));

    let refreshed = service(&store, &transport)
        .refresh_due_adjusted_prices(claim_at())
        .await
        .unwrap();

    assert!(!refreshed);
    assert_eq!(transport.requests(), 0);
    assert_eq!(store.writes(), [Write::Registered]);
}

#[tokio::test]
async fn due_adjusted_prices_are_fetched_and_stay_current_for_the_freshness_window() {
    let store = Arc::new(FakeEvidence {
        prices_due: true,
        ..Default::default()
    });
    let transport = Arc::new(EvidenceTransport::new(
        Ok(vec![price(34), price(35)]),
        Ok(vec![]),
    ));
    let now = claim_at();

    let refreshed = service(&store, &transport)
        .refresh_due_adjusted_prices(now)
        .await
        .unwrap();

    assert!(refreshed);
    assert_eq!(
        store.writes(),
        [
            Write::Registered,
            Write::PricesCompleted {
                type_ids: vec![34, 35],
                next_refresh_at: now + chrono::Duration::hours(6),
            }
        ]
    );
}

/// A failed fetch releases the claim no sooner than ESI's `Retry-After`,
/// which here is longer than the configured retry delay.
#[tokio::test]
async fn a_rate_limited_price_fetch_fails_the_claim_until_esi_allows_a_retry() {
    let store = Arc::new(FakeEvidence {
        prices_due: true,
        ..Default::default()
    });
    let transport = Arc::new(EvidenceTransport::new(
        Err(EsiError::RateLimited {
            retry_after_seconds: Some(300),
        }),
        Ok(vec![]),
    ));
    let now = claim_at();

    let result = service(&store, &transport)
        .refresh_due_adjusted_prices(now)
        .await;

    assert!(result.is_err());
    assert_eq!(
        store.writes(),
        [
            Write::Registered,
            Write::PricesFailed {
                next_refresh_at: now + chrono::Duration::minutes(5),
            }
        ]
    );
}

/// One `/industry/systems/` fetch serves every claimed system. A system
/// another worker claimed is left alone, and one ESI didn't return is
/// failed rather than completed with a made-up index.
#[tokio::test]
async fn one_system_fetch_completes_claimed_systems_and_fails_omitted_ones() {
    let (priced, omitted, taken) = (30_000_142, 30_000_144, 30_002_187);
    let store = Arc::new(FakeEvidence {
        candidates: vec![priced, omitted, taken],
        lost_claims: vec![taken],
        ..Default::default()
    });
    let transport = Arc::new(EvidenceTransport::new(
        Ok(vec![]),
        Ok(vec![system(priced, 42), system(taken, 7)]),
    ));
    let now = claim_at();
    let next_refresh_at = now + chrono::Duration::hours(1);

    let claimed = service(&store, &transport)
        .refresh_due_system_indices(now)
        .await
        .unwrap();

    assert_eq!(claimed, 2);
    assert_eq!(transport.requests(), 1);
    assert_eq!(
        store.writes(),
        [
            Write::IndexCompleted {
                system_id: priced,
                cost_index: Decimal::new(42, 3),
                next_refresh_at,
            },
            Write::IndexFailed {
                system_id: omitted,
                next_refresh_at,
                error: "ESI response omitted the registered solar system".to_string(),
            },
        ]
    );
}

#[tokio::test]
async fn no_claimed_systems_make_no_esi_request() {
    let store = Arc::new(FakeEvidence {
        candidates: vec![30_000_142],
        lost_claims: vec![30_000_142],
        ..Default::default()
    });
    let transport = Arc::new(EvidenceTransport::new(Ok(vec![]), Ok(vec![])));

    let claimed = service(&store, &transport)
        .refresh_due_system_indices(claim_at())
        .await
        .unwrap();

    assert_eq!(claimed, 0);
    assert_eq!(transport.requests(), 0);
    assert!(store.writes().is_empty());
}

#[test]
fn retry_floor_is_the_longer_of_the_retry_delay_and_esis_wait() {
    let delay = Duration::from_secs(60);
    assert_eq!(
        retry_floor(delay, &EsiError::PermanentFailure),
        chrono::Duration::minutes(1)
    );
    assert_eq!(
        retry_floor(
            delay,
            &EsiError::RateLimited {
                retry_after_seconds: Some(10),
            }
        ),
        chrono::Duration::minutes(1)
    );
    assert_eq!(
        retry_floor(
            delay,
            &EsiError::RateLimited {
                retry_after_seconds: Some(900),
            }
        ),
        chrono::Duration::minutes(15)
    );
}
