//! In-game EVE calendar events, synced per connection by the `calendar`
//! character source and read back by the Calendar.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{ConnectedCharacterId, InventoryError, WorkspaceId};

/// A character's answer to an event. Responses are per character, which is
/// why events are stored per connection.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarEventResponse {
    Accepted,
    Declined,
    Tentative,
    NotResponded,
}

impl CalendarEventResponse {
    #[must_use]
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::Tentative => "tentative",
            Self::NotResponded => "not_responded",
        }
    }

    #[must_use]
    pub fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "accepted" => Some(Self::Accepted),
            "declined" => Some(Self::Declined),
            "tentative" => Some(Self::Tentative),
            "not_responded" => Some(Self::NotResponded),
            _ => None,
        }
    }
}

/// One upcoming event as one connection sees it: the summary fields plus
/// the details fetched for it. `text` is raw player-authored markup and must
/// never be rendered as HTML.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CharacterCalendarEvent {
    pub connection_id: ConnectedCharacterId,
    pub event_id: i64,
    pub event_date: DateTime<Utc>,
    pub title: String,
    pub importance: i64,
    pub response: CalendarEventResponse,
    pub duration_minutes: i64,
    pub owner_id: Option<i64>,
    pub owner_name: Option<String>,
    pub owner_type: Option<String>,
    pub text: String,
    /// When this connection's copy was last synced.
    pub observed_at: DateTime<Utc>,
}

impl CharacterCalendarEvent {
    /// Whether a fresh summary still matches this stored copy, so its
    /// details can be kept instead of fetched again. The response is not
    /// compared: it comes with the summary and is refreshed on every sync.
    #[must_use]
    pub fn details_current(&self, event_date: DateTime<Utc>, title: &str, importance: i64) -> bool {
        self.event_date == event_date && self.title == title && self.importance == importance
    }
}

/// A stored event with the character that sees it, for the Calendar.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorkspaceCalendarEvent {
    pub event: CharacterCalendarEvent,
    pub eve_character_id: i64,
    pub character_name: String,
}

#[async_trait]
pub trait CalendarEventRepository: Send + Sync {
    /// The connection's stored events.
    async fn calendar_events(
        &self,
        connection_id: ConnectedCharacterId,
    ) -> Result<Vec<CharacterCalendarEvent>, InventoryError>;

    /// Stored copies of these events held by any connection in the
    /// workspace, so a corp event one character already synced is not
    /// fetched again for the next.
    async fn workspace_calendar_event_copies(
        &self,
        workspace_id: WorkspaceId,
        event_ids: &[i64],
    ) -> Result<Vec<CharacterCalendarEvent>, InventoryError>;

    /// Makes `events` the connection's whole set: events not in it are
    /// deleted, the rest inserted or updated.
    async fn replace_calendar_events(
        &self,
        connection_id: ConnectedCharacterId,
        events: &[CharacterCalendarEvent],
    ) -> Result<(), InventoryError>;

    /// Every stored event of the workspace's connected characters.
    async fn workspace_calendar_events(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<WorkspaceCalendarEvent>, InventoryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored() -> CharacterCalendarEvent {
        CharacterCalendarEvent {
            connection_id: ConnectedCharacterId(uuid::Uuid::nil()),
            event_id: 7,
            event_date: "2026-10-20T18:00:00Z".parse().unwrap(),
            title: "Fleet op".to_string(),
            importance: 0,
            response: CalendarEventResponse::Accepted,
            duration_minutes: 60,
            owner_id: None,
            owner_name: None,
            owner_type: None,
            text: String::new(),
            observed_at: "2026-10-15T00:00:00Z".parse().unwrap(),
        }
    }

    #[test]
    fn details_stay_current_until_the_date_title_or_importance_change() {
        let event = stored();
        let date = event.event_date;
        assert!(event.details_current(date, "Fleet op", 0));
        assert!(!event.details_current(date + chrono::Duration::hours(1), "Fleet op", 0));
        assert!(!event.details_current(date, "Fleet op (moved)", 0));
        assert!(!event.details_current(date, "Fleet op", 1));
    }

    #[test]
    fn response_round_trips_through_its_db_string() {
        for response in [
            CalendarEventResponse::Accepted,
            CalendarEventResponse::Declined,
            CalendarEventResponse::Tentative,
            CalendarEventResponse::NotResponded,
        ] {
            assert_eq!(
                CalendarEventResponse::from_db_str(response.as_db_str()),
                Some(response)
            );
        }
        assert_eq!(CalendarEventResponse::from_db_str("maybe"), None);
    }
}
