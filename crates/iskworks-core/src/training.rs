use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Maximum number of entries EVE allows in a character's skill queue.
///
/// ESI does not report this limit, so it is encoded here. CCP raised the
/// queue from 50 to 150 entries for both Alpha and Omega clones in the
/// August 2021 "Updates To Skill Training" patch, to fit the longer
/// Certified Skill Plans:
/// <https://www.eveonline.com/news/view/updates-to-skill-training>.
pub const SKILL_QUEUE_CAPACITY: usize = 150;

/// The current EVE skill-queue capacity ([`SKILL_QUEUE_CAPACITY`]). A
/// function so callers depend on one authoritative source for the rule
/// rather than sprinkling the literal `150` around.
#[must_use]
pub const fn skill_queue_capacity() -> usize {
    SKILL_QUEUE_CAPACITY
}

/// One ESI skill queue entry, as cached.
///
/// `entries` slices passed to [`derive_training_state`] must already be
/// sorted ascending by `queue_position` — this is a contract established
/// by `iskworks-esi`'s `character_skill_queue` fetch, not re-derived here.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillQueueEntry {
    pub skill_id: i64,
    pub finished_level: i64,
    pub queue_position: i64,
    pub start_date: Option<DateTime<Utc>>,
    pub finish_date: Option<DateTime<Utc>>,
    pub training_start_sp: Option<i64>,
    pub level_end_sp: Option<i64>,
}

/// The current training state, derived purely from cached queue entries
/// and the current time. Never persisted — recomputed on demand.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TrainingState {
    /// One entry's `[start_date, finish_date)` window contains `now`.
    Active { entry: SkillQueueEntry },
    /// Every cached entry lacks both `start_date` and `finish_date` — ESI's
    /// documented representation of a paused queue. This is only ever
    /// produced when *all* entries agree; a partial mix of dated and
    /// undated entries is malformed data, not a legitimate pause (see
    /// module docs).
    Paused { next: Option<SkillQueueEntry> },
    /// We have (or had) usable, dated entries, but every one's
    /// `finish_date` is now `<= now`. This describes the cached
    /// projection running out — not a claim that the authoritative EVE
    /// queue is actually empty; it may have been extended since our last
    /// sync.
    CachedQueueExpired { known_until: DateTime<Utc> },
    /// No entries at all, or every entry was individually unusable
    /// (partial/malformed dates on an otherwise-dated queue).
    Empty,
}

/// Derive the current training state from cached skill queue entries.
///
/// Confirmed against real ESI semantics:
/// a paused queue omits `start_date`/`finish_date` on *every* entry at
/// once, never a subset. A queue with some dated and some undated entries
/// is therefore treated as malformed rather than paused — the undated
/// entries are dropped individually and the scan continues over the rest.
#[must_use]
pub fn derive_training_state(entries: &[SkillQueueEntry], now: DateTime<Utc>) -> TrainingState {
    if entries.is_empty() {
        return TrainingState::Empty;
    }

    if entries
        .iter()
        .all(|entry| entry.start_date.is_none() && entry.finish_date.is_none())
    {
        return TrainingState::Paused {
            next: entries.first().cloned(),
        };
    }

    let usable: Vec<&SkillQueueEntry> = entries
        .iter()
        .filter(|entry| match (entry.start_date, entry.finish_date) {
            (Some(start), Some(finish)) => finish > start,
            _ => false,
        })
        .collect();

    if usable.is_empty() {
        return TrainingState::Empty;
    }

    for entry in &usable {
        let finish = entry
            .finish_date
            .expect("usable entries always have finish_date");
        if finish <= now {
            continue;
        }
        let start = entry
            .start_date
            .expect("usable entries always have start_date");
        return if start <= now {
            TrainingState::Active {
                entry: (*entry).clone(),
            }
        } else {
            TrainingState::Paused {
                next: Some((*entry).clone()),
            }
        };
    }

    let known_until = usable
        .iter()
        .filter_map(|entry| entry.finish_date)
        .max()
        .expect("usable is non-empty and every entry has finish_date");
    TrainingState::CachedQueueExpired { known_until }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn entry(
        queue_position: i64,
        start_date: Option<DateTime<Utc>>,
        finish_date: Option<DateTime<Utc>>,
    ) -> SkillQueueEntry {
        SkillQueueEntry {
            skill_id: 1000 + queue_position,
            finished_level: 1,
            queue_position,
            start_date,
            finish_date,
            training_start_sp: None,
            level_end_sp: None,
        }
    }

    #[test]
    fn skill_queue_capacity_is_the_current_eve_limit() {
        // Guards against a silent drift back to the pre-2021 value of 50.
        assert_eq!(skill_queue_capacity(), 150);
        assert_eq!(skill_queue_capacity(), SKILL_QUEUE_CAPACITY);
    }

    #[test]
    fn empty_slice_is_empty() {
        let now = Utc::now();
        assert_eq!(derive_training_state(&[], now), TrainingState::Empty);
    }

    #[test]
    fn single_active_entry_is_active() {
        let now = Utc::now();
        let e = entry(
            0,
            Some(now - Duration::hours(1)),
            Some(now + Duration::hours(1)),
        );
        assert_eq!(
            derive_training_state(std::slice::from_ref(&e), now),
            TrainingState::Active { entry: e }
        );
    }

    #[test]
    fn single_completed_entry_with_nothing_after_is_cached_queue_expired() {
        let now = Utc::now();
        let finish = now - Duration::minutes(5);
        let e = entry(0, Some(now - Duration::hours(1)), Some(finish));
        assert_eq!(
            derive_training_state(&[e], now),
            TrainingState::CachedQueueExpired {
                known_until: finish
            }
        );
    }

    #[test]
    fn skips_one_completed_entry_to_find_active_next() {
        let now = Utc::now();
        let completed = entry(
            0,
            Some(now - Duration::hours(2)),
            Some(now - Duration::hours(1)),
        );
        let active = entry(
            1,
            Some(now - Duration::hours(1)),
            Some(now + Duration::hours(1)),
        );
        assert_eq!(
            derive_training_state(&[completed, active.clone()], now),
            TrainingState::Active { entry: active }
        );
    }

    #[test]
    fn skips_multiple_completed_entries_to_find_active_next() {
        let now = Utc::now();
        let completed_a = entry(
            0,
            Some(now - Duration::hours(3)),
            Some(now - Duration::hours(2)),
        );
        let completed_b = entry(
            1,
            Some(now - Duration::hours(2)),
            Some(now - Duration::hours(1)),
        );
        let active = entry(
            2,
            Some(now - Duration::hours(1)),
            Some(now + Duration::hours(1)),
        );
        assert_eq!(
            derive_training_state(&[completed_a, completed_b, active.clone()], now),
            TrainingState::Active { entry: active }
        );
    }

    #[test]
    fn single_entry_uniformly_paused_reports_itself_as_next() {
        let now = Utc::now();
        let e = entry(0, None, None);
        assert_eq!(
            derive_training_state(std::slice::from_ref(&e), now),
            TrainingState::Paused { next: Some(e) }
        );
    }

    #[test]
    fn two_entries_uniformly_paused_reports_first_as_next() {
        let now = Utc::now();
        let first = entry(0, None, None);
        let second = entry(1, None, None);
        assert_eq!(
            derive_training_state(&[first.clone(), second], now),
            TrainingState::Paused { next: Some(first) }
        );
    }

    #[test]
    fn mixed_dated_and_undated_entries_is_not_treated_as_paused() {
        // Real ESI never mixes dated/undated entries within one queue
        // response — a pause omits dates on every entry at once. A mix
        // like this is malformed/stale data and must not be silently
        // upgraded into a confident Paused state (refinement #1).
        let now = Utc::now();
        let completed = entry(
            0,
            Some(now - Duration::hours(2)),
            Some(now - Duration::hours(1)),
        );
        let undated = entry(1, None, None);
        assert_eq!(
            derive_training_state(&[completed, undated], now),
            TrainingState::CachedQueueExpired {
                known_until: now - Duration::hours(1)
            }
        );
    }

    #[test]
    fn entry_missing_only_finish_date_is_dropped_as_malformed() {
        let now = Utc::now();
        let malformed = entry(0, Some(now - Duration::hours(1)), None);
        let active = entry(
            1,
            Some(now - Duration::minutes(30)),
            Some(now + Duration::hours(1)),
        );
        assert_eq!(
            derive_training_state(&[malformed, active.clone()], now),
            TrainingState::Active { entry: active }
        );
    }

    #[test]
    fn entry_missing_only_start_date_is_dropped_as_malformed() {
        let now = Utc::now();
        let malformed = entry(0, None, Some(now + Duration::hours(1)));
        let active = entry(
            1,
            Some(now - Duration::minutes(30)),
            Some(now + Duration::hours(2)),
        );
        assert_eq!(
            derive_training_state(&[malformed, active.clone()], now),
            TrainingState::Active { entry: active }
        );
    }

    #[test]
    fn malformed_time_range_entry_is_dropped_never_active() {
        let now = Utc::now();
        // finish_date <= start_date: invalid range.
        let malformed = entry(
            0,
            Some(now + Duration::hours(1)),
            Some(now - Duration::hours(1)),
        );
        assert_eq!(
            derive_training_state(&[malformed], now),
            TrainingState::Empty
        );
    }

    #[test]
    fn future_gap_with_no_active_match_folds_into_paused() {
        let now = Utc::now();
        let future = entry(
            0,
            Some(now + Duration::minutes(10)),
            Some(now + Duration::hours(1)),
        );
        assert_eq!(
            derive_training_state(std::slice::from_ref(&future), now),
            TrainingState::Paused { next: Some(future) }
        );
    }
}
