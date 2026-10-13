use iskworks_app::IndustryJobDto;
use iskworks_core::order::{match_ticket_industry_jobs, ObservedIndustryJob, TicketJobCandidate};

use super::*;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TicketJobProgress {
    ticket_id: TicketId,
    /// The assignee's ESI jobs this ticket is matched to, earliest-started
    /// first. Progress is derived client-side from their fixed dates.
    jobs: Vec<IndustryJobDto>,
}

/// In Progress Manufacturing/Reaction tickets matched to their assignee's
/// running ESI industry jobs -- display only, recomputed on every read,
/// never persisted (see `iskworks_core::order::match_ticket_industry_jobs`).
/// Empty when ESI isn't configured: no characters means no jobs.
pub(super) async fn list_ticket_job_progress(
    State(state): State<AppState>,
) -> Result<Json<Vec<TicketJobProgress>>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let tickets = state
        .order_repository()?
        .list_tickets(workspace_id, owner_id)
        .await?;
    let candidates: Vec<TicketJobCandidate> = tickets
        .iter()
        .filter(|ticket| ticket.archived_at.is_none())
        .filter_map(TicketJobCandidate::from_ticket)
        .collect();
    if candidates.is_empty() {
        return Ok(Json(Vec::new()));
    }
    let Ok(roster) = state.character_roster_service() else {
        return Ok(Json(Vec::new()));
    };
    let jobs = roster.workspace_industry_jobs(workspace_id).await?;
    Ok(Json(assemble_job_progress(&candidates, jobs)))
}

fn assemble_job_progress(
    candidates: &[TicketJobCandidate],
    jobs: Vec<(ConnectedCharacterId, IndustryJobDto)>,
) -> Vec<TicketJobProgress> {
    let observed: Vec<ObservedIndustryJob> = jobs
        .iter()
        .filter_map(|(connection_id, job)| {
            Some(ObservedIndustryJob {
                connection_id: *connection_id,
                job_id: job.job_id,
                activity: job.activity,
                product_type_id: job.product_type_id,
                runs: u64::try_from(job.runs).ok()?,
                start_date: job.start_date?,
            })
        })
        .collect();
    let mut by_id: HashMap<i64, IndustryJobDto> =
        jobs.into_iter().map(|(_, job)| (job.job_id, job)).collect();
    match_ticket_industry_jobs(candidates, &observed)
        .into_iter()
        .map(|matched| TicketJobProgress {
            ticket_id: matched.ticket_id,
            jobs: matched
                .job_ids
                .iter()
                .filter_map(|job_id| by_id.remove(job_id))
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use iskworks_core::IndustryActivity;
    use uuid::Uuid;

    use super::*;

    fn job(job_id: i64, start: Option<chrono::DateTime<Utc>>) -> IndustryJobDto {
        IndustryJobDto {
            job_id,
            activity: IndustryActivity::Reaction,
            activity_id: 9,
            status: "active".to_string(),
            blueprint_type_id: 46_166,
            blueprint_name: None,
            product_type_id: Some(16_670),
            product_name: None,
            runs: 5,
            facility_id: 1,
            facility_name: Some("Tatara".to_string()),
            solar_system_name: None,
            start_date: start,
            end_date: start.map(|start| start + Duration::hours(10)),
            pause_date: None,
        }
    }

    #[test]
    fn returns_matched_jobs_in_full_and_skips_jobs_without_a_start_date() {
        let character = ConnectedCharacterId(Uuid::new_v4());
        let created = Utc::now() - Duration::days(1);
        let candidate = TicketJobCandidate {
            ticket_id: TicketId::new(),
            assignee: character,
            activity: IndustryActivity::Reaction,
            product_type_id: 16_670,
            runs: Some(10),
            created_at: created,
        };
        let progress = assemble_job_progress(
            std::slice::from_ref(&candidate),
            vec![
                (character, job(2, Some(created + Duration::hours(2)))),
                (character, job(1, Some(created + Duration::hours(1)))),
                (character, job(3, None)),
            ],
        );

        assert_eq!(progress.len(), 1);
        assert_eq!(progress[0].ticket_id, candidate.ticket_id);
        let ids: Vec<i64> = progress[0].jobs.iter().map(|job| job.job_id).collect();
        assert_eq!(ids, vec![1, 2]);
        assert_eq!(progress[0].jobs[0].facility_name.as_deref(), Some("Tatara"));
    }
}
