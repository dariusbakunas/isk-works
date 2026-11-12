//! Upcoming calendar events and their details.

use chrono::{DateTime, Utc};
use iskworks_core::{
    calendar_event::{CalendarEventResponse, CharacterCalendarEvent},
    ConnectedCharacter,
};
use serde_json::Value;

use crate::character_sync::{CharacterSyncService, SourceFailure};

/// ESI returns at most this many calendar events per page.
const CALENDAR_PAGE_LIMIT: usize = 50;
/// Upper bound on calendar pages per sync (500 upcoming events).
const MAX_CALENDAR_PAGES: usize = 10;

fn calendar_response(response: iskworks_esi::CalendarEventResponse) -> CalendarEventResponse {
    match response {
        iskworks_esi::CalendarEventResponse::Accepted => CalendarEventResponse::Accepted,
        iskworks_esi::CalendarEventResponse::Declined => CalendarEventResponse::Declined,
        iskworks_esi::CalendarEventResponse::Tentative => CalendarEventResponse::Tentative,
        iskworks_esi::CalendarEventResponse::NotResponded => CalendarEventResponse::NotResponded,
    }
}

impl CharacterSyncService {
    /// Pages through the upcoming events (`from_event` = the last event
    /// seen), fetches details only for events with no current stored copy in
    /// the workspace, and replaces the connection's stored set -- events ESI
    /// no longer lists are dropped. Any failed call fails the source before
    /// anything is stored. The summary only records counts.
    pub(in crate::character_sync) async fn sync_calendar_events(
        &self,
        access_token: &str,
        connection: &ConnectedCharacter,
        now: DateTime<Utc>,
    ) -> Result<Value, SourceFailure> {
        let character_id = connection.eve_character_id;
        let mut summaries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut from_event = None;
        let mut pages = 0;
        while pages < MAX_CALENDAR_PAGES {
            let page = self
                .transport
                .character_calendar(access_token, character_id, from_event)
                .await
                .map_err(SourceFailure::from)?
                .records;
            pages += 1;
            let full_page = page.len() >= CALENDAR_PAGE_LIMIT;
            let mut advanced = false;
            for event in page {
                from_event = Some(event.event_id);
                if seen.insert(event.event_id) {
                    advanced = true;
                    summaries.push(event);
                }
            }
            if !full_page || !advanced {
                break;
            }
        }

        let event_ids: Vec<i64> = summaries.iter().map(|event| event.event_id).collect();
        let copies = self
            .repository
            .calendar_event_copies(connection.workspace_id, &event_ids)
            .await
            .map_err(|error| SourceFailure::from(error.to_string()))?;
        let mut events = Vec::with_capacity(summaries.len());
        let mut details_fetched = 0;
        for summary in summaries {
            let reusable = copies
                .iter()
                .filter(|copy| {
                    copy.event_id == summary.event_id
                        && copy.details_current(
                            summary.event_date,
                            &summary.title,
                            summary.importance,
                        )
                })
                // This connection's own copy first, then the newest.
                .max_by_key(|copy| (copy.connection_id == connection.id, copy.observed_at));
            let (duration_minutes, owner_id, owner_name, owner_type, text) = match reusable {
                Some(copy) => (
                    copy.duration_minutes,
                    copy.owner_id,
                    copy.owner_name.clone(),
                    copy.owner_type.clone(),
                    copy.text.clone(),
                ),
                None => {
                    let detail = self
                        .transport
                        .character_calendar_event(access_token, character_id, summary.event_id)
                        .await
                        .map_err(SourceFailure::from)?
                        .records
                        .into_iter()
                        .next()
                        .ok_or_else(|| {
                            format!("ESI returned no details for event {}", summary.event_id)
                        })?;
                    details_fetched += 1;
                    (
                        detail.duration_minutes.max(0),
                        detail.owner_id,
                        detail.owner_name,
                        detail.owner_type,
                        detail.text,
                    )
                }
            };
            events.push(CharacterCalendarEvent {
                connection_id: connection.id,
                event_id: summary.event_id,
                event_date: summary.event_date,
                title: summary.title,
                importance: summary.importance,
                response: calendar_response(summary.response),
                duration_minutes,
                owner_id,
                owner_name,
                owner_type,
                text,
                observed_at: now,
            });
        }
        self.repository
            .store_calendar_events(connection.id, &events)
            .await
            .map_err(|error| SourceFailure::from(error.to_string()))?;
        Ok(serde_json::json!({
            "eventCount": events.len(),
            "detailsFetched": details_fetched,
            "pages": pages,
        }))
    }
}
