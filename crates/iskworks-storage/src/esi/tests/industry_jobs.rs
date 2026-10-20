use super::*;
use chrono::{Duration, TimeZone, Utc};
use iskworks_esi::CharacterIndustryJobObservation;
use rust_decimal::Decimal;

fn job(job_id: i64, status: &str) -> CharacterIndustryJobObservation {
    let start = Utc.with_ymd_and_hms(2026, 10, 1, 12, 0, 0).unwrap();
    CharacterIndustryJobObservation {
        job_id,
        activity_id: 1,
        blueprint_id: Some(1_040_000_000_001),
        blueprint_type_id: 691,
        product_type_id: Some(626),
        facility_id: 60_003_760,
        station_id: Some(60_003_760),
        runs: 2,
        licensed_runs: Some(100),
        cost: Some(Decimal::new(1_250_000, 0)),
        probability: None,
        duration_seconds: Some(7_200),
        status: status.to_string(),
        start_date: start,
        end_date: start + Duration::hours(2),
        pause_date: None,
        completed_date: None,
        completed_character_id: None,
        successful_runs: None,
    }
}

async fn stored(
    pool: &PgPool,
) -> Vec<(
    i64,
    String,
    Option<i64>,
    chrono::DateTime<Utc>,
    chrono::DateTime<Utc>,
)> {
    sqlx::query_as(
        "SELECT job_id, status, successful_runs, first_seen_at, last_seen_at \
         FROM character_industry_jobs ORDER BY job_id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn recording_jobs_upserts_and_keeps_first_seen(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let connection = fixture_connection(&pool, workspace_id, owner_id, "Jobs Pilot").await;
    let repository = PgEsiRepository::new(pool.clone());
    let first = Utc.with_ymd_and_hms(2026, 10, 1, 13, 0, 0).unwrap();
    let later = first + Duration::hours(3);

    repository
        .record_industry_jobs(connection.id, &[job(1, "active"), job(2, "active")], first)
        .await
        .unwrap();
    let mut delivered = job(1, "delivered");
    delivered.completed_date = Some(later);
    delivered.successful_runs = Some(2);
    repository
        .record_industry_jobs(connection.id, &[delivered], later)
        .await
        .unwrap();

    let rows = stored(&pool).await;
    assert_eq!(rows.len(), 2);
    assert_eq!(
        (rows[0].0, rows[0].1.as_str(), rows[0].2),
        (1, "delivered", Some(2))
    );
    assert_eq!((rows[0].3, rows[0].4), (first, later));
    // A job ESI no longer returns is kept as last seen.
    assert_eq!(
        (rows[1].0, rows[1].1.as_str(), rows[1].4),
        (2, "active", first)
    );
    let workspace: uuid::Uuid =
        sqlx::query_scalar("SELECT DISTINCT workspace_id FROM character_industry_jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(workspace, workspace_id.0);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn recording_no_jobs_is_a_no_op(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let connection = fixture_connection(&pool, workspace_id, owner_id, "Idle Pilot").await;
    let repository = PgEsiRepository::new(pool.clone());

    repository
        .record_industry_jobs(connection.id, &[], Utc::now())
        .await
        .unwrap();

    assert!(stored(&pool).await.is_empty());
}

fn completed(
    job_id: i64,
    status: &str,
    on: chrono::DateTime<Utc>,
) -> CharacterIndustryJobObservation {
    let mut job = job(job_id, status);
    job.completed_date = Some(on);
    job.successful_runs = Some(2);
    job
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn completed_jobs_are_read_newest_first_within_the_window(pool: PgPool) {
    use iskworks_core::IndustryJobRepository;
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let connection = fixture_connection(&pool, workspace_id, owner_id, "History Pilot").await;
    let repository = PgEsiRepository::new(pool.clone());
    let day = |d: u32| Utc.with_ymd_and_hms(2026, 10, d, 12, 0, 0).unwrap();
    let seen = day(20);
    repository
        .record_industry_jobs(
            connection.id,
            &[
                completed(1, "delivered", day(5)),
                completed(2, "cancelled", day(12)),
                completed(3, "delivered", day(19)),
                job(4, "active"),
            ],
            seen,
        )
        .await
        .unwrap();

    let jobs = repository
        .completed_jobs(workspace_id, day(10), day(20))
        .await
        .unwrap();

    let ids: Vec<i64> = jobs.iter().map(|job| job.job_id).collect();
    assert_eq!(ids, vec![3, 2]);
    assert_eq!(jobs[0].connection_id, connection.id);
    assert_eq!(jobs[0].successful_runs, Some(2));
    assert_eq!(
        jobs[0].cost.map(|cost| cost.0),
        Some(Decimal::new(1_250_000, 0))
    );
    assert_eq!(jobs[1].status, "cancelled");
    assert_eq!(
        repository.earliest_job_seen(workspace_id).await.unwrap(),
        Some(seen)
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn ticket_links_name_the_linked_ticket(pool: PgPool) {
    use iskworks_core::IndustryJobRepository;
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let connection = fixture_connection(&pool, workspace_id, owner_id, "Linked Pilot").await;
    let repository = PgEsiRepository::new(pool.clone());
    let ticket_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tickets (id, workspace_id, owner_id, display_id, kind, type_id, quantity, \
           captured_name, status, created_at, updated_at) \
         VALUES ($1, $2, $3, 'TK-142', 'manufacturing', 626, 2, 'Ishtar', 'in_progress', now(), now())",
    )
    .bind(ticket_id)
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ticket_industry_jobs (workspace_id, job_id, ticket_id, connection_id, \
           activity_id, blueprint_type_id, product_type_id, facility_id, runs, status, \
           start_date, end_date, linked_at, last_seen_at) \
         VALUES ($1, 7, $2, $3, 1, 691, 626, 60003760, 2, 'active', now(), now(), now(), now())",
    )
    .bind(workspace_id.0)
    .bind(ticket_id)
    .bind(connection.id.0)
    .execute(&pool)
    .await
    .unwrap();

    let links = repository
        .ticket_links(workspace_id, &[7, 8])
        .await
        .unwrap();

    assert_eq!(links.len(), 1);
    assert_eq!(links[&7].display_id, "TK-142");
    assert_eq!(links[&7].ticket_id.0, ticket_id);
    assert!(repository
        .ticket_links(workspace_id, &[])
        .await
        .unwrap()
        .is_empty());
}
