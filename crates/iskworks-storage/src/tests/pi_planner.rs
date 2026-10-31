use std::str::FromStr;

use iskworks_core::pi_planner::{DensityWrite, PiPlannerInputsRepository, PlanetSettingsWrite};
use rust_decimal::Decimal;

use super::skill_plans::workspace;
use super::*;
use crate::PgPiPlannerInputsRepository;

fn dec(value: &str) -> Decimal {
    Decimal::from_str(value).unwrap()
}

fn density(planet_id: i64, density_pct: Option<&str>, head_value: Option<&str>) -> DensityWrite {
    DensityWrite {
        planet_id,
        resource_type_id: 2_267,
        density_pct: density_pct.map(dec),
        head_value: head_value.map(dec),
    }
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn densities_upsert_clear_and_stay_in_their_workspace(pool: PgPool) {
    let repository = PgPiPlannerInputsRepository::new(pool.clone());
    let a = workspace(&pool, "A").await;
    let b = workspace(&pool, "B").await;

    repository
        .save_densities(
            a,
            &[
                density(1, Some("52.5"), None),
                density(2, None, Some("146.1")),
            ],
        )
        .await
        .unwrap();
    repository
        .save_densities(b, &[density(1, Some("10"), None)])
        .await
        .unwrap();
    // Overwrite planet 1 and clear planet 2.
    repository
        .save_densities(
            a,
            &[density(1, Some("60"), Some("150")), density(2, None, None)],
        )
        .await
        .unwrap();

    let stored = repository.densities(a, &[1, 2]).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].planet_id, 1);
    assert_eq!(stored[0].density_pct, Some(dec("60")));
    assert_eq!(stored[0].head_value, Some(dec("150")));

    let other = repository.densities(b, &[1, 2]).await.unwrap();
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].density_pct, Some(dec("10")));
    assert!(repository.densities(a, &[3]).await.unwrap().is_empty());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn planet_settings_upsert_and_clear(pool: PgPool) {
    let repository = PgPiPlannerInputsRepository::new(pool.clone());
    let a = workspace(&pool, "A").await;
    let write = |planet_id, customs_rate: Option<&str>, contested| PlanetSettingsWrite {
        planet_id,
        customs_rate: customs_rate.map(dec),
        contested,
    };

    repository
        .save_planet_settings(a, &[write(1, Some("0.05"), false), write(2, None, true)])
        .await
        .unwrap();
    repository
        .save_planet_settings(a, &[write(1, None, false), write(2, Some("0.1"), true)])
        .await
        .unwrap();

    let stored = repository.planet_settings(a, &[1, 2]).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].planet_id, 2);
    assert_eq!(stored[0].customs_rate, Some(dec("0.1")));
    assert!(stored[0].contested);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_workspace_deletes_its_planner_inputs(pool: PgPool) {
    let repository = PgPiPlannerInputsRepository::new(pool.clone());
    let a = workspace(&pool, "A").await;
    repository
        .save_densities(a, &[density(1, Some("40"), None)])
        .await
        .unwrap();
    repository
        .save_planet_settings(
            a,
            &[PlanetSettingsWrite {
                planet_id: 1,
                customs_rate: None,
                contested: true,
            }],
        )
        .await
        .unwrap();

    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET CONSTRAINTS ALL DEFERRED")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("DELETE FROM owners WHERE workspace_id = $1")
        .bind(a.0)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(a.0)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    assert!(repository.densities(a, &[1]).await.unwrap().is_empty());
    assert!(repository
        .planet_settings(a, &[1])
        .await
        .unwrap()
        .is_empty());
}
