//! Prometheus metrics for ESI sync runs (see `docs/monitoring.md`).
//! Character source refreshes record in `CharacterSyncService`, manual
//! asset/wallet imports in `EsiApplicationService::sync`; the two never
//! record the same run. Which process ran it (API or worker) is the
//! scrape job's business, not a label. No-ops without a recorder.

use std::time::Instant;

/// How a sync run ended, as the `result` label.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SyncResult {
    Success,
    /// Finished, but some of it failed (`PartiallySucceeded`).
    Incomplete,
    Failed,
}

impl SyncResult {
    const ALL: [Self; 3] = [Self::Success, Self::Incomplete, Self::Failed];

    fn label(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Incomplete => "incomplete",
            Self::Failed => "failed",
        }
    }
}

/// Creates every `iskworks_esi_sync_runs_total` series at 0, so the first
/// failure after a restart shows in `increase()`/`rate()` (and in the
/// `EsiSyncFailing` alert). Call once at startup, after installing the
/// recorder.
pub fn init_metrics() {
    for kind in iskworks_core::CharacterSourceKind::all() {
        for result in SyncResult::ALL {
            metrics::counter!(
                "iskworks_esi_sync_runs_total",
                "kind" => kind.as_db_str(),
                "result" => result.label()
            )
            .increment(0);
        }
    }
}

/// Records one finished sync run of `kind` that started at `started`.
pub(crate) fn record_sync_run(kind: &'static str, result: SyncResult, started: Instant) {
    metrics::counter!(
        "iskworks_esi_sync_runs_total",
        "kind" => kind,
        "result" => result.label()
    )
    .increment(1);
    metrics::histogram!("iskworks_esi_sync_duration_seconds", "kind" => kind)
        .record(started.elapsed().as_secs_f64());
    if result == SyncResult::Success {
        metrics::gauge!("iskworks_esi_sync_last_success_timestamp_seconds", "kind" => kind)
            .set(chrono::Utc::now().timestamp() as f64);
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Reads what a test recorded through a thread-local
    //! `DebuggingRecorder`. Taking a snapshot drains its counters, so take
    //! one and query it.

    use metrics_util::debugging::{DebugValue, Snapshotter};
    use metrics_util::CompositeKey;

    pub(crate) struct Recorded(Vec<(CompositeKey, DebugValue)>);

    impl Recorded {
        pub(crate) fn take(snapshotter: &Snapshotter) -> Self {
            Self(
                snapshotter
                    .snapshot()
                    .into_vec()
                    .into_iter()
                    .map(|(key, _, _, value)| (key, value))
                    .collect(),
            )
        }

        fn matching<'a>(
            &'a self,
            name: &'a str,
            labels: &'a [(&str, &str)],
        ) -> impl Iterator<Item = &'a DebugValue> {
            self.0
                .iter()
                .filter(move |(key, _)| {
                    let key = key.key();
                    key.name() == name
                        && labels.iter().all(|(label, value)| {
                            key.labels()
                                .any(|l| l.key() == *label && l.value() == *value)
                        })
                })
                .map(|(_, value)| value)
        }

        pub(crate) fn counter(&self, name: &str, labels: &[(&str, &str)]) -> u64 {
            self.matching(name, labels)
                .map(|value| match value {
                    DebugValue::Counter(value) => *value,
                    other => panic!("{name} is not a counter: {other:?}"),
                })
                .sum()
        }

        pub(crate) fn has(&self, name: &str, labels: &[(&str, &str)]) -> bool {
            self.matching(name, labels).next().is_some()
        }

        pub(crate) fn histogram_count(&self, name: &str, labels: &[(&str, &str)]) -> usize {
            self.matching(name, labels)
                .map(|value| match value {
                    DebugValue::Histogram(values) => values.len(),
                    other => panic!("{name} is not a histogram: {other:?}"),
                })
                .sum()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::Recorded;
    use super::*;
    use metrics_util::debugging::DebuggingRecorder;

    #[test]
    fn every_kind_and_result_exists_at_zero_after_init() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        metrics::with_local_recorder(&recorder, init_metrics);

        let recorded = Recorded::take(&snapshotter);
        for kind in iskworks_core::CharacterSourceKind::all() {
            for result in SyncResult::ALL {
                let labels = [("kind", kind.as_db_str()), ("result", result.label())];
                assert!(
                    recorded.has("iskworks_esi_sync_runs_total", &labels),
                    "{labels:?}"
                );
            }
        }
        assert_eq!(recorded.counter("iskworks_esi_sync_runs_total", &[]), 0);
    }
}
