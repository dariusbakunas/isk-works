//! Prometheus metrics for ESI traffic (see `docs/monitoring.md`). Recorded
//! through the `metrics` facade: with no recorder installed (the binaries
//! install one only when `ISKWORKS_METRICS_ADDR` is set) every call here is
//! a no-op.
//!
//! Labels never carry IDs or tokens. Routes are a closed set of fixed names
//! ([`EsiRoute`]), never derived from URLs.

use std::time::Instant;

use reqwest::StatusCode;

use crate::error_limit::ERROR_LIMITED_STATUS;

/// Every ESI/SSO endpoint this crate calls, named for metric labels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EsiRoute {
    CharacterAssets,
    CharacterBlueprints,
    CharacterWalletTransactions,
    CharacterWallet,
    CharacterWalletJournal,
    UniverseNames,
    UniverseStructure,
    IndustrySystems,
    MarketPrices,
    RegionMarketOrders,
    StructureMarketOrders,
    CharacterPublicInfo,
    CharacterLocation,
    CharacterSkills,
    CharacterSkillQueue,
    CharacterIndustryJobs,
    CharacterPlanets,
    CharacterPlanetDetail,
    /// The downtime guard's `/status/` probe.
    Status,
    SsoToken,
    SsoRevoke,
    SsoJwks,
}

impl EsiRoute {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 22] = [
        Self::CharacterAssets,
        Self::CharacterBlueprints,
        Self::CharacterWalletTransactions,
        Self::CharacterWallet,
        Self::CharacterWalletJournal,
        Self::UniverseNames,
        Self::UniverseStructure,
        Self::IndustrySystems,
        Self::MarketPrices,
        Self::RegionMarketOrders,
        Self::StructureMarketOrders,
        Self::CharacterPublicInfo,
        Self::CharacterLocation,
        Self::CharacterSkills,
        Self::CharacterSkillQueue,
        Self::CharacterIndustryJobs,
        Self::CharacterPlanets,
        Self::CharacterPlanetDetail,
        Self::Status,
        Self::SsoToken,
        Self::SsoRevoke,
        Self::SsoJwks,
    ];

    /// The `route` label: ESI's own path template, or `sso:*` for EVE SSO.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::CharacterAssets => "/characters/{character_id}/assets/",
            Self::CharacterBlueprints => "/characters/{character_id}/blueprints/",
            Self::CharacterWalletTransactions => "/characters/{character_id}/wallet/transactions/",
            Self::CharacterWallet => "/characters/{character_id}/wallet/",
            Self::CharacterWalletJournal => "/characters/{character_id}/wallet/journal/",
            Self::UniverseNames => "/universe/names/",
            Self::UniverseStructure => "/universe/structures/{structure_id}/",
            Self::IndustrySystems => "/industry/systems/",
            Self::MarketPrices => "/markets/prices/",
            Self::RegionMarketOrders => "/markets/{region_id}/orders/",
            Self::StructureMarketOrders => "/markets/structures/{structure_id}/",
            Self::CharacterPublicInfo => "/characters/{character_id}/",
            Self::CharacterLocation => "/characters/{character_id}/location/",
            Self::CharacterSkills => "/characters/{character_id}/skills/",
            Self::CharacterSkillQueue => "/characters/{character_id}/skillqueue/",
            Self::CharacterIndustryJobs => "/characters/{character_id}/industry/jobs/",
            Self::CharacterPlanets => "/characters/{character_id}/planets/",
            Self::CharacterPlanetDetail => "/characters/{character_id}/planets/{planet_id}/",
            Self::Status => "/status/",
            Self::SsoToken => "sso:token",
            Self::SsoRevoke => "sso:revoke",
            Self::SsoJwks => "sso:jwks",
        }
    }

    fn method(self) -> &'static str {
        match self {
            Self::UniverseNames | Self::SsoToken | Self::SsoRevoke => "POST",
            _ => "GET",
        }
    }

    /// Routes ESI paginates with `?page=`.
    fn paged(self) -> bool {
        matches!(
            self,
            Self::CharacterAssets
                | Self::CharacterBlueprints
                | Self::CharacterWalletJournal
                | Self::RegionMarketOrders
                | Self::StructureMarketOrders
        )
    }
}

/// Records one ESI request. Start it once the guards have let the request
/// through, then call `sent` (or `transport_error`/`responded`) once. A
/// request the guards refused is recorded with [`RequestTimer::blocked`].
pub(crate) struct RequestTimer {
    route: EsiRoute,
    started: Instant,
}

impl RequestTimer {
    pub(crate) fn start(route: EsiRoute) -> Self {
        Self {
            route,
            started: Instant::now(),
        }
    }

    /// A local guard (error budget, rate-limit pause, downtime) refused to
    /// send. Nothing reached ESI, so no duration is recorded.
    pub(crate) fn blocked(route: EsiRoute) {
        Self::start(route).count("none", "blocked");
    }

    /// Records whatever `send()` came back with.
    pub(crate) fn sent(self, result: &Result<reqwest::Response, reqwest::Error>) {
        match result {
            Ok(response) => self.responded(response.status()),
            Err(_) => self.transport_error(),
        }
    }

    /// No HTTP response: connect failure, timeout, reset.
    pub(crate) fn transport_error(self) {
        self.observe_duration();
        self.count("none", "transport_error");
    }

    /// ESI answered. Duration covers send through response headers.
    pub(crate) fn responded(self, status: StatusCode) {
        self.observe_duration();
        let outcome = outcome(status);
        self.count(status_class(status), outcome);
        let route = self.route.label();
        if outcome == "not_modified" {
            metrics::counter!("iskworks_esi_not_modified_total", "route" => route).increment(1);
        }
        if self.route.paged() && matches!(outcome, "ok" | "not_modified") {
            metrics::counter!("iskworks_esi_pages_fetched_total", "route" => route).increment(1);
        }
    }

    fn observe_duration(&self) {
        metrics::histogram!(
            "iskworks_esi_request_duration_seconds",
            "route" => self.route.label()
        )
        .record(self.started.elapsed().as_secs_f64());
    }

    fn count(&self, status_class: &'static str, outcome: &'static str) {
        metrics::counter!(
            "iskworks_esi_requests_total",
            "route" => self.route.label(),
            "method" => self.route.method(),
            "status_class" => status_class,
            "outcome" => outcome
        )
        .increment(1);
    }
}

fn status_class(status: StatusCode) -> &'static str {
    match status.as_u16() {
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "other",
    }
}

fn outcome(status: StatusCode) -> &'static str {
    match status {
        StatusCode::NOT_MODIFIED => "not_modified",
        status if status.is_success() => "ok",
        status if status.as_u16() == ERROR_LIMITED_STATUS => "error_limited",
        // 418 is how `classify_status` treats a teapot-ing proxy: rate limited.
        StatusCode::TOO_MANY_REQUESTS | StatusCode::IM_A_TEAPOT => "rate_limited",
        status if status.is_server_error() => "server_error",
        _ => "client_error",
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Reads what a test recorded. Install the recorder thread-locally
    //! (`metrics::set_default_local_recorder`); `#[tokio::test]` runs the
    //! transport and its fake server on that one thread.

    use metrics_util::debugging::{DebugValue, Snapshotter};
    use metrics_util::CompositeKey;

    /// One snapshot of everything recorded so far. Taking a snapshot drains
    /// the debugging recorder's counters, so take it once, then query.
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

        /// The counter summed over every series matching `labels`.
        pub(crate) fn counter(&self, name: &str, labels: &[(&str, &str)]) -> u64 {
            self.matching(name, labels)
                .map(|value| match value {
                    DebugValue::Counter(value) => *value,
                    other => panic!("{name} is not a counter: {other:?}"),
                })
                .sum()
        }

        pub(crate) fn gauge(&self, name: &str, labels: &[(&str, &str)]) -> Option<f64> {
            self.matching(name, labels).last().map(|value| match value {
                DebugValue::Gauge(value) => value.into_inner(),
                other => panic!("{name} is not a gauge: {other:?}"),
            })
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
    use super::test_support::*;
    use super::*;
    use metrics_util::debugging::DebuggingRecorder;

    #[test]
    fn route_labels_are_unique_templates_without_ids() {
        let labels: std::collections::BTreeSet<_> =
            EsiRoute::ALL.iter().map(|route| route.label()).collect();
        assert_eq!(labels.len(), EsiRoute::ALL.len());
        for label in labels {
            assert!(
                !label.bytes().any(|b| b.is_ascii_digit()),
                "{label} carries an ID"
            );
        }
    }

    #[test]
    fn responses_are_counted_by_outcome_and_status_class() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        metrics::with_local_recorder(&recorder, || {
            RequestTimer::start(EsiRoute::CharacterAssets).responded(StatusCode::OK);
            RequestTimer::start(EsiRoute::CharacterAssets).responded(StatusCode::NOT_MODIFIED);
            RequestTimer::start(EsiRoute::CharacterAssets)
                .responded(StatusCode::from_u16(420).unwrap());
            RequestTimer::start(EsiRoute::MarketPrices).responded(StatusCode::TOO_MANY_REQUESTS);
            RequestTimer::start(EsiRoute::MarketPrices).responded(StatusCode::BAD_GATEWAY);
            RequestTimer::start(EsiRoute::MarketPrices).responded(StatusCode::NOT_FOUND);
            RequestTimer::start(EsiRoute::UniverseNames).transport_error();
            RequestTimer::blocked(EsiRoute::UniverseNames);
        });

        let recorded = Recorded::take(&snapshotter);
        let requests =
            |labels: &[(&str, &str)]| recorded.counter("iskworks_esi_requests_total", labels);
        let assets = "/characters/{character_id}/assets/";
        assert_eq!(
            requests(&[
                ("route", assets),
                ("outcome", "ok"),
                ("status_class", "2xx"),
                ("method", "GET")
            ]),
            1
        );
        assert_eq!(
            requests(&[
                ("route", assets),
                ("outcome", "not_modified"),
                ("status_class", "3xx")
            ]),
            1
        );
        assert_eq!(
            requests(&[
                ("route", assets),
                ("outcome", "error_limited"),
                ("status_class", "4xx")
            ]),
            1
        );
        assert_eq!(
            requests(&[("route", "/markets/prices/"), ("outcome", "rate_limited")]),
            1
        );
        assert_eq!(
            requests(&[
                ("route", "/markets/prices/"),
                ("outcome", "server_error"),
                ("status_class", "5xx")
            ]),
            1
        );
        assert_eq!(
            requests(&[("route", "/markets/prices/"), ("outcome", "client_error")]),
            1
        );
        assert_eq!(
            requests(&[
                ("route", "/universe/names/"),
                ("outcome", "transport_error"),
                ("status_class", "none"),
                ("method", "POST")
            ]),
            1
        );
        assert_eq!(
            requests(&[("route", "/universe/names/"), ("outcome", "blocked")]),
            1
        );

        assert_eq!(
            recorded.counter("iskworks_esi_not_modified_total", &[("route", assets)]),
            1
        );
        // Paged route: the 200 and the 304 are pages; the 420 isn't.
        assert_eq!(
            recorded.counter("iskworks_esi_pages_fetched_total", &[("route", assets)]),
            2
        );
        assert_eq!(recorded.counter("iskworks_esi_pages_fetched_total", &[]), 2);
        // Blocked requests never reached ESI: no duration sample.
        assert_eq!(
            recorded.histogram_count(
                "iskworks_esi_request_duration_seconds",
                &[("route", "/universe/names/")]
            ),
            1
        );
        assert_eq!(
            recorded.histogram_count(
                "iskworks_esi_request_duration_seconds",
                &[("route", assets)]
            ),
            3
        );
    }
}
