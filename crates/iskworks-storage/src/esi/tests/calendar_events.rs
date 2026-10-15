use iskworks_core::calendar_event::{
    CalendarEventRepository, CalendarEventResponse, CharacterCalendarEvent,
};

use super::*;

fn event(
    connection_id: ConnectedCharacterId,
    event_id: i64,
    title: &str,
) -> CharacterCalendarEvent {
    CharacterCalendarEvent {
        connection_id,
        event_id,
        event_date: "2026-10-20T18:00:00Z".parse().unwrap(),
        title: title.to_string(),
        importance: 0,
        response: CalendarEventResponse::NotResponded,
        duration_minutes: 60,
        owner_id: Some(98_000_001),
        owner_name: Some("Test Corp".to_string()),
        owner_type: Some("corporation".to_string()),
        text: "<b>Form up</b>".to_string(),
        observed_at: "2026-10-15T00:00:00Z".parse().unwrap(),
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn replacing_calendar_events_drops_missing_events_and_updates_the_rest(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let alpha = fixture_connection(&pool, workspace_id, owner_id, "Alpha").await;
    let bravo = fixture_connection(&pool, workspace_id, owner_id, "Bravo").await;
    let (other_workspace, _) = fixture_workspace(&pool).await;
    let repository = PgEsiRepository::new(pool);

    repository
        .replace_calendar_events(
            alpha.id,
            &[event(alpha.id, 1, "Op"), event(alpha.id, 2, "Gone")],
        )
        .await
        .unwrap();
    repository
        .replace_calendar_events(bravo.id, &[event(bravo.id, 1, "Op")])
        .await
        .unwrap();

    let mut accepted = event(alpha.id, 1, "Op");
    accepted.response = CalendarEventResponse::Accepted;
    repository
        .replace_calendar_events(alpha.id, &[accepted.clone()])
        .await
        .unwrap();

    assert_eq!(
        repository.calendar_events(alpha.id).await.unwrap(),
        vec![accepted]
    );
    // Another connection's copy of the same event is untouched.
    assert_eq!(
        repository.calendar_events(bravo.id).await.unwrap(),
        vec![event(bravo.id, 1, "Op")]
    );

    let workspace = repository
        .workspace_calendar_events(workspace_id)
        .await
        .unwrap();
    assert_eq!(
        workspace
            .iter()
            .map(|row| (row.character_name.as_str(), row.event.event_id))
            .collect::<Vec<_>>(),
        vec![("Alpha", 1), ("Bravo", 1)]
    );

    let copies = repository
        .workspace_calendar_event_copies(workspace_id, &[1, 99])
        .await
        .unwrap();
    assert_eq!(copies.len(), 2);
    assert!(copies.iter().all(|copy| copy.event_id == 1));
    assert!(repository
        .workspace_calendar_event_copies(other_workspace, &[1])
        .await
        .unwrap()
        .is_empty());

    repository
        .replace_calendar_events(alpha.id, &[])
        .await
        .unwrap();
    assert!(repository
        .calendar_events(alpha.id)
        .await
        .unwrap()
        .is_empty());
}
