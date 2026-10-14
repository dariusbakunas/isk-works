use super::*;
use chrono::Duration;
use iskworks_core::order::{
    ObservedIndustryJob, TicketIndustryJob, TicketJobRepository, TicketJobSyncPlan,
    DELIVERED_JOB_STATUS,
};
use iskworks_core::IndustryActivity;
use std::collections::HashSet;

// ─── Stored ticket -> ESI job links (`ticket_industry_jobs`) ──────────────

fn observed(connection_id: ConnectedCharacterId, job_id: i64, status: &str) -> ObservedIndustryJob {
    let start = crate::db_now();
    ObservedIndustryJob {
        connection_id,
        job_id,
        activity_id: 1,
        blueprint_type_id: 1000,
        product_type_id: Some(20185),
        facility_id: 1_050_000_000_001,
        runs: 10,
        cost: Money::parse("123456.78").ok(),
        status: status.to_string(),
        start_date: start,
        end_date: start + Duration::hours(5),
        pause_date: None,
    }
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn ticket_jobs_round_trip_and_a_job_keeps_its_first_ticket(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let character = fixture_character(&pool, workspace_id, owner_id, "Builder").await;
    let repository = PgOrderRepository::new(pool.clone());
    let new_ticket = |planned_runs| {
        let mut ticket = mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            20185,
            "Product",
            build_id,
            Vec::new(),
            planned_runs,
        );
        ticket.assignee_character_id = Some(character);
        ticket
    };
    let (first, _) = repository
        .create_ticket(new_ticket(Some(10)))
        .await
        .unwrap();
    let (second, _) = repository.create_ticket(new_ticket(None)).await.unwrap();

    // Only In Progress tickets are candidates.
    assert!(repository
        .ticket_job_candidates(workspace_id)
        .await
        .unwrap()
        .is_empty());
    repository
        .set_ticket_status(workspace_id, first.id, TicketStatus::InProgress)
        .await
        .unwrap();
    let candidates = repository
        .ticket_job_candidates(workspace_id)
        .await
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].ticket_id, first.id);
    assert_eq!(candidates[0].assignee, character);
    assert_eq!(candidates[0].runs, Some(10));

    let now = crate::db_now();
    let job = observed(character, 900, "active");
    repository
        .apply_ticket_job_sync(
            workspace_id,
            &TicketJobSyncPlan {
                upserts: vec![TicketIndustryJob {
                    ticket_id: first.id,
                    job: job.clone(),
                    last_seen_at: now,
                }],
                removed_job_ids: Vec::new(),
            },
        )
        .await
        .unwrap();
    let links = repository.list_ticket_jobs(workspace_id).await.unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].ticket_id, first.id);
    assert_eq!(links[0].job.cost, Money::parse("123456.78").ok());
    assert_eq!(links[0].job.runs, 10);

    // A later write naming another ticket refreshes the state only.
    let delivered = ObservedIndustryJob {
        status: DELIVERED_JOB_STATUS.to_string(),
        ..job
    };
    repository
        .apply_ticket_job_sync(
            workspace_id,
            &TicketJobSyncPlan {
                upserts: vec![TicketIndustryJob {
                    ticket_id: second.id,
                    job: delivered,
                    last_seen_at: now,
                }],
                removed_job_ids: Vec::new(),
            },
        )
        .await
        .unwrap();
    let links = repository.list_ticket_jobs(workspace_id).await.unwrap();
    assert_eq!(links[0].ticket_id, first.id);
    assert_eq!(links[0].job.status, DELIVERED_JOB_STATUS);

    // Removed jobs are deleted.
    repository
        .apply_ticket_job_sync(
            workspace_id,
            &TicketJobSyncPlan {
                upserts: Vec::new(),
                removed_job_ids: vec![900],
            },
        )
        .await
        .unwrap();
    assert!(repository
        .list_ticket_jobs(workspace_id)
        .await
        .unwrap()
        .is_empty());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn jobs_link_by_hand_move_between_tickets_and_unlinks_are_remembered(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let character = fixture_character(&pool, workspace_id, owner_id, "Builder").await;
    let repository = PgOrderRepository::new(pool.clone());
    let new_ticket = || {
        let mut ticket = mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            20185,
            "Product",
            build_id,
            Vec::new(),
            Some(10),
        );
        ticket.assignee_character_id = Some(character);
        ticket
    };
    let (first, _) = repository.create_ticket(new_ticket()).await.unwrap();
    let (second, _) = repository.create_ticket(new_ticket()).await.unwrap();

    // Any production ticket, whatever its status, has a target.
    let target = repository
        .ticket_job_target(workspace_id, first.id)
        .await
        .unwrap();
    assert_eq!(target.ticket_id, first.id);
    assert_eq!(target.assignee, Some(character));
    assert_eq!(target.activity, IndustryActivity::Manufacturing);
    assert_eq!(target.product_type_id, 20185);
    assert!(matches!(
        repository
            .ticket_job_target(workspace_id, TicketId::new())
            .await,
        Err(OrderError::TicketNotFound)
    ));

    let job = observed(character, 900, "active");
    repository
        .link_ticket_job(workspace_id, first.id, &job)
        .await
        .unwrap();
    let links = repository.list_ticket_jobs(workspace_id).await.unwrap();
    assert_eq!(
        links
            .iter()
            .map(|l| (l.ticket_id, l.job.job_id))
            .collect::<Vec<_>>(),
        vec![(first.id, 900)]
    );

    // Linking it to another ticket moves it.
    repository
        .link_ticket_job(workspace_id, second.id, &job)
        .await
        .unwrap();
    let links = repository.list_ticket_jobs(workspace_id).await.unwrap();
    assert_eq!(
        links
            .iter()
            .map(|l| (l.ticket_id, l.job.job_id))
            .collect::<Vec<_>>(),
        vec![(second.id, 900)]
    );

    // Unlinking deletes the link and remembers the pair; a job the ticket
    // doesn't have is not found.
    assert!(matches!(
        repository
            .unlink_ticket_job(workspace_id, first.id, 900)
            .await,
        Err(OrderError::TicketJobNotFound)
    ));
    repository
        .unlink_ticket_job(workspace_id, second.id, 900)
        .await
        .unwrap();
    assert!(repository
        .list_ticket_jobs(workspace_id)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        repository
            .list_ticket_job_exclusions(workspace_id)
            .await
            .unwrap(),
        HashSet::from([(second.id, 900)])
    );

    // Linking the pair by hand again clears its exclusion.
    repository
        .link_ticket_job(workspace_id, second.id, &job)
        .await
        .unwrap();
    assert!(repository
        .list_ticket_job_exclusions(workspace_id)
        .await
        .unwrap()
        .is_empty());
}
