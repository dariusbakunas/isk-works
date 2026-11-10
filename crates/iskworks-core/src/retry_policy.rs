//! How long a refresh that keeps failing waits before its next attempt:
//! exponential backoff from a base, capped. Storage applies it when it
//! records a failure, so a row that keeps failing stops spending ESI's
//! shared error budget every few minutes.

use chrono::{DateTime, Duration, Utc};

/// Doubling stops after this many consecutive failures (the cap is reached
/// well before it for every policy below).
const MAX_DOUBLINGS: u32 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// The wait after the first failure.
    pub base: Duration,
    /// The longest wait, however many failures came before.
    pub cap: Duration,
}

impl RetryPolicy {
    /// Shared ESI evidence: market coverage (per-workspace and public),
    /// adjusted prices and system cost indices. 1, 2, 4... minutes.
    pub const ESI_EVIDENCE: Self = Self {
        base: Duration::minutes(1),
        cap: Duration::hours(6),
    };

    /// A character's ESI sources (skills, assets, wallet...). 5, 10, 20...
    /// minutes.
    pub const CHARACTER_SOURCE: Self = Self {
        base: Duration::minutes(5),
        cap: Duration::hours(6),
    };

    /// The wait after `consecutive_failures` earlier failures:
    /// `base * 2^consecutive_failures`, capped.
    #[must_use]
    pub fn backoff(&self, consecutive_failures: u32) -> Duration {
        let factor = 1_i32 << consecutive_failures.min(MAX_DOUBLINGS);
        (self.base * factor).min(self.cap)
    }

    /// When a refresh that just failed at `attempted_at` may run again: the
    /// backoff, or `floor` when that is later (a wait ESI asked for, or the
    /// caller's own minimum).
    #[must_use]
    pub fn retry_at(
        &self,
        attempted_at: DateTime<Utc>,
        consecutive_failures: u32,
        floor: DateTime<Utc>,
    ) -> DateTime<Utc> {
        (attempted_at + self.backoff(consecutive_failures)).max(floor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_backoff_doubles_from_a_minute_up_to_six_hours() {
        let minutes = (0..12)
            .map(|failures| RetryPolicy::ESI_EVIDENCE.backoff(failures).num_minutes())
            .collect::<Vec<_>>();
        assert_eq!(minutes, [1, 2, 4, 8, 16, 32, 64, 128, 256, 360, 360, 360]);
    }

    #[test]
    fn character_source_backoff_doubles_from_five_minutes_up_to_six_hours() {
        let minutes = (0..8)
            .map(|failures| {
                RetryPolicy::CHARACTER_SOURCE
                    .backoff(failures)
                    .num_minutes()
            })
            .collect::<Vec<_>>();
        assert_eq!(minutes, [5, 10, 20, 40, 80, 160, 320, 360]);
    }

    #[test]
    fn backoff_stays_capped_for_any_failure_count() {
        for failures in [16, 17, 64, u32::MAX] {
            assert_eq!(
                RetryPolicy::CHARACTER_SOURCE.backoff(failures),
                Duration::hours(6)
            );
        }
    }

    #[test]
    fn retry_at_is_the_later_of_the_backoff_and_the_floor() {
        let attempted_at = DateTime::parse_from_rfc3339("2026-11-10T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let policy = RetryPolicy::ESI_EVIDENCE;

        let backoff_wins = policy.retry_at(attempted_at, 3, attempted_at + Duration::minutes(2));
        assert_eq!(backoff_wins, attempted_at + Duration::minutes(8));

        let floor_wins = policy.retry_at(attempted_at, 3, attempted_at + Duration::minutes(30));
        assert_eq!(floor_wins, attempted_at + Duration::minutes(30));
    }
}
