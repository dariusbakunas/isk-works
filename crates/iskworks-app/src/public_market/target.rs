use super::*;

impl PublicMarketService {
    /// One per-workspace coverage row: the `refresh_target` path for a
    /// price source. Kept as its own entry point so the source path and its
    /// tests read exactly as before public coverage existed.
    pub(super) async fn refresh_type(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        scope: MarketScope,
        item: MarketCoverageItem,
        cancel: &CancellationToken,
    ) -> MarketRefreshOutcome {
        self.refresh_target(
            RefreshTarget::Source {
                workspace_id,
                source_id,
            },
            scope,
            item,
            cancel,
        )
        .await
    }

    /// Leases one coverage row (per-workspace or app-wide), fetches its book
    /// from the public regional endpoint, and records the result: a new
    /// batch, a `304` revalidation, or a failure with backoff.
    pub(super) async fn refresh_target(
        &self,
        target: RefreshTarget,
        scope: MarketScope,
        item: MarketCoverageItem,
        cancel: &CancellationToken,
    ) -> MarketRefreshOutcome {
        // Shutdown before we lease anything: nothing to undo, nothing to
        // persist.
        if cancel.is_cancelled() {
            return MarketRefreshOutcome::Skipped;
        }
        let attempted_at = Utc::now();
        let lease_expires_at = attempted_at
            + Duration::from_std(self.lease_duration).unwrap_or_else(|_| Duration::minutes(2));
        let lease = match self
            .begin_target(target, item.type_id, attempted_at, lease_expires_at)
            .await
        {
            Ok(lease) => lease,
            Err(error) => {
                tracing::warn!(
                    %error,
                    source_id = %target,
                    item_name = %item.type_name,
                    type_id = item.type_id,
                    "market refresh lease failed"
                );
                return MarketRefreshOutcome::Failed;
            }
        };
        let Some(claim) = lease else {
            return MarketRefreshOutcome::Skipped;
        };
        // The request-concurrency permit and the shared rate-limit cooldown
        // are acquired per HTTP request inside `regional_market_request`, so
        // the retry and cooldown sleeps below hold neither.
        let result = self
            .fetch_type_with_retry(target, scope, &item, attempted_at, cancel)
            .await;
        match result {
            Ok(FetchTypeOutcome::Cancelled) => {
                // Shutdown mid-fetch. The row is leased to `refreshing`;
                // do NOT persist complete/fail -- `lease_expires_at`
                // recovery is the safety net, exactly as for a crash.
                MarketRefreshOutcome::Skipped
            }
            Ok(FetchTypeOutcome::Fetched(book)) => {
                let order_count = book.orders.len();
                let next_refresh_at = attempted_at
                    + Duration::from_std(self.refresh_interval)
                        .unwrap_or_else(|_| Duration::minutes(5));
                match self
                    .complete_target(target, claim, next_refresh_at, book)
                    .await
                {
                    Ok(true) => {
                        tracing::debug!(
                            source_id = %target,
                            item_name = %item.type_name,
                            type_id = item.type_id,
                            order_count,
                            "market refresh completed"
                        );
                        MarketRefreshOutcome::Succeeded
                    }
                    Ok(false) => MarketRefreshOutcome::Skipped,
                    Err(error) => {
                        let message = error.to_string();
                        tracing::warn!(
                            %error,
                            source_id = %target,
                            item_name = %item.type_name,
                            type_id = item.type_id,
                            "market refresh commit failed"
                        );
                        // Without recording this as a failure, coverage state
                        // stays stuck in `refreshing` with no `last_error`
                        // and only becomes reclaimable via the (short) lease
                        // timeout instead of the normal retry backoff --
                        // effectively a hot loop for any item whose commit
                        // keeps failing the same way every attempt.
                        self.record_failure(
                            target,
                            item.type_id,
                            claim,
                            attempted_at,
                            attempted_at + REFRESH_RETRY_DELAY,
                            message,
                        )
                        .await;
                        MarketRefreshOutcome::Failed
                    }
                }
            }
            Ok(FetchTypeOutcome::NotModified { cache_expires_at }) => {
                // ESI 304: the batch at `last_completed_batch_id` is still
                // authoritative. No new batch, no order rows -- just move
                // coverage back to `current` and advance freshness.
                let next_refresh_at = attempted_at
                    + Duration::from_std(self.refresh_interval)
                        .unwrap_or_else(|_| Duration::minutes(5));
                match self
                    .revalidate_target(
                        target,
                        item.type_id,
                        claim,
                        next_refresh_at,
                        cache_expires_at,
                    )
                    .await
                {
                    Ok(true) => {
                        tracing::info!(
                            source_id = %target,
                            item_name = %item.type_name,
                            type_id = item.type_id,
                            prior_observed_at = ?item.observed_at,
                            next_refresh_at = %next_refresh_at,
                            "market refresh revalidated (ESI 304 Not Modified)"
                        );
                        MarketRefreshOutcome::Revalidated
                    }
                    Ok(false) => MarketRefreshOutcome::Skipped,
                    Err(error) => {
                        let message = error.to_string();
                        tracing::warn!(
                            %error,
                            source_id = %target,
                            item_name = %item.type_name,
                            type_id = item.type_id,
                            "market refresh revalidation commit failed"
                        );
                        self.record_failure(
                            target,
                            item.type_id,
                            claim,
                            attempted_at,
                            attempted_at + REFRESH_RETRY_DELAY,
                            message,
                        )
                        .await;
                        MarketRefreshOutcome::Failed
                    }
                }
            }
            Err(failure) => {
                let message = failure.error.to_string();
                tracing::warn!(
                    error = %failure.error,
                    source_id = %target,
                    item_name = %item.type_name,
                    type_id = item.type_id,
                    attempts = failure.attempts,
                    cached_observation = item.observed_at.is_some(),
                    "market refresh failed"
                );
                // A rate-limited terminal failure must not become eligible
                // again before ESI's own `Retry-After` window: keep the
                // fixed 1-minute backoff as the floor, but honor a longer
                // server-directed cooldown when the 429 carried one.
                let next_refresh_at = failure
                    .error
                    .server_retry_after()
                    .and_then(|wait| Duration::from_std(wait).ok())
                    .map_or(attempted_at + REFRESH_RETRY_DELAY, |wait| {
                        (Utc::now() + wait).max(attempted_at + REFRESH_RETRY_DELAY)
                    });
                self.record_failure(
                    target,
                    item.type_id,
                    claim,
                    attempted_at,
                    next_refresh_at,
                    message,
                )
                .await;
                MarketRefreshOutcome::Failed
            }
        }
    }

    async fn begin_target(
        &self,
        target: RefreshTarget,
        type_id: i64,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, MarketError> {
        match target {
            RefreshTarget::Source {
                workspace_id,
                source_id,
            } => {
                self.repository
                    .begin(
                        workspace_id,
                        source_id,
                        type_id,
                        attempted_at,
                        lease_expires_at,
                    )
                    .await
            }
            RefreshTarget::Public { region_id } => {
                self.repository
                    .begin_public(region_id, type_id, attempted_at, lease_expires_at)
                    .await
            }
        }
    }

    async fn complete_target(
        &self,
        target: RefreshTarget,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        book: FetchedBook,
    ) -> Result<bool, MarketError> {
        match target {
            RefreshTarget::Source {
                workspace_id,
                source_id,
            } => {
                self.repository
                    .complete(
                        workspace_id,
                        claim,
                        next_refresh_at,
                        EsiMarketObservationBatch {
                            id: MarketObservationBatchId::new(),
                            source_id,
                            type_id: book.type_id,
                            type_name: book.type_name,
                            region_id: book.region_id,
                            solar_system_id: book.solar_system_id,
                            location_id: book.location_id,
                            observed_at: book.observed_at,
                            etag: book.etag,
                            expires_at: book.expires_at,
                            orders: book.orders,
                        },
                    )
                    .await
            }
            RefreshTarget::Public { region_id } => {
                self.repository
                    .complete_public(
                        claim,
                        next_refresh_at,
                        PublicMarketObservationBatch {
                            id: MarketObservationBatchId::new(),
                            type_id: book.type_id,
                            type_name: book.type_name,
                            region_id,
                            observed_at: book.observed_at,
                            etag: book.etag,
                            expires_at: book.expires_at,
                            orders: book.orders,
                        },
                    )
                    .await
            }
        }
    }

    async fn revalidate_target(
        &self,
        target: RefreshTarget,
        type_id: i64,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        cache_expires_at: Option<DateTime<Utc>>,
    ) -> Result<bool, MarketError> {
        match target {
            RefreshTarget::Source {
                workspace_id,
                source_id,
            } => {
                self.repository
                    .revalidate(
                        workspace_id,
                        source_id,
                        type_id,
                        claim,
                        next_refresh_at,
                        cache_expires_at,
                    )
                    .await
            }
            RefreshTarget::Public { region_id } => {
                self.repository
                    .revalidate_public(region_id, type_id, claim, next_refresh_at, cache_expires_at)
                    .await
            }
        }
    }

    /// Records a failed refresh; a failure to record it is only logged (the
    /// lease expiry is the backstop, as for a crash).
    async fn record_failure(
        &self,
        target: RefreshTarget,
        type_id: i64,
        claim: DateTime<Utc>,
        attempted_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        message: String,
    ) {
        let result = match target {
            RefreshTarget::Source {
                workspace_id,
                source_id,
            } => {
                self.repository
                    .fail(
                        workspace_id,
                        source_id,
                        MarketRefreshFailure {
                            type_id,
                            claim,
                            attempted_at,
                            next_refresh_at,
                            error_message: message,
                        },
                    )
                    .await
            }
            RefreshTarget::Public { region_id } => {
                self.repository
                    .fail_public(
                        region_id,
                        type_id,
                        claim,
                        attempted_at,
                        next_refresh_at,
                        message,
                    )
                    .await
            }
        };
        if let Err(persistence_error) = result {
            tracing::warn!(
                %persistence_error,
                type_id,
                "market refresh failure recording failed"
            );
        }
    }
}

/// Result of one `fetch_type` attempt: a freshly fetched order book, or
/// ESI's `304 Not Modified` for an eligible conditional request (the batch
/// at `last_completed_batch_id` stays authoritative -- `refresh_type`
/// revalidates coverage instead of completing a new batch).
/// Which coverage row a refresh leases and completes: one workspace's price
/// source, or an app-wide `public_market_coverage` row. Its `Display` is what
/// the refresh logs print as `source_id` (`public:<region>` for public rows),
/// so existing log searches keep working.
#[derive(Clone, Copy, Debug)]
pub(super) enum RefreshTarget {
    Source {
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    },
    Public {
        region_id: i64,
    },
}

impl std::fmt::Display for RefreshTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source { source_id, .. } => write!(f, "{}", source_id.0),
            Self::Public { region_id } => write!(f, "public:{region_id}"),
        }
    }
}
