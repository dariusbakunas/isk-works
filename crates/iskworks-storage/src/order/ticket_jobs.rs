//! `TicketJobRepository` for `PgOrderRepository`: stored ticket -> ESI job
//! links (`ticket_industry_jobs`).

use iskworks_core::order::{
    ObservedIndustryJob, TicketIndustryJob, TicketJobCandidate, TicketJobRepository,
    TicketJobSyncPlan,
};
use iskworks_core::IndustryActivity;

use super::*;

#[derive(sqlx::FromRow)]
struct CandidateRow {
    id: Uuid,
    kind: String,
    assignee_character_id: Uuid,
    type_id: i64,
    runs: Option<i64>,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct LinkRow {
    ticket_id: Uuid,
    connection_id: Uuid,
    job_id: i64,
    activity_id: i64,
    blueprint_type_id: i64,
    product_type_id: Option<i64>,
    facility_id: i64,
    runs: i64,
    cost: Option<Decimal>,
    status: String,
    start_date: DateTime<Utc>,
    end_date: DateTime<Utc>,
    pause_date: Option<DateTime<Utc>>,
    last_seen_at: DateTime<Utc>,
}

#[async_trait]
impl TicketJobRepository for PgOrderRepository {
    async fn ticket_job_candidates(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<TicketJobCandidate>, OrderError> {
        let rows: Vec<CandidateRow> = sqlx::query_as(
            "SELECT id, kind, assignee_character_id, type_id, \
                    (execution_snapshot->>'runs')::bigint AS runs, created_at \
             FROM tickets \
             WHERE workspace_id = $1 AND status = 'in_progress' AND archived_at IS NULL \
               AND kind IN ('manufacturing', 'reaction') \
               AND assignee_character_id IS NOT NULL AND type_id IS NOT NULL",
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?;
        rows.into_iter()
            .map(|row| {
                Ok(TicketJobCandidate {
                    ticket_id: TicketId(row.id),
                    assignee: ConnectedCharacterId(row.assignee_character_id),
                    activity: match ticket_kind_from_str(&row.kind)? {
                        TicketKind::Reaction => IndustryActivity::Reaction,
                        _ => IndustryActivity::Manufacturing,
                    },
                    product_type_id: row.type_id,
                    runs: row.runs.map(u64_from_i64).transpose()?,
                    created_at: row.created_at,
                })
            })
            .collect()
    }

    async fn list_ticket_jobs(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<TicketIndustryJob>, OrderError> {
        let rows: Vec<LinkRow> = sqlx::query_as(
            "SELECT ticket_id, connection_id, job_id, activity_id, blueprint_type_id, \
                    product_type_id, facility_id, runs, cost, status, start_date, end_date, \
                    pause_date, last_seen_at \
             FROM ticket_industry_jobs WHERE workspace_id = $1 \
             ORDER BY start_date, job_id",
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?;
        rows.into_iter()
            .map(|row| {
                Ok(TicketIndustryJob {
                    ticket_id: TicketId(row.ticket_id),
                    job: ObservedIndustryJob {
                        connection_id: ConnectedCharacterId(row.connection_id),
                        job_id: row.job_id,
                        activity_id: row.activity_id,
                        blueprint_type_id: row.blueprint_type_id,
                        product_type_id: row.product_type_id,
                        facility_id: row.facility_id,
                        runs: u64_from_i64(row.runs)?,
                        cost: row.cost.map(Money),
                        status: row.status,
                        start_date: row.start_date,
                        end_date: row.end_date,
                        pause_date: row.pause_date,
                    },
                    last_seen_at: row.last_seen_at,
                })
            })
            .collect()
    }

    async fn apply_ticket_job_sync(
        &self,
        workspace_id: WorkspaceId,
        plan: &TicketJobSyncPlan,
    ) -> Result<(), OrderError> {
        if plan.upserts.is_empty() && plan.removed_job_ids.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        if !plan.removed_job_ids.is_empty() {
            sqlx::query(
                "DELETE FROM ticket_industry_jobs WHERE workspace_id = $1 AND job_id = ANY($2)",
            )
            .bind(workspace_id.0)
            .bind(&plan.removed_job_ids)
            .execute(&mut *tx)
            .await
            .map_err(map_error)?;
        }
        for link in &plan.upserts {
            let job = &link.job;
            // The first link wins: a conflict refreshes the job's state but
            // never moves it to another ticket.
            sqlx::query(
                "INSERT INTO ticket_industry_jobs \
                   (workspace_id, job_id, ticket_id, connection_id, activity_id, \
                    blueprint_type_id, product_type_id, facility_id, runs, cost, status, \
                    start_date, end_date, pause_date, linked_at, last_seen_at) \
                 SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $15 \
                 WHERE EXISTS (SELECT 1 FROM tickets WHERE id = $3 AND workspace_id = $1) \
                 ON CONFLICT (workspace_id, job_id) DO UPDATE SET \
                   runs = EXCLUDED.runs, cost = EXCLUDED.cost, status = EXCLUDED.status, \
                   start_date = EXCLUDED.start_date, end_date = EXCLUDED.end_date, \
                   pause_date = EXCLUDED.pause_date, last_seen_at = EXCLUDED.last_seen_at",
            )
            .bind(workspace_id.0)
            .bind(job.job_id)
            .bind(link.ticket_id.0)
            .bind(job.connection_id.0)
            .bind(job.activity_id)
            .bind(job.blueprint_type_id)
            .bind(job.product_type_id)
            .bind(job.facility_id)
            .bind(i64_from_u64(job.runs)?)
            .bind(job.cost.map(|cost| cost.0))
            .bind(&job.status)
            .bind(job.start_date)
            .bind(job.end_date)
            .bind(job.pause_date)
            .bind(link.last_seen_at)
            .execute(&mut *tx)
            .await
            .map_err(map_error)?;
        }
        tx.commit().await.map_err(map_error)?;
        Ok(())
    }
}
