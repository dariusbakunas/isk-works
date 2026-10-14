use iskworks_app::{IndustryJobDto, TicketJobError};
use iskworks_core::order::TicketIndustryJob;

use super::*;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TicketJobProgress {
    ticket_id: TicketId,
    /// The ESI jobs linked to this ticket, earliest-started first, with
    /// their last known state (`delivered` once they have left ESI).
    /// Progress is derived client-side from their fixed dates.
    jobs: Vec<IndustryJobDto>,
}

/// Every ticket's linked ESI industry jobs. Links new jobs first (the
/// worker also does after each jobs sync), so a ticket moved to In
/// Progress since then links straight away. Empty when ESI isn't
/// configured: no characters means no jobs.
pub(super) async fn list_ticket_job_progress(
    State(state): State<AppState>,
) -> Result<Json<Vec<TicketJobProgress>>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let Ok(service) = state.ticket_job_service() else {
        return Ok(Json(Vec::new()));
    };
    // Stored links still answer when linking new ones fails.
    if let Err(error) = service.sync(workspace_id).await {
        tracing::warn!(%error, "linking tickets to industry jobs failed");
    }
    let links = service.links(workspace_id).await.map_err(api_error)?;
    let mut jobs: Vec<IndustryJobDto> = links
        .iter()
        .map(|link| IndustryJobDto::from_observed(&link.job))
        .collect();
    if let Ok(roster) = state.character_roster_service() {
        if let Err(error) = roster.label_industry_jobs(workspace_id, &mut jobs).await {
            tracing::warn!(%error, "naming linked industry jobs failed");
        }
    }
    Ok(Json(group_by_ticket(&links, jobs)))
}

fn api_error(error: TicketJobError) -> ApiError {
    match error {
        TicketJobError::Inventory(error) => error.into(),
        TicketJobError::Order(error) => error.into(),
    }
}

/// `jobs[i]` is `links[i]`'s job; tickets keep first-appearance order.
fn group_by_ticket(
    links: &[TicketIndustryJob],
    jobs: Vec<IndustryJobDto>,
) -> Vec<TicketJobProgress> {
    let mut progress: Vec<TicketJobProgress> = Vec::new();
    for (link, job) in links.iter().zip(jobs) {
        match progress
            .iter_mut()
            .find(|entry| entry.ticket_id == link.ticket_id)
        {
            Some(entry) => entry.jobs.push(job),
            None => progress.push(TicketJobProgress {
                ticket_id: link.ticket_id,
                jobs: vec![job],
            }),
        }
    }
    progress
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use iskworks_core::order::ObservedIndustryJob;
    use iskworks_core::ConnectedCharacterId;

    use super::*;

    fn link(ticket_id: TicketId, job_id: i64) -> TicketIndustryJob {
        let now = Utc::now();
        TicketIndustryJob {
            ticket_id,
            job: ObservedIndustryJob {
                connection_id: ConnectedCharacterId::new(),
                job_id,
                activity_id: 9,
                blueprint_type_id: 46_166,
                product_type_id: Some(16_670),
                facility_id: 1,
                runs: 5,
                cost: None,
                status: "active".to_string(),
                start_date: now,
                end_date: now,
                pause_date: None,
            },
            last_seen_at: now,
        }
    }

    #[test]
    fn groups_linked_jobs_by_ticket_in_order() {
        let (first, second) = (TicketId::new(), TicketId::new());
        let links = vec![link(first, 1), link(second, 2), link(first, 3)];
        let jobs = links
            .iter()
            .map(|link| IndustryJobDto::from_observed(&link.job))
            .collect();

        let progress = group_by_ticket(&links, jobs);

        assert_eq!(progress.len(), 2);
        assert_eq!(progress[0].ticket_id, first);
        let ids: Vec<i64> = progress[0].jobs.iter().map(|job| job.job_id).collect();
        assert_eq!(ids, vec![1, 3]);
        assert_eq!(progress[1].ticket_id, second);
    }
}
