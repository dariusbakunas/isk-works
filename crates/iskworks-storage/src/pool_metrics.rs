//! Prometheus metrics for the Postgres pool (issue #14). Recording is a
//! no-op unless a binary installed a recorder (`ISKWORKS_METRICS_ADDR`).

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::{Context, Filter, Layer};

const POOL_CONNECTIONS: &str = "iskworks_db_pool_connections";
const POOL_MAX_CONNECTIONS: &str = "iskworks_db_pool_max_connections";
/// The exporter's listener has no scrape-time hook, so the gauges are
/// sampled. Comfortably finer than the 15 s scrape interval.
const SAMPLE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

const ACQUIRE_TIMEOUTS: &str = "iskworks_db_pool_acquire_timeouts_total";
pub const ACQUIRE_DURATION: &str = "iskworks_db_pool_acquire_duration_seconds";

/// Buckets for [`ACQUIRE_DURATION`]. An uncontended acquire takes well under
/// a millisecond, so these start much finer than the shared
/// `_duration_seconds` buckets and run to the 30 s acquire timeout.
pub const ACQUIRE_BUCKETS: &[f64] = &[
    0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0,
];

/// sqlx's event for a finished acquire. The pool emits it on every acquire
/// at `acquire_time_level` (TRACE, see `DbPoolConfig::pool_options`), or at
/// WARN past `acquire_slow_threshold`. Both carry the wait.
const ACQUIRE_TARGET: &str = "sqlx::pool::acquire";
/// Sic: sqlx spells the field this way.
const ACQUIRED_AFTER_FIELD: &str = "aquired_after_secs";

/// Creates the alert-watched pool counters at 0, so the first timeout after
/// a restart shows up in `increase()`. Called from each binary's exporter
/// `install`.
pub fn init_metrics() {
    metrics::counter!(ACQUIRE_TIMEOUTS).increment(0);
}

/// Counts a pool acquire timeout: sqlx emits nothing of its own when an
/// acquire gives up. Each module's `sqlx::Error` mapper calls this. The few
/// inline `map_err` closures don't, so the count is a floor, not exact.
/// That's enough for an alert that fires on any timeout.
pub(crate) fn observe_error(error: &sqlx::Error) {
    if matches!(error, sqlx::Error::PoolTimedOut) {
        metrics::counter!(ACQUIRE_TIMEOUTS).increment(1);
    }
}

/// Samples the pool's in-use, idle and max connections every few seconds
/// for as long as the process runs. Start it only when metrics are on.
pub fn spawn_pool_sampler(pool: sqlx::PgPool) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(SAMPLE_INTERVAL);
        loop {
            ticks.tick().await;
            record_pool_gauges(&pool);
        }
    })
}

fn record_pool_gauges(pool: &sqlx::PgPool) {
    let open = pool.size();
    // `size` and `num_idle` are read separately, so a release between the
    // two can briefly make idle exceed open.
    let idle = u32::try_from(pool.num_idle()).unwrap_or(u32::MAX).min(open);
    metrics::gauge!(POOL_CONNECTIONS, "state" => "in_use").set(f64::from(open - idle));
    metrics::gauge!(POOL_CONNECTIONS, "state" => "idle").set(f64::from(idle));
    metrics::gauge!(POOL_MAX_CONNECTIONS).set(f64::from(pool.options().get_max_connections()));
}

/// A tracing layer that records each pool acquire's wait into
/// [`ACQUIRE_DURATION`]. It carries its own filter, so add it next to a
/// fmt layer that has its *own* `EnvFilter` (`.with_filter(..)`). A global
/// `EnvFilter` at `info` would drop sqlx's TRACE event before this layer
/// sees it.
pub fn db_pool_metrics_layer<S>() -> impl Layer<S>
where
    S: Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    AcquireRecorder.with_filter(acquire_events())
}

fn acquire_events<S>() -> impl Filter<S> {
    Targets::new().with_target(ACQUIRE_TARGET, Level::TRACE)
}

struct AcquireRecorder;

impl<S: Subscriber> Layer<S> for AcquireRecorder {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut waited = AcquiredAfter(None);
        event.record(&mut waited);
        if let Some(seconds) = waited.0 {
            metrics::histogram!(ACQUIRE_DURATION).record(seconds);
        }
    }
}

struct AcquiredAfter(Option<f64>);

impl Visit for AcquiredAfter {
    fn record_f64(&mut self, field: &Field, value: f64) {
        if field.name() == ACQUIRED_AFTER_FIELD {
            self.0 = Some(value);
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DbPoolConfig;
    use metrics_util::debugging::{DebugValue, DebuggingRecorder};
    use tracing_subscriber::layer::SubscriberExt;

    fn acquire_timeouts(recorder: &DebuggingRecorder) -> Option<u64> {
        recorder
            .snapshotter()
            .snapshot()
            .into_vec()
            .into_iter()
            .find(|(key, ..)| key.key().name() == ACQUIRE_TIMEOUTS)
            .map(|(.., value)| match value {
                DebugValue::Counter(count) => count,
                other => panic!("{ACQUIRE_TIMEOUTS} is not a counter: {other:?}"),
            })
    }

    #[test]
    fn init_creates_the_timeout_counter_at_zero() {
        let recorder = DebuggingRecorder::new();
        metrics::with_local_recorder(&recorder, init_metrics);
        assert_eq!(acquire_timeouts(&recorder), Some(0));
    }

    #[test]
    fn only_pool_timeouts_are_counted() {
        let recorder = DebuggingRecorder::new();
        metrics::with_local_recorder(&recorder, || {
            observe_error(&sqlx::Error::RowNotFound);
            observe_error(&sqlx::Error::PoolClosed);
            observe_error(&sqlx::Error::PoolTimedOut);
        });
        assert_eq!(acquire_timeouts(&recorder), Some(1));
    }

    #[test]
    fn workspace_errors_count_pool_timeouts() {
        let recorder = DebuggingRecorder::new();
        metrics::with_local_recorder(&recorder, || {
            let _ = crate::workspace::map_sqlx_error(sqlx::Error::PoolTimedOut);
        });
        assert_eq!(acquire_timeouts(&recorder), Some(1));
    }

    fn acquire_waits(recorder: &DebuggingRecorder) -> Vec<f64> {
        recorder
            .snapshotter()
            .snapshot()
            .into_vec()
            .into_iter()
            .filter(|(key, ..)| key.key().name() == ACQUIRE_DURATION)
            .flat_map(|(.., value)| match value {
                DebugValue::Histogram(values) => values.into_iter().map(|v| v.into_inner()),
                other => panic!("{ACQUIRE_DURATION} is not a histogram: {other:?}"),
            })
            .collect()
    }

    /// Emits what sqlx's pool emits, under a subscriber shaped like the
    /// binaries': fmt-style filtering at `info` must not hide the TRACE
    /// acquire event from the metrics layer.
    #[test]
    fn records_acquire_waits_from_sqlx_events_at_any_level() {
        let recorder = DebuggingRecorder::new();
        let subscriber = tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_test_writer()
                    .with_filter(tracing_subscriber::EnvFilter::new("info")),
            )
            .with(db_pool_metrics_layer());
        metrics::with_local_recorder(&recorder, || {
            tracing::subscriber::with_default(subscriber, || {
                tracing::event!(
                    target: "sqlx::pool::acquire",
                    Level::TRACE,
                    aquired_after_secs = 0.002_f64,
                    "acquired connection"
                );
                tracing::event!(
                    target: "sqlx::pool::acquire",
                    Level::WARN,
                    aquired_after_secs = 3.5_f64,
                    slow_acquire_threshold_secs = 2.0_f64,
                    "acquired connection, but time to acquire exceeded slow threshold"
                );
                // Other targets and events without the field are ignored.
                tracing::event!(target: "sqlx::query", Level::WARN, aquired_after_secs = 9.0_f64);
                tracing::event!(target: "sqlx::pool::acquire", Level::TRACE, "no wait field");
            });
        });
        assert_eq!(acquire_waits(&recorder), vec![0.002, 3.5]);
    }

    #[tokio::test]
    async fn pool_gauges_report_open_idle_and_max_connections() {
        let pool = DbPoolConfig {
            max_connections: 7,
            slow_statement: None,
        }
        .pool_options()
        .connect_lazy("postgres://unused@127.0.0.1:1/unused")
        .unwrap();
        let recorder = DebuggingRecorder::new();
        metrics::with_local_recorder(&recorder, || record_pool_gauges(&pool));

        let mut gauges: Vec<(String, f64)> = recorder
            .snapshotter()
            .snapshot()
            .into_vec()
            .into_iter()
            .map(|(key, _, _, value)| {
                let labels: Vec<String> = key
                    .key()
                    .labels()
                    .map(|label| label.value().to_string())
                    .collect();
                let DebugValue::Gauge(value) = value else {
                    panic!("{} is not a gauge", key.key().name());
                };
                (
                    format!("{}{labels:?}", key.key().name()),
                    value.into_inner(),
                )
            })
            .collect();
        gauges.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            gauges,
            vec![
                ("iskworks_db_pool_connections[\"idle\"]".to_string(), 0.0),
                ("iskworks_db_pool_connections[\"in_use\"]".to_string(), 0.0),
                ("iskworks_db_pool_max_connections[]".to_string(), 7.0),
            ]
        );
    }
}
