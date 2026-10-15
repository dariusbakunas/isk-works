use std::collections::hash_map::Entry;

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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TicketJobChoiceDto {
    job: IndustryJobDto,
    /// The ticket the job is linked to now, if any -- linking moves it.
    linked_ticket_id: Option<TicketId>,
    linked_ticket_display_id: Option<String>,
}

/// The jobs ESI reports now that could be linked to the ticket by hand:
/// its assignee's, for its activity and exact product, not already on it.
/// `ticket_job_needs_assignee` for an unassigned ticket.
pub(super) async fn list_ticket_job_choices(
    State(state): State<AppState>,
    Path(ticket_id): Path<uuid::Uuid>,
) -> Result<Json<Vec<TicketJobChoiceDto>>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let service = state.ticket_job_service()?;
    let choices = service
        .job_choices(workspace_id, TicketId(ticket_id))
        .await
        .map_err(api_error)?;
    let mut jobs: Vec<IndustryJobDto> = choices
        .iter()
        .map(|choice| IndustryJobDto::from_observed(&choice.job))
        .collect();
    if let Ok(roster) = state.character_roster_service() {
        if let Err(error) = roster.label_industry_jobs(workspace_id, &mut jobs).await {
            tracing::warn!(%error, "naming industry job choices failed");
        }
    }
    let repository = state.order_repository()?;
    let mut display_ids: HashMap<TicketId, String> = HashMap::new();
    for linked in choices.iter().filter_map(|choice| choice.linked_ticket_id) {
        if let Entry::Vacant(entry) = display_ids.entry(linked) {
            entry.insert(
                repository
                    .get_ticket(workspace_id, linked)
                    .await?
                    .display_id,
            );
        }
    }
    Ok(Json(
        choices
            .iter()
            .zip(jobs)
            .map(|(choice, job)| TicketJobChoiceDto {
                job,
                linked_ticket_id: choice.linked_ticket_id,
                linked_ticket_display_id: choice
                    .linked_ticket_id
                    .and_then(|id| display_ids.get(&id).cloned()),
            })
            .collect(),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LinkTicketJobRequest {
    job_id: i64,
}

/// Links a job to the ticket by hand, moving it off any other ticket.
/// `job_does_not_match_ticket` unless the ticket's assignee ran it for the
/// ticket's activity and exact product; `ticket_job_not_found` if ESI
/// doesn't report it now.
pub(super) async fn link_ticket_job(
    State(state): State<AppState>,
    Path(ticket_id): Path<uuid::Uuid>,
    Json(request): Json<LinkTicketJobRequest>,
) -> Result<StatusCode, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    state
        .ticket_job_service()?
        .link_job(workspace_id, TicketId(ticket_id), request.job_id)
        .await
        .map_err(api_error)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Unlinks a job from the ticket; automatic matching won't link it back.
pub(super) async fn unlink_ticket_job(
    State(state): State<AppState>,
    Path((ticket_id, job_id)): Path<(uuid::Uuid, i64)>,
) -> Result<StatusCode, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    state
        .ticket_job_service()?
        .unlink_job(workspace_id, TicketId(ticket_id), job_id)
        .await
        .map_err(api_error)?;
    Ok(StatusCode::NO_CONTENT)
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
