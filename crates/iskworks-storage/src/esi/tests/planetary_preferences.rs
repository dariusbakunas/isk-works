use super::*;

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn planetary_preferences_default_then_round_trip_and_overwrite(pool: PgPool) {
    use iskworks_core::planetary::{
        ExcludedExport, PlanetaryPreferences, PlanetaryPreferencesRepository,
    };
    let repository = PgEsiRepository::new(pool.clone());
    let (workspace_id, _) = fixture_workspace(&pool).await;

    assert_eq!(
        repository
            .planetary_preferences(workspace_id)
            .await
            .unwrap(),
        PlanetaryPreferences::default()
    );

    let saved = PlanetaryPreferences {
        excluded_exports: vec![ExcludedExport {
            character_id: 2_119_000_001,
            planet_id: 40_050_359,
            type_id: 2_398,
        }],
        character_order: vec![2_119_000_002, 2_119_000_001],
    };
    repository
        .save_planetary_preferences(workspace_id, &saved)
        .await
        .unwrap();
    assert_eq!(
        repository
            .planetary_preferences(workspace_id)
            .await
            .unwrap(),
        saved
    );

    repository
        .save_planetary_preferences(workspace_id, &PlanetaryPreferences::default())
        .await
        .unwrap();
    assert_eq!(
        repository
            .planetary_preferences(workspace_id)
            .await
            .unwrap(),
        PlanetaryPreferences::default()
    );
}
