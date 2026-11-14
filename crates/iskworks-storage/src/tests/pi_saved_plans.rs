use std::str::FromStr;

use iskworks_core::pi_planner::{NewSavedPlan, PiSavedPlansRepository, SavedPlanUpdate};
use iskworks_core::StoreError;
use rust_decimal::Decimal;
use serde_json::json;

use super::skill_plans::workspace;
use super::*;
use crate::PgPiSavedPlansRepository;

fn new_plan(name: &str) -> NewSavedPlan {
    NewSavedPlan {
        name: name.to_string(),
        snapshot_version: 1,
        scenario: json!({ "homeSystemId": 30_000_797, "characterNames": ["Valka"] }),
        solution: json!({ "plan": { "iskPerMonth": "1420000000.5", "colonies": [{}, {}, {}] } }),
        isk_per_month: Decimal::from_str("1420000000.5").unwrap(),
        colonies: 3,
        home_system_id: 30_000_797,
        home_system_name: Some("Q-3HS5".to_string()),
        price_observed_at: None,
    }
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn saved_plans_round_trip_and_stay_in_their_workspace(pool: PgPool) {
    let repository = PgPiSavedPlansRepository::new(pool.clone());
    let a = workspace(&pool, "A").await;
    let b = workspace(&pool, "B").await;

    let first = repository
        .create_saved_plan(a, new_plan("First"))
        .await
        .unwrap();
    let second = repository
        .create_saved_plan(a, new_plan("Second"))
        .await
        .unwrap();
    assert_eq!(first.summary.name, "First");
    assert_eq!(first.snapshot_version, 1);
    assert_eq!(first.scenario, new_plan("x").scenario);
    assert_eq!(first.solution, new_plan("x").solution);
    assert_eq!(
        first.summary.isk_per_month,
        Decimal::from_str("1420000000.5").unwrap()
    );
    assert_eq!(first.summary.colonies, 3);
    assert_eq!(first.summary.set_up, 0);
    assert_eq!(first.summary.home_system_name.as_deref(), Some("Q-3HS5"));

    // Ticking colonies bumps it to the top of the list.
    let ticked = repository
        .update_saved_plan(
            a,
            first.summary.id,
            SavedPlanUpdate {
                name: None,
                set_up_colonies: Some(vec![0, 2]),
            },
        )
        .await
        .unwrap();
    assert_eq!(ticked.set_up_colonies, vec![0, 2]);
    assert_eq!(ticked.summary.set_up, 2);
    assert_eq!(ticked.summary.name, "First");
    assert!(ticked.summary.updated_at > first.summary.updated_at);
    let list = repository.saved_plans(a).await.unwrap();
    assert_eq!(
        list.iter()
            .map(|plan| plan.name.as_str())
            .collect::<Vec<_>>(),
        ["First", "Second"]
    );
    assert_eq!(list[0].set_up, 2);

    let renamed = repository
        .update_saved_plan(
            a,
            second.summary.id,
            SavedPlanUpdate {
                name: Some("Renamed".to_string()),
                set_up_colonies: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(renamed.summary.name, "Renamed");

    // Workspace B sees none of it.
    assert!(repository.saved_plans(b).await.unwrap().is_empty());
    assert_eq!(
        repository.saved_plan(b, first.summary.id).await,
        Err(StoreError::NotFound)
    );
    assert_eq!(
        repository
            .update_saved_plan(b, first.summary.id, SavedPlanUpdate::default())
            .await,
        Err(StoreError::NotFound)
    );
    assert_eq!(
        repository.delete_saved_plan(b, first.summary.id).await,
        Err(StoreError::NotFound)
    );

    repository
        .delete_saved_plan(a, first.summary.id)
        .await
        .unwrap();
    assert_eq!(
        repository.saved_plan(a, first.summary.id).await,
        Err(StoreError::NotFound)
    );
    assert_eq!(repository.saved_plans(a).await.unwrap().len(), 1);
}
