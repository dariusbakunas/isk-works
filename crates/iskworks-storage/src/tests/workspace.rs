use super::*;

#[test]
fn db_now_has_the_microsecond_precision_postgres_stores() {
    use chrono::Timelike;
    for _ in 0..1_000 {
        assert_eq!(crate::db_now().nanosecond() % 1_000, 0);
    }
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn workspace_and_owner_are_created_transactionally(pool: PgPool) {
    let repository = PgWorkspaceRepository::new(pool.clone());
    let service = WorkspaceService::new(repository);

    let created = service
        .create_workspace(CreateWorkspaceCommand {
            name: "Personal Industry".to_string(),
        })
        .await
        .unwrap();

    assert!(created.configured);
    assert_eq!(
        created.workspace.as_ref().unwrap().name,
        "Personal Industry"
    );
    assert!(created.owner.as_ref().unwrap().hidden);

    let workspace_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspaces")
        .fetch_one(&pool)
        .await
        .unwrap();
    let owner_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owners")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(workspace_count, 1);
    assert_eq!(owner_count, 1);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn duplicate_setup_is_rejected(pool: PgPool) {
    let repository = PgWorkspaceRepository::new(pool);
    let service = WorkspaceService::new(repository);

    service
        .create_workspace(CreateWorkspaceCommand {
            name: "First".to_string(),
        })
        .await
        .unwrap();

    let duplicate = service
        .create_workspace(CreateWorkspaceCommand {
            name: "Second".to_string(),
        })
        .await
        .unwrap_err();

    assert!(matches!(duplicate, AppError::WorkspaceAlreadyConfigured));
}
