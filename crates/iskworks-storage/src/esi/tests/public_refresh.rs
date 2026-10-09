use super::*;

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn latest_adjusted_prices_returns_latest_requested_observation_even_when_stale(pool: PgPool) {
    let repository = PgEsiRepository::new(pool.clone());
    let now = crate::db_now();
    for (type_id, price, observed_at, expires_at) in [
        (
            34_i64,
            Decimal::new(400, 2),
            now - chrono::Duration::hours(2),
            now + chrono::Duration::hours(1),
        ),
        (
            34,
            Decimal::new(425, 2),
            now - chrono::Duration::minutes(5),
            now + chrono::Duration::hours(1),
        ),
        (
            35,
            Decimal::new(800, 2),
            now - chrono::Duration::hours(2),
            now - chrono::Duration::minutes(1),
        ),
        (
            36,
            Decimal::new(900, 2),
            now,
            now + chrono::Duration::hours(1),
        ),
    ] {
        sqlx::query("INSERT INTO industry_adjusted_price_observations (id,type_id,adjusted_price,observed_at,expires_at,source_checksum,source_url) VALUES ($1,$2,$3,$4,$5,'fixture','fixture')")
            .bind(Uuid::new_v4()).bind(type_id).bind(price).bind(observed_at).bind(expires_at)
            .execute(&pool).await.unwrap();
    }
    let prices = repository
        .latest_adjusted_prices(&[34, 35, 34], now)
        .await
        .unwrap();
    assert_eq!(prices.len(), 2);
    assert_eq!(prices.get(&34), Some(&Decimal::new(425, 2)));
    assert_eq!(prices.get(&35), Some(&Decimal::new(800, 2)));
    assert_eq!(
        repository
            .latest_adjusted_price_observed_at(&[34, 35])
            .await
            .unwrap(),
        Some(now - chrono::Duration::hours(2))
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn adjusted_price_refresh_claim_is_exclusive_and_recovers_after_lease_expiry(pool: PgPool) {
    let repository = PgEsiRepository::new(pool);
    let now = crate::db_now();

    repository
        .register_adjusted_price_refresh(now)
        .await
        .unwrap();
    assert!(repository
        .begin_adjusted_price_refresh(now, now + chrono::Duration::minutes(2))
        .await
        .unwrap()
        .is_some());
    assert!(repository
        .begin_adjusted_price_refresh(
            now + chrono::Duration::minutes(1),
            now + chrono::Duration::minutes(3),
        )
        .await
        .unwrap()
        .is_none());
    assert!(repository
        .begin_adjusted_price_refresh(
            now + chrono::Duration::minutes(3),
            now + chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .is_some());
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn expired_adjusted_price_claim_cannot_complete_or_fail_a_newer_claim(pool: PgPool) {
    let repository = PgEsiRepository::new(pool.clone());
    let first = crate::db_now();
    repository
        .register_adjusted_price_refresh(first)
        .await
        .unwrap();
    let first_claim = repository
        .begin_adjusted_price_refresh(first, first + chrono::Duration::minutes(1))
        .await
        .unwrap()
        .unwrap();
    let second = first + chrono::Duration::minutes(2);
    let second_claim = repository
        .begin_adjusted_price_refresh(second, second + chrono::Duration::minutes(1))
        .await
        .unwrap()
        .unwrap();

    assert!(!repository
        .complete_adjusted_price_refresh(
            first_claim,
            &[],
            first,
            first + chrono::Duration::hours(6),
            None,
            None,
        )
        .await
        .unwrap());
    assert!(!repository
        .fail_adjusted_price_refresh(
            first_claim,
            first,
            first + chrono::Duration::minutes(1),
            "late failure",
        )
        .await
        .unwrap());
    assert!(repository
        .fail_adjusted_price_refresh(
            second_claim,
            second,
            second + chrono::Duration::minutes(1),
            "current failure",
        )
        .await
        .unwrap());
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn failed_adjusted_price_refresh_preserves_the_last_dataset(pool: PgPool) {
    let repository = PgEsiRepository::new(pool);
    let observed_at = crate::db_now() - chrono::Duration::hours(12);
    repository
        .register_adjusted_price_refresh(observed_at)
        .await
        .unwrap();
    let initial_claim = repository
        .begin_adjusted_price_refresh(observed_at, observed_at + chrono::Duration::minutes(2))
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_adjusted_price_refresh(
            initial_claim,
            &[iskworks_esi::AdjustedPrice {
                type_id: 34,
                adjusted_price: Decimal::new(425, 2),
            }],
            observed_at,
            observed_at + chrono::Duration::hours(6),
            None,
            Some("fixture"),
        )
        .await
        .unwrap();
    let retry_at = crate::db_now();
    let retry_claim = repository
        .begin_adjusted_price_refresh(retry_at, retry_at + chrono::Duration::minutes(2))
        .await
        .unwrap()
        .unwrap();
    repository
        .fail_adjusted_price_refresh(
            retry_claim,
            retry_at,
            retry_at + chrono::Duration::minutes(1),
            "temporary ESI error",
        )
        .await
        .unwrap();

    assert_eq!(
        repository
            .latest_adjusted_prices(&[34], crate::db_now())
            .await
            .unwrap()
            .get(&34),
        Some(&Decimal::new(425, 2))
    );
    let overlay = repository.adjusted_price_refresh_overlay().await.unwrap();
    assert!(!overlay.pending);
    assert_eq!(overlay.last_error.as_deref(), Some("temporary ESI error"));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn system_index_registration_is_idempotent_and_claims_due_system_once(pool: PgPool) {
    let repository = PgEsiRepository::new(pool);
    let now = crate::db_now();

    repository
        .register_system_cost_index(30_000_142, now)
        .await
        .unwrap();
    repository
        .register_system_cost_index(30_000_142, now)
        .await
        .unwrap();
    let due = repository
        .system_cost_index_refresh_candidates(now, 10)
        .await
        .unwrap();
    assert_eq!(due, vec![30_000_142]);
    assert!(repository
        .begin_system_cost_index_refresh(30_000_142, now, now + chrono::Duration::minutes(2),)
        .await
        .unwrap()
        .is_some());
    assert!(repository
        .begin_system_cost_index_refresh(30_000_142, now, now + chrono::Duration::minutes(2),)
        .await
        .unwrap()
        .is_none());
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn prioritizing_prices_and_indices_waits_for_esi_expires_or_a_retry(pool: PgPool) {
    let repository = PgEsiRepository::new(pool);
    let t0 = crate::db_now();
    let expires = t0 + chrono::Duration::minutes(40);
    repository
        .register_adjusted_price_refresh(t0)
        .await
        .unwrap();
    let claim = repository
        .begin_adjusted_price_refresh(t0, t0 + chrono::Duration::minutes(2))
        .await
        .unwrap()
        .unwrap();
    assert!(repository
        .complete_adjusted_price_refresh(
            claim,
            &[],
            t0,
            t0 + chrono::Duration::hours(6),
            Some(expires),
            None,
        )
        .await
        .unwrap());
    let clicked = t0 + chrono::Duration::minutes(1);
    assert!(
        AdjustedPriceRepository::prioritize_adjusted_price_refresh(&repository, clicked)
            .await
            .unwrap()
    );
    assert!(!repository
        .adjusted_price_refresh_due(clicked)
        .await
        .unwrap());
    assert!(repository
        .adjusted_price_refresh_due(expires)
        .await
        .unwrap());

    let system_id = 30_000_142;
    repository
        .register_system_cost_index(system_id, t0)
        .await
        .unwrap();
    let claim = repository
        .begin_system_cost_index_refresh(system_id, t0, t0 + chrono::Duration::minutes(2))
        .await
        .unwrap()
        .unwrap();
    let retry_at = t0 + chrono::Duration::minutes(1);
    assert!(repository
        .fail_system_cost_index_refresh(system_id, claim, t0, retry_at, "503")
        .await
        .unwrap());
    assert!(
        AdjustedPriceRepository::prioritize_system_cost_index_refresh(&repository, system_id, t0,)
            .await
            .unwrap()
    );
    assert!(repository
        .system_cost_index_refresh_candidates(t0, 10)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        repository
            .system_cost_index_refresh_candidates(retry_at, 10)
            .await
            .unwrap(),
        vec![system_id]
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn failing_prices_and_indices_back_off_exponentially_until_a_success(pool: PgPool) {
    let repository = PgEsiRepository::new(pool.clone());
    let system_id = 30_000_142;
    let mut at = crate::db_now();
    repository
        .register_adjusted_price_refresh(at)
        .await
        .unwrap();
    repository
        .register_system_cost_index(system_id, at)
        .await
        .unwrap();
    let price_next = || async {
        sqlx::query_scalar::<_, DateTime<Utc>>(
            "SELECT next_refresh_at FROM industry_adjusted_price_refresh_state",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    let index_next = || async {
        sqlx::query_scalar::<_, DateTime<Utc>>(
            "SELECT next_refresh_at FROM industry_system_cost_index_registrations",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    };

    let mut price_waits = Vec::new();
    let mut index_waits = Vec::new();
    for _ in 0..4 {
        let lease = at + chrono::Duration::minutes(2);
        let claim = repository
            .begin_adjusted_price_refresh(at, lease)
            .await
            .unwrap()
            .unwrap();
        assert!(repository
            .fail_adjusted_price_refresh(claim, at, at + chrono::Duration::minutes(1), "503")
            .await
            .unwrap());
        let claim = repository
            .begin_system_cost_index_refresh(system_id, at, lease)
            .await
            .unwrap()
            .unwrap();
        assert!(repository
            .fail_system_cost_index_refresh(
                system_id,
                claim,
                at,
                at + chrono::Duration::minutes(1),
                "503"
            )
            .await
            .unwrap());
        let (price, index) = (price_next().await, index_next().await);
        price_waits.push((price - at).num_minutes());
        index_waits.push((index - at).num_minutes());
        at = price.max(index);
    }
    assert_eq!(price_waits, vec![1, 2, 4, 8]);
    assert_eq!(index_waits, vec![1, 2, 4, 8]);

    // A success resets both.
    let lease = at + chrono::Duration::minutes(2);
    let claim = repository
        .begin_adjusted_price_refresh(at, lease)
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_adjusted_price_refresh(claim, &[], at, at, None, None)
        .await
        .unwrap();
    let claim = repository
        .begin_system_cost_index_refresh(system_id, at, lease)
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_system_cost_index_refresh(
            system_id,
            claim,
            Decimal::new(1, 2),
            at,
            at,
            None,
            None,
        )
        .await
        .unwrap();
    let claim = repository
        .begin_adjusted_price_refresh(at, lease)
        .await
        .unwrap()
        .unwrap();
    repository
        .fail_adjusted_price_refresh(claim, at, at + chrono::Duration::minutes(1), "503")
        .await
        .unwrap();
    assert_eq!(price_next().await, at + chrono::Duration::minutes(1));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn manual_system_index_priority_precedes_routine_missing_work(pool: PgPool) {
    let repository = PgEsiRepository::new(pool);
    let now = crate::db_now();
    repository
        .register_system_cost_index(30_000_142, now)
        .await
        .unwrap();
    repository
        .register_system_cost_index(30_002_187, now)
        .await
        .unwrap();
    assert!(AdjustedPriceRepository::prioritize_system_cost_index_refresh(
        &repository,
        30_002_187,
        now,
    ).await.unwrap());

    assert_eq!(
        repository
            .system_cost_index_refresh_candidates(now, 1)
            .await
            .unwrap(),
        vec![30_002_187]
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn expired_system_index_claim_cannot_overwrite_a_newer_claim(pool: PgPool) {
    let repository = PgEsiRepository::new(pool.clone());
    let system_id = 30_000_142;
    let first = crate::db_now();
    repository
        .register_system_cost_index(system_id, first)
        .await
        .unwrap();
    let first_claim = repository
        .begin_system_cost_index_refresh(system_id, first, first + chrono::Duration::minutes(1))
        .await
        .unwrap()
        .unwrap();
    let second = first + chrono::Duration::minutes(2);
    let second_claim = repository
        .begin_system_cost_index_refresh(system_id, second, second + chrono::Duration::minutes(1))
        .await
        .unwrap()
        .unwrap();

    assert!(!repository
        .complete_system_cost_index_refresh(
            system_id,
            first_claim,
            Decimal::new(1, 2),
            first,
            first + chrono::Duration::hours(1),
            None,
            None,
        )
        .await
        .unwrap());
    assert!(repository
        .complete_system_cost_index_refresh(
            system_id,
            second_claim,
            Decimal::new(2, 2),
            second,
            second + chrono::Duration::hours(1),
            None,
            None,
        )
        .await
        .unwrap());
    assert_eq!(
        repository
            .latest_system_cost_index(system_id)
            .await
            .unwrap(),
        Some((Decimal::new(2, 2), second))
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn adjusted_price_observations_cannot_be_altered_but_can_be_pruned(pool: PgPool) {
    let now = crate::db_now();
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO industry_adjusted_price_observations (id,type_id,adjusted_price,observed_at,expires_at,source_checksum,source_url) VALUES ($1,34,1,$2,$2,'fixture','fixture')")
        .bind(id).bind(now).execute(&pool).await.unwrap();

    assert!(sqlx::query(
        "UPDATE industry_adjusted_price_observations SET adjusted_price=2 WHERE id=$1"
    )
    .bind(id)
    .execute(&pool)
    .await
    .is_err());
    sqlx::query("DELETE FROM industry_adjusted_price_observations WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn adjusted_price_sweep_keeps_only_each_types_latest_observation(pool: PgPool) {
    let repository = PgEsiRepository::new(pool.clone());
    let now = crate::db_now();
    let at = |hours_ago: i64| now - chrono::Duration::hours(hours_ago);
    for (type_id, price, observed_at) in [
        (34, 1, at(18)),
        (34, 2, at(12)),
        (34, 3, at(6)),
        (35, 10, at(12)),
        (35, 20, at(6)),
        // No longer listed by ESI: its last observation stays.
        (36, 7, at(18)),
    ] {
        sqlx::query("INSERT INTO industry_adjusted_price_observations (id,type_id,adjusted_price,observed_at,expires_at,source_checksum,source_url) VALUES ($1,$2,$3,$4,$4,'fixture','fixture')")
            .bind(Uuid::new_v4()).bind(type_id).bind(Decimal::from(price)).bind(observed_at)
            .execute(&pool).await.unwrap();
    }

    let first = repository
        .prune_superseded_adjusted_prices(2, 1)
        .await
        .unwrap();
    assert_eq!((first.rows_deleted, first.drained), (2, false));
    let rest = repository
        .prune_superseded_adjusted_prices(2, 10)
        .await
        .unwrap();
    assert_eq!((rest.rows_deleted, rest.drained), (1, true));

    let left: Vec<(i64, Decimal)> = sqlx::query_as(
        "SELECT type_id, adjusted_price FROM industry_adjusted_price_observations ORDER BY type_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        left,
        vec![
            (34, Decimal::from(3)),
            (35, Decimal::from(20)),
            (36, Decimal::from(7)),
        ]
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn failed_system_index_refresh_preserves_the_last_stale_observation(pool: PgPool) {
    let repository = PgEsiRepository::new(pool);
    let observed_at = crate::db_now() - chrono::Duration::hours(3);
    repository
        .register_system_cost_index(30_000_142, observed_at)
        .await
        .unwrap();
    let initial_claim = repository
        .begin_system_cost_index_refresh(
            30_000_142,
            observed_at,
            observed_at + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_system_cost_index_refresh(
            30_000_142,
            initial_claim,
            Decimal::new(425, 4),
            observed_at,
            observed_at + chrono::Duration::hours(1),
            None,
            Some("fixture"),
        )
        .await
        .unwrap();
    let retry_at = crate::db_now();
    let retry_claim = repository
        .begin_system_cost_index_refresh(
            30_000_142,
            retry_at,
            retry_at + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .fail_system_cost_index_refresh(
            30_000_142,
            retry_claim,
            retry_at,
            retry_at + chrono::Duration::minutes(1),
            "temporary ESI error",
        )
        .await
        .unwrap();

    assert_eq!(
        repository
            .latest_system_cost_index(30_000_142)
            .await
            .unwrap(),
        Some((Decimal::new(425, 4), observed_at))
    );
    let overlay = repository
        .system_cost_index_refresh_overlay(30_000_142)
        .await
        .unwrap();
    assert!(!overlay.pending);
    assert_eq!(overlay.last_error.as_deref(), Some("temporary ESI error"));
}
