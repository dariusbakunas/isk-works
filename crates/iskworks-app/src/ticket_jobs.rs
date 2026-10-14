//! Keeps Board tickets linked to the ESI industry jobs run for them (see
//! `iskworks_core::order::plan_ticket_job_sync`). Runs after each
//! successful `industry_jobs` sync, and before the Board reads links, so a
//! ticket moved to In Progress between syncs links straight away.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use iskworks_core::order::{
    plan_ticket_job_sync, ObservedIndustryJob, OrderError, TicketIndustryJob, TicketJobRepository,
};
use iskworks_core::{InventoryError, WorkspaceId};
use iskworks_storage::PgEsiRepository;
use thiserror::Error;

use crate::character_roster::workspace_job_observations;

#[derive(Debug, Error)]
pub enum TicketJobError {
    #[error(transparent)]
    Inventory(#[from] InventoryError),
    #[error(transparent)]
    Order(#[from] OrderError),
}

/// Told when a workspace's ESI industry jobs were refreshed.
#[async_trait]
pub trait IndustryJobsSynced: Send + Sync {
    async fn industry_jobs_synced(&self, workspace_id: WorkspaceId);
}

#[derive(Clone)]
pub struct TicketJobService {
    esi: Arc<PgEsiRepository>,
    links: Arc<dyn TicketJobRepository>,
}

impl TicketJobService {
    #[must_use]
    pub fn new(esi: Arc<PgEsiRepository>, links: Arc<dyn TicketJobRepository>) -> Self {
        Self { esi, links }
    }

    /// Links new jobs to In Progress tickets and refreshes linked ones.
    pub async fn sync(&self, workspace_id: WorkspaceId) -> Result<(), TicketJobError> {
        let observations = workspace_job_observations(&self.esi, workspace_id).await?;
        let observed_connections: HashSet<_> = observations
            .iter()
            .map(|(connection, _)| *connection)
            .collect();
        let observed: Vec<ObservedIndustryJob> = observations
            .iter()
            .flat_map(|(connection, jobs)| {
                jobs.iter().filter_map(|job| job.to_observed(*connection))
            })
            .collect();
        let candidates = self.links.ticket_job_candidates(workspace_id).await?;
        let linked = self.links.list_ticket_jobs(workspace_id).await?;
        let plan = plan_ticket_job_sync(
            &candidates,
            &linked,
            &observed,
            &observed_connections,
            Utc::now(),
        );
        self.links
            .apply_ticket_job_sync(workspace_id, &plan)
            .await?;
        Ok(())
    }

    /// Every stored link in the workspace, earliest-started first.
    pub async fn links(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<TicketIndustryJob>, TicketJobError> {
        Ok(self.links.list_ticket_jobs(workspace_id).await?)
    }
}

#[async_trait]
impl IndustryJobsSynced for TicketJobService {
    async fn industry_jobs_synced(&self, workspace_id: WorkspaceId) {
        if let Err(error) = self.sync(workspace_id).await {
            tracing::warn!(%error, "linking tickets to industry jobs failed");
        }
    }
}
