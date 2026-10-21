//! Tranquility goes down every day at 11:00 UTC (usually under 5 minutes,
//! sometimes an hour or more), and now and then unscheduled. While it's down
//! every ESI request fails, and each failure spends ESI's per-IP error
//! budget -- the worker would exhaust it within seconds. So while ESI is
//! down the transport doesn't call it at all; it checks the cheap `/status/`
//! route every `PROBE_INTERVAL` instead, and the first healthy answer ends
//! the pause. There is no deadline: however long ESI stays down, only
//! `/status/` ends the pause.
//!
//! A pause starts two ways:
//! - Scheduled: at `WINDOW_START` each day, until `/status/` shows a server
//!   that has restarted since 11:00. A plain 200 isn't enough at first:
//!   Tranquility keeps answering for a few seconds after 11:00 and ESI
//!   caches `/status/` for 30 seconds, so an early probe sees the old
//!   server. After `STALE_STATUS_GRACE` any healthy 200 counts, so a day
//!   without a restart can't keep ESI paused.
//! - Unscheduled: ESI answers 502, 503 or 504 at any other time (an
//!   emergency downtime). Any healthy 200 ends it.

use std::sync::{Arc, Mutex, OnceLock};

use chrono::{DateTime, NaiveDate, NaiveTime, Timelike, Utc};
use reqwest::StatusCode;

/// Stop calling ESI this long before the daily downtime starts.
const WINDOW_START: u32 = 10 * 3600 + 58 * 60;
/// The daily downtime begins; status probes start.
const DOWNTIME_START: u32 = 11 * 3600;
/// How long after 11:00 a `/status/` answer may still come from the old
/// server (Tranquility's last seconds plus ESI's 30-second cache, with
/// plenty of margin).
const STALE_STATUS_GRACE: chrono::Duration = chrono::Duration::minutes(10);
/// How often `/status/` is checked while waiting for ESI to come back.
const PROBE_INTERVAL: chrono::Duration = chrono::Duration::seconds(30);

/// Why ESI is paused, and since when.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pause {
    /// The daily downtime that starts at 11:00 UTC on this day.
    Daily(NaiveDate),
    /// ESI answered as if it were down at this moment.
    Outage(DateTime<Utc>),
}

/// What the transport should do with a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DowntimeDecision {
    /// ESI isn't known to be down: send it.
    Send,
    /// ESI is down: don't send; try again in about this many seconds.
    Wait { retry_after_seconds: u64 },
    /// Check `/status/` first, judge the answer with `server_is_back`, then
    /// report it with `probed`.
    Probe(Pause),
}

#[derive(Debug, Default)]
struct State {
    /// The pause in force, if any.
    pause: Option<Pause>,
    /// The last day whose daily downtime is over.
    resumed_on: Option<NaiveDate>,
    /// When the last `/status/` check was handed out.
    last_probe: Option<DateTime<Utc>>,
}

#[derive(Debug, Default)]
pub(crate) struct DowntimeGuard {
    state: Mutex<State>,
}

impl DowntimeGuard {
    /// The guard shared by every transport in this process.
    pub(crate) fn global() -> Arc<Self> {
        static GLOBAL: OnceLock<Arc<DowntimeGuard>> = OnceLock::new();
        Arc::clone(GLOBAL.get_or_init(|| Arc::new(Self::default())))
    }

    /// Call before sending an ESI request.
    pub(crate) fn check(&self, now: DateTime<Utc>) -> DowntimeDecision {
        let today = now.date_naive();
        let time = now.time().num_seconds_from_midnight();
        let mut state = self.state.lock().expect("downtime lock");
        if state.pause.is_none() && time >= WINDOW_START && state.resumed_on != Some(today) {
            state.pause = Some(Pause::Daily(today));
        }
        let Some(pause) = state.pause else {
            return DowntimeDecision::Send;
        };
        if let Pause::Daily(day) = pause {
            let downtime_start = daily_downtime_start(day);
            if now < downtime_start {
                return wait_until(now, downtime_start);
            }
        }
        match state.last_probe {
            Some(last) if now < last + PROBE_INTERVAL => wait_until(now, last + PROBE_INTERVAL),
            _ => {
                // One caller probes; everyone else waits for its answer.
                state.last_probe = Some(now);
                DowntimeDecision::Probe(pause)
            }
        }
    }

    /// Report a `/status/` check handed out by `check`.
    pub(crate) fn probed(&self, pause: Pause, healthy: bool) {
        if !healthy {
            return;
        }
        let mut state = self.state.lock().expect("downtime lock");
        if let Pause::Daily(day) = pause {
            state.resumed_on = state.resumed_on.max(Some(day));
        }
        if state.pause == Some(pause) {
            state.pause = None;
            tracing::info!("ESI is back; resuming requests");
        }
    }

    /// Call with the status of every ESI response. A gateway error means ESI
    /// itself may be down, so it pauses everything until `/status/` says
    /// otherwise -- unless a probe just said ESI is fine, which leaves a
    /// single broken route to the normal error handling.
    pub(crate) fn observe(&self, now: DateTime<Utc>, status: StatusCode) {
        if !matches!(
            status,
            StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT
        ) {
            return;
        }
        let mut state = self.state.lock().expect("downtime lock");
        let probed_recently = state
            .last_probe
            .is_some_and(|last| now < last + PROBE_INTERVAL);
        if state.pause.is_none() && !probed_recently {
            state.pause = Some(Pause::Outage(now));
            tracing::warn!(
                status = status.as_u16(),
                "ESI looks down; pausing requests until /status/ is healthy"
            );
        }
    }
}

/// Whether a `/status/` answer (already a 200) shows ESI is back from
/// `pause`. VIP mode is developer-only, so it never counts.
pub(crate) fn server_is_back(
    pause: Pause,
    now: DateTime<Utc>,
    start_time: DateTime<Utc>,
    vip: bool,
) -> bool {
    if vip {
        return false;
    }
    match pause {
        Pause::Daily(day) => {
            let downtime_start = daily_downtime_start(day);
            start_time >= downtime_start || now >= downtime_start + STALE_STATUS_GRACE
        }
        Pause::Outage(_) => true,
    }
}

fn daily_downtime_start(day: NaiveDate) -> DateTime<Utc> {
    day.and_time(
        NaiveTime::from_num_seconds_from_midnight_opt(DOWNTIME_START, 0)
            .expect("valid downtime start"),
    )
    .and_utc()
}

fn wait_until(now: DateTime<Utc>, until: DateTime<Utc>) -> DowntimeDecision {
    DowntimeDecision::Wait {
        retry_after_seconds: u64::try_from((until - now).num_seconds())
            .unwrap_or(0)
            .max(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hms: &str) -> DateTime<Utc> {
        on("2026-10-07", hms)
    }

    fn on(day: &str, hms: &str) -> DateTime<Utc> {
        format!("{day}T{hms}Z").parse().unwrap()
    }

    fn today() -> Pause {
        Pause::Daily("2026-10-07".parse().unwrap())
    }

    /// Takes the probe `check` hands out and reports its answer.
    fn probe(guard: &DowntimeGuard, now: DateTime<Utc>, healthy: bool) {
        let DowntimeDecision::Probe(pause) = guard.check(now) else {
            panic!("expected a probe at {now}");
        };
        guard.probed(pause, healthy);
    }

    #[test]
    fn requests_flow_before_the_daily_window() {
        let guard = DowntimeGuard::default();
        for time in ["00:00:00", "06:00:00", "10:57:59"] {
            assert_eq!(guard.check(at(time)), DowntimeDecision::Send, "{time}");
        }
    }

    #[test]
    fn requests_pause_just_before_downtime() {
        let guard = DowntimeGuard::default();
        assert_eq!(
            guard.check(at("10:58:30")),
            DowntimeDecision::Wait {
                retry_after_seconds: 90
            }
        );
    }

    #[test]
    fn one_status_check_at_a_time_until_esi_answers() {
        let guard = DowntimeGuard::default();
        assert_eq!(
            guard.check(at("11:00:00")),
            DowntimeDecision::Probe(today())
        );
        assert_eq!(
            guard.check(at("11:00:05")),
            DowntimeDecision::Wait {
                retry_after_seconds: 25
            },
            "others wait for the probe"
        );
        guard.probed(today(), false);
        probe(&guard, at("11:00:30"), true);
        assert_eq!(guard.check(at("11:00:32")), DowntimeDecision::Send);
        assert_eq!(guard.check(at("23:59:59")), DowntimeDecision::Send);
    }

    #[test]
    fn each_day_has_its_own_downtime() {
        let guard = DowntimeGuard::default();
        probe(&guard, at("11:01:00"), true);
        let tomorrow = on("2026-10-08", "11:01:00");
        assert_eq!(
            guard.check(tomorrow),
            DowntimeDecision::Probe(Pause::Daily("2026-10-08".parse().unwrap()))
        );
    }

    #[test]
    fn a_process_started_after_downtime_checks_status_once() {
        let guard = DowntimeGuard::default();
        probe(&guard, at("15:00:00"), true);
        assert_eq!(guard.check(at("15:00:01")), DowntimeDecision::Send);
    }

    #[test]
    fn a_long_downtime_stays_paused_until_status_is_healthy() {
        let guard = DowntimeGuard::default();
        probe(&guard, at("11:00:00"), false);
        probe(&guard, at("11:30:00"), false);
        probe(&guard, at("12:45:00"), false);
        assert!(matches!(
            guard.check(at("12:45:10")),
            DowntimeDecision::Wait { .. }
        ));
        probe(&guard, at("12:45:30"), true);
        assert_eq!(guard.check(at("12:45:31")), DowntimeDecision::Send);
    }

    #[test]
    fn a_downtime_still_running_at_midnight_stays_paused() {
        let guard = DowntimeGuard::default();
        probe(&guard, at("23:59:50"), false);
        let after_midnight = on("2026-10-08", "00:00:30");
        assert_eq!(
            guard.check(after_midnight),
            DowntimeDecision::Probe(today())
        );
        guard.probed(today(), true);
        assert_eq!(
            guard.check(on("2026-10-08", "00:00:31")),
            DowntimeDecision::Send
        );
    }

    #[test]
    fn a_gateway_error_starts_an_unscheduled_pause() {
        let guard = DowntimeGuard::default();
        let now = at("08:00:00");
        guard.observe(now, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            guard.check(now),
            DowntimeDecision::Probe(Pause::Outage(now))
        );
        guard.probed(Pause::Outage(now), false);
        assert!(matches!(
            guard.check(at("08:00:10")),
            DowntimeDecision::Wait { .. }
        ));
        probe(&guard, at("08:00:30"), true);
        assert_eq!(guard.check(at("08:00:31")), DowntimeDecision::Send);
    }

    #[test]
    fn other_errors_dont_pause() {
        let guard = DowntimeGuard::default();
        for status in [
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::NOT_FOUND,
            StatusCode::FORBIDDEN,
            StatusCode::TOO_MANY_REQUESTS,
        ] {
            guard.observe(at("08:00:00"), status);
        }
        assert_eq!(guard.check(at("08:00:00")), DowntimeDecision::Send);
    }

    #[test]
    fn a_gateway_error_right_after_a_healthy_probe_doesnt_pause_again() {
        // One route failing while /status/ is fine is that route's problem.
        let guard = DowntimeGuard::default();
        guard.observe(at("08:00:00"), StatusCode::BAD_GATEWAY);
        probe(&guard, at("08:00:00"), true);
        guard.observe(at("08:00:20"), StatusCode::BAD_GATEWAY);
        assert_eq!(guard.check(at("08:00:20")), DowntimeDecision::Send);
        guard.observe(at("08:00:40"), StatusCode::BAD_GATEWAY);
        assert!(matches!(
            guard.check(at("08:00:40")),
            DowntimeDecision::Probe(Pause::Outage(_))
        ));
    }

    #[test]
    fn a_daily_pause_needs_a_restarted_server_at_first() {
        let yesterday = on("2026-10-06", "11:04:00");
        assert!(!server_is_back(today(), at("11:00:03"), yesterday, false));
        assert!(!server_is_back(
            today(),
            at("11:00:03"),
            at("10:59:59"),
            false
        ));
        assert!(!server_is_back(
            today(),
            at("11:06:00"),
            at("11:05:00"),
            true
        ));
        assert!(server_is_back(
            today(),
            at("11:06:00"),
            at("11:05:00"),
            false
        ));
    }

    #[test]
    fn a_daily_pause_accepts_any_healthy_status_after_the_grace() {
        // A day without a restart mustn't keep ESI paused forever.
        let yesterday = on("2026-10-06", "11:04:00");
        assert!(!server_is_back(today(), at("11:09:59"), yesterday, false));
        assert!(server_is_back(today(), at("11:10:00"), yesterday, false));
        assert!(!server_is_back(today(), at("11:10:00"), yesterday, true));
    }

    #[test]
    fn an_unscheduled_pause_accepts_any_healthy_status() {
        let outage = Pause::Outage(at("18:00:00"));
        let earlier = on("2026-10-07", "11:04:00");
        assert!(server_is_back(outage, at("18:00:30"), earlier, false));
        assert!(!server_is_back(outage, at("18:00:30"), earlier, true));
    }
}
