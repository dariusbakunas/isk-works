//! Display-only matching of In Progress Manufacturing/Reaction tickets to
//! the ESI industry jobs their assignee is running, so the Board can show
//! job progress. Nothing here is persisted or fed back into any derivation:
//! a wrong match is a wrong progress bar, never wrong accounting, and the
//! ticket's workflow `status` stays user-controlled.

use std::collections::HashSet;

use super::*;
use crate::IndustryActivity;

/// The parts of an In Progress production ticket the matcher reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketJobCandidate {
    pub ticket_id: TicketId,
    pub assignee: ConnectedCharacterId,
    pub activity: IndustryActivity,
    pub product_type_id: i64,
    /// The ticket's frozen runs; `None` (a legacy ticket with no execution
    /// snapshot) claims every matching job.
    pub runs: Option<u64>,
    pub created_at: DateTime<Utc>,
}

impl TicketJobCandidate {
    /// `Some` only for an In Progress, assigned Manufacturing/Reaction
    /// ticket -- nothing else can be matched to a job.
    #[must_use]
    pub fn from_ticket(ticket: &Ticket) -> Option<Self> {
        if ticket.status != TicketStatus::InProgress {
            return None;
        }
        let activity = match ticket.kind {
            TicketKind::Manufacturing => IndustryActivity::Manufacturing,
            TicketKind::Reaction => IndustryActivity::Reaction,
            TicketKind::Acquisition | TicketKind::Generic => return None,
        };
        Some(Self {
            ticket_id: ticket.id,
            assignee: ticket.assignee_character_id?,
            activity,
            product_type_id: ticket.type_id?,
            runs: ticket
                .execution_snapshot
                .as_ref()
                .map(|snapshot| snapshot.runs),
            created_at: ticket.created_at,
        })
    }
}

/// One slot-occupying ESI job, as the matcher sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedIndustryJob {
    pub connection_id: ConnectedCharacterId,
    pub job_id: i64,
    pub activity: IndustryActivity,
    pub product_type_id: Option<i64>,
    pub runs: u64,
    pub start_date: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketJobMatch {
    pub ticket_id: TicketId,
    /// Earliest-started first.
    pub job_ids: Vec<i64>,
}

/// Assign jobs to tickets: same assignee, activity and product, started no
/// earlier than the ticket was created. Oldest ticket first, each claiming
/// earliest-started unclaimed jobs until its runs are covered, so a job
/// counts toward one ticket at most. Tickets with no match are omitted.
#[must_use]
pub fn match_ticket_industry_jobs(
    tickets: &[TicketJobCandidate],
    jobs: &[ObservedIndustryJob],
) -> Vec<TicketJobMatch> {
    let mut ordered_tickets: Vec<&TicketJobCandidate> = tickets.iter().collect();
    ordered_tickets.sort_by_key(|ticket| (ticket.created_at, ticket.ticket_id.0));
    let mut ordered_jobs: Vec<&ObservedIndustryJob> = jobs.iter().collect();
    ordered_jobs.sort_by_key(|job| (job.start_date, job.job_id));

    let mut claimed: HashSet<i64> = HashSet::new();
    let mut matches = Vec::new();
    for ticket in ordered_tickets {
        let mut job_ids = Vec::new();
        let mut covered_runs: u64 = 0;
        for job in &ordered_jobs {
            if ticket.runs.is_some_and(|runs| covered_runs >= runs) {
                break;
            }
            let fits = job.connection_id == ticket.assignee
                && job.activity == ticket.activity
                && job.product_type_id == Some(ticket.product_type_id)
                && job.start_date >= ticket.created_at
                && !claimed.contains(&job.job_id);
            if fits {
                claimed.insert(job.job_id);
                job_ids.push(job.job_id);
                covered_runs += job.runs;
            }
        }
        if !job_ids.is_empty() {
            matches.push(TicketJobMatch {
                ticket_id: ticket.ticket_id,
                job_ids,
            });
        }
    }
    matches
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};
    use uuid::Uuid;

    use super::*;

    const PLATE: i64 = 11_545;
    const CARBIDE: i64 = 16_673;

    fn at(hour: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 13, 0, 0, 0)
            .single()
            .unwrap()
            + Duration::hours(hour)
    }

    fn character(n: u128) -> ConnectedCharacterId {
        ConnectedCharacterId(Uuid::from_u128(n))
    }

    fn ticket(n: u128, created: i64, runs: Option<u64>) -> TicketJobCandidate {
        TicketJobCandidate {
            ticket_id: TicketId(Uuid::from_u128(n)),
            assignee: character(1),
            activity: IndustryActivity::Manufacturing,
            product_type_id: PLATE,
            runs,
            created_at: at(created),
        }
    }

    fn job(job_id: i64, started: i64, runs: u64) -> ObservedIndustryJob {
        ObservedIndustryJob {
            connection_id: character(1),
            job_id,
            activity: IndustryActivity::Manufacturing,
            product_type_id: Some(PLATE),
            runs,
            start_date: at(started),
        }
    }

    fn job_ids(matches: &[TicketJobMatch], ticket_n: u128) -> Vec<i64> {
        matches
            .iter()
            .find(|m| m.ticket_id == TicketId(Uuid::from_u128(ticket_n)))
            .map(|m| m.job_ids.clone())
            .unwrap_or_default()
    }

    #[test]
    fn matches_a_job_on_assignee_activity_and_product() {
        let matches = match_ticket_industry_jobs(&[ticket(1, 0, Some(10))], &[job(100, 1, 10)]);
        assert_eq!(job_ids(&matches, 1), vec![100]);
    }

    #[test]
    fn ignores_jobs_for_another_character_activity_or_product() {
        let other_character = ObservedIndustryJob {
            connection_id: character(2),
            ..job(100, 1, 10)
        };
        let reaction = ObservedIndustryJob {
            activity: IndustryActivity::Reaction,
            ..job(101, 1, 10)
        };
        let other_product = ObservedIndustryJob {
            product_type_id: Some(CARBIDE),
            ..job(102, 1, 10)
        };
        let matches = match_ticket_industry_jobs(
            &[ticket(1, 0, Some(10))],
            &[other_character, reaction, other_product],
        );
        assert!(matches.is_empty());
    }

    #[test]
    fn ignores_jobs_started_before_the_ticket_was_created() {
        let matches = match_ticket_industry_jobs(&[ticket(1, 5, Some(10))], &[job(100, 4, 10)]);
        assert!(matches.is_empty());
    }

    #[test]
    fn a_split_ticket_claims_several_jobs_until_its_runs_are_covered() {
        let matches = match_ticket_industry_jobs(
            &[ticket(1, 0, Some(20))],
            &[job(102, 3, 10), job(100, 1, 10), job(101, 2, 10)],
        );
        assert_eq!(job_ids(&matches, 1), vec![100, 101]);
    }

    #[test]
    fn the_oldest_ticket_claims_first_and_a_job_is_never_shared() {
        let matches = match_ticket_industry_jobs(
            &[ticket(2, 1, Some(10)), ticket(1, 0, Some(10))],
            &[job(100, 2, 10), job(101, 3, 10)],
        );
        assert_eq!(job_ids(&matches, 1), vec![100]);
        assert_eq!(job_ids(&matches, 2), vec![101]);
    }

    #[test]
    fn a_ticket_without_frozen_runs_claims_every_matching_job() {
        let matches =
            match_ticket_industry_jobs(&[ticket(1, 0, None)], &[job(100, 1, 5), job(101, 2, 5)]);
        assert_eq!(job_ids(&matches, 1), vec![100, 101]);
    }
}
