//! Tranquility's daily downtime (11:00 UTC, usually under 5 minutes). While
//! the server is down every ESI request fails, and each failure spends ESI's
//! per-IP error budget -- the worker would exhaust it within seconds. So for
//! a short window around downtime the transport doesn't call ESI at all;
//! from 11:00 it checks the cheap `/status/` route every
//! `PROBE_INTERVAL`, and the first answer showing the server has restarted
//! since 11:00 ends the pause for the day. A plain 200 isn't enough:
//! Tranquility keeps answering for a few seconds after 11:00 and ESI caches
//! `/status/` for 30 seconds, so an early probe sees the old server. The window closes at `WINDOW_END` regardless, handing an unusually
//! long downtime back to the normal error handling.

use std::sync::{Arc, Mutex, OnceLock};

use chrono::{DateTime, NaiveDate, Timelike, Utc};

/// Stop calling ESI this long before downtime starts.
const WINDOW_START: u32 = 10 * 3600 + 58 * 60;
/// Downtime begins; status probes start.
const DOWNTIME_START: u32 = 11 * 3600;
/// The pause never runs past this, however long downtime lasts.
const WINDOW_END: u32 = 11 * 3600 + 30 * 60;
/// How often `/status/` is checked while waiting for ESI to come back.
const PROBE_INTERVAL: chrono::Duration = chrono::Duration::seconds(30);

/// What the transport should do with a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DowntimeDecision {
    /// Outside downtime, or ESI is already back: send it.
    Send,
    /// Downtime: don't send; try again in about this many seconds.
    Wait { retry_after_seconds: u64 },
    /// Check `/status/` first, then report the result with `probed`.
    Probe,
}

#[derive(Debug, Default)]
struct State {
    /// The day ESI was seen healthy again after downtime.
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
        if !(WINDOW_START..WINDOW_END).contains(&time) {
            return DowntimeDecision::Send;
        }
        let mut state = self.state.lock().expect("downtime lock");
        if state.resumed_on == Some(today) {
            return DowntimeDecision::Send;
        }
        if time < DOWNTIME_START {
            return wait_until(
                now,
                now + chrono::Duration::seconds(i64::from(DOWNTIME_START - time)),
            );
        }
        match state.last_probe {
            Some(last) if now < last + PROBE_INTERVAL => wait_until(now, last + PROBE_INTERVAL),
            _ => {
                // One caller probes; everyone else waits for its answer.
                state.last_probe = Some(now);
                DowntimeDecision::Probe
            }
        }
    }

    /// Report a `/status/` check handed out by `check`.
    pub(crate) fn probed(&self, now: DateTime<Utc>, healthy: bool) {
        if healthy {
            let mut state = self.state.lock().expect("downtime lock");
            state.resumed_on = Some(now.date_naive());
            tracing::info!("ESI is back after daily downtime; resuming requests");
        }
    }
}

/// Whether a `/status/` answer comes from a server that has restarted since
/// today's downtime began, and isn't still in VIP (developer-only) mode.
pub(crate) fn restarted_since_downtime(
    now: DateTime<Utc>,
    start_time: DateTime<Utc>,
    vip: bool,
) -> bool {
    let downtime_start = now.date_naive().and_time(
        chrono::NaiveTime::from_num_seconds_from_midnight_opt(DOWNTIME_START, 0)
            .expect("valid downtime start"),
    );
    !vip && start_time.naive_utc() >= downtime_start
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
        format!("2026-10-07T{hms}Z").parse().unwrap()
    }

    #[test]
    fn requests_flow_outside_the_downtime_window() {
        let guard = DowntimeGuard::default();
        for time in ["00:00:00", "10:57:59", "11:30:00", "23:59:59"] {
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
        assert_eq!(guard.check(at("11:00:00")), DowntimeDecision::Probe);
        assert_eq!(
            guard.check(at("11:00:05")),
            DowntimeDecision::Wait {
                retry_after_seconds: 25
            },
            "others wait for the probe"
        );
        guard.probed(at("11:00:01"), false);
        assert_eq!(guard.check(at("11:00:30")), DowntimeDecision::Probe);
        guard.probed(at("11:00:31"), true);
        assert_eq!(guard.check(at("11:00:32")), DowntimeDecision::Send);
        assert_eq!(guard.check(at("11:20:00")), DowntimeDecision::Send);
    }

    #[test]
    fn each_day_has_its_own_downtime() {
        let guard = DowntimeGuard::default();
        assert_eq!(guard.check(at("11:01:00")), DowntimeDecision::Probe);
        guard.probed(at("11:01:00"), true);
        let tomorrow: DateTime<Utc> = "2026-10-08T11:01:00Z".parse().unwrap();
        assert_eq!(guard.check(tomorrow), DowntimeDecision::Probe);
    }

    #[test]
    fn only_a_server_started_since_11_00_counts_as_back() {
        let started = |hms: &str| at(hms);
        let yesterday: DateTime<Utc> = "2026-10-06T11:04:00Z".parse().unwrap();
        assert!(!restarted_since_downtime(at("11:00:03"), yesterday, false));
        assert!(!restarted_since_downtime(
            at("11:00:03"),
            started("10:59:59"),
            false
        ));
        assert!(!restarted_since_downtime(
            at("11:06:00"),
            started("11:05:00"),
            true
        ));
        assert!(restarted_since_downtime(
            at("11:06:00"),
            started("11:05:00"),
            false
        ));
    }

    #[test]
    fn a_long_downtime_is_handed_back_to_normal_error_handling() {
        let guard = DowntimeGuard::default();
        assert_eq!(guard.check(at("11:29:59")), DowntimeDecision::Probe);
        guard.probed(at("11:29:59"), false);
        assert_eq!(guard.check(at("11:30:00")), DowntimeDecision::Send);
    }
}
