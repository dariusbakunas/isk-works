use super::*;

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn first_login_claims_the_pre_existing_legacy_workspace(pool: PgPool) {
    let workspace_service = WorkspaceService::new(PgWorkspaceRepository::new(pool.clone()));
    let legacy = workspace_service
        .create_workspace(CreateWorkspaceCommand {
            name: "Pre-existing Industry".to_string(),
        })
        .await
        .unwrap();
    let legacy_workspace_id = legacy.workspace.unwrap().id;

    let users = PgUserRepository::new(pool.clone());
    let user = users
        .claim_unclaimed_workspace_or_provision(
            EveIdentity {
                character_id: 1,
                character_name: "First Character".to_string(),
            },
            iskworks_core::InviteGrant::NotRequired,
        )
        .await
        .unwrap();

    assert_eq!(user.workspace_id, legacy_workspace_id);

    let claimed_at: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT claimed_at FROM workspaces WHERE id = $1")
            .bind(legacy_workspace_id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(claimed_at.is_some());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn second_distinct_character_gets_a_freshly_provisioned_workspace(pool: PgPool) {
    let workspace_service = WorkspaceService::new(PgWorkspaceRepository::new(pool.clone()));
    let legacy = workspace_service
        .create_workspace(CreateWorkspaceCommand {
            name: "Pre-existing Industry".to_string(),
        })
        .await
        .unwrap();
    let legacy_workspace_id = legacy.workspace.unwrap().id;

    let users = PgUserRepository::new(pool.clone());
    let first = users
        .claim_unclaimed_workspace_or_provision(
            EveIdentity {
                character_id: 1,
                character_name: "First Character".to_string(),
            },
            iskworks_core::InviteGrant::NotRequired,
        )
        .await
        .unwrap();
    let second = users
        .claim_unclaimed_workspace_or_provision(
            EveIdentity {
                character_id: 2,
                character_name: "Second Character".to_string(),
            },
            iskworks_core::InviteGrant::NotRequired,
        )
        .await
        .unwrap();

    assert_eq!(first.workspace_id, legacy_workspace_id);
    assert_ne!(second.workspace_id, legacy_workspace_id);

    let workspace_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspaces")
        .fetch_one(&pool)
        .await
        .unwrap();
    let owner_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owners")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(workspace_count, 2);
    assert_eq!(owner_count, 2);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn repeat_login_is_idempotent(pool: PgPool) {
    let users = PgUserRepository::new(pool.clone());
    let identity = EveIdentity {
        character_id: 1,
        character_name: "Repeat Character".to_string(),
    };

    let first = users
        .claim_unclaimed_workspace_or_provision(
            identity.clone(),
            iskworks_core::InviteGrant::NotRequired,
        )
        .await
        .unwrap();
    let second = users
        .claim_unclaimed_workspace_or_provision(identity, iskworks_core::InviteGrant::NotRequired)
        .await
        .unwrap();

    assert_eq!(first.id, second.id);
    assert_eq!(first.workspace_id, second.workspace_id);

    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(user_count, 1);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn users_workspace_id_is_unique_at_the_schema_level(pool: PgPool) {
    let users = PgUserRepository::new(pool.clone());
    let first = users
        .claim_unclaimed_workspace_or_provision(
            EveIdentity {
                character_id: 1,
                character_name: "First Character".to_string(),
            },
            iskworks_core::InviteGrant::NotRequired,
        )
        .await
        .unwrap();

    let violation = sqlx::query(
        r#"
        INSERT INTO users (id, eve_character_id, eve_character_name, workspace_id, created_at, last_login_at)
        VALUES ($1, $2, $3, $4, now(), now())
        "#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(2_i64)
    .bind("Second Character")
    .bind(first.workspace_id.0)
    .execute(&pool)
    .await
    .unwrap_err();

    assert!(matches!(
        violation,
        sqlx::Error::Database(database_error) if database_error.is_unique_violation()
    ));
}

/// Nothing else ever deletes expired sessions or stale pending-authorization
/// rows, and an anonymous caller can mint a pending login row per request.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn purge_expired_removes_only_dead_sessions_and_stale_pending_authorizations(pool: PgPool) {
    let users = PgUserRepository::new(pool.clone());
    let user = users
        .claim_unclaimed_workspace_or_provision(
            EveIdentity {
                character_id: 1,
                character_name: "Purge Character".to_string(),
            },
            iskworks_core::InviteGrant::NotRequired,
        )
        .await
        .unwrap();
    let sessions = PgSessionRepository::new(pool.clone());
    let now = Utc::now();
    sessions
        .create_session(user.id, "live-session".to_string(), now + Duration::days(1))
        .await
        .unwrap();
    sessions
        .create_session(
            user.id,
            "dead-session".to_string(),
            now - Duration::minutes(1),
        )
        .await
        .unwrap();

    let cipher = iskworks_esi::SecretCipher::for_tests();
    for state_hash in ["fresh-login", "stale-login"] {
        users
            .begin_login_authorization(
                state_hash.to_string(),
                cipher.encrypt("verifier").unwrap(),
                None,
            )
            .await
            .unwrap();
    }
    sqlx::query(
        "UPDATE login_oauth_pending_authorizations SET created_at = now() - interval '2 days' WHERE state_hash = 'stale-login'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let owner_id: Uuid = sqlx::query_scalar("SELECT owner_id FROM workspaces WHERE id = $1")
        .bind(user.workspace_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    let esi = PgEsiRepository::new(pool.clone());
    for state_hash in ["fresh-link", "stale-link"] {
        esi.begin_authorization(
            state_hash.to_string(),
            user.workspace_id,
            iskworks_core::OwnerId(owner_id),
            cipher.encrypt("verifier").unwrap(),
            &[],
        )
        .await
        .unwrap();
    }
    sqlx::query(
        "UPDATE eve_oauth_pending_authorizations SET created_at = now() - interval '2 days', expires_at = now() - interval '2 days' WHERE state_hash = 'stale-link'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let purged = PgAuthMaintenance::new(pool.clone())
        .purge_expired(now)
        .await
        .unwrap();
    assert_eq!(
        purged,
        AuthPurgeOutcome {
            sessions: 1,
            login_authorizations: 1,
            link_authorizations: 1,
        }
    );

    let remaining = |table: &'static str, column: &'static str| {
        let pool = pool.clone();
        async move {
            let mut values: Vec<String> =
                sqlx::query_scalar(&format!("SELECT {column} FROM {table}"))
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            values.sort();
            values
        }
    };
    assert_eq!(remaining("sessions", "token_hash").await, ["live-session"]);
    assert_eq!(
        remaining("login_oauth_pending_authorizations", "state_hash").await,
        ["fresh-login"]
    );
    assert_eq!(
        remaining("eve_oauth_pending_authorizations", "state_hash").await,
        ["fresh-link"]
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn session_hashing_round_trips_through_postgres(pool: PgPool) {
    let users = PgUserRepository::new(pool.clone());
    let user = users
        .claim_unclaimed_workspace_or_provision(
            EveIdentity {
                character_id: 1,
                character_name: "Session Character".to_string(),
            },
            iskworks_core::InviteGrant::NotRequired,
        )
        .await
        .unwrap();

    let sessions = PgSessionRepository::new(pool.clone());
    let token = iskworks_core::SessionToken::generate();
    sessions
        .create_session(user.id, token.hash(), Utc::now() + Duration::days(1))
        .await
        .unwrap();

    let resolved = sessions.resolve_session(&token.hash()).await.unwrap();
    assert_eq!(resolved.as_ref().map(|u| u.user_id), Some(user.id));
    assert_eq!(
        resolved.as_ref().map(|u| u.eve_character_name.clone()),
        Some("Session Character".to_string())
    );

    sessions.delete_session(&token.hash()).await.unwrap();
    assert!(sessions
        .resolve_session(&token.hash())
        .await
        .unwrap()
        .is_none());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn expired_session_does_not_resolve(pool: PgPool) {
    let users = PgUserRepository::new(pool.clone());
    let user = users
        .claim_unclaimed_workspace_or_provision(
            EveIdentity {
                character_id: 1,
                character_name: "Session Character".to_string(),
            },
            iskworks_core::InviteGrant::NotRequired,
        )
        .await
        .unwrap();

    let sessions = PgSessionRepository::new(pool.clone());
    let token = iskworks_core::SessionToken::generate();
    sessions
        .create_session(user.id, token.hash(), Utc::now() - Duration::seconds(1))
        .await
        .unwrap();

    assert!(sessions
        .resolve_session(&token.hash())
        .await
        .unwrap()
        .is_none());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn admin_user_list_counts_linked_characters_and_sessions(pool: PgPool) {
    use crate::PgAdminRepository;
    use iskworks_core::AdminUsersRepository;

    let users = PgUserRepository::new(pool.clone());
    let provision = |id: i64, name: &str| {
        users.claim_unclaimed_workspace_or_provision(
            EveIdentity {
                character_id: id,
                character_name: name.to_string(),
            },
            iskworks_core::InviteGrant::NotRequired,
        )
    };
    let busy = provision(1, "Busy Pilot").await.unwrap();
    let _idle = provision(2, "Idle Pilot").await.unwrap();

    let owner_id: Uuid = sqlx::query_scalar("SELECT owner_id FROM workspaces WHERE id = $1")
        .bind(busy.workspace_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    for (character_id, status, disconnected) in [
        (11_i64, "connected", false),
        (12, "needs_reconnection", false),
        (13, "connected", true),
    ] {
        sqlx::query(
            r#"
            INSERT INTO eve_connections (
              id, workspace_id, owner_id, eve_character_id, character_name, status,
              granted_scopes, connected_at, updated_at, disconnected_at
            ) VALUES ($1, $2, $3, $4, 'Alt', $5, '{}', now(), now(),
                      CASE WHEN $6 THEN now() END)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(busy.workspace_id.0)
        .bind(owner_id)
        .bind(character_id)
        .bind(status)
        .bind(disconnected)
        .execute(&pool)
        .await
        .unwrap();
    }

    let sessions = PgSessionRepository::new(pool.clone());
    for expires in [Duration::days(1), Duration::days(2), Duration::seconds(-1)] {
        sessions
            .create_session(
                busy.id,
                iskworks_core::SessionToken::generate().hash(),
                Utc::now() + expires,
            )
            .await
            .unwrap();
    }

    let listed = PgAdminRepository::new(pool.clone())
        .list_users()
        .await
        .unwrap();
    assert_eq!(listed.len(), 2);
    let busy_row = listed.iter().find(|u| u.user_id == busy.id).unwrap();
    assert_eq!(busy_row.eve_character_name, "Busy Pilot");
    assert_eq!(
        busy_row.character_count, 2,
        "disconnected links are excluded"
    );
    assert_eq!(busy_row.characters_needing_attention, 1);
    assert_eq!(
        busy_row.active_session_count, 2,
        "expired sessions are excluded"
    );
    let idle_row = listed.iter().find(|u| u.user_id != busy.id).unwrap();
    assert_eq!(idle_row.character_count, 0);
    assert_eq!(idle_row.active_session_count, 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn disabling_a_user_blocks_sessions_and_enabling_restores_sign_in(pool: PgPool) {
    use crate::PgAdminRepository;
    use iskworks_core::AdminUsersRepository;

    let users = PgUserRepository::new(pool.clone());
    let user = users
        .claim_unclaimed_workspace_or_provision(
            EveIdentity {
                character_id: 1,
                character_name: "Blocked Pilot".to_string(),
            },
            iskworks_core::InviteGrant::NotRequired,
        )
        .await
        .unwrap();
    let sessions = PgSessionRepository::new(pool.clone());
    let admin = PgAdminRepository::new(pool.clone());
    let token = iskworks_core::SessionToken::generate();
    sessions
        .create_session(user.id, token.hash(), Utc::now() + Duration::days(1))
        .await
        .unwrap();
    assert!(sessions
        .resolve_session(&token.hash())
        .await
        .unwrap()
        .is_some());

    assert!(admin.set_user_disabled(user.id, true).await.unwrap());
    assert!(
        admin.set_user_disabled(user.id, true).await.unwrap(),
        "idempotent"
    );
    assert!(sessions
        .resolve_session(&token.hash())
        .await
        .unwrap()
        .is_none());
    let found = users.find_by_character_id(1).await.unwrap().unwrap();
    assert!(found.disabled_at.is_some());
    assert!(admin
        .find_user(user.id)
        .await
        .unwrap()
        .unwrap()
        .disabled_at
        .is_some());

    // A session that somehow exists for a disabled user still does not resolve.
    sessions
        .create_session(user.id, token.hash(), Utc::now() + Duration::days(1))
        .await
        .unwrap();
    assert!(sessions
        .resolve_session(&token.hash())
        .await
        .unwrap()
        .is_none());

    assert!(admin.set_user_disabled(user.id, false).await.unwrap());
    assert!(users
        .find_by_character_id(1)
        .await
        .unwrap()
        .unwrap()
        .disabled_at
        .is_none());
    assert!(sessions
        .resolve_session(&token.hash())
        .await
        .unwrap()
        .is_some());

    assert!(!admin
        .set_user_disabled(iskworks_core::UserId::new(), true)
        .await
        .unwrap());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_user_removes_the_account_and_its_esi_access(pool: PgPool) {
    use crate::PgAdminRepository;
    use iskworks_core::AdminUsersRepository;

    let users = PgUserRepository::new(pool.clone());
    let identity = |name: &str| EveIdentity {
        character_id: 1,
        character_name: name.to_string(),
    };
    let user = users
        .claim_unclaimed_workspace_or_provision(
            identity("Leaving Pilot"),
            iskworks_core::InviteGrant::NotRequired,
        )
        .await
        .unwrap();
    let bystander = users
        .claim_unclaimed_workspace_or_provision(
            EveIdentity {
                character_id: 2,
                character_name: "Staying Pilot".to_string(),
            },
            iskworks_core::InviteGrant::NotRequired,
        )
        .await
        .unwrap();

    let owner_id: Uuid = sqlx::query_scalar("SELECT owner_id FROM workspaces WHERE id = $1")
        .bind(user.workspace_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    let connection_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO eve_connections (
          id, workspace_id, owner_id, eve_character_id, character_name, status,
          granted_scopes, connected_at, updated_at
        ) VALUES ($1, $2, $3, 11, 'Alt', 'connected', '{}', now(), now())
        "#,
    )
    .bind(connection_id)
    .bind(user.workspace_id.0)
    .bind(owner_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO eve_connection_tokens (connection_id, refresh_token_envelope, updated_at) VALUES ($1, '{}', now())",
    )
    .bind(connection_id)
    .execute(&pool)
    .await
    .unwrap();
    let sessions = PgSessionRepository::new(pool.clone());
    let token = iskworks_core::SessionToken::generate();
    sessions
        .create_session(user.id, token.hash(), Utc::now() + Duration::days(1))
        .await
        .unwrap();

    let admin = PgAdminRepository::new(pool.clone());
    assert!(admin.delete_user(user.id).await.unwrap());
    assert!(!admin.delete_user(user.id).await.unwrap(), "already gone");

    assert!(users.find_by_character_id(1).await.unwrap().is_none());
    assert!(sessions
        .resolve_session(&token.hash())
        .await
        .unwrap()
        .is_none());
    let tokens: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM eve_connection_tokens")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tokens, 0, "refresh tokens are dropped");
    let connections: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM eve_connections WHERE id = $1")
        .bind(connection_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        connections, 0,
        "character links are erased with the workspace"
    );
    let workspace_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspaces WHERE id = $1")
        .bind(user.workspace_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(workspace_rows, 0, "the workspace itself is erased");

    // Other users are untouched, and the character can register afresh into
    // a brand-new workspace.
    assert!(users.find_by_character_id(2).await.unwrap().is_some());
    let _ = bystander;
    let again = users
        .claim_unclaimed_workspace_or_provision(
            identity("Returning Pilot"),
            iskworks_core::InviteGrant::NotRequired,
        )
        .await
        .unwrap();
    assert_ne!(again.workspace_id, user.workspace_id);
}
