//! Refresh bookkeeping for ESI-backed data, as ports: each character
//! source's sync state, and the shared adjusted-price and system cost index
//! evidence the worker keeps current. Storage implements both for
//! `PgEsiRepository`.
//!
//! Both follow the same claim protocol: `begin_*` claims a due row with a
//! lease and returns the claim (its `last_attempted_at`), or `None` when
//! the row isn't due or another worker holds it. `complete_*` and `fail_*`
//! take that claim back and return `false` when it was lost in the
//! meantime, writing nothing.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;

use crate::{AdjustedPrice, CharacterSourceKind, ConnectedCharacterId, StoreError};

#[async_trait]
pub trait CharacterSourceRefreshRepository: Send + Sync {
    /// Gives the connection a sync-state row for every source kind; rows
    /// that already exist are left alone.
    async fn register_character_sources(
        &self,
        connection_id: ConnectedCharacterId,
    ) -> Result<(), StoreError>;

    async fn begin_character_source_refresh(
        &self,
        connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, StoreError>;

    async fn complete_character_source_refresh(
        &self,
        connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        summary: Value,
        observed_at: DateTime<Utc>,
    ) -> Result<bool, StoreError>;

    /// `next_refresh_at` is the caller's floor (it may carry ESI's
    /// `Retry-After`); consecutive failures back off on top, per
    /// `RetryPolicy::CHARACTER_SOURCE`.
    async fn fail_character_source_refresh(
        &self,
        connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        claim: DateTime<Utc>,
        attempted_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        error_message: String,
    ) -> Result<bool, StoreError>;

    /// Up to `limit` sources due by `now`, oldest first, from connections
    /// that can actually sync: connected, not flagged for reconnection, and
    /// not belonging to a disabled user.
    async fn due_character_sources(
        &self,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<(ConnectedCharacterId, CharacterSourceKind)>, StoreError>;

    /// Pushes this connection's due-or-sooner sources out to `until`, for a
    /// transient failure that affects all of them (the token refresh). Rows
    /// already scheduled later, or mid-refresh, are left alone.
    async fn defer_character_sources(
        &self,
        connection_id: ConnectedCharacterId,
        until: DateTime<Utc>,
    ) -> Result<(), StoreError>;
}

/// The worker's side of the shared adjusted-price and system cost index
/// evidence. Registering demand and reading the latest values is
/// `AdjustedPriceRepository`'s.
#[async_trait]
pub trait EvidenceRefreshRepository: Send + Sync {
    async fn adjusted_price_refresh_due(&self, now: DateTime<Utc>) -> Result<bool, StoreError>;

    async fn begin_adjusted_price_refresh(
        &self,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, StoreError>;

    /// Stores the prices and marks the refresh current until
    /// `next_refresh_at`, or until ESI's `cache_expires_at` if later.
    async fn complete_adjusted_price_refresh(
        &self,
        claim: DateTime<Utc>,
        prices: &[AdjustedPrice],
        observed_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        cache_expires_at: Option<DateTime<Utc>>,
        etag: Option<&str>,
    ) -> Result<bool, StoreError>;

    /// `next_refresh_at` is the caller's floor; consecutive failures back
    /// off on top, per `RetryPolicy::ESI_EVIDENCE`.
    async fn fail_adjusted_price_refresh(
        &self,
        claim: DateTime<Utc>,
        attempted_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        error_message: &str,
    ) -> Result<bool, StoreError>;

    /// Stores adjusted prices fetched outside the worker's refresh (an
    /// on-demand pricing request), with no claim and no state change.
    async fn save_adjusted_prices(
        &self,
        prices: &[AdjustedPrice],
        observed_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        etag: Option<&str>,
    ) -> Result<(), StoreError>;

    /// Up to `limit` registered solar systems due by `now`, prioritized
    /// ones first.
    async fn system_cost_index_refresh_candidates(
        &self,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<i64>, StoreError>;

    async fn begin_system_cost_index_refresh(
        &self,
        solar_system_id: i64,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, StoreError>;

    /// Stores the system's manufacturing cost index and marks it current,
    /// as `complete_adjusted_price_refresh` does.
    #[allow(clippy::too_many_arguments)]
    async fn complete_system_cost_index_refresh(
        &self,
        solar_system_id: i64,
        claim: DateTime<Utc>,
        cost_index: Decimal,
        observed_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        cache_expires_at: Option<DateTime<Utc>>,
        etag: Option<&str>,
    ) -> Result<bool, StoreError>;

    /// Backs off as `fail_adjusted_price_refresh` does.
    async fn fail_system_cost_index_refresh(
        &self,
        solar_system_id: i64,
        claim: DateTime<Utc>,
        attempted_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        error_message: &str,
    ) -> Result<bool, StoreError>;
}
