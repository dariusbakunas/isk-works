//! Prometheus metrics for the Postgres pool (issue #14). Recording is a
//! no-op unless a binary installed a recorder (`ISKWORKS_METRICS_ADDR`).

const ACQUIRE_TIMEOUTS: &str = "iskworks_db_pool_acquire_timeouts_total";

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

#[cfg(test)]
mod tests {
    use super::*;
    use metrics_util::debugging::{DebugValue, DebuggingRecorder};

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
}
