use std::sync::Arc;

use iskworks_core::esi_scope::{MARKET_ORDERS_SCOPE, PLANETS_SCOPE, SKILLS_SCOPE};
use iskworks_core::{PlanetaryCharacterSource, SkillsCharacterSource, TraderCharacterSource};

use super::*;

/// Every character-read port answers from the connections table through
/// `dyn`, and the `*CharacterSource` ports skip disconnected characters.
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn the_character_read_ports_list_connected_characters_with_their_scopes(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let scoped = fixture_connection(&pool, workspace_id, owner_id, "Alpha Pilot").await;
    let unscoped = fixture_connection(&pool, workspace_id, owner_id, "Beta Pilot").await;
    let gone = fixture_connection(&pool, workspace_id, owner_id, "Gone Pilot").await;
    sqlx::query("UPDATE eve_connections SET granted_scopes = $2 WHERE id = $1")
        .bind(scoped.id.0)
        .bind(vec![
            MARKET_ORDERS_SCOPE.to_string(),
            PLANETS_SCOPE.to_string(),
            SKILLS_SCOPE.to_string(),
        ])
        .execute(&pool)
        .await
        .unwrap();
    let repository = Arc::new(PgEsiRepository::new(pool));
    for connection in [&scoped, &unscoped, &gone] {
        repository
            .register_character_sources(connection.id)
            .await
            .unwrap();
    }
    repository.disconnect(gone.id).await.unwrap();

    let connections: Arc<dyn CharacterConnectionReader> = repository.clone();
    assert_eq!(
        connections
            .list_connections(workspace_id)
            .await
            .unwrap()
            .len(),
        3,
        "the connection list keeps disconnected characters"
    );
    assert_eq!(
        connections
            .character_source_state(scoped.id)
            .await
            .unwrap()
            .len(),
        CharacterSourceKind::all().len()
    );

    let traders: Arc<dyn TraderCharacterSource> = repository.clone();
    let traders = traders.trader_characters(workspace_id).await.unwrap();
    assert_eq!(
        traders
            .iter()
            .map(|c| (c.character_name.as_str(), c.orders_scope_missing))
            .collect::<Vec<_>>(),
        [("Alpha Pilot", false), ("Beta Pilot", true)]
    );
    assert!(traders.iter().all(|c| c.accounting_level.is_none()));

    let planetary: Arc<dyn PlanetaryCharacterSource> = repository.clone();
    let mut planetary = planetary.planetary_characters(workspace_id).await.unwrap();
    planetary.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(
        planetary
            .iter()
            .map(|c| (c.name.as_str(), c.scope_granted, c.planets_source.is_some()))
            .collect::<Vec<_>>(),
        [("Alpha Pilot", true, true), ("Beta Pilot", false, true)]
    );

    let skills: Arc<dyn SkillsCharacterSource> = repository.clone();
    let mut all = skills.skills_characters(workspace_id).await.unwrap();
    all.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(
        all.iter()
            .map(|c| (c.name.as_str(), c.skills_scope_granted))
            .collect::<Vec<_>>(),
        [("Alpha Pilot", true), ("Beta Pilot", false)]
    );
    let one = skills
        .skills_character(workspace_id, scoped.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(one.name, "Alpha Pilot");
    assert!(skills
        .skills_character(workspace_id, gone.id)
        .await
        .unwrap()
        .is_none());
}
