//! Durations of a few known-heavy operations (issue #14), so the dashboard
//! can say which one a slow route or worker pass spent its time in. The
//! `operation` label is this closed enum; nothing about the workspace or
//! the data goes in it. No-ops without a recorder.

use std::future::Future;
use std::time::Instant;

const DURATION: &str = "iskworks_operation_duration_seconds";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
    /// The canonical build-materials projection (`project_build_materials`),
    /// behind the build worksheet and Epic snapshots.
    BuildMaterials,
    /// Writing one character's ESI asset snapshot (`complete_assets`).
    AssetSnapshotComplete,
    /// Writing one refreshed market order book, private source or public.
    MarketBookWrite,
}

impl Operation {
    fn label(self) -> &'static str {
        match self {
            Self::BuildMaterials => "build_materials",
            Self::AssetSnapshotComplete => "asset_snapshot_complete",
            Self::MarketBookWrite => "market_book_write",
        }
    }

    /// Awaits `work` and records how long it took, whether it succeeded
    /// or not.
    pub(crate) async fn timed<T>(self, work: impl Future<Output = T>) -> T {
        let started = Instant::now();
        let output = work.await;
        metrics::histogram!(DURATION, "operation" => self.label())
            .record(started.elapsed().as_secs_f64());
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use metrics_util::debugging::{DebugValue, DebuggingRecorder};

    #[test]
    fn labels_are_stable() {
        // Dashboards and alerts query these; renaming one breaks them.
        assert_eq!(Operation::BuildMaterials.label(), "build_materials");
        assert_eq!(
            Operation::AssetSnapshotComplete.label(),
            "asset_snapshot_complete"
        );
        assert_eq!(Operation::MarketBookWrite.label(), "market_book_write");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn timed_records_one_observation_and_passes_the_output_through() {
        let recorder = DebuggingRecorder::new();
        let _guard = metrics::set_default_local_recorder(&recorder);

        let output = Operation::MarketBookWrite
            .timed(async { Err::<(), _>("write failed") })
            .await;

        assert_eq!(output, Err("write failed"));
        let recorded: Vec<_> = recorder
            .snapshotter()
            .snapshot()
            .into_vec()
            .into_iter()
            .map(|(key, _, _, value)| {
                let labels: Vec<String> = key
                    .key()
                    .labels()
                    .map(|label| format!("{}={}", label.key(), label.value()))
                    .collect();
                let DebugValue::Histogram(values) = value else {
                    panic!("{} is not a histogram", key.key().name());
                };
                (key.key().name().to_string(), labels, values.len())
            })
            .collect();
        assert_eq!(
            recorded,
            vec![(
                DURATION.to_string(),
                vec!["operation=market_book_write".to_string()],
                1
            )]
        );
    }
}
