//! The base for test fakes of [`MarketRepository`] (`test-support` only).
//!
//! [`MarketRepository`] has no default methods, so the Postgres
//! implementation must answer every one. A fake implements
//! [`FakeMarketRepository`] instead and overrides only what its test uses:
//! the blanket impl below makes it a [`MarketRepository`], and any other
//! method panics with its name, so a test that reaches an unstubbed method
//! fails loudly instead of seeing empty data.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{ConnectedCharacterId, PriceSourceId, WorkspaceId};

use super::errors::MarketError;
use super::repository::MarketRepository;
use super::types::*;

/// [`MarketRepository`] with every method defaulted for fakes: each one
/// panics naming the fake and the method, except
/// [`Self::get_source_order_book`] and [`Self::get_source_order_books`],
/// which fall back to [`Self::get_order_book`].
#[async_trait]
pub trait FakeMarketRepository: Send + Sync {
    async fn resolve_type_name(&self, type_id: i64) -> Result<Option<String>, MarketError> {
        let _ = type_id;
        unimplemented!(
            "{} does not stub FakeMarketRepository::resolve_type_name",
            std::any::type_name::<Self>()
        )
    }
    async fn resolve_type_market_group(&self, type_id: i64) -> Result<Option<i64>, MarketError> {
        let _ = type_id;
        unimplemented!(
            "{} does not stub FakeMarketRepository::resolve_type_market_group",
            std::any::type_name::<Self>()
        )
    }
    async fn location_names(
        &self,
        workspace_id: WorkspaceId,
        location_ids: &[i64],
    ) -> Result<BTreeMap<i64, String>, MarketError> {
        let _ = (workspace_id, location_ids);
        unimplemented!(
            "{} does not stub FakeMarketRepository::location_names",
            std::any::type_name::<Self>()
        )
    }
    async fn resolve_order_location_names(
        &self,
        workspace_id: WorkspaceId,
        location_ids: &[i64],
    ) -> Result<BTreeMap<i64, String>, MarketError> {
        let _ = (workspace_id, location_ids);
        unimplemented!(
            "{} does not stub FakeMarketRepository::resolve_order_location_names",
            std::any::type_name::<Self>()
        )
    }
    async fn save_location_names(
        &self,
        workspace_id: WorkspaceId,
        locations: Vec<ResolvedMarketLocation>,
    ) -> Result<(), MarketError> {
        let _ = (workspace_id, locations);
        unimplemented!(
            "{} does not stub FakeMarketRepository::save_location_names",
            std::any::type_name::<Self>()
        )
    }
    async fn known_locations_in_region(
        &self,
        workspace_id: WorkspaceId,
        region_id: i64,
    ) -> Result<Vec<KnownMarketLocation>, MarketError> {
        let _ = (workspace_id, region_id);
        unimplemented!(
            "{} does not stub FakeMarketRepository::known_locations_in_region",
            std::any::type_name::<Self>()
        )
    }
    async fn classify_location(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
    ) -> Result<MarketLocationClassification, MarketError> {
        let _ = (workspace_id, location_id);
        unimplemented!(
            "{} does not stub FakeMarketRepository::classify_location",
            std::any::type_name::<Self>()
        )
    }
    async fn market_access_connection(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
    ) -> Result<Option<ConnectedCharacterId>, MarketError> {
        let _ = (workspace_id, location_id);
        unimplemented!(
            "{} does not stub FakeMarketRepository::market_access_connection",
            std::any::type_name::<Self>()
        )
    }
    async fn remember_market_access(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
        connection_id: ConnectedCharacterId,
        checked_at: DateTime<Utc>,
    ) -> Result<(), MarketError> {
        let _ = (workspace_id, location_id, connection_id, checked_at);
        unimplemented!(
            "{} does not stub FakeMarketRepository::remember_market_access",
            std::any::type_name::<Self>()
        )
    }
    async fn clear_market_access(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
        checked_at: DateTime<Utc>,
    ) -> Result<(), MarketError> {
        let _ = (workspace_id, location_id, checked_at);
        unimplemented!(
            "{} does not stub FakeMarketRepository::clear_market_access",
            std::any::type_name::<Self>()
        )
    }
    async fn list_known_structures(
        &self,
        workspace_id: WorkspaceId,
        query: &str,
    ) -> Result<Vec<MarketStructureListing>, MarketError> {
        let _ = (workspace_id, query);
        unimplemented!(
            "{} does not stub FakeMarketRepository::list_known_structures",
            std::any::type_name::<Self>()
        )
    }
    async fn scope_freshness(
        &self,
        workspace_id: WorkspaceId,
        scopes: &[(i64, i64)],
    ) -> Result<BTreeMap<(i64, i64), ScopeFreshness>, MarketError> {
        let _ = (workspace_id, scopes);
        unimplemented!(
            "{} does not stub FakeMarketRepository::scope_freshness",
            std::any::type_name::<Self>()
        )
    }
    async fn item_freshness(
        &self,
        workspace_id: WorkspaceId,
        scope: MarketScope,
        type_ids: &[i64],
    ) -> Result<Option<DateTime<Utc>>, MarketError> {
        let _ = (workspace_id, scope, type_ids);
        unimplemented!(
            "{} does not stub FakeMarketRepository::item_freshness",
            std::any::type_name::<Self>()
        )
    }
    async fn scoped_order_books(
        &self,
        workspace_id: WorkspaceId,
        scope: MarketScope,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<MarketOrderView>>, MarketError> {
        let _ = (workspace_id, scope, type_ids);
        unimplemented!(
            "{} does not stub FakeMarketRepository::scoped_order_books",
            std::any::type_name::<Self>()
        )
    }
    async fn resolve_scope_price_sources(
        &self,
        workspace_id: WorkspaceId,
        scope: MarketScope,
    ) -> Result<Vec<PriceSourceId>, MarketError> {
        let _ = (workspace_id, scope);
        unimplemented!(
            "{} does not stub FakeMarketRepository::resolve_scope_price_sources",
            std::any::type_name::<Self>()
        )
    }
    async fn resolve_source_scope(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<MarketScope, MarketError> {
        let _ = (workspace_id, source_id);
        unimplemented!(
            "{} does not stub FakeMarketRepository::resolve_source_scope",
            std::any::type_name::<Self>()
        )
    }
    async fn ensure_esi_price_source_for_scope(
        &self,
        workspace_id: WorkspaceId,
        scope: MarketScope,
    ) -> Result<PriceSourceId, MarketError> {
        let _ = (workspace_id, scope);
        unimplemented!(
            "{} does not stub FakeMarketRepository::ensure_esi_price_source_for_scope",
            std::any::type_name::<Self>()
        )
    }
    async fn imported_file_checksums(
        &self,
        workspace_id: WorkspaceId,
        checksums: &[String],
    ) -> Result<BTreeSet<String>, MarketError> {
        let _ = (workspace_id, checksums);
        unimplemented!(
            "{} does not stub FakeMarketRepository::imported_file_checksums",
            std::any::type_name::<Self>()
        )
    }
    async fn commit_import(
        &self,
        workspace_id: WorkspaceId,
        files: Vec<ResolvedMarketExport>,
        skipped_duplicate_files: u64,
        warnings: Vec<String>,
    ) -> Result<MarketImportBatch, MarketError> {
        let _ = (workspace_id, files, skipped_duplicate_files, warnings);
        unimplemented!(
            "{} does not stub FakeMarketRepository::commit_import",
            std::any::type_name::<Self>()
        )
    }
    async fn list_imports(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<MarketImportBatch>, MarketError> {
        let _ = workspace_id;
        unimplemented!(
            "{} does not stub FakeMarketRepository::list_imports",
            std::any::type_name::<Self>()
        )
    }
    async fn get_import(
        &self,
        workspace_id: WorkspaceId,
        batch_id: MarketImportBatchId,
    ) -> Result<MarketImportBatch, MarketError> {
        let _ = (workspace_id, batch_id);
        unimplemented!(
            "{} does not stub FakeMarketRepository::get_import",
            std::any::type_name::<Self>()
        )
    }
    async fn get_order_book(
        &self,
        workspace_id: WorkspaceId,
        type_id: i64,
        location_id: i64,
        pinned_batch_id: Option<MarketImportBatchId>,
    ) -> Result<MarketOrderBook, MarketError> {
        let _ = (workspace_id, type_id, location_id, pinned_batch_id);
        unimplemented!(
            "{} does not stub FakeMarketRepository::get_order_book",
            std::any::type_name::<Self>()
        )
    }
    async fn get_source_order_book(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_id: i64,
        location_id: i64,
        pinned_batch_id: Option<MarketImportBatchId>,
    ) -> Result<MarketOrderBook, MarketError> {
        let _ = source_id;
        self.get_order_book(workspace_id, type_id, location_id, pinned_batch_id)
            .await
    }
    async fn get_source_order_books(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        location_id: i64,
        pinned_batch_id: Option<MarketImportBatchId>,
    ) -> Result<BTreeMap<i64, MarketOrderBook>, MarketError> {
        let mut books = BTreeMap::new();
        for type_id in type_ids.iter().copied().collect::<BTreeSet<_>>() {
            match self
                .get_source_order_book(
                    workspace_id,
                    source_id,
                    type_id,
                    location_id,
                    pinned_batch_id,
                )
                .await
            {
                Ok(book) => {
                    books.insert(type_id, book);
                }
                Err(MarketError::OrdersUnavailable) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(books)
    }
    async fn get_market_price_source(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<MarketPriceSource, MarketError> {
        let _ = (workspace_id, source_id);
        unimplemented!(
            "{} does not stub FakeMarketRepository::get_market_price_source",
            std::any::type_name::<Self>()
        )
    }
    async fn register_market_coverage(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        let _ = (workspace_id, source_id, items);
        unimplemented!(
            "{} does not stub FakeMarketRepository::register_market_coverage",
            std::any::type_name::<Self>()
        )
    }
    async fn upsert_market_coverage(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<(), MarketError> {
        let _ = (workspace_id, source_id, items);
        unimplemented!(
            "{} does not stub FakeMarketRepository::upsert_market_coverage",
            std::any::type_name::<Self>()
        )
    }
    async fn market_refresh_candidates(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        let _ = (workspace_id, source_id, now, limit);
        unimplemented!(
            "{} does not stub FakeMarketRepository::market_refresh_candidates",
            std::any::type_name::<Self>()
        )
    }
    async fn market_refresh_candidates_for_type_ids(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        let _ = (workspace_id, source_id, type_ids, now);
        unimplemented!(
            "{} does not stub FakeMarketRepository::market_refresh_candidates_for_type_ids",
            std::any::type_name::<Self>()
        )
    }
    async fn prioritize_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<bool, MarketError> {
        let _ = (workspace_id, source_id, type_ids, now);
        unimplemented!(
            "{} does not stub FakeMarketRepository::prioritize_market_refresh",
            std::any::type_name::<Self>()
        )
    }
    async fn begin_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_id: i64,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, MarketError> {
        let _ = (
            workspace_id,
            source_id,
            type_id,
            attempted_at,
            lease_expires_at,
        );
        unimplemented!(
            "{} does not stub FakeMarketRepository::begin_market_refresh",
            std::any::type_name::<Self>()
        )
    }
    async fn complete_esi_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        batch: EsiMarketObservationBatch,
    ) -> Result<bool, MarketError> {
        let _ = (workspace_id, claim, next_refresh_at, batch);
        unimplemented!(
            "{} does not stub FakeMarketRepository::complete_esi_market_refresh",
            std::any::type_name::<Self>()
        )
    }
    async fn fail_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        failure: MarketRefreshFailure,
    ) -> Result<bool, MarketError> {
        let _ = (workspace_id, source_id, failure);
        unimplemented!(
            "{} does not stub FakeMarketRepository::fail_market_refresh",
            std::any::type_name::<Self>()
        )
    }
    async fn revalidate_esi_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_id: i64,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        cache_expires_at: Option<DateTime<Utc>>,
    ) -> Result<bool, MarketError> {
        let _ = (
            workspace_id,
            source_id,
            type_id,
            claim,
            next_refresh_at,
            cache_expires_at,
        );
        unimplemented!(
            "{} does not stub FakeMarketRepository::revalidate_esi_market_refresh",
            std::any::type_name::<Self>()
        )
    }
    async fn register_public_market_demand(
        &self,
        region_id: i64,
        items: Vec<MarketCoverageRegistration>,
        prioritize: bool,
        now: DateTime<Utc>,
    ) -> Result<(), MarketError> {
        let _ = (region_id, items, prioritize, now);
        unimplemented!(
            "{} does not stub FakeMarketRepository::register_public_market_demand",
            std::any::type_name::<Self>()
        )
    }
    async fn prioritize_public_market_refresh(
        &self,
        region_id: i64,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<bool, MarketError> {
        let _ = (region_id, type_ids, now);
        unimplemented!(
            "{} does not stub FakeMarketRepository::prioritize_public_market_refresh",
            std::any::type_name::<Self>()
        )
    }
    async fn public_market_coverage(
        &self,
        region_id: i64,
        type_ids: &[i64],
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        let _ = (region_id, type_ids);
        unimplemented!(
            "{} does not stub FakeMarketRepository::public_market_coverage",
            std::any::type_name::<Self>()
        )
    }
    async fn begin_public_market_refresh(
        &self,
        region_id: i64,
        type_id: i64,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, MarketError> {
        let _ = (region_id, type_id, attempted_at, lease_expires_at);
        unimplemented!(
            "{} does not stub FakeMarketRepository::begin_public_market_refresh",
            std::any::type_name::<Self>()
        )
    }
    async fn complete_public_market_refresh(
        &self,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        batch: PublicMarketObservationBatch,
    ) -> Result<bool, MarketError> {
        let _ = (claim, next_refresh_at, batch);
        unimplemented!(
            "{} does not stub FakeMarketRepository::complete_public_market_refresh",
            std::any::type_name::<Self>()
        )
    }
    async fn revalidate_public_market_refresh(
        &self,
        region_id: i64,
        type_id: i64,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        cache_expires_at: Option<DateTime<Utc>>,
    ) -> Result<bool, MarketError> {
        let _ = (region_id, type_id, claim, next_refresh_at, cache_expires_at);
        unimplemented!(
            "{} does not stub FakeMarketRepository::revalidate_public_market_refresh",
            std::any::type_name::<Self>()
        )
    }
    async fn fail_public_market_refresh(
        &self,
        region_id: i64,
        type_id: i64,
        claim: DateTime<Utc>,
        attempted_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        error_message: String,
    ) -> Result<bool, MarketError> {
        let _ = (
            region_id,
            type_id,
            claim,
            attempted_at,
            next_refresh_at,
            error_message,
        );
        unimplemented!(
            "{} does not stub FakeMarketRepository::fail_public_market_refresh",
            std::any::type_name::<Self>()
        )
    }
}

/// Every [`FakeMarketRepository`] is a [`MarketRepository`].
#[async_trait]
impl<T: FakeMarketRepository> MarketRepository for T {
    async fn resolve_type_name(&self, type_id: i64) -> Result<Option<String>, MarketError> {
        FakeMarketRepository::resolve_type_name(self, type_id).await
    }
    async fn resolve_type_market_group(&self, type_id: i64) -> Result<Option<i64>, MarketError> {
        FakeMarketRepository::resolve_type_market_group(self, type_id).await
    }
    async fn location_names(
        &self,
        workspace_id: WorkspaceId,
        location_ids: &[i64],
    ) -> Result<BTreeMap<i64, String>, MarketError> {
        FakeMarketRepository::location_names(self, workspace_id, location_ids).await
    }
    async fn resolve_order_location_names(
        &self,
        workspace_id: WorkspaceId,
        location_ids: &[i64],
    ) -> Result<BTreeMap<i64, String>, MarketError> {
        FakeMarketRepository::resolve_order_location_names(self, workspace_id, location_ids).await
    }
    async fn save_location_names(
        &self,
        workspace_id: WorkspaceId,
        locations: Vec<ResolvedMarketLocation>,
    ) -> Result<(), MarketError> {
        FakeMarketRepository::save_location_names(self, workspace_id, locations).await
    }
    async fn known_locations_in_region(
        &self,
        workspace_id: WorkspaceId,
        region_id: i64,
    ) -> Result<Vec<KnownMarketLocation>, MarketError> {
        FakeMarketRepository::known_locations_in_region(self, workspace_id, region_id).await
    }
    async fn classify_location(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
    ) -> Result<MarketLocationClassification, MarketError> {
        FakeMarketRepository::classify_location(self, workspace_id, location_id).await
    }
    async fn market_access_connection(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
    ) -> Result<Option<ConnectedCharacterId>, MarketError> {
        FakeMarketRepository::market_access_connection(self, workspace_id, location_id).await
    }
    async fn remember_market_access(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
        connection_id: ConnectedCharacterId,
        checked_at: DateTime<Utc>,
    ) -> Result<(), MarketError> {
        FakeMarketRepository::remember_market_access(
            self,
            workspace_id,
            location_id,
            connection_id,
            checked_at,
        )
        .await
    }
    async fn clear_market_access(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
        checked_at: DateTime<Utc>,
    ) -> Result<(), MarketError> {
        FakeMarketRepository::clear_market_access(self, workspace_id, location_id, checked_at).await
    }
    async fn list_known_structures(
        &self,
        workspace_id: WorkspaceId,
        query: &str,
    ) -> Result<Vec<MarketStructureListing>, MarketError> {
        FakeMarketRepository::list_known_structures(self, workspace_id, query).await
    }
    async fn scope_freshness(
        &self,
        workspace_id: WorkspaceId,
        scopes: &[(i64, i64)],
    ) -> Result<BTreeMap<(i64, i64), ScopeFreshness>, MarketError> {
        FakeMarketRepository::scope_freshness(self, workspace_id, scopes).await
    }
    async fn item_freshness(
        &self,
        workspace_id: WorkspaceId,
        scope: MarketScope,
        type_ids: &[i64],
    ) -> Result<Option<DateTime<Utc>>, MarketError> {
        FakeMarketRepository::item_freshness(self, workspace_id, scope, type_ids).await
    }
    async fn scoped_order_books(
        &self,
        workspace_id: WorkspaceId,
        scope: MarketScope,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<MarketOrderView>>, MarketError> {
        FakeMarketRepository::scoped_order_books(self, workspace_id, scope, type_ids).await
    }
    async fn resolve_scope_price_sources(
        &self,
        workspace_id: WorkspaceId,
        scope: MarketScope,
    ) -> Result<Vec<PriceSourceId>, MarketError> {
        FakeMarketRepository::resolve_scope_price_sources(self, workspace_id, scope).await
    }
    async fn resolve_source_scope(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<MarketScope, MarketError> {
        FakeMarketRepository::resolve_source_scope(self, workspace_id, source_id).await
    }
    async fn ensure_esi_price_source_for_scope(
        &self,
        workspace_id: WorkspaceId,
        scope: MarketScope,
    ) -> Result<PriceSourceId, MarketError> {
        FakeMarketRepository::ensure_esi_price_source_for_scope(self, workspace_id, scope).await
    }
    async fn imported_file_checksums(
        &self,
        workspace_id: WorkspaceId,
        checksums: &[String],
    ) -> Result<BTreeSet<String>, MarketError> {
        FakeMarketRepository::imported_file_checksums(self, workspace_id, checksums).await
    }
    async fn commit_import(
        &self,
        workspace_id: WorkspaceId,
        files: Vec<ResolvedMarketExport>,
        skipped_duplicate_files: u64,
        warnings: Vec<String>,
    ) -> Result<MarketImportBatch, MarketError> {
        FakeMarketRepository::commit_import(
            self,
            workspace_id,
            files,
            skipped_duplicate_files,
            warnings,
        )
        .await
    }
    async fn list_imports(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<MarketImportBatch>, MarketError> {
        FakeMarketRepository::list_imports(self, workspace_id).await
    }
    async fn get_import(
        &self,
        workspace_id: WorkspaceId,
        batch_id: MarketImportBatchId,
    ) -> Result<MarketImportBatch, MarketError> {
        FakeMarketRepository::get_import(self, workspace_id, batch_id).await
    }
    async fn get_order_book(
        &self,
        workspace_id: WorkspaceId,
        type_id: i64,
        location_id: i64,
        pinned_batch_id: Option<MarketImportBatchId>,
    ) -> Result<MarketOrderBook, MarketError> {
        FakeMarketRepository::get_order_book(
            self,
            workspace_id,
            type_id,
            location_id,
            pinned_batch_id,
        )
        .await
    }
    async fn get_source_order_book(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_id: i64,
        location_id: i64,
        pinned_batch_id: Option<MarketImportBatchId>,
    ) -> Result<MarketOrderBook, MarketError> {
        FakeMarketRepository::get_source_order_book(
            self,
            workspace_id,
            source_id,
            type_id,
            location_id,
            pinned_batch_id,
        )
        .await
    }
    async fn get_source_order_books(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        location_id: i64,
        pinned_batch_id: Option<MarketImportBatchId>,
    ) -> Result<BTreeMap<i64, MarketOrderBook>, MarketError> {
        FakeMarketRepository::get_source_order_books(
            self,
            workspace_id,
            source_id,
            type_ids,
            location_id,
            pinned_batch_id,
        )
        .await
    }
    async fn get_market_price_source(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<MarketPriceSource, MarketError> {
        FakeMarketRepository::get_market_price_source(self, workspace_id, source_id).await
    }
    async fn register_market_coverage(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        FakeMarketRepository::register_market_coverage(self, workspace_id, source_id, items).await
    }
    async fn upsert_market_coverage(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<(), MarketError> {
        FakeMarketRepository::upsert_market_coverage(self, workspace_id, source_id, items).await
    }
    async fn market_refresh_candidates(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        FakeMarketRepository::market_refresh_candidates(self, workspace_id, source_id, now, limit)
            .await
    }
    async fn market_refresh_candidates_for_type_ids(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        FakeMarketRepository::market_refresh_candidates_for_type_ids(
            self,
            workspace_id,
            source_id,
            type_ids,
            now,
        )
        .await
    }
    async fn prioritize_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<bool, MarketError> {
        FakeMarketRepository::prioritize_market_refresh(
            self,
            workspace_id,
            source_id,
            type_ids,
            now,
        )
        .await
    }
    async fn begin_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_id: i64,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, MarketError> {
        FakeMarketRepository::begin_market_refresh(
            self,
            workspace_id,
            source_id,
            type_id,
            attempted_at,
            lease_expires_at,
        )
        .await
    }
    async fn complete_esi_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        batch: EsiMarketObservationBatch,
    ) -> Result<bool, MarketError> {
        FakeMarketRepository::complete_esi_market_refresh(
            self,
            workspace_id,
            claim,
            next_refresh_at,
            batch,
        )
        .await
    }
    async fn fail_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        failure: MarketRefreshFailure,
    ) -> Result<bool, MarketError> {
        FakeMarketRepository::fail_market_refresh(self, workspace_id, source_id, failure).await
    }
    async fn revalidate_esi_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_id: i64,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        cache_expires_at: Option<DateTime<Utc>>,
    ) -> Result<bool, MarketError> {
        FakeMarketRepository::revalidate_esi_market_refresh(
            self,
            workspace_id,
            source_id,
            type_id,
            claim,
            next_refresh_at,
            cache_expires_at,
        )
        .await
    }
    async fn register_public_market_demand(
        &self,
        region_id: i64,
        items: Vec<MarketCoverageRegistration>,
        prioritize: bool,
        now: DateTime<Utc>,
    ) -> Result<(), MarketError> {
        FakeMarketRepository::register_public_market_demand(self, region_id, items, prioritize, now)
            .await
    }
    async fn prioritize_public_market_refresh(
        &self,
        region_id: i64,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<bool, MarketError> {
        FakeMarketRepository::prioritize_public_market_refresh(self, region_id, type_ids, now).await
    }
    async fn public_market_coverage(
        &self,
        region_id: i64,
        type_ids: &[i64],
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        FakeMarketRepository::public_market_coverage(self, region_id, type_ids).await
    }
    async fn begin_public_market_refresh(
        &self,
        region_id: i64,
        type_id: i64,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, MarketError> {
        FakeMarketRepository::begin_public_market_refresh(
            self,
            region_id,
            type_id,
            attempted_at,
            lease_expires_at,
        )
        .await
    }
    async fn complete_public_market_refresh(
        &self,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        batch: PublicMarketObservationBatch,
    ) -> Result<bool, MarketError> {
        FakeMarketRepository::complete_public_market_refresh(self, claim, next_refresh_at, batch)
            .await
    }
    async fn revalidate_public_market_refresh(
        &self,
        region_id: i64,
        type_id: i64,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        cache_expires_at: Option<DateTime<Utc>>,
    ) -> Result<bool, MarketError> {
        FakeMarketRepository::revalidate_public_market_refresh(
            self,
            region_id,
            type_id,
            claim,
            next_refresh_at,
            cache_expires_at,
        )
        .await
    }
    async fn fail_public_market_refresh(
        &self,
        region_id: i64,
        type_id: i64,
        claim: DateTime<Utc>,
        attempted_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        error_message: String,
    ) -> Result<bool, MarketError> {
        FakeMarketRepository::fail_public_market_refresh(
            self,
            region_id,
            type_id,
            claim,
            attempted_at,
            next_refresh_at,
            error_message,
        )
        .await
    }
}
