use super::*;
use crate::invite::consume_invite_in_tx;
use crate::{NewInvite, PgInviteRepository};
use iskworks_core::{
    generate_invite_code, hash_invite_code_from_raw, AuthError, EveIdentity, InviteGrant, InviteId,
    UserRepository,
};

async fn insert_invite(
    pool: &PgPool,
    raw_code: &str,
    max_uses: i32,
    expires_at: Option<DateTime<Utc>>,
    disabled: bool,
) -> InviteId {
    let invites = PgInviteRepository::new(pool.clone());
    let id = invites
        .create_invite(NewInvite {
            code_hash: hash_invite_code_from_raw(raw_code),
            max_uses,
            expires_at,
            note: Some("test".to_string()),
            code_ciphertext: None,
        })
        .await
        .unwrap();
    if disabled {
        assert!(invites.disable_invite(id).await.unwrap());
    }
    id
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deleted_invite_is_gone_and_no_longer_redeemable(pool: PgPool) {
    let code = generate_invite_code();
    let id = insert_invite(&pool, &code, 1, None, false).await;
    let invites = PgInviteRepository::new(pool.clone());

    assert!(invites.delete_invite(id).await.unwrap());
    assert!(
        !invites.delete_invite(id).await.unwrap(),
        "second delete finds nothing"
    );
    assert!(invites.list_invites().await.unwrap().is_empty());
    assert_eq!(
        invites
            .find_redeemable_invite_id(&hash_invite_code_from_raw(&code))
            .await
            .unwrap(),
        None
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn hash_round_trips_and_is_input_normalized(pool: PgPool) {
    let code = generate_invite_code();
    let id = insert_invite(&pool, &code, 1, None, false).await;
    let invites = PgInviteRepository::new(pool.clone());

    // Exact, lower-cased, and dash-mangled all resolve to the same row.
    for variant in [code.clone(), code.to_lowercase(), code.replace('-', " ")] {
        let hash = hash_invite_code_from_raw(&variant);
        assert_eq!(
            invites.find_redeemable_invite_id(&hash).await.unwrap(),
            Some(id),
            "variant {variant:?} should resolve"
        );
    }

    // An unrelated string resolves to nothing.
    let miss = hash_invite_code_from_raw("ISK-0000-0000-0000-0000");
    assert_eq!(
        invites.find_redeemable_invite_id(&miss).await.unwrap(),
        None
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn expired_disabled_and_exhausted_are_not_redeemable(pool: PgPool) {
    let invites = PgInviteRepository::new(pool.clone());

    let expired = insert_invite(
        &pool,
        "ISK-AAAA-AAAA-AAAA-AAAA",
        1,
        Some(Utc::now() - Duration::minutes(1)),
        false,
    )
    .await;
    let disabled = insert_invite(&pool, "ISK-BBBB-BBBB-BBBB-BBBB", 1, None, true).await;
    let exhausted = insert_invite(&pool, "ISK-CCCC-CCCC-CCCC-CCCC", 1, None, false).await;

    // Exhaust the third one via the atomic path.
    let mut tx = pool.begin().await.unwrap();
    consume_invite_in_tx(&mut tx, exhausted).await.unwrap();
    tx.commit().await.unwrap();

    for (label, id_hash) in [
        ("expired", "ISK-AAAA-AAAA-AAAA-AAAA"),
        ("disabled", "ISK-BBBB-BBBB-BBBB-BBBB"),
        ("exhausted", "ISK-CCCC-CCCC-CCCC-CCCC"),
    ] {
        let hash = hash_invite_code_from_raw(id_hash);
        assert_eq!(
            invites.find_redeemable_invite_id(&hash).await.unwrap(),
            None,
            "{label} must not be redeemable"
        );
    }

    // consume_invite_in_tx also refuses all three.
    for id in [expired, disabled, exhausted] {
        let mut tx = pool.begin().await.unwrap();
        assert!(matches!(
            consume_invite_in_tx(&mut tx, id).await,
            Err(AuthError::InviteRejected)
        ));
    }
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn multi_use_invite_increments_to_its_exact_boundary(pool: PgPool) {
    let id = insert_invite(&pool, "ISK-DDDD-DDDD-DDDD-DDDD", 3, None, false).await;

    for expected in 1..=3 {
        let mut tx = pool.begin().await.unwrap();
        consume_invite_in_tx(&mut tx, id).await.unwrap();
        tx.commit().await.unwrap();
        let count: i32 = sqlx::query_scalar("SELECT use_count FROM invite_codes WHERE id = $1")
            .bind(id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, expected);
    }

    // Fourth attempt is refused, count unchanged.
    let mut tx = pool.begin().await.unwrap();
    assert!(matches!(
        consume_invite_in_tx(&mut tx, id).await,
        Err(AuthError::InviteRejected)
    ));
    drop(tx);
    let count: i32 = sqlx::query_scalar("SELECT use_count FROM invite_codes WHERE id = $1")
        .bind(id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 3);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn provisioning_consumes_the_invite_atomically(pool: PgPool) {
    let id = insert_invite(&pool, "ISK-EEEE-EEEE-EEEE-EEEE", 1, None, false).await;
    let users = PgUserRepository::new(pool.clone());

    let user = users
        .claim_unclaimed_workspace_or_provision(
            EveIdentity {
                character_id: 42,
                character_name: "Invited Pilot".to_string(),
            },
            InviteGrant::Required(id),
        )
        .await
        .unwrap();

    // Workspace + user exist, invite is spent.
    let workspace_count: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE id = $1")
        .bind(user.workspace_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(workspace_count, 1);
    let use_count: i32 = sqlx::query_scalar("SELECT use_count FROM invite_codes WHERE id = $1")
        .bind(id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(use_count, 1);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_rejected_invite_rolls_back_the_whole_provisioning(pool: PgPool) {
    let id = insert_invite(&pool, "ISK-FFFF-FFFF-FFFF-FFFF", 1, None, true).await; // disabled
    let users = PgUserRepository::new(pool.clone());

    let result = users
        .claim_unclaimed_workspace_or_provision(
            EveIdentity {
                character_id: 43,
                character_name: "Rejected Pilot".to_string(),
            },
            InviteGrant::Required(id),
        )
        .await;
    assert!(matches!(result, Err(AuthError::InviteRejected)));

    let workspaces: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces")
        .fetch_one(&pool)
        .await
        .unwrap();
    let users_count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(workspaces, 0, "no workspace for the loser");
    assert_eq!(users_count, 0, "no user for the loser");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_returning_identity_never_consumes_an_invite(pool: PgPool) {
    let id = insert_invite(&pool, "ISK-GGGG-GGGG-GGGG-GGGG", 5, None, false).await;
    let users = PgUserRepository::new(pool.clone());
    let identity = EveIdentity {
        character_id: 44,
        character_name: "Repeat Pilot".to_string(),
    };

    // First login provisions with no invite (open registration).
    users
        .claim_unclaimed_workspace_or_provision(identity.clone(), InviteGrant::NotRequired)
        .await
        .unwrap();
    // Second login arrives WITH an invite grant attached (as if invite
    // mode were toggled on) — it must be ignored for an existing user.
    users
        .claim_unclaimed_workspace_or_provision(identity, InviteGrant::Required(id))
        .await
        .unwrap();

    let use_count: i32 = sqlx::query_scalar("SELECT use_count FROM invite_codes WHERE id = $1")
        .bind(id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(use_count, 0, "returning user must not spend an invite");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_redemption_of_a_single_use_invite_admits_exactly_one(pool: PgPool) {
    let id = insert_invite(&pool, "ISK-HHHH-HHHH-HHHH-HHHH", 1, None, false).await;

    let users_a = PgUserRepository::new(pool.clone());
    let users_b = PgUserRepository::new(pool.clone());
    let invite_id = id;

    let first = tokio::spawn(async move {
        users_a
            .claim_unclaimed_workspace_or_provision(
                EveIdentity {
                    character_id: 100,
                    character_name: "Racer One".to_string(),
                },
                InviteGrant::Required(invite_id),
            )
            .await
    });
    let second = tokio::spawn(async move {
        users_b
            .claim_unclaimed_workspace_or_provision(
                EveIdentity {
                    character_id: 200,
                    character_name: "Racer Two".to_string(),
                },
                InviteGrant::Required(invite_id),
            )
            .await
    });

    let results = [first.await.unwrap(), second.await.unwrap()];
    let winners = results.iter().filter(|r| r.is_ok()).count();
    let losers = results
        .iter()
        .filter(|r| matches!(r, Err(AuthError::InviteRejected)))
        .count();
    assert_eq!(winners, 1, "exactly one provisioning succeeds");
    assert_eq!(losers, 1, "the other gets InviteRejected");

    let use_count: i32 = sqlx::query_scalar("SELECT use_count FROM invite_codes WHERE id = $1")
        .bind(id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(use_count, 1);
    let workspaces: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces")
        .fetch_one(&pool)
        .await
        .unwrap();
    let users_count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(workspaces, 1, "only the winner's workspace exists");
    assert_eq!(users_count, 1, "only the winner's user exists");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn disable_is_idempotent_and_list_never_shows_a_code(pool: PgPool) {
    let invites = PgInviteRepository::new(pool.clone());
    let id = insert_invite(&pool, "ISK-JJJJ-JJJJ-JJJJ-JJJJ", 1, None, false).await;

    assert!(invites.disable_invite(id).await.unwrap());
    let first_disabled_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT disabled_at FROM invite_codes WHERE id = $1")
            .bind(id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    // Second disable keeps the original timestamp.
    assert!(invites.disable_invite(id).await.unwrap());
    let second_disabled_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT disabled_at FROM invite_codes WHERE id = $1")
            .bind(id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(first_disabled_at, second_disabled_at);

    // Unknown id -> false, not an error.
    assert!(!invites.disable_invite(InviteId::new()).await.unwrap());

    let listed = invites.list_invites().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, id);
    assert_eq!(listed[0].max_uses, 1);
}
