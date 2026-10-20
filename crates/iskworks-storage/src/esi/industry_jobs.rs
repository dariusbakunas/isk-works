//! Reads of `character_industry_jobs` (the Jobs history) and the Board
//! ticket each job is linked to.

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
    ) -> Result<Vec<StoredIndustryJob>, InventoryError> {
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
    ) -> Result<Option<DateTime<Utc>>, InventoryError> {
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
    ) -> Result<HashMap<i64, JobTicketLink>, InventoryError> {
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
}
