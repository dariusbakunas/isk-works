//! ESI's error limit: every 4xx/5xx response spends from a per-IP budget
//! (`X-ESI-Error-Limit-Remain`, refilled after `X-ESI-Error-Limit-Reset`
//! seconds), and running it dry gets every request from this IP refused with
//! 420 -- for all tenants, API and worker alike. So the budget is guarded
//! process-wide: once it runs low, or ESI answers 420, every ESI call fails
//! fast with `EsiError::EsiErrorLimit` until the window resets, instead of
//! spending what's left.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use reqwest::header::HeaderMap;
use reqwest::StatusCode;

use crate::EsiError;

/// Stop sending once this few errors remain in the window. ESI's budget is
/// 100 per window; the floor leaves headroom for requests already in flight.
pub(crate) const ERROR_LIMIT_FLOOR: u64 = 20;
/// ESI's "error limited" status. Not `StatusCode::IM_A_TEAPOT` -- that's 418.
pub(crate) const ERROR_LIMITED_STATUS: u16 = 420;
/// Pause used when ESI doesn't say when the window resets.
const DEFAULT_PAUSE: Duration = Duration::from_secs(60);
/// Never trust a reset longer than this (ESI's window is 60s).
const MAX_PAUSE: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Default)]
pub(crate) struct ErrorLimitGuard {
    blocked_until: Mutex<Option<Instant>>,
}

impl ErrorLimitGuard {
    /// The guard shared by every transport in this process -- the budget
    /// is per IP, not per transport instance.
    pub(crate) fn global() -> Arc<Self> {
        // This crate's unit tests run in parallel in one process against
        // fake ESI servers, several of which report a low error budget on
        // purpose; a shared guard would let one test pause every other
        // test's requests. Each transport gets its own guard there.
        if cfg!(test) {
            return Arc::new(Self::default());
        }
        static GLOBAL: OnceLock<Arc<ErrorLimitGuard>> = OnceLock::new();
        Arc::clone(GLOBAL.get_or_init(|| Arc::new(Self::default())))
    }

    /// Call before sending an ESI request.
    pub(crate) fn check(&self) -> Result<(), EsiError> {
        self.check_at(Instant::now())
    }

    /// Call with every ESI response (success or not).
    pub(crate) fn observe(&self, status: StatusCode, headers: &HeaderMap) {
        self.observe_at(Instant::now(), status, headers);
    }

    fn check_at(&self, now: Instant) -> Result<(), EsiError> {
        let blocked_until = *self.blocked_until.lock().expect("error limit lock");
        match blocked_until {
            Some(until) if until > now => Err(EsiError::EsiErrorLimit {
                reset_seconds: Some((until - now).as_secs().max(1)),
            }),
            _ => Ok(()),
        }
    }

    fn observe_at(&self, now: Instant, status: StatusCode, headers: &HeaderMap) {
        let remain = header_number(headers, "x-esi-error-limit-remain");
        let reset = header_number(headers, "x-esi-error-limit-reset").map(Duration::from_secs);
        let exhausted = status.as_u16() == ERROR_LIMITED_STATUS;
        if !exhausted && remain.map_or(true, |remain| remain > ERROR_LIMIT_FLOOR) {
            return;
        }
        let pause = reset.unwrap_or(DEFAULT_PAUSE).min(MAX_PAUSE);
        let until = now + pause;
        let mut blocked_until = self.blocked_until.lock().expect("error limit lock");
        if blocked_until.map_or(true, |current| current < until) {
            *blocked_until = Some(until);
            tracing::warn!(
                status = status.as_u16(),
                error_limit_remain = remain,
                pause_seconds = pause.as_secs(),
                "ESI error limit is nearly exhausted; pausing all ESI requests"
            );
        }
    }
}

fn header_number(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers.get(name)?.to_str().ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    fn error_limited() -> StatusCode {
        StatusCode::from_u16(ERROR_LIMITED_STATUS).unwrap()
    }

    fn headers(remain: Option<u64>, reset: Option<u64>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(remain) = remain {
            headers.insert(
                "x-esi-error-limit-remain",
                HeaderValue::from_str(&remain.to_string()).unwrap(),
            );
        }
        if let Some(reset) = reset {
            headers.insert(
                "x-esi-error-limit-reset",
                HeaderValue::from_str(&reset.to_string()).unwrap(),
            );
        }
        headers
    }

    #[test]
    fn a_healthy_budget_never_blocks() {
        let guard = ErrorLimitGuard::default();
        let now = Instant::now();
        guard.observe_at(now, StatusCode::OK, &headers(Some(97), Some(42)));
        guard.observe_at(now, StatusCode::NOT_FOUND, &headers(Some(21), Some(42)));
        guard.observe_at(now, StatusCode::OK, &headers(None, None));
        assert!(guard.check_at(now).is_ok());
    }

    #[test]
    fn a_low_budget_blocks_until_the_window_resets() {
        let guard = ErrorLimitGuard::default();
        let now = Instant::now();
        guard.observe_at(now, StatusCode::NOT_FOUND, &headers(Some(20), Some(30)));

        assert!(matches!(
            guard.check_at(now + Duration::from_secs(10)),
            Err(EsiError::EsiErrorLimit {
                reset_seconds: Some(20)
            })
        ));
        assert!(guard.check_at(now + Duration::from_secs(30)).is_ok());
    }

    #[test]
    fn a_420_blocks_even_without_headers() {
        let guard = ErrorLimitGuard::default();
        let now = Instant::now();
        guard.observe_at(now, error_limited(), &headers(None, None));

        assert!(guard.check_at(now + Duration::from_secs(59)).is_err());
        assert!(guard.check_at(now + DEFAULT_PAUSE).is_ok());
    }

    #[test]
    fn a_later_healthy_response_does_not_lift_an_active_pause() {
        let guard = ErrorLimitGuard::default();
        let now = Instant::now();
        guard.observe_at(now, error_limited(), &headers(Some(0), Some(40)));
        guard.observe_at(now, StatusCode::OK, &headers(Some(100), Some(40)));
        // A shorter reset reported later doesn't shorten the pause either.
        guard.observe_at(now, StatusCode::NOT_FOUND, &headers(Some(5), Some(2)));

        assert!(guard.check_at(now + Duration::from_secs(39)).is_err());
    }

    #[test]
    fn an_absurd_reset_is_capped() {
        let guard = ErrorLimitGuard::default();
        let now = Instant::now();
        guard.observe_at(now, StatusCode::NOT_FOUND, &headers(Some(1), Some(86_400)));
        assert!(guard.check_at(now + MAX_PAUSE).is_ok());
    }
}
