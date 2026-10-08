//! Market pricing persistence (`PgMarketRepository`).
//!
//! Split by persistence concern into sibling modules; this file keeps the
//! struct, the single `impl MarketRepository for PgMarketRepository` (each
//! method delegates to a `pub(super)` inherent method of the same name --
//! inherent methods win resolution, so `self.NAME(..)` is not recursive),
//! plus the shared `convert` / `rows` helpers.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_core::{
    ConnectedCharacterId, EsiMarketObservationBatch, MarketCoverageItem,
    MarketCoverageRegistration, MarketError, MarketImportBatch, MarketImportBatchId,
    MarketLocationClassification, MarketOrderBook, MarketOrderView, MarketPriceSource,
    MarketRefreshFailure, MarketRepository, MarketStructureListing, PriceSourceId,
    PublicMarketObservationBatch, ResolvedMarketExport, ResolvedMarketLocation, ScopeFreshness,
    WorkspaceId,
};
use sqlx::PgPool;

mod convert;
mod coverage;
mod freshness;
mod imports;
mod order_books;
mod pruning;
mod public_coverage;
mod refresh;
mod rows;
mod scope;
mod structures;

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub struct PgMarketRepository {
    pool: PgPool,
}

impl PgMarketRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Result of a `prune_orphaned_market_observations` pass.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MarketObservationPruneOutcome {
    pub observations_deleted: u64,
    pub batches_deleted: u64,
    pub chunks_run: u32,
    /// `false` when `max_chunks` was reached with orphan batches still to
    /// collect -- the next pass continues from the oldest remaining.
    pub drained: bool,
}

#[async_trait]
impl MarketRepository for PgMarketRepository {
    async fn get_source_order_books(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        location_id: i64,
        pinned_batch_id: Option<MarketImportBatchId>,
    ) -> Result<BTreeMap<i64, MarketOrderBook>, MarketError> {
        self.get_source_order_books(
            workspace_id,
            source_id,
            type_ids,
            location_id,
            pinned_batch_id,
        )
        .await
    }

    async fn resolve_scope_price_sources(
        &self,
        workspace_id: WorkspaceId,
        scope: iskworks_core::MarketScope,
    ) -> Result<Vec<PriceSourceId>, MarketError> {
        self.resolve_scope_price_sources(workspace_id, scope).await
    }

    async fn resolve_source_scope(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<iskworks_core::MarketScope, MarketError> {
        self.resolve_source_scope(workspace_id, source_id).await
    }

    async fn ensure_esi_price_source_for_scope(
        &self,
        workspace_id: WorkspaceId,
        scope: iskworks_core::MarketScope,
    ) -> Result<PriceSourceId, MarketError> {
        self.ensure_esi_price_source_for_scope(workspace_id, scope)
            .await
    }

    async fn scoped_order_books(
        &self,
        workspace_id: WorkspaceId,
        scope: iskworks_core::MarketScope,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<MarketOrderView>>, MarketError> {
        self.scoped_order_books(workspace_id, scope, type_ids).await
    }

    async fn resolve_type_name(&self, type_id: i64) -> Result<Option<String>, MarketError> {
        self.resolve_type_name(type_id).await
    }

    async fn resolve_type_market_group(&self, type_id: i64) -> Result<Option<i64>, MarketError> {
        self.resolve_type_market_group(type_id).await
    }

    async fn location_names(
        &self,
        workspace_id: WorkspaceId,
        location_ids: &[i64],
    ) -> Result<BTreeMap<i64, String>, MarketError> {
        self.location_names(workspace_id, location_ids).await
    }

    async fn resolve_order_location_names(
        &self,
        workspace_id: WorkspaceId,
        location_ids: &[i64],
    ) -> Result<BTreeMap<i64, String>, MarketError> {
        self.resolve_order_location_names(workspace_id, location_ids)
            .await
    }

    async fn known_locations_in_region(
        &self,
        workspace_id: WorkspaceId,
        region_id: i64,
    ) -> Result<Vec<iskworks_core::KnownMarketLocation>, MarketError> {
        self.known_locations_in_region(workspace_id, region_id)
            .await
    }

    async fn classify_location(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
    ) -> Result<MarketLocationClassification, MarketError> {
        self.classify_location(workspace_id, location_id).await
    }

    async fn market_access_connection(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
    ) -> Result<Option<ConnectedCharacterId>, MarketError> {
        self.market_access_connection(workspace_id, location_id)
            .await
    }

    async fn remember_market_access(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
        connection_id: ConnectedCharacterId,
        checked_at: DateTime<Utc>,
    ) -> Result<(), MarketError> {
        self.remember_market_access(workspace_id, location_id, connection_id, checked_at)
            .await
    }

    async fn clear_market_access(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
        checked_at: DateTime<Utc>,
    ) -> Result<(), MarketError> {
        self.clear_market_access(workspace_id, location_id, checked_at)
            .await
    }

    async fn list_known_structures(
        &self,
        workspace_id: WorkspaceId,
        query: &str,
    ) -> Result<Vec<MarketStructureListing>, MarketError> {
        self.list_known_structures(workspace_id, query).await
    }

    async fn scope_freshness(
        &self,
        workspace_id: WorkspaceId,
        scopes: &[(i64, i64)],
    ) -> Result<BTreeMap<(i64, i64), ScopeFreshness>, MarketError> {
        self.scope_freshness(workspace_id, scopes).await
    }

    async fn item_freshness(
        &self,
        workspace_id: WorkspaceId,
        scope: iskworks_core::MarketScope,
        type_ids: &[i64],
    ) -> Result<Option<DateTime<Utc>>, MarketError> {
        self.item_freshness(workspace_id, scope, type_ids).await
    }

    async fn save_location_names(
        &self,
        workspace_id: WorkspaceId,
        locations: Vec<ResolvedMarketLocation>,
    ) -> Result<(), MarketError> {
        self.save_location_names(workspace_id, locations).await
    }

    async fn imported_file_checksums(
        &self,
        workspace_id: WorkspaceId,
        checksums: &[String],
    ) -> Result<BTreeSet<String>, MarketError> {
        self.imported_file_checksums(workspace_id, checksums).await
    }

    async fn commit_import(
        &self,
        workspace_id: WorkspaceId,
        files: Vec<ResolvedMarketExport>,
        skipped_duplicate_files: u64,
        warnings: Vec<String>,
    ) -> Result<MarketImportBatch, MarketError> {
        self.commit_import(workspace_id, files, skipped_duplicate_files, warnings)
            .await
    }

    async fn list_imports(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<MarketImportBatch>, MarketError> {
        self.list_imports(workspace_id).await
    }

    async fn get_import(
        &self,
        workspace_id: WorkspaceId,
        batch_id: MarketImportBatchId,
    ) -> Result<MarketImportBatch, MarketError> {
        self.get_import(workspace_id, batch_id).await
    }

    async fn get_order_book(
        &self,
        workspace_id: WorkspaceId,
        type_id: i64,
        location_id: i64,
        pinned_batch_id: Option<MarketImportBatchId>,
    ) -> Result<MarketOrderBook, MarketError> {
        self.get_order_book(workspace_id, type_id, location_id, pinned_batch_id)
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
        self.get_source_order_book(
            workspace_id,
            source_id,
            type_id,
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
        self.get_market_price_source(workspace_id, source_id).await
    }

    async fn register_market_coverage(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        self.register_market_coverage(workspace_id, source_id, items)
            .await
    }

    async fn upsert_market_coverage(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<(), MarketError> {
        self.upsert_market_coverage(workspace_id, source_id, items)
            .await
    }

    async fn market_refresh_candidates(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        self.market_refresh_candidates(workspace_id, source_id, now, limit)
            .await
    }

    async fn market_refresh_candidates_for_type_ids(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        self.market_refresh_candidates_for_type_ids(workspace_id, source_id, type_ids, now)
            .await
    }

    async fn prioritize_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<bool, MarketError> {
        self.prioritize_market_refresh(workspace_id, source_id, type_ids, now)
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
        self.begin_market_refresh(
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
        self.complete_esi_market_refresh(workspace_id, claim, next_refresh_at, batch)
            .await
    }

    async fn fail_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        failure: MarketRefreshFailure,
    ) -> Result<bool, MarketError> {
        self.fail_market_refresh(workspace_id, source_id, failure)
            .await
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
        self.revalidate_esi_market_refresh(
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
        self.register_public_market_demand(region_id, items, prioritize, now)
            .await
    }

    async fn prioritize_public_market_refresh(
        &self,
        region_id: i64,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<bool, MarketError> {
        self.prioritize_public_market_refresh(region_id, type_ids, now)
            .await
    }

    async fn public_market_coverage(
        &self,
        region_id: i64,
        type_ids: &[i64],
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        self.public_market_coverage(region_id, type_ids).await
    }

    async fn begin_public_market_refresh(
        &self,
        region_id: i64,
        type_id: i64,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, MarketError> {
        self.begin_public_market_refresh(region_id, type_id, attempted_at, lease_expires_at)
            .await
    }

    async fn complete_public_market_refresh(
        &self,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        batch: PublicMarketObservationBatch,
    ) -> Result<bool, MarketError> {
        self.complete_public_market_refresh(claim, next_refresh_at, batch)
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
        self.revalidate_public_market_refresh(
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
        self.fail_public_market_refresh(
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
