//! Keeps Board tickets linked to the ESI industry jobs run for them (see
//! `iskworks_core::order::plan_ticket_job_sync`). Runs after each
//! successful `industry_jobs` sync, and before the Board reads links, so a
//! ticket moved to In Progress between syncs links straight away.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use iskworks_core::order::{
    job_fits_ticket, plan_ticket_job_sync, ticket_job_choices, ObservedIndustryJob, OrderError,
    TicketId, TicketIndustryJob, TicketJobChoice, TicketJobRepository,
};
use iskworks_core::{ConnectedCharacterId, InventoryError, WorkspaceId};
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
        let (observed_connections, observed) = self.observe(workspace_id).await?;
        let candidates = self.links.ticket_job_candidates(workspace_id).await?;
        let linked = self.links.list_ticket_jobs(workspace_id).await?;
        let excluded = self.links.list_ticket_job_exclusions(workspace_id).await?;
        let plan = plan_ticket_job_sync(
            &candidates,
            &linked,
            &excluded,
            &observed,
            &observed_connections,
            Utc::now(),
        );
        self.links
            .apply_ticket_job_sync(workspace_id, &plan)
            .await?;
        Ok(())
    }

    /// The jobs ESI reports now that the user could link to the ticket by
    /// hand -- its assignee's, for its activity and product -- each with
    /// the ticket it is on now, if any.
    pub async fn job_choices(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
    ) -> Result<Vec<TicketJobChoice>, TicketJobError> {
        let target = self
            .links
            .ticket_job_target(workspace_id, ticket_id)
            .await?;
        if target.assignee.is_none() {
            return Err(OrderError::TicketJobNeedsAssignee.into());
        }
        let (_, observed) = self.observe(workspace_id).await?;
        let linked = self.links.list_ticket_jobs(workspace_id).await?;
        Ok(ticket_job_choices(&target, &observed, &linked))
    }

    /// Links a job ESI reports now to the ticket by hand, moving it off any
    /// other ticket. Refused unless it fits ([`job_fits_ticket`]).
    pub async fn link_job(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        job_id: i64,
    ) -> Result<(), TicketJobError> {
        let target = self
            .links
            .ticket_job_target(workspace_id, ticket_id)
            .await?;
        let (_, observed) = self.observe(workspace_id).await?;
        let job = observed
            .into_iter()
            .find(|job| job.job_id == job_id)
            .ok_or(OrderError::TicketJobNotFound)?;
        job_fits_ticket(&target, &job)?;
        Ok(self
            .links
            .link_ticket_job(workspace_id, ticket_id, &job)
            .await?)
    }

    /// Unlinks a job from the ticket; automatic matching won't link it back.
    pub async fn unlink_job(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        job_id: i64,
    ) -> Result<(), TicketJobError> {
        Ok(self
            .links
            .unlink_ticket_job(workspace_id, ticket_id, job_id)
            .await?)
    }

    /// The connections whose jobs were observed, and those jobs.
    async fn observe(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<(HashSet<ConnectedCharacterId>, Vec<ObservedIndustryJob>), TicketJobError> {
        let observations = workspace_job_observations(&self.esi, workspace_id).await?;
        let connections = observations
            .iter()
            .map(|(connection, _)| *connection)
            .collect();
        let observed = observations
            .iter()
            .flat_map(|(connection, jobs)| {
                jobs.iter().filter_map(|job| job.to_observed(*connection))
            })
            .collect();
        Ok((connections, observed))
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
