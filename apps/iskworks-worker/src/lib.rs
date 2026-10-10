use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use iskworks_esi::EsiTransport;
use iskworks_storage::AuthPurgeOutcome;
use tokio_util::sync::CancellationToken;

pub mod metrics_exporter;
mod runtime;
pub use runtime::EvidenceWorker;

/// How a periodic pass reacts when shutdown is requested mid-pass.
#[derive(Clone, Copy)]
enum PassMode {
    /// The pass is itself cancellation-aware: it takes the token, stops
    /// dispatching new work, and drains its own fan-out. Await it to
    /// completion -- it returns promptly once cancelled, and dropping it
    /// instead would orphan the child tasks it spawned (a dropped
    /// `JoinHandle` does not cancel its task).
    Cooperative,
    /// The pass does not take a token and spawns nothing. Drop the in-flight
    /// future on cancellation: abandoning its current sqlx/HTTP await is
    /// safe because any row it leased recovers via `lease_expires_at`.
    DropOnCancel,
}

/// Shared skeleton for the worker's periodic loops: fire immediately, then
/// every `interval`, running `run_pass` each time. Observes `cancel` both
/// while waiting for the next tick and (for `DropOnCancel` passes) while the
/// pass runs, and never starts another pass once cancelled.
async fn poll_loop<F, Fut>(
    name: &'static str,
    interval: Duration,
    cancel: CancellationToken,
    mode: PassMode,
    mut run_pass: F,
) where
    F: FnMut(CancellationToken) -> Fut,
    Fut: Future<Output = ()>,
{
    let mut ticks = tokio::time::interval(interval);
    loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => break,
            _ = ticks.tick() => {}
        }
        // A tick and cancellation can both be ready in the same poll; prefer
        // cancellation and do not begin a pass we were just told to abandon.
        if cancel.is_cancelled() {
            break;
        }
        match mode {
            PassMode::Cooperative => run_pass(cancel.clone()).await,
            PassMode::DropOnCancel => {
                tokio::select! {
                    biased;
                    () = cancel.cancelled() => break,
                    () = run_pass(cancel.clone()) => {}
                }
            }
        }
    }
    tracing::info!(worker = name, "worker loop stopped");
}

async fn market_loop<T: EsiTransport + 'static>(
    worker: Arc<EvidenceWorker<T>>,
    interval: Duration,
    cancel: CancellationToken,
) {
    poll_loop(
        "market",
        interval,
        cancel,
        PassMode::Cooperative,
        move |cancel| {
            let worker = Arc::clone(&worker);
            async move {
                match worker.refresh_due_market(Utc::now(), &cancel).await {
                    Ok(0) => tracing::trace!("no market evidence is due"),
                    Ok(count) => tracing::info!(count, "market evidence refresh pass completed"),
                    Err(error) => tracing::warn!(%error, "market evidence refresh pass failed"),
                }
            }
        },
    )
    .await
}

async fn auth_gc_loop<T: EsiTransport + 'static>(
    worker: Arc<EvidenceWorker<T>>,
    interval: Duration,
    cancel: CancellationToken,
) {
    poll_loop(
        "auth_gc",
        interval,
        cancel,
        PassMode::DropOnCancel,
        move |_cancel| {
            let worker = Arc::clone(&worker);
            async move {
                match worker.purge_expired_auth(Utc::now()).await {
                    Ok(outcome) if outcome == AuthPurgeOutcome::default() => {
                        tracing::trace!("no expired auth rows to purge")
                    }
                    Ok(outcome) => tracing::info!(
                        sessions = outcome.sessions,
                        login_authorizations = outcome.login_authorizations,
                        link_authorizations = outcome.link_authorizations,
                        "purged expired auth rows"
                    ),
                    Err(error) => tracing::warn!(%error, "auth purge pass failed"),
                }
            }
        },
    )
    .await
}

async fn market_gc_loop<T: EsiTransport + 'static>(
    worker: Arc<EvidenceWorker<T>>,
    interval: Duration,
    cancel: CancellationToken,
) {
    poll_loop(
        "market_gc",
        interval,
        cancel,
        PassMode::DropOnCancel,
        move |_cancel| {
            let worker = Arc::clone(&worker);
            async move {
                match worker.sweep_orphaned_market_observations(Utc::now()).await {
                    Ok(outcome)
                        if outcome.observations_deleted == 0 && outcome.batches_deleted == 0 =>
                    {
                        tracing::trace!("no orphaned market observations to prune")
                    }
                    Ok(outcome) => tracing::info!(
                        observations = outcome.observations_deleted,
                        batches = outcome.batches_deleted,
                        drained = outcome.drained,
                        "pruned orphaned market observations"
                    ),
                    Err(error) => tracing::warn!(%error, "market observation GC pass failed"),
                }
            }
        },
    )
    .await
}

async fn esi_gc_loop<T: EsiTransport + 'static>(
    worker: Arc<EvidenceWorker<T>>,
    interval: Duration,
    cancel: CancellationToken,
) {
    poll_loop(
        "esi_gc",
        interval,
        cancel,
        PassMode::DropOnCancel,
        move |_cancel| {
            let worker = Arc::clone(&worker);
            async move {
                match worker.sweep_superseded_esi_observations().await {
                    Ok((assets, prices))
                        if assets.rows_deleted == 0 && prices.rows_deleted == 0 =>
                    {
                        tracing::trace!("no superseded ESI observations to prune")
                    }
                    Ok((assets, prices)) => tracing::info!(
                        asset_snapshots = assets.rows_deleted,
                        adjusted_prices = prices.rows_deleted,
                        drained = assets.drained && prices.drained,
                        "pruned superseded ESI observations"
                    ),
                    Err(error) => tracing::warn!(%error, "ESI observation GC pass failed"),
                }
            }
        },
    )
    .await
}

async fn adjusted_price_loop<T: EsiTransport + 'static>(
    worker: Arc<EvidenceWorker<T>>,
    interval: Duration,
    cancel: CancellationToken,
) {
    poll_loop(
        "adjusted_price",
        interval,
        cancel,
        PassMode::DropOnCancel,
        move |_cancel| {
            let worker = Arc::clone(&worker);
            async move {
                match worker.refresh_due_adjusted_prices(Utc::now()).await {
                    Ok(false) => tracing::trace!("adjusted-price evidence is not due"),
                    Ok(true) => tracing::info!("adjusted-price evidence refreshed"),
                    Err(error) => tracing::warn!(%error, "adjusted-price refresh failed"),
                }
            }
        },
    )
    .await
}

async fn system_index_loop<T: EsiTransport + 'static>(
    worker: Arc<EvidenceWorker<T>>,
    interval: Duration,
    cancel: CancellationToken,
) {
    poll_loop(
        "system_index",
        interval,
        cancel,
        PassMode::DropOnCancel,
        move |_cancel| {
            let worker = Arc::clone(&worker);
            async move {
                match worker.refresh_due_system_indices(Utc::now()).await {
                    Ok(0) => tracing::trace!("no system-index evidence is due"),
                    Ok(count) => {
                        tracing::info!(count, "system-index evidence refresh pass completed")
                    }
                    Err(error) => tracing::warn!(%error, "system-index refresh failed"),
                }
            }
        },
    )
    .await
}

async fn character_sync_loop<T: EsiTransport + 'static>(
    worker: Arc<EvidenceWorker<T>>,
    interval: Duration,
    cancel: CancellationToken,
) {
    poll_loop(
        "character_sync",
        interval,
        cancel,
        PassMode::Cooperative,
        move |cancel| {
            let worker = Arc::clone(&worker);
            async move {
                match worker
                    .refresh_due_character_sources(Utc::now(), &cancel)
                    .await
                {
                    Ok(0) => tracing::trace!("no character sources are due"),
                    Ok(count) => tracing::info!(count, "character sync pass completed"),
                    Err(error) => tracing::warn!(%error, "character sync pass failed"),
                }
            }
        },
    )
    .await
}

/// Outcome of the post-shutdown drain, surfaced for logging and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DrainResult {
    Completed,
    TimedOut,
}

/// Await every loop task, but never longer than `shutdown_timeout`. This is
/// the last-resort bound: cooperative cancellation should make the loops
/// finish well within it, and the timeout only matters if some in-flight
/// await (e.g. a hung ESI response with no HTTP timeout) refuses to yield.
async fn drain_with_timeout(
    tasks: impl Future<Output = ()>,
    shutdown_timeout: Duration,
) -> DrainResult {
    match tokio::time::timeout(shutdown_timeout, tasks).await {
        Ok(()) => DrainResult::Completed,
        Err(_) => DrainResult::TimedOut,
    }
}

/// Run the six periodic worker loops until `shutdown` resolves, then cancel
/// them cooperatively and drain within `config.shutdown_timeout`.
pub async fn run<T, S>(worker: Arc<EvidenceWorker<T>>, config: &WorkerConfig, shutdown: S)
where
    T: EsiTransport + 'static,
    S: Future<Output = ()>,
{
    let cancel = CancellationToken::new();

    let market = tokio::spawn(market_loop(
        Arc::clone(&worker),
        config.market_poll_interval,
        cancel.clone(),
    ));
    let market_gc = tokio::spawn(market_gc_loop(
        Arc::clone(&worker),
        config.market_gc_poll_interval,
        cancel.clone(),
    ));
    let esi_gc = tokio::spawn(esi_gc_loop(
        Arc::clone(&worker),
        config.esi_gc_poll_interval,
        cancel.clone(),
    ));
    let auth_gc = tokio::spawn(auth_gc_loop(
        Arc::clone(&worker),
        config.auth_gc_poll_interval,
        cancel.clone(),
    ));
    let adjusted_price = tokio::spawn(adjusted_price_loop(
        Arc::clone(&worker),
        config.adjusted_price_poll_interval,
        cancel.clone(),
    ));
    let system_index = tokio::spawn(system_index_loop(
        Arc::clone(&worker),
        config.system_index_poll_interval,
        cancel.clone(),
    ));
    let character_sync = tokio::spawn(character_sync_loop(
        worker,
        config.character_sync_poll_interval,
        cancel.clone(),
    ));

    shutdown.await;
    tracing::info!("shutdown signal received; cancelling worker loops");
    cancel.cancel();

    let drain = async {
        let _ = tokio::join!(
            market,
            market_gc,
            esi_gc,
            auth_gc,
            adjusted_price,
            system_index,
            character_sync
        );
    };
    match drain_with_timeout(drain, config.shutdown_timeout).await {
        DrainResult::Completed => tracing::info!("worker graceful drain completed"),
        DrainResult::TimedOut => tracing::warn!(
            timeout_secs = config.shutdown_timeout.as_secs(),
            "worker graceful drain timed out; abandoning in-flight work"
        ),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerConfig {
    pub market_poll_interval: Duration,
    pub market_freshness: Duration,
    /// How often the orphaned-observation GC sweep runs (the periodic
    /// backstop to `complete_esi_market_refresh`'s own prune).
    pub market_gc_poll_interval: Duration,
    /// A batch must be older than this before the GC sweep will touch it,
    /// so an in-flight refresh is never raced.
    pub market_gc_grace: Duration,
    /// How often superseded asset snapshots and adjusted prices are pruned.
    pub esi_gc_poll_interval: Duration,
    /// How often expired sessions and stale pending OAuth authorizations
    /// are purged (`PgAuthMaintenance`).
    pub auth_gc_poll_interval: Duration,
    pub adjusted_price_poll_interval: Duration,
    pub adjusted_price_freshness: Duration,
    pub system_index_poll_interval: Duration,
    pub system_index_freshness: Duration,
    pub lease_duration: Duration,
    pub retry_delay: Duration,
    pub source_batch_size: i64,
    pub market_item_batch_size: i64,
    pub system_batch_size: i64,
    pub character_sync_poll_interval: Duration,
    pub character_sync_batch_size: i64,
    pub character_sync_concurrency: usize,
    pub shutdown_timeout: Duration,
    /// Where the Prometheus `/metrics` listener binds
    /// (`ISKWORKS_METRICS_ADDR`). `None` keeps metrics off entirely.
    pub metrics_addr: Option<SocketAddr>,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            market_poll_interval: Duration::from_secs(5),
            market_freshness: Duration::from_secs(15 * 60),
            market_gc_poll_interval: Duration::from_secs(60 * 60),
            market_gc_grace: Duration::from_secs(60 * 60),
            esi_gc_poll_interval: Duration::from_secs(60 * 60),
            auth_gc_poll_interval: Duration::from_secs(60 * 60),
            adjusted_price_poll_interval: Duration::from_secs(30),
            adjusted_price_freshness: Duration::from_secs(6 * 60 * 60),
            system_index_poll_interval: Duration::from_secs(30),
            system_index_freshness: Duration::from_secs(60 * 60),
            lease_duration: Duration::from_secs(2 * 60),
            retry_delay: Duration::from_secs(60),
            source_batch_size: 10,
            market_item_batch_size: 100,
            system_batch_size: 100,
            character_sync_poll_interval: Duration::from_secs(30),
            character_sync_batch_size: 50,
            character_sync_concurrency: 4,
            shutdown_timeout: Duration::from_secs(30),
            metrics_addr: None,
        }
    }
}

impl WorkerConfig {
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    fn from_lookup(mut lookup: impl FnMut(&str) -> Option<String>) -> Result<Self, String> {
        let mut config = Self::default();
        config.market_poll_interval = duration_setting(
            &mut lookup,
            "ISKWORKS_WORKER_MARKET_POLL_SECONDS",
            config.market_poll_interval,
        )?;
        config.market_freshness = duration_setting(
            &mut lookup,
            "ISKWORKS_WORKER_MARKET_FRESHNESS_SECONDS",
            config.market_freshness,
        )?;
        config.market_gc_poll_interval = duration_setting(
            &mut lookup,
            "ISKWORKS_WORKER_MARKET_GC_POLL_SECONDS",
            config.market_gc_poll_interval,
        )?;
        config.market_gc_grace = duration_setting(
            &mut lookup,
            "ISKWORKS_WORKER_MARKET_GC_GRACE_SECONDS",
            config.market_gc_grace,
        )?;
        config.esi_gc_poll_interval = duration_setting(
            &mut lookup,
            "ISKWORKS_WORKER_ESI_GC_POLL_SECONDS",
            config.esi_gc_poll_interval,
        )?;
        config.auth_gc_poll_interval = duration_setting(
            &mut lookup,
            "ISKWORKS_WORKER_AUTH_GC_POLL_SECONDS",
            config.auth_gc_poll_interval,
        )?;
        config.adjusted_price_poll_interval = duration_setting(
            &mut lookup,
            "ISKWORKS_WORKER_ADJUSTED_PRICE_POLL_SECONDS",
            config.adjusted_price_poll_interval,
        )?;
        config.adjusted_price_freshness = duration_setting(
            &mut lookup,
            "ISKWORKS_WORKER_ADJUSTED_PRICE_FRESHNESS_SECONDS",
            config.adjusted_price_freshness,
        )?;
        config.system_index_poll_interval = duration_setting(
            &mut lookup,
            "ISKWORKS_WORKER_SYSTEM_INDEX_POLL_SECONDS",
            config.system_index_poll_interval,
        )?;
        config.system_index_freshness = duration_setting(
            &mut lookup,
            "ISKWORKS_WORKER_SYSTEM_INDEX_FRESHNESS_SECONDS",
            config.system_index_freshness,
        )?;
        config.lease_duration = duration_setting(
            &mut lookup,
            "ISKWORKS_WORKER_LEASE_SECONDS",
            config.lease_duration,
        )?;
        config.retry_delay = duration_setting(
            &mut lookup,
            "ISKWORKS_WORKER_RETRY_SECONDS",
            config.retry_delay,
        )?;
        config.shutdown_timeout = duration_setting(
            &mut lookup,
            "ISKWORKS_WORKER_SHUTDOWN_TIMEOUT_SECONDS",
            config.shutdown_timeout,
        )?;
        config.source_batch_size = positive_i64_setting(
            &mut lookup,
            "ISKWORKS_WORKER_SOURCE_BATCH_SIZE",
            config.source_batch_size,
        )?;
        config.market_item_batch_size = positive_i64_setting(
            &mut lookup,
            "ISKWORKS_WORKER_MARKET_ITEM_BATCH_SIZE",
            config.market_item_batch_size,
        )?;
        config.system_batch_size = positive_i64_setting(
            &mut lookup,
            "ISKWORKS_WORKER_SYSTEM_BATCH_SIZE",
            config.system_batch_size,
        )?;
        config.character_sync_poll_interval = duration_setting(
            &mut lookup,
            "ISKWORKS_WORKER_CHARACTER_SYNC_POLL_SECONDS",
            config.character_sync_poll_interval,
        )?;
        config.character_sync_batch_size = positive_i64_setting(
            &mut lookup,
            "ISKWORKS_WORKER_CHARACTER_SYNC_BATCH_SIZE",
            config.character_sync_batch_size,
        )?;
        config.character_sync_concurrency = positive_i64_setting(
            &mut lookup,
            "ISKWORKS_WORKER_CHARACTER_SYNC_CONCURRENCY",
            config.character_sync_concurrency as i64,
        )?
        .try_into()
        .map_err(|_| "ISKWORKS_WORKER_CHARACTER_SYNC_CONCURRENCY must fit in usize".to_string())?;
        config.metrics_addr = metrics_exporter::metrics_addr(lookup("ISKWORKS_METRICS_ADDR"))?;
        Ok(config)
    }
}

fn positive_i64_setting(
    lookup: &mut impl FnMut(&str) -> Option<String>,
    name: &str,
    default: i64,
) -> Result<i64, String> {
    let Some(value) = lookup(name) else {
        return Ok(default);
    };
    value
        .parse::<i64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{name} must be a positive integer"))
}

fn duration_setting(
    lookup: &mut impl FnMut(&str) -> Option<String>,
    name: &str,
    default: Duration,
) -> Result<Duration, String> {
    let Some(value) = lookup(name) else {
        return Ok(default);
    };
    let seconds = value
        .parse::<u64>()
        .map_err(|_| format!("{name} must be a positive integer"))?;
    if seconds == 0 {
        return Err(format!("{name} must be greater than zero"));
    }
    Ok(Duration::from_secs(seconds))
}

#[cfg(test)]
mod tests;
