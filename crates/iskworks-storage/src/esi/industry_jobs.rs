//! The Jobs history (`character_industry_jobs`): what each jobs sync
//! records, and the reads of it, plus the Board ticket each job is linked to.

use std::collections::HashMap;

use async_trait::async_trait;
use iskworks_core::order::TicketId;
use iskworks_core::{IndustryJobRepository, JobTicketLink, Money, StoredIndustryJob};

use super::*;

#[async_trait]
impl IndustryJobRepository for PgEsiRepository {
    async fn completed_jobs(
        &self,
        workspace_id: WorkspaceId,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<StoredIndustryJob>, StoreError> {
        #[allow(clippy::type_complexity)]
        let rows: Vec<(
            Uuid,
            i64,
            i64,
            i64,
            Option<i64>,
            i64,
            i64,
            Option<i64>,
            Option<Decimal>,
            String,
            DateTime<Utc>,
            DateTime<Utc>,
            Option<DateTime<Utc>>,
        )> = sqlx::query_as(
            "SELECT connection_id, job_id, activity_id, blueprint_type_id, product_type_id, \
               facility_id, runs, successful_runs, cost, status, start_date, end_date, \
               completed_date \
             FROM character_industry_jobs \
             WHERE workspace_id = $1 AND completed_date >= $2 AND completed_date < $3 \
             ORDER BY completed_date DESC, job_id DESC",
        )
        .bind(workspace_id.0)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    connection_id,
                    job_id,
                    activity_id,
                    blueprint_type_id,
                    product_type_id,
                    facility_id,
                    runs,
                    successful_runs,
                    cost,
                    status,
                    start_date,
                    end_date,
                    completed_date,
                )| StoredIndustryJob {
                    connection_id: ConnectedCharacterId(connection_id),
                    job_id,
                    activity_id,
                    blueprint_type_id,
                    product_type_id,
                    facility_id,
                    runs,
                    successful_runs,
                    cost: cost.map(Money),
                    status,
                    start_date,
                    end_date,
                    completed_date,
                },
            )
            .collect())
    }

    async fn earliest_job_seen(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Option<DateTime<Utc>>, StoreError> {
        sqlx::query_scalar(
            "SELECT MIN(first_seen_at) FROM character_industry_jobs WHERE workspace_id = $1",
        )
        .bind(workspace_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx)
    }

    async fn ticket_links(
        &self,
        workspace_id: WorkspaceId,
        job_ids: &[i64],
    ) -> Result<HashMap<i64, JobTicketLink>, StoreError> {
        if job_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<(i64, Uuid, String)> = sqlx::query_as(
            "SELECT l.job_id, t.id, t.display_id FROM ticket_industry_jobs l \
             JOIN tickets t ON t.id = l.ticket_id \
             WHERE l.workspace_id = $1 AND l.job_id = ANY($2)",
        )
        .bind(workspace_id.0)
        .bind(job_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(rows
            .into_iter()
            .map(|(job_id, ticket_id, display_id)| {
                (
                    job_id,
                    JobTicketLink {
                        ticket_id: TicketId(ticket_id),
                        display_id,
                    },
                )
            })
            .collect())
    }

    async fn record_industry_jobs(
        &self,
        connection_id: ConnectedCharacterId,
        jobs: &[iskworks_core::CharacterIndustryJobObservation],
        observed_at: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        if jobs.is_empty() {
            return Ok(());
        }
        let mut query = sqlx::QueryBuilder::new(
            "INSERT INTO character_industry_jobs (workspace_id, connection_id, job_id, \
             activity_id, blueprint_id, blueprint_type_id, product_type_id, facility_id, \
             station_id, runs, licensed_runs, successful_runs, probability, cost, status, \
             start_date, end_date, pause_date, completed_date, completed_character_id, \
             first_seen_at, last_seen_at) ",
        );
        query.push_values(jobs, |mut row, job| {
            row.push("(SELECT workspace_id FROM eve_connections WHERE id = ")
                .push_bind_unseparated(connection_id.0)
                .push_unseparated(")")
                .push_bind(connection_id.0)
                .push_bind(job.job_id)
                .push_bind(job.activity_id)
                .push_bind(job.blueprint_id)
                .push_bind(job.blueprint_type_id)
                .push_bind(job.product_type_id)
                .push_bind(job.facility_id)
                .push_bind(job.station_id)
                .push_bind(job.runs)
                .push_bind(job.licensed_runs)
                .push_bind(job.successful_runs)
                .push_bind(job.probability)
                .push_bind(job.cost)
                .push_bind(&job.status)
                .push_bind(job.start_date)
                .push_bind(job.end_date)
                .push_bind(job.pause_date)
                .push_bind(job.completed_date)
                .push_bind(job.completed_character_id)
                .push_bind(observed_at)
                .push_bind(observed_at);
        });
        query.push(
            " ON CONFLICT (workspace_id, job_id) DO UPDATE SET \
             connection_id = EXCLUDED.connection_id, \
             blueprint_id = COALESCE(EXCLUDED.blueprint_id, character_industry_jobs.blueprint_id), \
             product_type_id = EXCLUDED.product_type_id, runs = EXCLUDED.runs, \
             licensed_runs = EXCLUDED.licensed_runs, successful_runs = EXCLUDED.successful_runs, \
             probability = EXCLUDED.probability, cost = EXCLUDED.cost, status = EXCLUDED.status, \
             end_date = EXCLUDED.end_date, pause_date = EXCLUDED.pause_date, \
             completed_date = EXCLUDED.completed_date, \
             completed_character_id = EXCLUDED.completed_character_id, \
             last_seen_at = EXCLUDED.last_seen_at",
        );
        query.build().execute(&self.pool).await.map_err(map_sqlx)?;
        Ok(())
    }
}
