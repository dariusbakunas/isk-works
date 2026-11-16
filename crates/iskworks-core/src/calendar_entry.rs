//! Custom Calendar entries the player adds by hand: a title at an instant,
//! with an optional description and character. The instant is stored in UTC
//! next to the IANA time zone it was entered in, so an edit shows the
//! original wall-clock time; converting between them is the client's job.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{ConnectedCharacterId, StoreError, WorkspaceId};

/// Longest entry title, in characters.
pub const CALENDAR_ENTRY_TITLE_MAX: usize = 120;
/// Longest entry description, in characters.
pub const CALENDAR_ENTRY_DESCRIPTION_MAX: usize = 4000;
/// Longest time zone name (IANA names are far shorter).
pub const CALENDAR_ENTRY_TIME_ZONE_MAX: usize = 64;

/// The character an entry belongs to, as the Calendar shows it.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CalendarEntryCharacter {
    pub connection_id: ConnectedCharacterId,
    pub eve_character_id: i64,
    pub character_name: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CalendarEntry {
    pub id: Uuid,
    pub occurs_at: DateTime<Utc>,
    pub time_zone: String,
    pub title: String,
    /// Plain text; empty when there is none.
    pub description: String,
    /// `None` for a workspace-wide entry.
    pub character: Option<CalendarEntryCharacter>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// What a create or an update writes: an update replaces every field.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CalendarEntryInput {
    pub occurs_at: DateTime<Utc>,
    pub time_zone: String,
    pub title: String,
    pub description: String,
    pub connection_id: Option<ConnectedCharacterId>,
}

impl CalendarEntryInput {
    /// Trims the text fields and checks their lengths and the zone name's
    /// shape (letters, digits and `_ + - /`, as IANA names use).
    pub fn validated(self) -> Result<Self, StoreError> {
        let title = self.title.trim().to_string();
        let title_length = title.chars().count();
        if title_length == 0 || title_length > CALENDAR_ENTRY_TITLE_MAX {
            return Err(StoreError::Validation(format!(
                "title must be 1 to {CALENDAR_ENTRY_TITLE_MAX} characters"
            )));
        }
        let description = self.description.trim().to_string();
        if description.chars().count() > CALENDAR_ENTRY_DESCRIPTION_MAX {
            return Err(StoreError::Validation(format!(
                "description must be at most {CALENDAR_ENTRY_DESCRIPTION_MAX} characters"
            )));
        }
        let time_zone = self.time_zone.trim().to_string();
        let zone_shaped = !time_zone.is_empty()
            && time_zone.len() <= CALENDAR_ENTRY_TIME_ZONE_MAX
            && time_zone
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-' | '/'));
        if !zone_shaped {
            return Err(StoreError::Validation(
                "timeZone must be an IANA time zone name".to_string(),
            ));
        }
        Ok(Self {
            occurs_at: self.occurs_at,
            time_zone,
            title,
            description,
            connection_id: self.connection_id,
        })
    }
}

#[async_trait]
pub trait CalendarEntryRepository: Send + Sync {
    /// The workspace's entries with `from <= occurs_at < to`, by time.
    async fn calendar_entries(
        &self,
        workspace_id: WorkspaceId,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<CalendarEntry>, StoreError>;

    /// `Validation` when the character is not one of the workspace's.
    async fn create_calendar_entry(
        &self,
        workspace_id: WorkspaceId,
        input: CalendarEntryInput,
    ) -> Result<CalendarEntry, StoreError>;

    /// `NotFound` when the entry is missing or in another workspace;
    /// `Validation` like [`Self::create_calendar_entry`].
    async fn update_calendar_entry(
        &self,
        workspace_id: WorkspaceId,
        id: Uuid,
        input: CalendarEntryInput,
    ) -> Result<CalendarEntry, StoreError>;

    /// `NotFound` like [`Self::update_calendar_entry`].
    async fn delete_calendar_entry(
        &self,
        workspace_id: WorkspaceId,
        id: Uuid,
    ) -> Result<(), StoreError>;
}

/// An in-memory [`CalendarEntryRepository`] for tests. Characters it knows
/// are registered with [`Self::with_character`]; any other is refused.
#[cfg(any(test, feature = "test-support"))]
#[derive(Default)]
pub struct FakeCalendarEntries {
    entries: std::sync::Mutex<Vec<(WorkspaceId, CalendarEntry)>>,
    characters: Vec<(WorkspaceId, CalendarEntryCharacter)>,
}

#[cfg(any(test, feature = "test-support"))]
impl FakeCalendarEntries {
    #[must_use]
    pub fn with_character(
        mut self,
        workspace_id: WorkspaceId,
        character: CalendarEntryCharacter,
    ) -> Self {
        self.characters.push((workspace_id, character));
        self
    }

    fn character(
        &self,
        workspace_id: WorkspaceId,
        connection_id: Option<ConnectedCharacterId>,
    ) -> Result<Option<CalendarEntryCharacter>, StoreError> {
        let Some(connection_id) = connection_id else {
            return Ok(None);
        };
        self.characters
            .iter()
            .find(|(workspace, character)| {
                *workspace == workspace_id && character.connection_id == connection_id
            })
            .map(|(_, character)| Some(character.clone()))
            .ok_or_else(foreign_character)
    }
}

/// The error for a character outside the workspace.
#[must_use]
pub fn foreign_character() -> StoreError {
    StoreError::Validation("connectionId is not a character of this workspace".to_string())
}

#[cfg(any(test, feature = "test-support"))]
#[async_trait]
impl CalendarEntryRepository for FakeCalendarEntries {
    async fn calendar_entries(
        &self,
        workspace_id: WorkspaceId,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<CalendarEntry>, StoreError> {
        let mut entries: Vec<CalendarEntry> = self
            .entries
            .lock()
            .unwrap()
            .iter()
            .filter(|(workspace, entry)| {
                *workspace == workspace_id && entry.occurs_at >= from && entry.occurs_at < to
            })
            .map(|(_, entry)| entry.clone())
            .collect();
        entries.sort_by_key(|entry| (entry.occurs_at, entry.id));
        Ok(entries)
    }

    async fn create_calendar_entry(
        &self,
        workspace_id: WorkspaceId,
        input: CalendarEntryInput,
    ) -> Result<CalendarEntry, StoreError> {
        let character = self.character(workspace_id, input.connection_id)?;
        let now = Utc::now();
        let entry = CalendarEntry {
            id: Uuid::new_v4(),
            occurs_at: input.occurs_at,
            time_zone: input.time_zone,
            title: input.title,
            description: input.description,
            character,
            created_at: now,
            updated_at: now,
        };
        self.entries
            .lock()
            .unwrap()
            .push((workspace_id, entry.clone()));
        Ok(entry)
    }

    async fn update_calendar_entry(
        &self,
        workspace_id: WorkspaceId,
        id: Uuid,
        input: CalendarEntryInput,
    ) -> Result<CalendarEntry, StoreError> {
        let character = self.character(workspace_id, input.connection_id)?;
        let mut entries = self.entries.lock().unwrap();
        let (_, entry) = entries
            .iter_mut()
            .find(|(workspace, entry)| *workspace == workspace_id && entry.id == id)
            .ok_or(StoreError::NotFound)?;
        entry.occurs_at = input.occurs_at;
        entry.time_zone = input.time_zone;
        entry.title = input.title;
        entry.description = input.description;
        entry.character = character;
        entry.updated_at = Utc::now();
        Ok(entry.clone())
    }

    async fn delete_calendar_entry(
        &self,
        workspace_id: WorkspaceId,
        id: Uuid,
    ) -> Result<(), StoreError> {
        let mut entries = self.entries.lock().unwrap();
        let before = entries.len();
        entries.retain(|(workspace, entry)| !(*workspace == workspace_id && entry.id == id));
        if entries.len() == before {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> CalendarEntryInput {
        CalendarEntryInput {
            occurs_at: "2026-11-20T18:00:00Z".parse().unwrap(),
            time_zone: "America/New_York".to_string(),
            title: "  Fleet op  ".to_string(),
            description: "  Bring ammo \n".to_string(),
            connection_id: None,
        }
    }

    #[test]
    fn validation_trims_the_text_fields() {
        let valid = input().validated().unwrap();
        assert_eq!(valid.title, "Fleet op");
        assert_eq!(valid.description, "Bring ammo");
        assert_eq!(valid.time_zone, "America/New_York");
    }

    #[test]
    fn titles_must_have_one_to_the_maximum_characters() {
        for title in ["", "   ", &"x".repeat(CALENDAR_ENTRY_TITLE_MAX + 1)] {
            let result = CalendarEntryInput {
                title: title.to_string(),
                ..input()
            }
            .validated();
            assert!(
                matches!(result, Err(StoreError::Validation(_))),
                "{title:?}"
            );
        }
        let longest = CalendarEntryInput {
            title: "é".repeat(CALENDAR_ENTRY_TITLE_MAX),
            ..input()
        };
        assert!(longest.validated().is_ok());
    }

    #[test]
    fn descriptions_may_be_empty_but_not_too_long() {
        let empty = CalendarEntryInput {
            description: "  ".to_string(),
            ..input()
        };
        assert_eq!(empty.validated().unwrap().description, "");
        let long = CalendarEntryInput {
            description: "x".repeat(CALENDAR_ENTRY_DESCRIPTION_MAX + 1),
            ..input()
        };
        assert!(matches!(long.validated(), Err(StoreError::Validation(_))));
    }

    #[test]
    fn time_zones_must_look_like_iana_names() {
        for zone in [
            "UTC",
            "Etc/GMT+5",
            "America/Argentina/Buenos_Aires",
            "America/Port-au-Prince",
        ] {
            let valid = CalendarEntryInput {
                time_zone: zone.to_string(),
                ..input()
            };
            assert!(valid.validated().is_ok(), "{zone}");
        }
        for zone in ["", "Europe/Paris; DROP", "Zone name", &"A".repeat(65)] {
            let invalid = CalendarEntryInput {
                time_zone: zone.to_string(),
                ..input()
            };
            assert!(
                matches!(invalid.validated(), Err(StoreError::Validation(_))),
                "{zone}"
            );
        }
    }
}
