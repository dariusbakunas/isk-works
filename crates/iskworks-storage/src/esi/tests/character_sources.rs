use super::*;

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn registering_a_connection_creates_a_row_for_every_source_kind(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let connection = fixture_connection(&pool, workspace_id, owner_id, "Roster Pilot").await;
    let repository = PgEsiRepository::new(pool);

    repository
        .register_character_sources(connection.id)
        .await
        .unwrap();

    let expected_kinds: std::collections::HashSet<_> =
        CharacterSourceKind::all().into_iter().collect();

    let state = repository
        .character_source_state(connection.id)
        .await
        .unwrap();
    assert_eq!(
        state
            .iter()
            .map(|row| row.source_kind)
            .collect::<std::collections::HashSet<_>>(),
        expected_kinds,
    );
    assert!(state
        .iter()
        .all(|row| row.refresh_state == MarketRefreshState::Missing && row.summary.is_none()));

    // Idempotent: registering again must not duplicate rows.
    repository
        .register_character_sources(connection.id)
        .await
        .unwrap();
    let state_again = repository
        .character_source_state(connection.id)
        .await
        .unwrap();
    assert_eq!(state_again.len(), state.len());
    assert_eq!(
        state_again
            .iter()
            .map(|row| row.source_kind)
            .collect::<std::collections::HashSet<_>>(),
        expected_kinds,
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn begin_claims_a_due_row_and_a_second_claim_before_lease_expiry_is_rejected(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let connection = fixture_connection(&pool, workspace_id, owner_id, "Roster Pilot").await;
    let repository = PgEsiRepository::new(pool);
    repository
        .register_character_sources(connection.id)
        .await
        .unwrap();
    let now = crate::db_now();

    let first = repository
        .begin_character_source_refresh(
            connection.id,
            CharacterSourceKind::Location,
            now,
            now + chrono::Duration::seconds(120),
        )
        .await
        .unwrap();
    assert!(first.is_some());

    let second = repository
        .begin_character_source_refresh(
            connection.id,
            CharacterSourceKind::Location,
            now + chrono::Duration::seconds(1),
            now + chrono::Duration::seconds(121),
        )
        .await
        .unwrap();
    assert!(second.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn complete_persists_summary_and_clears_the_refreshing_state(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let connection = fixture_connection(&pool, workspace_id, owner_id, "Roster Pilot").await;
    let repository = PgEsiRepository::new(pool);
    repository
        .register_character_sources(connection.id)
        .await
        .unwrap();
    let now = crate::db_now();
    let claim = repository
        .begin_character_source_refresh(
            connection.id,
            CharacterSourceKind::Wallet,
            now,
            now + chrono::Duration::seconds(120),
        )
        .await
        .unwrap()
        .unwrap();

    let next_refresh_at = now + chrono::Duration::minutes(10);
    let summary = json!({ "balance": "1250000.00" });
    let completed = repository
        .complete_character_source_refresh(
            connection.id,
            CharacterSourceKind::Wallet,
            claim,
            next_refresh_at,
            summary.clone(),
            now,
        )
        .await
        .unwrap();
    assert!(completed);

    let state = repository
        .character_source_state(connection.id)
        .await
        .unwrap();
    let wallet = state
        .into_iter()
        .find(|row| row.source_kind == CharacterSourceKind::Wallet)
        .unwrap();
    assert_eq!(wallet.refresh_state, MarketRefreshState::Current);
    assert_eq!(wallet.summary, Some(summary));
    assert_eq!(wallet.observed_at, Some(now));
    assert_eq!(wallet.next_refresh_at, Some(next_refresh_at));
    assert!(wallet.last_error.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn fail_records_the_error_and_schedules_a_retry_instead_of_hot_looping(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let connection = fixture_connection(&pool, workspace_id, owner_id, "Roster Pilot").await;
    let repository = PgEsiRepository::new(pool);
    repository
        .register_character_sources(connection.id)
        .await
        .unwrap();
    let now = crate::db_now();
    let claim = repository
        .begin_character_source_refresh(
            connection.id,
            CharacterSourceKind::IndustryJobs,
            now,
            now + chrono::Duration::seconds(120),
        )
        .await
        .unwrap()
        .unwrap();

    let retry_at = now + chrono::Duration::minutes(1);
    let failed = repository
        .fail_character_source_refresh(
            connection.id,
            CharacterSourceKind::IndustryJobs,
            claim,
            now,
            retry_at,
            "missing scope".to_string(),
        )
        .await
        .unwrap();
    assert!(failed);

    let state = repository
        .character_source_state(connection.id)
        .await
        .unwrap();
    let jobs = state
        .into_iter()
        .find(|row| row.source_kind == CharacterSourceKind::IndustryJobs)
        .unwrap();
    assert_eq!(jobs.refresh_state, MarketRefreshState::Failed);
    assert_eq!(jobs.last_error.as_deref(), Some("missing scope"));
    // The first failure backs off 5 minutes even when the caller asked for less.
    let retry_at = now + chrono::Duration::minutes(5);
    assert_eq!(jobs.next_refresh_at, Some(retry_at));

    // The row must be claimable again once the (short-circuited, already
    // elapsed) next_refresh_at is due -- not stuck behind a stale lease.
    let reclaimed = repository
        .begin_character_source_refresh(
            connection.id,
            CharacterSourceKind::IndustryJobs,
            retry_at,
            retry_at + chrono::Duration::seconds(120),
        )
        .await
        .unwrap();
    assert!(reclaimed.is_some());
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn repeated_source_failures_back_off_exponentially_until_a_success(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let connection = fixture_connection(&pool, workspace_id, owner_id, "Roster Pilot").await;
    let repository = PgEsiRepository::new(pool);
    repository
        .register_character_sources(connection.id)
        .await
        .unwrap();
    let kind = CharacterSourceKind::IndustryJobs;
    let next_refresh_at = || async {
        repository
            .character_source_state(connection.id)
            .await
            .unwrap()
            .into_iter()
            .find(|row| row.source_kind == kind)
            .unwrap()
            .next_refresh_at
            .unwrap()
    };
    let mut at = crate::db_now();
    let mut waits = Vec::new();
    for _ in 0..4 {
        let claim = repository
            .begin_character_source_refresh(
                connection.id,
                kind,
                at,
                at + chrono::Duration::seconds(120),
            )
            .await
            .unwrap()
            .unwrap();
        assert!(repository
            .fail_character_source_refresh(
                connection.id,
                kind,
                claim,
                at,
                at + chrono::Duration::minutes(5),
                "403".to_string(),
            )
            .await
            .unwrap());
        let next = next_refresh_at().await;
        waits.push((next - at).num_minutes());
        at = next;
    }
    assert_eq!(waits, vec![5, 10, 20, 40]);

    // A server-directed wait longer than the backoff wins.
    let claim = repository
        .begin_character_source_refresh(
            connection.id,
            kind,
            at,
            at + chrono::Duration::seconds(120),
        )
        .await
        .unwrap()
        .unwrap();
    let retry_after = at + chrono::Duration::hours(2);
    repository
        .fail_character_source_refresh(
            connection.id,
            kind,
            claim,
            at,
            retry_after,
            "429".to_string(),
        )
        .await
        .unwrap();
    assert_eq!(next_refresh_at().await, retry_after);

    // A success resets the backoff.
    at = retry_after;
    let claim = repository
        .begin_character_source_refresh(
            connection.id,
            kind,
            at,
            at + chrono::Duration::seconds(120),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_character_source_refresh(
            connection.id,
            kind,
            claim,
            at + chrono::Duration::minutes(30),
            serde_json::json!({}),
            at,
        )
        .await
        .unwrap();
    at += chrono::Duration::minutes(30);
    let claim = repository
        .begin_character_source_refresh(
            connection.id,
            kind,
            at,
            at + chrono::Duration::seconds(120),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .fail_character_source_refresh(
            connection.id,
            kind,
            claim,
            at,
            at + chrono::Duration::minutes(5),
            "503".to_string(),
        )
        .await
        .unwrap();
    assert_eq!(next_refresh_at().await, at + chrono::Duration::minutes(5));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn due_character_sources_excludes_refreshing_and_not_yet_due_rows(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let connection = fixture_connection(&pool, workspace_id, owner_id, "Roster Pilot").await;
    let repository = PgEsiRepository::new(pool);
    repository
        .register_character_sources(connection.id)
        .await
        .unwrap();
    let now = crate::db_now();

    // Location: still "missing" -- due.
    // Wallet: claim it so it's "refreshing" with an unexpired lease -- not due.
    repository
        .begin_character_source_refresh(
            connection.id,
            CharacterSourceKind::Wallet,
            now,
            now + chrono::Duration::seconds(120),
        )
        .await
        .unwrap();
    // Skills: complete it with a next_refresh_at in the past -- due again.
    let skills_claim = repository
        .begin_character_source_refresh(
            connection.id,
            CharacterSourceKind::Skills,
            now,
            now + chrono::Duration::seconds(120),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_character_source_refresh(
            connection.id,
            CharacterSourceKind::Skills,
            skills_claim,
            now - chrono::Duration::minutes(1),
            json!({}),
            now,
        )
        .await
        .unwrap();
    // Character info: complete it with a next_refresh_at in the future -- not due.
    let info_claim = repository
        .begin_character_source_refresh(
            connection.id,
            CharacterSourceKind::CharacterInfo,
            now,
            now + chrono::Duration::seconds(120),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_character_source_refresh(
            connection.id,
            CharacterSourceKind::CharacterInfo,
            info_claim,
            now + chrono::Duration::hours(6),
            json!({}),
            now,
        )
        .await
        .unwrap();

    let due = repository.due_character_sources(now, 10).await.unwrap();
    let due_kinds: std::collections::HashSet<_> = due.into_iter().collect();
    assert!(due_kinds.contains(&(connection.id, CharacterSourceKind::Location)));
    assert!(due_kinds.contains(&(connection.id, CharacterSourceKind::Skills)));
    assert!(!due_kinds.contains(&(connection.id, CharacterSourceKind::Wallet)));
    assert!(!due_kinds.contains(&(connection.id, CharacterSourceKind::CharacterInfo)));
}

/// Sources of a connection that can't sync anymore must drop out of the due
/// queue -- otherwise they sit at its head (`next_refresh_at NULLS FIRST`)
/// forever and starve every other tenant's character sync.
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn due_character_sources_skips_dead_connections_and_disabled_users(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let live = fixture_connection(&pool, workspace_id, owner_id, "Live Pilot").await;
    let disconnected = fixture_connection(&pool, workspace_id, owner_id, "Gone Pilot").await;
    let revoked = fixture_connection(&pool, workspace_id, owner_id, "Revoked Pilot").await;
    let unscoped = fixture_connection(&pool, workspace_id, owner_id, "Unscoped Pilot").await;
    let (disabled_workspace_id, disabled_owner_id) = fixture_workspace(&pool).await;
    let disabled_users_connection = fixture_connection(
        &pool,
        disabled_workspace_id,
        disabled_owner_id,
        "Banned Pilot",
    )
    .await;
    sqlx::query(
        "INSERT INTO users (id, eve_character_id, eve_character_name, workspace_id, created_at, last_login_at, disabled_at)
         VALUES ($1, 990001, 'Banned Pilot', $2, now(), now(), now())",
    )
    .bind(Uuid::new_v4())
    .bind(disabled_workspace_id.0)
    .execute(&pool)
    .await
    .unwrap();

    let repository = PgEsiRepository::new(pool);
    for connection in [
        &live,
        &disconnected,
        &revoked,
        &unscoped,
        &disabled_users_connection,
    ] {
        repository
            .register_character_sources(connection.id)
            .await
            .unwrap();
    }
    repository.disconnect(disconnected.id).await.unwrap();
    repository
        .mark_connection_unrefreshable(
            revoked.id,
            ConnectionStatus::NeedsReconnection,
            "refresh_rejected",
        )
        .await
        .unwrap();
    repository
        .mark_connection_unrefreshable(unscoped.id, ConnectionStatus::MissingScope, "missing_scope")
        .await
        .unwrap();

    let due = repository
        .due_character_sources(crate::db_now(), 100)
        .await
        .unwrap();
    let due_connections: std::collections::HashSet<_> = due
        .into_iter()
        .map(|(connection_id, _)| connection_id)
        .collect();
    assert_eq!(due_connections, std::collections::HashSet::from([live.id]));

    let revoked = repository.get_connection(revoked.id).await.unwrap();
    assert_eq!(revoked.status, ConnectionStatus::NeedsReconnection);
    let unscoped = repository.get_connection(unscoped.id).await.unwrap();
    assert_eq!(unscoped.status, ConnectionStatus::MissingScope);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn mark_connection_unrefreshable_rejects_non_failure_statuses(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let connection = fixture_connection(&pool, workspace_id, owner_id, "Roster Pilot").await;
    let repository = PgEsiRepository::new(pool);
    for status in [ConnectionStatus::Connected, ConnectionStatus::Disconnected] {
        assert!(matches!(
            repository
                .mark_connection_unrefreshable(connection.id, status, "nope")
                .await,
            Err(InventoryError::Validation(_))
        ));
    }
}

/// A transient token failure defers the connection's due sources instead of
/// leaving them at the head of the queue to be retried every pass.
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn defer_character_sources_pushes_due_sources_back(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let connection = fixture_connection(&pool, workspace_id, owner_id, "Roster Pilot").await;
    let repository = PgEsiRepository::new(pool);
    repository
        .register_character_sources(connection.id)
        .await
        .unwrap();
    let now = crate::db_now();
    // Character info is fresh until well after the deferral -- the deferral
    // must not pull it forward.
    let info_claim = repository
        .begin_character_source_refresh(
            connection.id,
            CharacterSourceKind::CharacterInfo,
            now,
            now + chrono::Duration::seconds(120),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_character_source_refresh(
            connection.id,
            CharacterSourceKind::CharacterInfo,
            info_claim,
            now + chrono::Duration::hours(6),
            json!({}),
            now,
        )
        .await
        .unwrap();

    repository
        .defer_character_sources(connection.id, now + chrono::Duration::minutes(15))
        .await
        .unwrap();

    assert!(repository
        .due_character_sources(now, 100)
        .await
        .unwrap()
        .is_empty());
    let later = repository
        .due_character_sources(now + chrono::Duration::minutes(16), 100)
        .await
        .unwrap();
    assert!(!later.is_empty());
    assert!(later
        .iter()
        .all(|(_, kind)| *kind != CharacterSourceKind::CharacterInfo));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn character_source_state_returns_every_registered_source_for_a_connection(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let connection = fixture_connection(&pool, workspace_id, owner_id, "Roster Pilot").await;
    let repository = PgEsiRepository::new(pool);

    repository
        .register_character_sources(connection.id)
        .await
        .unwrap();

    let state = repository
        .character_source_state(connection.id)
        .await
        .unwrap();
    let kinds: std::collections::HashSet<_> = state.iter().map(|row| row.source_kind).collect();
    assert_eq!(
        kinds,
        CharacterSourceKind::all()
            .into_iter()
            .collect::<std::collections::HashSet<_>>(),
    );
    for row in &state {
        assert_eq!(row.refresh_state, MarketRefreshState::Missing);
        assert_eq!(row.connection_id, connection.id);
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn entity_names_returns_only_the_cached_ids_and_ignores_unknown_ones(pool: PgPool) {
    let repository = PgEsiRepository::new(pool);
    repository
        .cache_entity_names(&[
            iskworks_esi::EveEntityName {
                id: 98_000_001,
                name: "Perimeter Industrial Holdings".to_string(),
                category: "corporation".to_string(),
            },
            iskworks_esi::EveEntityName {
                id: 30_000_142,
                name: "Jita".to_string(),
                category: "solar_system".to_string(),
            },
        ])
        .await
        .unwrap();

    let names = repository
        .entity_names(&[98_000_001, 30_000_142, 99_999_999])
        .await
        .unwrap();

    assert_eq!(names.len(), 2);
    assert_eq!(
        names.get(&98_000_001).map(String::as_str),
        Some("Perimeter Industrial Holdings")
    );
    assert_eq!(names.get(&30_000_142).map(String::as_str), Some("Jita"));
    assert!(!names.contains_key(&99_999_999));

    assert!(repository.entity_names(&[]).await.unwrap().is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn completing_a_connection_registers_its_character_sources(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let repository = PgEsiRepository::new(pool);
    let cipher = SecretCipher::for_tests();
    let envelope = cipher.encrypt("dummy-refresh-token").unwrap();

    let connection = repository
        .mock_connect(workspace_id, owner_id, envelope, &[])
        .await
        .unwrap();

    let state = repository
        .character_source_state(connection.id)
        .await
        .unwrap();
    let kinds: std::collections::HashSet<_> = state.iter().map(|row| row.source_kind).collect();
    assert_eq!(
        kinds,
        CharacterSourceKind::all()
            .into_iter()
            .collect::<std::collections::HashSet<_>>(),
        "a newly connected character must have every source kind registered up \
         front, otherwise it can never be selected by due_character_sources and \
         the worker will never sync it"
    );
    for row in &state {
        assert_eq!(row.refresh_state, MarketRefreshState::Missing);
    }

    // Reconnecting (mock_connect always targets the same fixture character)
    // must stay idempotent rather than resetting already-progressed state.
    repository
        .begin_character_source_refresh(
            connection.id,
            CharacterSourceKind::Wallet,
            crate::db_now(),
            crate::db_now() + chrono::Duration::minutes(2),
        )
        .await
        .unwrap();
    repository
        .mock_connect(
            workspace_id,
            owner_id,
            cipher.encrypt("rotated").unwrap(),
            &[],
        )
        .await
        .unwrap();
    let wallet_state = repository
        .character_source_state(connection.id)
        .await
        .unwrap()
        .into_iter()
        .find(|row| row.source_kind == CharacterSourceKind::Wallet)
        .unwrap();
    assert_eq!(wallet_state.refresh_state, MarketRefreshState::Refreshing);
}
