use super::*;

/// Process-wide "no new public regional market-order request may begin
/// before this instant" gate, shared by every in-flight and queued
/// `refresh_type` task on one `PublicMarketService` (its single `Arc` is
/// cloned along with the service).
///
/// Deliberately coarse. ESI rate-limits the public
/// `GET /markets/{region_id}/orders/` endpoint by client IP, not by
/// region or type, so a 429 for one `(region, type)` means every other
/// concurrent regional market-order request is about to be refused too.
/// We do not yet parse `X-Ratelimit-Group`, so this gate covers *all*
/// regional market-order traffic from this process regardless of region.
/// It intentionally does **not** cover the authenticated structure-market
/// endpoint (`GET /markets/structures/{id}/`), which is a different route
/// with its own bucket.
///
/// A 429 carrying `Retry-After` pushes the gate out via [`Self::extend`];
/// concurrent 429s can only ever push it further out, never pull it in.
#[derive(Clone, Default)]
pub(super) struct RegionalMarketCooldown {
    until: Arc<Mutex<Option<tokio::time::Instant>>>,
}

impl RegionalMarketCooldown {
    /// The instant before which no request may start, or `None` when the
    /// gate is currently open.
    fn active_until(&self) -> Option<tokio::time::Instant> {
        let deadline = (*self
            .until
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()))?;
        (deadline > tokio::time::Instant::now()).then_some(deadline)
    }

    /// Sleeps until the gate opens, re-reading after each wait so a 429
    /// that lands *while* this task was waiting extends the wait rather
    /// than letting it through early. Returns early (still gated) if
    /// `cancel` fires -- the caller re-checks cancellation before issuing
    /// a request.
    async fn wait(&self, cancel: &CancellationToken) {
        while let Some(deadline) = self.active_until() {
            tokio::select! {
                biased;
                () = cancel.cancelled() => return,
                () = tokio::time::sleep_until(deadline) => {}
            }
        }
    }

    /// Pushes the gate out to at least `now + retry_after`. Never brings
    /// it in -- an existing later deadline wins.
    fn extend(&self, retry_after: std::time::Duration) {
        let target = tokio::time::Instant::now() + retry_after;
        let mut guard = self
            .until
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        *guard = Some((*guard).map_or(target, |existing| existing.max(target)));
    }
}
impl PublicMarketService {
    /// Performs one HTTP request against the public regional market-order
    /// endpoint under both the request-concurrency semaphore and the
    /// shared [`RegionalMarketCooldown`].
    ///
    /// Ordering matters:
    /// 1. wait for the shared cooldown to open -- **without** a permit, so
    ///    a backing-off request never occupies one of the four HTTP slots;
    /// 2. acquire a permit;
    /// 3. re-check the cooldown: a 429 seen by another task while this one
    ///    was queued for a permit may have re-closed the gate, so drop the
    ///    permit and go back to step 1 rather than firing into a window
    ///    already known to be closed;
    /// 4. make the request, holding the permit only for its duration;
    /// 5. on a 429 carrying `Retry-After`, push the shared cooldown out
    ///    before returning the error.
    ///
    /// Requests already in flight when another task's 429 arrives are not
    /// cancelled -- only *new* requests are gated.
    ///
    /// `etag` (when `Some`) is forwarded as `If-None-Match`; a conditional
    /// request takes the identical cooldown/semaphore path and a `304`
    /// comes back as `EsiResponse { not_modified: true, .. }`.
    /// `Ok(None)` means shutdown was requested (via `cancel`) before or
    /// during the request -- the caller treats it as `FetchTypeOutcome::Cancelled`,
    /// never as an error. The cooldown/semaphore/re-check ordering above
    /// always holds; the cancellation checks sit *between* those steps,
    /// never in place of the 429 gate.
    async fn regional_market_request(
        &self,
        region_id: i64,
        type_id: i64,
        page: u32,
        etag: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<Option<EsiResponse<MarketOrderObservation>>, EsiError> {
        loop {
            self.market_cooldown.wait(cancel).await;
            if cancel.is_cancelled() {
                return Ok(None);
            }
            let permit = tokio::select! {
                biased;
                () = cancel.cancelled() => return Ok(None),
                acquired = self.request_limit.acquire() => match acquired {
                    Ok(permit) => permit,
                    // The semaphore is never closed; treat the impossible
                    // `AcquireError` as a transient failure so the normal
                    // retry path handles it.
                    Err(_) => return Err(EsiError::TemporaryFailure),
                },
            };
            if self.market_cooldown.active_until().is_some() {
                drop(permit);
                continue;
            }
            let result = tokio::select! {
                biased;
                () = cancel.cancelled() => {
                    // Dropping the in-flight request future cancels the
                    // underlying reqwest/hyper request; the permit is
                    // released here.
                    drop(permit);
                    return Ok(None);
                }
                r = self
                    .transport
                    .regional_market_orders(region_id, type_id, page, etag) => r,
            };
            // Close the shared gate *before* releasing the HTTP slot, so
            // the next task to take a permit sees the cooldown on its
            // pre-request re-check rather than racing past it.
            if let Err(EsiError::RateLimited {
                retry_after_seconds: Some(seconds),
            }) = &result
            {
                self.market_cooldown
                    .extend(std::time::Duration::from_secs(*seconds));
            }
            drop(permit);
            return result.map(Some);
        }
    }

    pub(super) async fn fetch_type_with_retry(
        &self,
        target: RefreshTarget,
        scope: MarketScope,
        item: &MarketCoverageItem,
        observed_at: DateTime<Utc>,
        cancel: &CancellationToken,
    ) -> Result<FetchTypeOutcome, MarketFetchFailure> {
        let mut attempt = 1;
        loop {
            match self
                .fetch_type(target, scope, item, observed_at, cancel)
                .await
            {
                // A fresh book, a `304`, and shutdown are all non-retryable.
                Ok(outcome) => return Ok(outcome),
                Err(error) if error.retryable() && attempt < MAX_FETCH_ATTEMPTS => {
                    // Local exponential backoff for errors with no server
                    // timing hint (5xx, network). For a 429 the effective
                    // wait is at least ESI's `Retry-After`, so the storm of
                    // 250ms/500ms retries against a still-closed rate-limit
                    // window cannot happen.
                    let local_backoff = FETCH_RETRY_DELAY * attempt;
                    let retry_delay = error
                        .server_retry_after()
                        .map_or(local_backoff, |server| server.max(local_backoff));
                    tracing::warn!(
                        error = %error,
                        source_id = %target,
                        item_name = %item.type_name,
                        type_id = item.type_id,
                        attempt,
                        next_attempt = attempt + 1,
                        retry_delay_ms = retry_delay.as_millis() as u64,
                        retryable = true,
                        "market refresh retry scheduled"
                    );
                    tokio::select! {
                        biased;
                        () = cancel.cancelled() => return Ok(FetchTypeOutcome::Cancelled),
                        () = tokio::time::sleep(retry_delay) => {}
                    }
                    attempt += 1;
                }
                Err(error) => {
                    return Err(MarketFetchFailure {
                        error,
                        attempts: attempt,
                    });
                }
            }
        }
    }

    /// Fetches `item`'s current order book for `scope` -- a single station
    /// or structure when `scope.location_id` is `Some`, or every order in
    /// the region when it's `None`. The per-order `system_id` ESI reports is recorded as-is
    /// rather than cross-checked against an expected value: a location
    /// belongs to exactly one solar system by construction, so the location
    /// filter below already implies it, and a region-wide fetch has no
    /// single expected system to check against in the first place.
    async fn fetch_type(
        &self,
        target: RefreshTarget,
        scope: MarketScope,
        item: &MarketCoverageItem,
        observed_at: DateTime<Utc>,
        cancel: &CancellationToken,
    ) -> Result<FetchTypeOutcome, MarketFetchError> {
        if cancel.is_cancelled() {
            return Ok(FetchTypeOutcome::Cancelled);
        }
        // Conditional (`If-None-Match`) only for a small, definitely
        // single-page previous book -- see `CONDITIONAL_REFRESH_MAX_ORDERS`.
        // A NULL prior ETag (never completed, or region-wide) also opts out.
        let conditional_etag = item
            .prior_etag
            .as_deref()
            .filter(|_| item.order_count < CONDITIONAL_REFRESH_MAX_ORDERS);
        let Some(mut first) = self
            .regional_market_request(scope.region_id, item.type_id, 1, conditional_etag, cancel)
            .await?
        else {
            return Ok(FetchTypeOutcome::Cancelled);
        };
        if first.not_modified {
            // A 304 only reaches here when we actually sent `If-None-Match`.
            // Accept it as "the current batch is still authoritative" only
            // if ESI does not report the logical result as multi-page; if
            // it does, discard the 304 and restart at page 1 unconditionally.
            match first.metadata.pages {
                None | Some(1) => {
                    return Ok(FetchTypeOutcome::NotModified {
                        cache_expires_at: first.metadata.expires_at(),
                    })
                }
                Some(pages) => {
                    tracing::warn!(
                        source_id = %target,
                        item_name = %item.type_name,
                        type_id = item.type_id,
                        pages,
                        "conditional market refresh 304 reported multiple pages; \
                         falling back to a full unconditional fetch"
                    );
                    let Some(refetched) = self
                        .regional_market_request(scope.region_id, item.type_id, 1, None, cancel)
                        .await?
                    else {
                        return Ok(FetchTypeOutcome::Cancelled);
                    };
                    first = refetched;
                    if first.not_modified {
                        return Err(MarketFetchError::InvalidResponse);
                    }
                }
            }
        }
        let pages = first.metadata.pages.unwrap_or(1);
        if pages == 0 || pages > MAX_PAGES_PER_TYPE {
            return Err(MarketFetchError::InvalidResponse);
        }
        let etag = first.metadata.etag.clone();
        let expires_at = first.metadata.expires_at();
        let mut records = first.records;
        for page in 2..=pages {
            if cancel.is_cancelled() {
                return Ok(FetchTypeOutcome::Cancelled);
            }
            let Some(response) = self
                .regional_market_request(scope.region_id, item.type_id, page, None, cancel)
                .await?
            else {
                return Ok(FetchTypeOutcome::Cancelled);
            };
            if response.not_modified {
                return Err(MarketFetchError::InvalidResponse);
            }
            records.extend(response.records);
        }
        let mut orders = Vec::new();
        // 0 is the "unknown/region-wide" sentinel -- best-effort filled in from the first matching order below for a
        // location-scoped fetch, left at 0 for a region-wide one since no
        // single system applies.
        let mut solar_system_id = 0i64;
        for record in records {
            if record.type_id != item.type_id {
                return Err(MarketFetchError::InvalidResponse);
            }
            if let Some(location_id) = scope.location_id {
                if record.location_id != location_id {
                    continue;
                }
            }
            // ESI occasionally returns an individual order with duration=0
            // (seen live for a real Thrasher sell order) -- duration_days is
            // display/storage-only, never used in any pricing calculation,
            // so skip just that one order rather than losing every other
            // valid order for this item to a single malformed record.
            if record.duration_days == 0 {
                tracing::warn!(
                    type_id = record.type_id,
                    order_id = record.order_id,
                    "skipping esi market order with non-positive duration"
                );
                continue;
            }
            if scope.location_id.is_some() && solar_system_id == 0 {
                solar_system_id = record.system_id;
            }
            orders.push(to_core_order(record, scope.location_id)?);
        }
        Ok(FetchTypeOutcome::Fetched(FetchedBook {
            type_id: item.type_id,
            type_name: item.type_name.clone(),
            region_id: scope.region_id,
            solar_system_id,
            location_id: scope.location_id.unwrap_or(0),
            observed_at,
            etag,
            expires_at,
            orders,
        }))
    }
}

/// One fetched order book, before it is stored as a per-workspace batch or
/// an app-wide one (`RefreshTarget`).
#[derive(Debug)]
pub(super) struct FetchedBook {
    pub(super) type_id: i64,
    pub(super) type_name: String,
    pub(super) region_id: i64,
    pub(super) solar_system_id: i64,
    pub(super) location_id: i64,
    pub(super) observed_at: DateTime<Utc>,
    pub(super) etag: Option<String>,
    pub(super) expires_at: Option<DateTime<Utc>>,
    pub(super) orders: Vec<EsiMarketOrder>,
}

#[derive(Debug)]
pub(super) enum FetchTypeOutcome {
    Fetched(FetchedBook),
    /// ESI's 304: the current book stands until `cache_expires_at`.
    NotModified {
        cache_expires_at: Option<DateTime<Utc>>,
    },
    /// Shutdown was requested before or during the fetch. Control flow,
    /// not a failure: `refresh_type` returns `Skipped` and persists
    /// nothing; a row already leased to `refreshing` recovers via
    /// `lease_expires_at`.
    Cancelled,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum MarketFetchError {
    #[error("{0}")]
    Esi(#[from] EsiError),
    #[error("{0}")]
    Market(#[from] MarketError),
    #[error("EVE returned an invalid market order response")]
    InvalidResponse,
}

#[derive(Debug)]
pub(super) struct MarketFetchFailure {
    pub(super) error: MarketFetchError,
    pub(super) attempts: u32,
}

/// Distinguishes a mid-pagination denial (retryable once, per
/// `refresh_structure_scope`) from any other structure-fetch failure.
#[derive(Debug)]
pub(super) enum FetchStructureError {
    AccessDenied,
    Other(String),
    /// Shutdown requested mid-pagination -- `refresh_structure_scope`
    /// returns `Skipped` (no `fail_claimed`); leased rows recover via
    /// `lease_expires_at`.
    Cancelled,
}

impl MarketFetchError {
    fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Esi(
                EsiError::TemporaryFailure
                    | EsiError::RateLimited { .. }
                    | EsiError::EsiErrorLimit { .. }
            )
        )
    }

    /// ESI's server-directed minimum wait before the next attempt, when
    /// this failure carried one: a 429's `Retry-After`, or the remaining
    /// error-limit window (the transport pauses every ESI call until then,
    /// so retrying sooner can only fail). Every other error returns `None`,
    /// leaving the local exponential backoff untouched.
    pub(super) fn server_retry_after(&self) -> Option<std::time::Duration> {
        match self {
            Self::Esi(
                EsiError::RateLimited {
                    retry_after_seconds: Some(seconds),
                }
                | EsiError::EsiErrorLimit {
                    reset_seconds: Some(seconds),
                }
                | EsiError::ServerDowntime {
                    retry_after_seconds: Some(seconds),
                },
            ) => Some(std::time::Duration::from_secs(*seconds)),
            _ => None,
        }
    }
}
