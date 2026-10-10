//! The worker's Prometheus `/metrics` listener. The API carries an
//! identical copy (`iskworks-api`'s `metrics_exporter`): the exporter brings
//! an HTTP server, which `iskworks-app` must stay free of, and the shared
//! part is too small for a crate of its own.

use std::net::SocketAddr;

use metrics_exporter_prometheus::{BuildError, Matcher, PrometheusBuilder};

/// Buckets for every `*_duration_seconds` histogram: single ESI requests
/// (tens of ms to the 30 s request deadline) through whole sync runs and
/// worker passes (minutes).
const DURATION_BUCKETS: &[f64] = &[
    0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0,
];

/// `ISKWORKS_METRICS_ADDR`: unset or blank keeps metrics off.
pub(crate) fn metrics_addr(value: Option<String>) -> Result<Option<SocketAddr>, String> {
    match value.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(value) => value
            .parse()
            .map(Some)
            .map_err(|_| "ISKWORKS_METRICS_ADDR must be a valid socket address".to_string()),
    }
}

/// Installs the global metrics recorder and serves it on `addr`. Must run
/// inside the Tokio runtime, which the listener is spawned onto. Without
/// this call every `metrics` macro in the process is a no-op.
pub fn install(addr: SocketAddr) -> Result<(), BuildError> {
    PrometheusBuilder::new()
        .with_http_listener(addr)
        .set_buckets_for_metric(
            Matcher::Suffix("_duration_seconds".to_string()),
            DURATION_BUCKETS,
        )?
        .install()?;
    // Series alerts watch must exist at 0 before their first increment.
    iskworks_esi::init_metrics();
    iskworks_app::init_sync_metrics();
    crate::init_loop_metrics();
    let version = std::env::var("APP_VERSION").unwrap_or_else(|_| "dev".to_string());
    metrics::gauge!("iskworks_build_info", "service" => "worker", "version" => version).set(1.0);
    Ok(())
}
