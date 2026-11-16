use iskworks_core::calendar_entry::{CalendarEntryInput, CalendarEntryRepository};

use super::*;
use crate::PgCalendarEntryRepository;

fn input(title: &str, occurs_at: &str) -> CalendarEntryInput {
    CalendarEntryInput {
        occurs_at: occurs_at.parse().unwrap(),
        time_zone: "Europe/Vilnius".to_string(),
        title: title.to_string(),
        description: "Bring ammo".to_string(),
        connection_id: None,
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn calendar_entries_round_trip_by_range_and_stay_in_their_workspace(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let (other_workspace, _) = fixture_workspace(&pool).await;
    let alpha = fixture_connection(&pool, workspace_id, owner_id, "Alpha").await;
    let repository = PgCalendarEntryRepository::new(pool.clone());

    let late = repository
        .create_calendar_entry(workspace_id, input("Late", "2026-11-20T20:00:00Z"))
        .await
        .unwrap();
    let early = repository
        .create_calendar_entry(
            workspace_id,
            CalendarEntryInput {
                connection_id: Some(alpha.id),
                ..input("Early", "2026-11-20T08:00:00Z")
            },
        )
        .await
        .unwrap();
    repository
        .create_calendar_entry(workspace_id, input("Next month", "2026-12-01T00:00:00Z"))
        .await
        .unwrap();
    assert_eq!(late.time_zone, "Europe/Vilnius");
    assert_eq!(late.description, "Bring ammo");
    assert!(late.character.is_none());
    let character = early.character.clone().unwrap();
    assert_eq!(character.connection_id, alpha.id);
    assert_eq!(character.character_name, "Alpha");

    let november = repository
        .calendar_entries(
            workspace_id,
            "2026-11-01T00:00:00Z".parse().unwrap(),
            "2026-12-01T00:00:00Z".parse().unwrap(),
        )
        .await
        .unwrap();
    let titles: Vec<_> = november.iter().map(|entry| entry.title.as_str()).collect();
    assert_eq!(titles, ["Early", "Late"]);
    assert!(repository
        .calendar_entries(
            other_workspace,
            "2026-11-01T00:00:00Z".parse().unwrap(),
            "2026-12-01T00:00:00Z".parse().unwrap(),
        )
        .await
        .unwrap()
        .is_empty());

    let moved = repository
        .update_calendar_entry(
            workspace_id,
            early.id,
            input("Moved", "2026-11-21T09:30:00Z"),
        )
        .await
        .unwrap();
    assert_eq!(moved.title, "Moved");
    assert_eq!(
        moved.occurs_at,
        "2026-11-21T09:30:00Z"
            .parse::<chrono::DateTime<chrono::Utc>>()
            .unwrap()
    );
    assert!(moved.character.is_none());
    assert!(moved.updated_at > early.updated_at);

    assert!(matches!(
        repository
            .update_calendar_entry(
                other_workspace,
                late.id,
                input("Hijack", "2026-11-20T20:00:00Z")
            )
            .await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        repository
            .delete_calendar_entry(other_workspace, late.id)
            .await,
        Err(StoreError::NotFound)
    ));
    repository
        .delete_calendar_entry(workspace_id, late.id)
        .await
        .unwrap();
    assert!(matches!(
        repository
            .delete_calendar_entry(workspace_id, late.id)
            .await,
        Err(StoreError::NotFound)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn calendar_entries_refuse_foreign_characters_and_outlive_a_deleted_one(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let (other_workspace, other_owner) = fixture_workspace(&pool).await;
    let alpha = fixture_connection(&pool, workspace_id, owner_id, "Alpha").await;
    let stranger = fixture_connection(&pool, other_workspace, other_owner, "Stranger").await;
    let repository = PgCalendarEntryRepository::new(pool.clone());

    let refused = repository
        .create_calendar_entry(
            workspace_id,
            CalendarEntryInput {
                connection_id: Some(stranger.id),
                ..input("Spy", "2026-11-20T08:00:00Z")
            },
        )
        .await;
    assert!(matches!(refused, Err(StoreError::Validation(_))));

    let entry = repository
        .create_calendar_entry(
            workspace_id,
            CalendarEntryInput {
                connection_id: Some(alpha.id),
                ..input("Alpha op", "2026-11-20T08:00:00Z")
            },
        )
        .await
        .unwrap();
    sqlx::query("DELETE FROM eve_connections WHERE id = $1")
        .bind(alpha.id.0)
        .execute(&pool)
        .await
        .unwrap();
    let kept = repository
        .calendar_entries(
            workspace_id,
            "2026-11-01T00:00:00Z".parse().unwrap(),
            "2026-12-01T00:00:00Z".parse().unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].id, entry.id);
    assert!(kept[0].character.is_none());
}
