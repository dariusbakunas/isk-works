use std::collections::BTreeMap;

use iskworks_core::fitting::{
    FitDraft, FitId, FitItem, FitMutation, FitRepository, ModuleState, SlotKind,
};

use super::skill_plans::workspace;
use super::*;
use crate::PgFitRepository;

const BUSTARD: i64 = 12_731;

fn module(type_id: i64, slot_kind: SlotKind, slot_index: i16, state: ModuleState) -> FitItem {
    FitItem {
        type_id,
        slot_kind,
        slot_index: Some(slot_index),
        quantity: 1,
        state,
        charge_type_id: None,
        mutation: None,
    }
}

fn draft(name: &str, items: Vec<FitItem>) -> FitDraft {
    FitDraft {
        name: name.to_string(),
        description: "Null hauling".to_string(),
        ship_type_id: BUSTARD,
        items,
    }
}

/// Every column an item has, in an order that isn't by slot, to prove the
/// saved order is kept.
fn items() -> Vec<FitItem> {
    let mut gun = module(2_873, SlotKind::High, 1, ModuleState::Overload);
    gun.charge_type_id = Some(185);
    let mut mutated = module(47_408, SlotKind::Low, 0, ModuleState::Active);
    mutated.mutation = Some(FitMutation {
        base_type_id: 2_048,
        attributes: BTreeMap::from([(974, 0.255), (50, 27.5)]),
    });
    vec![
        module(5_975, SlotKind::Med, 0, ModuleState::Offline),
        gun,
        mutated,
        FitItem {
            type_id: 2_456,
            slot_kind: SlotKind::DroneBay,
            slot_index: None,
            quantity: 5,
            state: ModuleState::Active,
            charge_type_id: None,
            mutation: None,
        },
        FitItem {
            type_id: 28_668,
            slot_kind: SlotKind::Cargo,
            slot_index: None,
            quantity: 100,
            state: ModuleState::Offline,
            charge_type_id: None,
            mutation: None,
        },
    ]
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn fits_round_trip_in_item_order(pool: PgPool) {
    let repository = PgFitRepository::new(pool.clone());
    let workspace_id = workspace(&pool, "Fits").await;

    let created = repository
        .create_fit(workspace_id, &draft("Null Transport", items()), Utc::now())
        .await
        .unwrap();

    assert_eq!(created.name, "Null Transport");
    assert_eq!(created.description, "Null hauling");
    assert_eq!(created.ship_type_id, BUSTARD);
    assert_eq!(created.items, items());
    let fetched = repository
        .fit(workspace_id, created.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched, created);
    assert_eq!(
        repository.list_fits(workspace_id).await.unwrap(),
        vec![created]
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn fits_list_by_name(pool: PgPool) {
    let repository = PgFitRepository::new(pool.clone());
    let workspace_id = workspace(&pool, "Fits").await;
    for name in ["bravo", "Alpha", "charlie"] {
        repository
            .create_fit(workspace_id, &draft(name, Vec::new()), Utc::now())
            .await
            .unwrap();
    }
    let names: Vec<String> = repository
        .list_fits(workspace_id)
        .await
        .unwrap()
        .into_iter()
        .map(|fit| fit.name)
        .collect();
    assert_eq!(names, ["Alpha", "bravo", "charlie"]);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn updating_replaces_the_fit_and_every_item(pool: PgPool) {
    let repository = PgFitRepository::new(pool.clone());
    let workspace_id = workspace(&pool, "Fits").await;
    let created = repository
        .create_fit(workspace_id, &draft("Old", items()), Utc::now())
        .await
        .unwrap();

    let only = vec![module(5_975, SlotKind::Med, 3, ModuleState::Active)];
    let mut renamed = draft("New", only.clone());
    renamed.ship_type_id = 587;
    let updated = repository
        .update_fit(workspace_id, created.id, &renamed, Utc::now())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.name, "New");
    assert_eq!(updated.ship_type_id, 587);
    assert_eq!(updated.items, only);
    assert_eq!(updated.created_at, created.created_at);
    assert!(updated.updated_at >= created.updated_at);
    assert_eq!(
        repository.fit(workspace_id, created.id).await.unwrap(),
        Some(updated)
    );
    let missing = repository
        .update_fit(
            workspace_id,
            FitId(uuid::Uuid::new_v4()),
            &renamed,
            Utc::now(),
        )
        .await
        .unwrap();
    assert!(missing.is_none());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn fits_are_scoped_to_their_workspace(pool: PgPool) {
    let repository = PgFitRepository::new(pool.clone());
    let mine = workspace(&pool, "Mine").await;
    let theirs = workspace(&pool, "Theirs").await;
    let fit = repository
        .create_fit(mine, &draft("Mine", items()), Utc::now())
        .await
        .unwrap();

    assert!(repository.list_fits(theirs).await.unwrap().is_empty());
    assert!(repository.fit(theirs, fit.id).await.unwrap().is_none());
    assert!(repository
        .update_fit(theirs, fit.id, &draft("Stolen", Vec::new()), Utc::now())
        .await
        .unwrap()
        .is_none());
    assert!(!repository.delete_fit(theirs, fit.id).await.unwrap());
    assert!(repository.fit(mine, fit.id).await.unwrap().is_some());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_fit_removes_its_items(pool: PgPool) {
    let repository = PgFitRepository::new(pool.clone());
    let workspace_id = workspace(&pool, "Fits").await;
    let fit = repository
        .create_fit(workspace_id, &draft("Gone", items()), Utc::now())
        .await
        .unwrap();

    assert!(repository.delete_fit(workspace_id, fit.id).await.unwrap());
    assert!(repository
        .fit(workspace_id, fit.id)
        .await
        .unwrap()
        .is_none());
    let items: i64 = sqlx::query_scalar("SELECT count(*) FROM fit_items WHERE fit_id = $1")
        .bind(fit.id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(items, 0);
    assert!(!repository.delete_fit(workspace_id, fit.id).await.unwrap());
}

fn tags(names: &[&str]) -> Vec<String> {
    names.iter().map(ToString::to_string).collect()
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_fits_tags_are_replaced_and_read_back_by_name(pool: PgPool) {
    let repository = PgFitRepository::new(pool.clone());
    let workspace_id = workspace(&pool, "Fits").await;
    let created = repository
        .create_fit(workspace_id, &draft("Cyno", items()), Utc::now())
        .await
        .unwrap();
    assert!(created.tags.is_empty());

    let stored = repository
        .replace_fit_tags(workspace_id, created.id, &tags(&["ratting", "Cyno"]))
        .await
        .unwrap();
    assert_eq!(stored, Some(tags(&["Cyno", "ratting"])));
    let fit = repository
        .fit(workspace_id, created.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fit.tags, tags(&["Cyno", "ratting"]));
    // Tagging is organisation, not editing.
    assert_eq!(fit.updated_at, created.updated_at);
    assert_eq!(
        repository.list_fits(workspace_id).await.unwrap()[0].tags,
        tags(&["Cyno", "ratting"])
    );

    let updated = repository
        .update_fit(
            workspace_id,
            created.id,
            &draft("Cyno", Vec::new()),
            Utc::now(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.tags, tags(&["Cyno", "ratting"]));

    repository
        .replace_fit_tags(workspace_id, created.id, &[])
        .await
        .unwrap();
    assert!(repository
        .fit(workspace_id, created.id)
        .await
        .unwrap()
        .unwrap()
        .tags
        .is_empty());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn tags_are_edited_on_many_fits_of_the_workspace_only(pool: PgPool) {
    let repository = PgFitRepository::new(pool.clone());
    let mine = workspace(&pool, "Mine").await;
    let theirs = workspace(&pool, "Theirs").await;
    let one = repository
        .create_fit(mine, &draft("One", Vec::new()), Utc::now())
        .await
        .unwrap();
    let two = repository
        .create_fit(mine, &draft("Two", Vec::new()), Utc::now())
        .await
        .unwrap();
    let other = repository
        .create_fit(theirs, &draft("Other", Vec::new()), Utc::now())
        .await
        .unwrap();
    repository
        .replace_fit_tags(mine, one.id, &tags(&["Cyno", "Old"]))
        .await
        .unwrap();

    repository
        .edit_fit_tags(
            mine,
            &[one.id, two.id, other.id],
            &tags(&["cyno", "Ratting"]),
            &tags(&["OLD"]),
        )
        .await
        .unwrap();

    let tags_of = |id: FitId, workspace_id| {
        let repository = repository.clone();
        async move {
            repository
                .fit(workspace_id, id)
                .await
                .unwrap()
                .unwrap()
                .tags
        }
    };
    // "cyno" is already held as "Cyno"; removal ignores case.
    assert_eq!(tags_of(one.id, mine).await, tags(&["Cyno", "Ratting"]));
    assert_eq!(tags_of(two.id, mine).await, tags(&["cyno", "Ratting"]));
    assert!(tags_of(other.id, theirs).await.is_empty());
    assert!(repository
        .replace_fit_tags(theirs, one.id, &tags(&["Stolen"]))
        .await
        .unwrap()
        .is_none());

    let mut in_use = repository.fit_tags_in_use(mine).await.unwrap();
    in_use.sort_by_key(|tag| tag.to_lowercase());
    assert_eq!(in_use.len(), 2);
    assert!(in_use[0].eq_ignore_ascii_case("cyno"));
    assert_eq!(in_use[1], "Ratting");
    assert!(repository.fit_tags_in_use(theirs).await.unwrap().is_empty());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_fit_removes_its_tags(pool: PgPool) {
    let repository = PgFitRepository::new(pool.clone());
    let workspace_id = workspace(&pool, "Fits").await;
    let fit = repository
        .create_fit(workspace_id, &draft("Gone", Vec::new()), Utc::now())
        .await
        .unwrap();
    repository
        .replace_fit_tags(workspace_id, fit.id, &tags(&["Cyno"]))
        .await
        .unwrap();

    assert!(repository.delete_fit(workspace_id, fit.id).await.unwrap());
    assert!(repository
        .fit_tags_in_use(workspace_id)
        .await
        .unwrap()
        .is_empty());
}
