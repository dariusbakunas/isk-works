//! ESI's floating-window rate limits. Each route belongs to a group
//! (`X-Ratelimit-Group`), and each group has a token bucket per caller: per
//! character for authenticated routes, per IP for public ones. A request
//! past the bucket gets 429 with `Retry-After`.
//!
//! This guard keeps the whole process out of a bucket ESI has closed: a 429
//! pauses that group for that caller until `Retry-After`, and a bucket
//! that is nearly empty (`X-Ratelimit-Remaining`) pauses briefly before ESI
//! has to refuse anything. A route's group is only known from ESI's answer,
//! so it is learned from every response.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use reqwest::header::HeaderMap;
use reqwest::StatusCode;

use crate::EsiError;

/// Wait after a 429 that didn't say how long.
const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(60);
/// Never trust a `Retry-After` longer than ESI's 15-minute window.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(15 * 60);
/// Pause a bucket once fewer than this share of its tokens remain...
const LOW_REMAINING_FRACTION: u64 = 20;
/// ...for this long, letting the floating window give tokens back.
const LOW_REMAINING_PAUSE: Duration = Duration::from_secs(30);

/// Who a bucket belongs to: the access token's holder, or (public routes)
/// this IP. Tokens are stored hashed; a refreshed token starts a new key,
/// which at worst costs one more 429 from ESI.
pub(crate) type Caller = Option<u64>;

pub(crate) fn caller(access_token: Option<&str>) -> Caller {
    access_token.map(|token| {
        let mut hasher = DefaultHasher::new();
        token.hash(&mut hasher);
        hasher.finish()
    })
}

#[derive(Debug, Default)]
struct State {
    /// Route template -> rate-limit group, as ESI reported it.
    groups: HashMap<String, String>,
    /// (group, caller) -> paused until.
    paused: HashMap<(String, Caller), Instant>,
}

#[derive(Debug, Default)]
pub(crate) struct RateLimitGuard {
    state: Mutex<State>,
}

impl RateLimitGuard {
    /// The guard shared by every transport in this process -- public
    /// buckets are per IP. Unit tests get their own, as with `error_limit`.
    pub(crate) fn global() -> Arc<Self> {
        if cfg!(test) {
            return Arc::new(Self::default());
        }
        static GLOBAL: OnceLock<Arc<RateLimitGuard>> = OnceLock::new();
        Arc::clone(GLOBAL.get_or_init(|| Arc::new(Self::default())))
    }

    /// Call before sending a request to `url`.
    pub(crate) fn check(&self, url: &str, caller: Caller) -> Result<(), EsiError> {
        self.check_at(Instant::now(), url, caller)
    }

    /// Call with every response to a request sent to `url`.
    pub(crate) fn observe(
        &self,
        url: &str,
        caller: Caller,
        status: StatusCode,
        headers: &HeaderMap,
    ) {
        self.observe_at(Instant::now(), Utc::now(), url, caller, status, headers);
    }

    fn check_at(&self, now: Instant, url: &str, caller: Caller) -> Result<(), EsiError> {
        let route = route_template(url);
        let mut state = self.state.lock().expect("rate limit lock");
        let group = state.groups.get(&route).cloned().unwrap_or(route);
        let key = (group, caller);
        match state.paused.get(&key) {
            Some(until) if *until > now => {
                metrics::counter!("iskworks_esi_rate_limit_blocked_total", "group" => key.0.clone())
                    .increment(1);
                Err(EsiError::RateLimited {
                    retry_after_seconds: Some((*until - now).as_secs().max(1)),
                })
            }
            Some(_) => {
                state.paused.remove(&key);
                Ok(())
            }
            None => Ok(()),
        }
    }

    fn observe_at(
        &self,
        now: Instant,
        wall_clock: DateTime<Utc>,
        url: &str,
        caller: Caller,
        status: StatusCode,
        headers: &HeaderMap,
    ) {
        let route = route_template(url);
        let mut state = self.state.lock().expect("rate limit lock");
        let group = match header_text(headers, "x-ratelimit-group") {
            Some(group) => {
                state.groups.insert(route, group.clone());
                group
            }
            None => state.groups.get(&route).cloned().unwrap_or(route),
        };
        let bucket = bucket(headers);
        if let Some((remaining, limit)) = bucket {
            metrics::gauge!("iskworks_esi_rate_limit_remaining_ratio", "group" => group.clone())
                .set(remaining as f64 / limit as f64);
        }
        let pause = if status == StatusCode::TOO_MANY_REQUESTS {
            metrics::counter!("iskworks_esi_rate_limited_total", "group" => group.clone())
                .increment(1);
            Some((
                retry_after(headers, wall_clock).unwrap_or(DEFAULT_RETRY_AFTER),
                "retry_after",
            ))
        } else if bucket
            .is_some_and(|(remaining, limit)| remaining < limit / LOW_REMAINING_FRACTION)
        {
            Some((LOW_REMAINING_PAUSE, "low_remaining"))
        } else {
            None
        };
        let Some((pause, reason)) = pause else {
            return;
        };
        metrics::counter!(
            "iskworks_esi_rate_limit_paused_total",
            "group" => group.clone(),
            "reason" => reason
        )
        .increment(1);
        let until = now + pause.min(MAX_RETRY_AFTER);
        let current = state.paused.entry((group.clone(), caller)).or_insert(until);
        if *current < until {
            *current = until;
        }
        tracing::warn!(
            rate_limit_group = %group,
            status = status.as_u16(),
            pause_seconds = pause.min(MAX_RETRY_AFTER).as_secs(),
            "ESI rate limit reached; pausing this route group"
        );
    }
}

/// `Retry-After` as seconds or as an HTTP date.
pub(crate) fn retry_after(headers: &HeaderMap, now: DateTime<Utc>) -> Option<Duration> {
    let value = header_text(headers, "retry-after")?;
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let at = DateTime::parse_from_rfc2822(&value)
        .ok()?
        .with_timezone(&Utc);
    Some((at - now).to_std().unwrap_or(Duration::ZERO))
}

/// The bucket's `X-Ratelimit-Remaining` and its size
/// (`X-Ratelimit-Limit`, e.g. "150/15m"). It counts as nearly empty below
/// 1/`LOW_REMAINING_FRACTION` of the size.
fn bucket(headers: &HeaderMap) -> Option<(u64, u64)> {
    let remaining = header_text(headers, "x-ratelimit-remaining")?
        .parse::<u64>()
        .ok()?;
    let limit = header_text(headers, "x-ratelimit-limit")?
        .split('/')
        .next()?
        .trim()
        .parse::<u64>()
        .ok()?;
    (limit > 0).then_some((remaining, limit))
}

/// The route a URL belongs to: its path, with numeric segments (character,
/// structure, region IDs) replaced, so every character's wallet maps to
/// one route.
fn route_template(url: &str) -> String {
    let path = url.split_once("://").map_or(url, |(_, rest)| {
        rest.find('/').map_or("", |slash| &rest[slash..])
    });
    let path = path.split(['?', '#']).next().unwrap_or_default();
    path.split('/')
        .map(|segment| {
            if !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit()) {
                "{}"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn header_text(headers: &HeaderMap, name: &str) -> Option<String> {
    Some(headers.get(name)?.to_str().ok()?.trim().to_string()).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(*name, HeaderValue::from_str(value).unwrap());
        }
        headers
    }

    const WALLET: &str = "https://esi.evetech.net/latest/characters/123/wallet/";
    const JOURNAL: &str = "https://esi.evetech.net/latest/characters/456/wallet/journal/?page=2";
    const ORDERS: &str = "https://esi.evetech.net/latest/markets/10000002/orders/?type_id=34";

    #[test]
    fn routes_ignore_ids_and_query_strings() {
        assert_eq!(route_template(WALLET), "/latest/characters/{}/wallet/");
        assert_eq!(
            route_template(JOURNAL),
            "/latest/characters/{}/wallet/journal/"
        );
        assert_eq!(route_template(ORDERS), "/latest/markets/{}/orders/");
    }

    #[test]
    fn a_429_pauses_that_group_for_that_caller_only() {
        let guard = RateLimitGuard::default();
        let now = Instant::now();
        let alice = caller(Some("alice-token"));
        let bob = caller(Some("bob-token"));
        // Both wallet routes are learned to share one group.
        let ok = headers(&[("x-ratelimit-group", "char-wallet")]);
        guard.observe_at(now, Utc::now(), WALLET, alice, StatusCode::OK, &ok);
        guard.observe_at(now, Utc::now(), JOURNAL, alice, StatusCode::OK, &ok);

        let limited = headers(&[("x-ratelimit-group", "char-wallet"), ("retry-after", "45")]);
        guard.observe_at(
            now,
            Utc::now(),
            JOURNAL,
            alice,
            StatusCode::TOO_MANY_REQUESTS,
            &limited,
        );

        assert_eq!(
            guard.check_at(now + Duration::from_secs(5), WALLET, alice),
            Err(EsiError::RateLimited {
                retry_after_seconds: Some(40)
            }),
            "the same group, even on another route"
        );
        assert!(
            guard.check_at(now, WALLET, bob).is_ok(),
            "another character's bucket"
        );
        assert!(guard.check_at(now, ORDERS, None).is_ok(), "another group");
        assert!(guard
            .check_at(now + Duration::from_secs(45), WALLET, alice)
            .is_ok());
    }

    #[test]
    fn pauses_429s_refusals_and_bucket_levels_are_recorded_per_group() {
        use crate::metrics::test_support::Recorded;
        use metrics_util::debugging::DebuggingRecorder;

        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        metrics::with_local_recorder(&recorder, || {
            let guard = RateLimitGuard::default();
            let now = Instant::now();
            let low = headers(&[
                ("x-ratelimit-group", "market-order"),
                ("x-ratelimit-limit", "12000/15m"),
                ("x-ratelimit-remaining", "500"),
            ]);
            guard.observe_at(now, Utc::now(), ORDERS, None, StatusCode::OK, &low);
            assert!(guard.check_at(now, ORDERS, None).is_err());
            let limited = headers(&[("x-ratelimit-group", "char-wallet"), ("retry-after", "40")]);
            guard.observe_at(
                now,
                Utc::now(),
                WALLET,
                Some(1),
                StatusCode::TOO_MANY_REQUESTS,
                &limited,
            );
            assert!(guard.check_at(now, WALLET, Some(1)).is_err());
        });

        let recorded = Recorded::take(&snapshotter);
        let market = ("group", "market-order");
        let wallet = ("group", "char-wallet");
        assert_eq!(
            recorded.gauge("iskworks_esi_rate_limit_remaining_ratio", &[market]),
            Some(500.0 / 12000.0)
        );
        assert_eq!(
            recorded.counter(
                "iskworks_esi_rate_limit_paused_total",
                &[market, ("reason", "low_remaining")]
            ),
            1
        );
        assert_eq!(
            recorded.counter(
                "iskworks_esi_rate_limit_paused_total",
                &[wallet, ("reason", "retry_after")]
            ),
            1
        );
        assert_eq!(
            recorded.counter("iskworks_esi_rate_limited_total", &[wallet]),
            1
        );
        assert_eq!(
            recorded.counter("iskworks_esi_rate_limited_total", &[market]),
            0
        );
        assert_eq!(
            recorded.counter("iskworks_esi_rate_limit_blocked_total", &[market]),
            1
        );
        assert_eq!(
            recorded.counter("iskworks_esi_rate_limit_blocked_total", &[wallet]),
            1
        );
    }

    #[test]
    fn a_nearly_empty_bucket_pauses_briefly_before_esi_refuses() {
        let guard = RateLimitGuard::default();
        let now = Instant::now();
        let low = headers(&[
            ("x-ratelimit-group", "market-order"),
            ("x-ratelimit-limit", "12000/15m"),
            ("x-ratelimit-remaining", "500"),
        ]);
        guard.observe_at(now, Utc::now(), ORDERS, None, StatusCode::OK, &low);
        assert!(guard.check_at(now, ORDERS, None).is_err());
        assert!(guard
            .check_at(now + LOW_REMAINING_PAUSE, ORDERS, None)
            .is_ok());

        let healthy = headers(&[
            ("x-ratelimit-group", "market-order"),
            ("x-ratelimit-limit", "12000/15m"),
            ("x-ratelimit-remaining", "9611"),
        ]);
        let later = now + LOW_REMAINING_PAUSE;
        guard.observe_at(later, Utc::now(), ORDERS, None, StatusCode::OK, &healthy);
        assert!(guard.check_at(later, ORDERS, None).is_ok());
    }

    #[test]
    fn retry_after_may_be_seconds_or_a_date_and_is_capped() {
        let now: DateTime<Utc> = "2026-10-07T12:00:00Z".parse().unwrap();
        assert_eq!(
            retry_after(&headers(&[("retry-after", "30")]), now),
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            retry_after(
                &headers(&[("retry-after", "Wed, 07 Oct 2026 12:02:00 GMT")]),
                now
            ),
            Some(Duration::from_secs(120))
        );
        assert_eq!(retry_after(&headers(&[("retry-after", "soon")]), now), None);

        let guard = RateLimitGuard::default();
        let instant = Instant::now();
        let absurd = headers(&[("retry-after", "86400")]);
        guard.observe_at(
            instant,
            now,
            ORDERS,
            None,
            StatusCode::TOO_MANY_REQUESTS,
            &absurd,
        );
        assert!(guard
            .check_at(instant + MAX_RETRY_AFTER, ORDERS, None)
            .is_ok());
    }
}
