//! When each character source is refreshed again: its flat interval, the
//! backoff after a failure (`SourceFailure`, honouring ESI's own wait), and
//! the skills source's reschedule for when the training skill finishes.

use chrono::{DateTime, Duration, Utc};
use iskworks_core::{
    derive_training_state, CharacterSourceKind, RetryPolicy, SkillQueueEntry, TrainingState,
};
use iskworks_esi::EsiError;
use serde_json::Value;

use crate::esi_service::EsiApplicationError;

pub(super) fn refresh_interval(kind: CharacterSourceKind) -> Duration {
    match kind {
        CharacterSourceKind::CharacterInfo => Duration::hours(24),
        CharacterSourceKind::Location => Duration::minutes(10),
        CharacterSourceKind::Skills => Duration::minutes(60),
        CharacterSourceKind::Wallet => Duration::minutes(15),
        CharacterSourceKind::IndustryJobs => Duration::minutes(30),
        CharacterSourceKind::Assets => Duration::minutes(60),
        CharacterSourceKind::WalletTransactions => Duration::minutes(30),
        CharacterSourceKind::Planets => Duration::minutes(30),
        // ESI caches the calendar for 5 minutes; this stays well above it.
        CharacterSourceKind::Calendar => Duration::minutes(30),
        // Matches ESI's 20-minute cache on a character's orders.
        CharacterSourceKind::MarketOrders => Duration::minutes(20),
    }
}

/// The first retry after a failed source refresh: the base of
/// `RetryPolicy::CHARACTER_SOURCE`, which storage doubles for each
/// consecutive failure (capped at 6 hours), so a source that keeps failing
/// -- a 403 the character can't fix, an endpoint that's down -- stops
/// spending ESI's error budget every five minutes.
pub(super) fn retry_interval() -> Duration {
    RetryPolicy::CHARACTER_SOURCE.base
}

/// Why a source refresh failed, and how long ESI asked us to wait before
/// trying again (`Retry-After` on a 429, or the error-limit reset).
#[derive(Debug)]
pub(crate) struct SourceFailure {
    pub(super) message: String,
    retry_after: Option<Duration>,
}

impl SourceFailure {
    /// The earliest retry: the first backoff step, or ESI's own wait when
    /// that is longer.
    pub(super) fn retry_floor(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        now + self.retry_after.unwrap_or_default().max(retry_interval())
    }
}

impl From<EsiError> for SourceFailure {
    fn from(error: EsiError) -> Self {
        let retry_after = match &error {
            EsiError::RateLimited {
                retry_after_seconds: Some(seconds),
            }
            | EsiError::EsiErrorLimit {
                reset_seconds: Some(seconds),
            }
            | EsiError::ServerDowntime {
                retry_after_seconds: Some(seconds),
            } => i64::try_from(*seconds).ok().map(Duration::seconds),
            _ => None,
        };
        Self {
            message: error.to_string(),
            retry_after,
        }
    }
}

impl From<EsiApplicationError> for SourceFailure {
    fn from(error: EsiApplicationError) -> Self {
        match error {
            EsiApplicationError::Protocol(error) => error.into(),
            other => other.to_string().into(),
        }
    }
}

impl From<serde_json::Error> for SourceFailure {
    fn from(error: serde_json::Error) -> Self {
        error.to_string().into()
    }
}

impl From<String> for SourceFailure {
    fn from(message: String) -> Self {
        Self {
            message,
            retry_after: None,
        }
    }
}

/// Schedule the next `Skills` source refresh to land near the currently
/// training entry's known `finish_date` (capped by the flat interval as a
/// ceiling) instead of always waiting out the full flat interval -- reuses
/// the existing 30-second `due_character_sources` poll, no new scheduler.
///
/// A `CachedQueueExpired` result means we already know this very summary
/// is stale (every cached entry's `finish_date` is `<= now`) -- that must
/// not fall back to the flat ceiling, or reconciliation for data we
/// already know is stale would wait out a full cadence unnecessarily.
pub(super) fn skills_next_refresh_at(summary: &Value, now: DateTime<Utc>) -> DateTime<Utc> {
    let ceiling = now + refresh_interval(CharacterSourceKind::Skills);
    let entries = parse_skill_queue_entries(summary);
    match derive_training_state(&entries, now) {
        TrainingState::Active { entry } => {
            let finish = entry
                .finish_date
                .expect("derive_training_state only returns Active for fully-dated entries");
            (finish + Duration::seconds(60)).min(ceiling)
        }
        TrainingState::CachedQueueExpired { .. } => now,
        TrainingState::Paused { .. } | TrainingState::Empty => ceiling,
    }
}

pub(super) fn parse_skill_queue_entries(summary: &Value) -> Vec<SkillQueueEntry> {
    let Some(queue) = summary.get("skillQueue").and_then(Value::as_array) else {
        return Vec::new();
    };
    queue
        .iter()
        .filter_map(|entry| {
            Some(SkillQueueEntry {
                skill_id: entry.get("skill_id").and_then(Value::as_i64)?,
                finished_level: entry.get("finished_level").and_then(Value::as_i64)?,
                queue_position: entry.get("queue_position").and_then(Value::as_i64)?,
                start_date: entry
                    .get("start_date")
                    .and_then(Value::as_str)
                    .and_then(|text| text.parse::<DateTime<Utc>>().ok()),
                finish_date: entry
                    .get("finish_date")
                    .and_then(Value::as_str)
                    .and_then(|text| text.parse::<DateTime<Utc>>().ok()),
                training_start_sp: entry.get("training_start_sp").and_then(Value::as_i64),
                level_end_sp: entry.get("level_end_sp").and_then(Value::as_i64),
            })
        })
        .collect()
}
