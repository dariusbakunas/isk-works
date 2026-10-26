use iskworks_core::skill_plans::{PlanEntry, SkillPlanId, SkillPlanInput, SkillPlanRepository};
use iskworks_core::WorkspaceId;

use super::*;
use crate::PgSkillPlanRepository;

/// A workspace and its owner, inserted directly: the workspace service only
/// ever creates the one legacy workspace.
pub(super) async fn workspace(pool: &PgPool, name: &str) -> WorkspaceId {
    let workspace_id = uuid::Uuid::new_v4();
    let owner_id = uuid::Uuid::new_v4();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET CONSTRAINTS ALL DEFERRED")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, display_name, owner_id, created_at, updated_at)
         VALUES ($1, $3, $2, now(), now())",
    )
    .bind(workspace_id)
    .bind(owner_id)
    .bind(name)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO owners (id, workspace_id, owner_kind, display_name, created_at, updated_at)
         VALUES ($1, $2, 'manual', $3, now(), now())",
    )
    .bind(owner_id)
    .bind(workspace_id)
    .bind(name)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    WorkspaceId(workspace_id)
}

fn entry(skill_type_id: i64, level: i16) -> PlanEntry {
    PlanEntry {
        skill_type_id,
        level,
    }
}

fn input(name: &str, entries: Vec<PlanEntry>) -> SkillPlanInput {
    SkillPlanInput {
        name: name.to_string(),
        description: "Drone basics".to_string(),
        entries,
    }
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn skill_plans_round_trip_in_entry_order(pool: PgPool) {
    let repository = PgSkillPlanRepository::new(pool.clone());
    let workspace_id = workspace(&pool, "Plans").await;
    // Deliberately not sorted by skill id, to prove position order is kept.
    let entries = vec![
        entry(3_436, 1),
        entry(3_436, 2),
        entry(3_437, 1),
        entry(3_348, 1),
    ];

    let created = repository
        .create_skill_plan(
            workspace_id,
            &input("Core | Drones", entries.clone()),
            Utc::now(),
        )
        .await
        .unwrap();

    assert_eq!(created.name, "Core | Drones");
    assert_eq!(created.entries, entries);
    let fetched = repository
        .skill_plan(workspace_id, created.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched, created);
    let listed = repository.list_skill_plans(workspace_id).await.unwrap();
    assert_eq!(listed, vec![created]);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn updating_replaces_name_and_every_entry(pool: PgPool) {
    let repository = PgSkillPlanRepository::new(pool.clone());
    let workspace_id = workspace(&pool, "Plans").await;
    let created = repository
        .create_skill_plan(
            workspace_id,
            &input("Old", vec![entry(3_436, 1), entry(3_436, 2)]),
            Utc::now(),
        )
        .await
        .unwrap();

    let updated = repository
        .update_skill_plan(
            workspace_id,
            created.id,
            &input("New", vec![entry(3_348, 1)]),
            Utc::now(),
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.name, "New");
    assert_eq!(updated.entries, vec![entry(3_348, 1)]);
    assert!(updated.updated_at >= created.updated_at);
    assert_eq!(updated.created_at, created.created_at);
    let missing = repository
        .update_skill_plan(
            workspace_id,
            SkillPlanId(uuid::Uuid::new_v4()),
            &input("Nope", Vec::new()),
            Utc::now(),
        )
        .await
        .unwrap();
    assert!(missing.is_none());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn plans_are_scoped_to_their_workspace(pool: PgPool) {
    let repository = PgSkillPlanRepository::new(pool.clone());
    let mine = workspace(&pool, "Mine").await;
    let theirs = workspace(&pool, "Theirs").await;
    let plan = repository
        .create_skill_plan(mine, &input("Mine", vec![entry(3_436, 1)]), Utc::now())
        .await
        .unwrap();

    assert!(repository
        .list_skill_plans(theirs)
        .await
        .unwrap()
        .is_empty());
    assert!(repository
        .skill_plan(theirs, plan.id)
        .await
        .unwrap()
        .is_none());
    assert!(repository
        .update_skill_plan(theirs, plan.id, &input("Taken", Vec::new()), Utc::now())
        .await
        .unwrap()
        .is_none());
    assert!(!repository.delete_skill_plan(theirs, plan.id).await.unwrap());

    assert!(repository.delete_skill_plan(mine, plan.id).await.unwrap());
    assert!(repository
        .skill_plan(mine, plan.id)
        .await
        .unwrap()
        .is_none());
    let orphans: i64 = sqlx::query_scalar("SELECT count(*) FROM skill_plan_entries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(orphans, 0);
}
