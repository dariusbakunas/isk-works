use super::*;

#[async_trait]
pub trait MarketRefreshRepository: Send + Sync {
    /// Which region/location this source's fetches target -- resolved once
    /// per refresh batch rather than per item.
    async fn scope(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<MarketScope, MarketError>;
    async fn register(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError>;
    /// Cheaper sibling of `register` for callers that only need the
    /// upsert side effect, not the freshly reloaded coverage list --
    /// `register`'s Postgres-backed reload re-aggregates *every* row for
    /// this `(workspace_id, source_id)`, which gets expensive as a
    /// workspace's tracked-item count grows, regardless of how many items
    /// this call is registering. Defaults to `register` itself (fine for
    /// hand-written test fakes, which don't pay that cost either way); the
    /// real `MarketRepository`-backed blanket impl below overrides this to
    /// skip the reload entirely.
    async fn register_coverage_only(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<(), MarketError> {
        self.register(workspace_id, source_id, items).await?;
        Ok(())
    }
    async fn candidates(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<MarketCoverageItem>, MarketError>;
    /// Identity-scoped counterpart to `candidates()` -- see
    /// `MarketRepository::market_refresh_candidates_for_type_ids`'s doc
    /// comment for why `register_and_refresh_now` needs this instead of a
    /// priority-ordered `LIMIT`.
    async fn candidates_for_type_ids(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError>;
    /// Bumps `type_ids`' `priority_requested_at` so `candidates()`'s
    /// `ORDER BY priority_requested_at DESC NULLS LAST` favors them over
    /// older due work. `register_and_refresh` calls this for whatever it
    /// just registered -- otherwise two different never-yet-fetched
    /// batches (e.g. a large category's remaining backlog and a later,
    /// smaller explicit request) tie on `next_refresh_at IS NULL` and fall
    /// back to `type_id` ordering, with no guarantee the later request is
    /// served first.
    async fn prioritize(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<bool, MarketError>;
    async fn begin(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_id: i64,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, MarketError>;
    async fn complete(
        &self,
        workspace_id: WorkspaceId,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        batch: EsiMarketObservationBatch,
    ) -> Result<bool, MarketError>;
    async fn fail(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        failure: MarketRefreshFailure,
    ) -> Result<bool, MarketError>;
    /// Records a successful conditional refresh whose ESI response was
    /// `304 Not Modified` -- coverage-only, no batch/order writes,
    /// `last_completed_batch_id` unchanged. See
    /// `MarketRepository::revalidate_esi_market_refresh`.
    async fn revalidate(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_id: i64,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        cache_expires_at: Option<DateTime<Utc>>,
    ) -> Result<bool, MarketError>;
    /// Which ESI fetch mechanism `location_id` needs -- public region
    /// endpoint (`NpcStation`) or authenticated structure endpoint
    /// (`Structure`).
    async fn classify_location(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
    ) -> Result<MarketLocationClassification, MarketError>;
    async fn market_access_connection(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
    ) -> Result<Option<ConnectedCharacterId>, MarketError>;
    async fn remember_market_access(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
        connection_id: ConnectedCharacterId,
        checked_at: DateTime<Utc>,
    ) -> Result<(), MarketError>;
    async fn clear_market_access(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
        checked_at: DateTime<Utc>,
    ) -> Result<(), MarketError>;
    /// Records app-wide demand for `items` in `region_id`
    /// (`MarketRepository::register_public_market_demand`).
    async fn register_public_demand(
        &self,
        region_id: i64,
        items: Vec<MarketCoverageRegistration>,
        prioritize: bool,
        now: DateTime<Utc>,
    ) -> Result<(), MarketError>;
    /// App-wide coverage rows for `type_ids` in `region_id`
    /// (`MarketRepository::public_market_coverage`).
    async fn public_coverage(
        &self,
        region_id: i64,
        type_ids: &[i64],
    ) -> Result<Vec<MarketCoverageItem>, MarketError>;
    /// App-wide `public_market_coverage` lifecycle: the `begin`/`complete`/
    /// `revalidate`/`fail` counterparts keyed by `(region_id, type_id)`.
    async fn begin_public(
        &self,
        region_id: i64,
        type_id: i64,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, MarketError>;
    async fn complete_public(
        &self,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        batch: PublicMarketObservationBatch,
    ) -> Result<bool, MarketError>;
    async fn revalidate_public(
        &self,
        region_id: i64,
        type_id: i64,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        cache_expires_at: Option<DateTime<Utc>>,
    ) -> Result<bool, MarketError>;
    async fn fail_public(
        &self,
        region_id: i64,
        type_id: i64,
        claim: DateTime<Utc>,
        attempted_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        error_message: String,
    ) -> Result<bool, MarketError>;
}

#[async_trait]
impl<T> MarketRefreshRepository for T
where
    T: MarketRepository + Send + Sync + ?Sized,
{
    async fn scope(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<MarketScope, MarketError> {
        self.resolve_source_scope(workspace_id, source_id).await
    }

    async fn register(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        self.register_market_coverage(workspace_id, source_id, items)
            .await
    }

    async fn register_coverage_only(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<(), MarketError> {
        self.upsert_market_coverage(workspace_id, source_id, items)
            .await
    }

    async fn candidates(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        self.market_refresh_candidates(workspace_id, source_id, now, limit)
            .await
    }

    async fn candidates_for_type_ids(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        self.market_refresh_candidates_for_type_ids(workspace_id, source_id, type_ids, now)
            .await
    }

    async fn prioritize(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<bool, MarketError> {
        self.prioritize_market_refresh(workspace_id, source_id, type_ids, now)
            .await
    }

    async fn begin(
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

    async fn complete(
        &self,
        workspace_id: WorkspaceId,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        batch: EsiMarketObservationBatch,
    ) -> Result<bool, MarketError> {
        self.complete_esi_market_refresh(workspace_id, claim, next_refresh_at, batch)
            .await
    }

    async fn fail(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        failure: MarketRefreshFailure,
    ) -> Result<bool, MarketError> {
        self.fail_market_refresh(workspace_id, source_id, failure)
            .await
    }

    async fn revalidate(
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

    async fn classify_location(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
    ) -> Result<MarketLocationClassification, MarketError> {
        MarketRepository::classify_location(self, workspace_id, location_id).await
    }

    async fn market_access_connection(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
    ) -> Result<Option<ConnectedCharacterId>, MarketError> {
        MarketRepository::market_access_connection(self, workspace_id, location_id).await
    }

    async fn remember_market_access(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
        connection_id: ConnectedCharacterId,
        checked_at: DateTime<Utc>,
    ) -> Result<(), MarketError> {
        MarketRepository::remember_market_access(
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
        MarketRepository::clear_market_access(self, workspace_id, location_id, checked_at).await
    }

    async fn register_public_demand(
        &self,
        region_id: i64,
        items: Vec<MarketCoverageRegistration>,
        prioritize: bool,
        now: DateTime<Utc>,
    ) -> Result<(), MarketError> {
        self.register_public_market_demand(region_id, items, prioritize, now)
            .await
    }

    async fn public_coverage(
        &self,
        region_id: i64,
        type_ids: &[i64],
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        self.public_market_coverage(region_id, type_ids).await
    }

    async fn begin_public(
        &self,
        region_id: i64,
        type_id: i64,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, MarketError> {
        self.begin_public_market_refresh(region_id, type_id, attempted_at, lease_expires_at)
            .await
    }

    async fn complete_public(
        &self,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        batch: PublicMarketObservationBatch,
    ) -> Result<bool, MarketError> {
        self.complete_public_market_refresh(claim, next_refresh_at, batch)
            .await
    }

    async fn revalidate_public(
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

    async fn fail_public(
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
