//! The `MarketRepository` port: every persistence/lookup operation the
//! market service and callers depend on. Signatures, async behaviour, and
//! intentional default method bodies are unchanged from the pre-split module.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{ConnectedCharacterId, PriceSourceId, WorkspaceId};

use super::errors::MarketError;
use super::types::*;

#[async_trait]
pub trait MarketRepository: Send + Sync {
    async fn resolve_type_name(&self, type_id: i64) -> Result<Option<String>, MarketError>;
    /// A type's SDE market group, so the market item detail view can show a
    /// category/group breadcrumb by walking the already-fetched category
    /// tree (`build_market_category_tree`) rather than duplicating group
    /// ancestry resolution here. Defaults to `None`, matching every other
    /// optional `MarketRepository` method.
    async fn resolve_type_market_group(&self, _type_id: i64) -> Result<Option<i64>, MarketError> {
        Ok(None)
    }
    async fn location_names(
        &self,
        workspace_id: WorkspaceId,
        location_ids: &[i64],
    ) -> Result<BTreeMap<i64, String>, MarketError>;
    /// Batched location-name resolution for order-book rows: NPC stations
    /// resolve straight from the SDE (canonical, no per-workspace state
    /// needed, with the solar system's security status appended, e.g.
    /// "Jita IV - Moon 4 - Caldari Navy Assembly Plant (0.9)") with
    /// `market_location_names` as the fallback for player structures --
    /// mirrors `resolve_scope_display`'s own station-then-structure lookup,
    /// just batched across many ids instead of resolving one at a time. Ids
    /// with neither are simply absent from the result; callers render their
    /// own graceful fallback (same convention as `location_names`).
    /// Defaults to empty so existing `MarketRepository` fakes don't need
    /// updating for this.
    async fn resolve_order_location_names(
        &self,
        _workspace_id: WorkspaceId,
        _location_ids: &[i64],
    ) -> Result<BTreeMap<i64, String>, MarketError> {
        Ok(BTreeMap::new())
    }
    async fn save_location_names(
        &self,
        workspace_id: WorkspaceId,
        locations: Vec<ResolvedMarketLocation>,
    ) -> Result<(), MarketError>;
    /// Known player structures (and their solar system name) within one
    /// region, for the market-scope location list -- reads
    /// `market_location_names` directly, joined through SDE geography, with
    /// no dependency on any `PriceSource`. Defaults to empty so existing
    /// `MarketRepository` fakes across the test suite don't need to
    /// implement it (same pattern as `SdeReadRepository`'s optional
    /// search methods).
    async fn known_locations_in_region(
        &self,
        _workspace_id: WorkspaceId,
        _region_id: i64,
    ) -> Result<Vec<KnownMarketLocation>, MarketError> {
        Ok(Vec::new())
    }
    /// Positive-evidence classification of a single `location_id` (ESI
    /// structure market orders) --
    /// `NpcStation` only when found in the SDE's NPC station table,
    /// `Structure` only when resolved into `market_location_names` with a
    /// captured `structure_type_id`, `Unknown` otherwise. Never infers
    /// `NpcStation` from the absence of structure evidence. Defaults to
    /// `Unknown`, matching every other optional `MarketRepository` method.
    async fn classify_location(
        &self,
        _workspace_id: WorkspaceId,
        _location_id: i64,
    ) -> Result<MarketLocationClassification, MarketError> {
        Ok(MarketLocationClassification::Unknown)
    }
    /// The connected character last confirmed to have both docking access
    /// and the `esi-markets.structure_markets.v1` scope for a resolved
    /// structure, if any -- a separate concern from
    /// `ResolvedMarketLocation::resolved_by_connection_id`, which only
    /// records who resolved the structure's *name*. `None` means "never
    /// checked, or the remembered character no longer qualifies."
    async fn market_access_connection(
        &self,
        _workspace_id: WorkspaceId,
        _location_id: i64,
    ) -> Result<Option<ConnectedCharacterId>, MarketError> {
        Ok(None)
    }
    /// Remembers which character confirmed market access for a structure,
    /// so future refreshes don't need to re-try every eligible character.
    async fn remember_market_access(
        &self,
        _workspace_id: WorkspaceId,
        _location_id: i64,
        _connection_id: ConnectedCharacterId,
        _checked_at: DateTime<Utc>,
    ) -> Result<(), MarketError> {
        Ok(())
    }
    /// Clears a remembered connection (e.g. after a 403 on a later
    /// refresh) so the next attempt re-resolves from the eligible-character
    /// list rather than retrying a connection that's known to have lost
    /// access.
    async fn clear_market_access(
        &self,
        _workspace_id: WorkspaceId,
        _location_id: i64,
        _checked_at: DateTime<Utc>,
    ) -> Result<(), MarketError> {
        Ok(())
    }
    /// Workspace-known Upwell structures, enriched with market-access and
    /// freshness state -- the Market Scope Selector's "My Structures" tab
    /// (`query=""`, the whole list) and its global search (`query` set,
    /// reusing this same access/freshness-aware query rather than a second,
    /// simpler one). `query=""` matches every structure, mirroring
    /// `SdeReadRepository::search_npc_stations`'s own `$1='' OR ...`
    /// convention -- and like that method, a non-empty `query` matches the
    /// structure's own name *or* its solar system's name. Defaults to
    /// empty, matching every other optional `MarketRepository` method.
    async fn list_known_structures(
        &self,
        _workspace_id: WorkspaceId,
        _query: &str,
    ) -> Result<Vec<MarketStructureListing>, MarketError> {
        Ok(Vec::new())
    }
    /// Batched freshness for a set of `(region_id, location_id)` scopes --
    /// one query serving the Market Scope Selector's Major Hubs, My
    /// Structures, and Region-locations views alike, so freshness logic
    /// exists in exactly one place rather than once per view. See
    /// `ScopeFreshness`'s own doc comment for why this deliberately isn't a
    /// single timestamp. A scope with no `market_source_coverage` rows at
    /// all is simply absent from the result map (never an error) --
    /// callers should treat a missing key the same as
    /// `ScopeFreshness::default()`. Defaults to an empty map, matching
    /// every other optional `MarketRepository` method.
    async fn scope_freshness(
        &self,
        _workspace_id: WorkspaceId,
        _scopes: &[(i64, i64)],
    ) -> Result<BTreeMap<(i64, i64), ScopeFreshness>, MarketError> {
        Ok(BTreeMap::new())
    }
    /// The most recent observation timestamp among `type_ids` within
    /// `scope`, or `None` if none of them have ever completed a refresh --
    /// the cheap "did anything change" signal the Market Browser polls
    /// instead of re-running `list_market_items`'s full paginated/joined
    /// query on every tick. Deliberately narrower than `scope_freshness`
    /// (which aggregates a whole scope for the selector's hub/structure/
    /// region rows): this is always called with exactly the currently
    /// *visible* page's type_ids, so it can answer "has any row I'm
    /// looking at changed" precisely rather than "has anything anywhere in
    /// this scope changed."
    async fn item_freshness(
        &self,
        _workspace_id: WorkspaceId,
        _scope: MarketScope,
        _type_ids: &[i64],
    ) -> Result<Option<DateTime<Utc>>, MarketError> {
        Ok(None)
    }
    /// Every requested type's order-book observations within `scope`,
    /// merged across whichever configured market-derived `PriceSource`s
    /// happen to match it (a shim -- `MarketScope` has no
    /// coverage/observation table of its own). A type absent from the returned map has no
    /// observations in this scope at all -- not an error, an explicit
    /// no-data state for the caller to render. `location_id: None` merges
    /// every matching source in the region; a region no source is
    /// configured for returns an empty map for every requested type.
    /// Defaults to empty, matching every other optional `MarketRepository`
    /// method added so far.
    async fn scoped_order_books(
        &self,
        _workspace_id: WorkspaceId,
        _scope: MarketScope,
        _type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<MarketOrderView>>, MarketError> {
        Ok(BTreeMap::new())
    }
    /// Every `PriceSource` currently configured for `scope` (via
    /// `market_price_source_configs`) -- the same shim `scoped_order_books`
    /// uses, exposed on its own for "Request market data", which
    /// needs the ids to call `register_market_coverage`/`register_and_refresh`
    /// against, not the order books themselves. Empty means no source is
    /// configured for this scope yet -- the caller should surface that
    /// plainly rather than silently doing nothing.
    async fn resolve_scope_price_sources(
        &self,
        _workspace_id: WorkspaceId,
        _scope: MarketScope,
    ) -> Result<Vec<PriceSourceId>, MarketError> {
        Ok(Vec::new())
    }
    /// An `esi_market_orders` `PriceSource`'s own configured scope, read
    /// back from `market_price_source_configs` via `price_source_id` --
    /// lets the ESI fetch/refresh path (`market_esi.rs`) ask "which region
    /// and location am I fetching for?" instead of a hardcoded constant
    ///. Defaults to not-found, matching every other optional
    /// `MarketRepository` method.
    async fn resolve_source_scope(
        &self,
        _workspace_id: WorkspaceId,
        _source_id: PriceSourceId,
    ) -> Result<MarketScope, MarketError> {
        Err(MarketError::PriceSourceNotFound)
    }
    /// Get-or-create the workspace's `esi_market_orders` `PriceSource` for
    /// any region/location `scope` --
    /// "Request market data" needs a coverage anchor to register
    /// against for whatever scope the user is browsing, not just Jita.
    /// Idempotent: concurrent calls for the same scope resolve to the same
    /// source. Defaults to unavailable, matching every other optional
    /// write-side `MarketRepository` method.
    async fn ensure_esi_price_source_for_scope(
        &self,
        _workspace_id: WorkspaceId,
        _scope: MarketScope,
    ) -> Result<PriceSourceId, MarketError> {
        Err(MarketError::Persistence(
            "automatic market source provisioning is unavailable".to_string(),
        ))
    }
    async fn imported_file_checksums(
        &self,
        workspace_id: WorkspaceId,
        checksums: &[String],
    ) -> Result<BTreeSet<String>, MarketError>;
    async fn commit_import(
        &self,
        workspace_id: WorkspaceId,
        files: Vec<ResolvedMarketExport>,
        skipped_duplicate_files: u64,
        warnings: Vec<String>,
    ) -> Result<MarketImportBatch, MarketError>;
    async fn list_imports(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<MarketImportBatch>, MarketError>;
    async fn get_import(
        &self,
        workspace_id: WorkspaceId,
        batch_id: MarketImportBatchId,
    ) -> Result<MarketImportBatch, MarketError>;
    async fn get_order_book(
        &self,
        workspace_id: WorkspaceId,
        type_id: i64,
        location_id: i64,
        pinned_batch_id: Option<MarketImportBatchId>,
    ) -> Result<MarketOrderBook, MarketError>;
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
    /// The one by-id `PriceSource` read: `MarketService::preview_price`
    /// (used by Inventory's legacy explicit-`?priceSourceId=` path) and
    /// Opportunities' `evaluate()` (which resolves a scope to a source via
    /// `ensure_esi_price_source_for_scope` and then fetches it here) both
    /// need to read an already-existing source by id. No route creates,
    /// lists, or edits one -- sources
    /// are exclusively auto-provisioned via `ensure_esi_price_source_for_scope`.
    async fn get_market_price_source(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<MarketPriceSource, MarketError>;
    async fn register_market_coverage(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        let _ = (workspace_id, source_id, items);
        Err(MarketError::Persistence(
            "automatic market coverage is unavailable".to_string(),
        ))
    }
    /// Cheaper sibling of `register_market_coverage` for callers that only
    /// need the upsert side effect and never read back a coverage list --
    /// `register_market_coverage` unconditionally re-aggregates *every*
    /// row for `(workspace_id, source_id)` (a `LEFT JOIN` through
    /// `market_observation_batches`/`market_order_observations` with a
    /// `GROUP BY`/`count(...) FILTER`), which gets expensive as a
    /// workspace's tracked-item count grows regardless of how many items
    /// this particular call is registering -- observed as a multi-second
    /// `sqlx::query: slow statement` warning once coverage reaches a few
    /// thousand rows. Every current caller that doesn't need the reload
    /// (a single-item "Request market data" click, and the worker's
    /// per-tick `refresh_source`) should use this instead.
    async fn upsert_market_coverage(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<(), MarketError> {
        let _ = (workspace_id, source_id, items);
        Err(MarketError::Persistence(
            "automatic market coverage is unavailable".to_string(),
        ))
    }
    async fn market_refresh_candidates(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        let _ = (workspace_id, source_id, now, limit);
        Err(MarketError::Persistence(
            "automatic market refresh is unavailable".to_string(),
        ))
    }
    /// The identity-scoped counterpart to `market_refresh_candidates`: every
    /// row for exactly `type_ids` that isn't currently leased by another
    /// in-flight refresh, with no `ORDER BY`/`LIMIT` and no `next_refresh_at`
    /// due-check (callers of this always call `prioritize_market_refresh`
    /// on the same ids first, which already bumps `next_refresh_at` to
    /// now). Used for a synchronous, on-demand refresh of a caller-specified
    /// set of items -- unlike `market_refresh_candidates`'s priority-ordered
    /// `LIMIT`, this guarantees the dispatch set is *exactly* the caller's
    /// own ids regardless of what else is due/prioritized elsewhere
    /// concurrently. Also used to read back the persisted outcome after
    /// dispatch, since `dispatch_refresh`'s in-memory `Vec<MarketRefreshOutcome>`
    /// isn't reliably positioned against the input for every dispatch path.
    async fn market_refresh_candidates_for_type_ids(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        let _ = (workspace_id, source_id, type_ids, now);
        Err(MarketError::Persistence(
            "automatic market refresh is unavailable".to_string(),
        ))
    }
    async fn prioritize_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<bool, MarketError> {
        let _ = (workspace_id, source_id, type_ids, now);
        Err(MarketError::Persistence(
            "automatic market refresh is unavailable".to_string(),
        ))
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
        Err(MarketError::Persistence(
            "automatic market refresh is unavailable".to_string(),
        ))
    }
    async fn complete_esi_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        batch: EsiMarketObservationBatch,
    ) -> Result<bool, MarketError> {
        let _ = (workspace_id, claim, next_refresh_at, batch);
        Err(MarketError::Persistence(
            "automatic market refresh is unavailable".to_string(),
        ))
    }
    async fn fail_market_refresh(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        failure: MarketRefreshFailure,
    ) -> Result<bool, MarketError> {
        let MarketRefreshFailure {
            type_id,
            claim,
            attempted_at,
            next_refresh_at,
            error_message,
        } = failure;
        let _ = (
            workspace_id,
            source_id,
            type_id,
            claim,
            attempted_at,
            next_refresh_at,
            error_message,
        );
        Err(MarketError::Persistence(
            "automatic market refresh is unavailable".to_string(),
        ))
    }
    /// Records a successful conditional refresh whose ESI response was
    /// `304 Not Modified`: the batch at `last_completed_batch_id` is still
    /// authoritative, so no batch or order rows are written. A single
    /// `market_source_coverage` UPDATE moves the claimed row back to
    /// `current` (clearing `last_error`, lease and priority state) and
    /// advances `last_attempted_at`/`next_refresh_at`. Uses the same claim
    /// guard as `complete_esi_market_refresh`
    /// (`refresh_state = 'refreshing' AND last_attempted_at = claim`);
    /// returns `false` (mutating nothing) when the claim is stale.
    /// `last_completed_batch_id` and `batch.observed_at` are left unchanged.
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
        Err(MarketError::Persistence(
            "automatic market refresh is unavailable".to_string(),
        ))
    }

    /// Records that a workspace needs these types' app-wide public books in
    /// `region_id` (`public_market_coverage`); `prioritize` makes them due
    /// now. Demand is a hint, so repositories without app-wide coverage
    /// ignore it.
    async fn register_public_market_demand(
        &self,
        region_id: i64,
        items: Vec<MarketCoverageRegistration>,
        prioritize: bool,
        now: DateTime<Utc>,
    ) -> Result<(), MarketError> {
        let _ = (region_id, items, prioritize, now);
        Ok(())
    }

    /// Makes existing app-wide rows for `type_ids` due now, ahead of routine
    /// work. `false` when none of them could be prioritized.
    async fn prioritize_public_market_refresh(
        &self,
        region_id: i64,
        type_ids: &[i64],
        now: DateTime<Utc>,
    ) -> Result<bool, MarketError> {
        let _ = (region_id, type_ids, now);
        Ok(false)
    }

    /// App-wide coverage rows for `type_ids` in `region_id` (any state).
    /// Empty for repositories without app-wide coverage.
    async fn public_market_coverage(
        &self,
        region_id: i64,
        type_ids: &[i64],
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        let _ = (region_id, type_ids);
        Ok(Vec::new())
    }

    /// App-wide counterpart of `begin_market_refresh` for one
    /// `public_market_coverage` row.
    async fn begin_public_market_refresh(
        &self,
        region_id: i64,
        type_id: i64,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, MarketError> {
        let _ = (region_id, type_id, attempted_at, lease_expires_at);
        Err(MarketError::Persistence(
            "public market refresh is unavailable".to_string(),
        ))
    }

    /// Stores a fetched regional book app-wide; `false` when `claim` is stale.
    async fn complete_public_market_refresh(
        &self,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        batch: PublicMarketObservationBatch,
    ) -> Result<bool, MarketError> {
        let _ = (claim, next_refresh_at, batch);
        Err(MarketError::Persistence(
            "public market refresh is unavailable".to_string(),
        ))
    }

    /// A `304 Not Modified` for a public book: coverage-only, no batch write.
    async fn revalidate_public_market_refresh(
        &self,
        region_id: i64,
        type_id: i64,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        cache_expires_at: Option<DateTime<Utc>>,
    ) -> Result<bool, MarketError> {
        let _ = (region_id, type_id, claim, next_refresh_at, cache_expires_at);
        Err(MarketError::Persistence(
            "public market refresh is unavailable".to_string(),
        ))
    }

    /// A failed public refresh, with the same backoff as per-workspace rows.
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
        Err(MarketError::Persistence(
            "public market refresh is unavailable".to_string(),
        ))
    }
}
