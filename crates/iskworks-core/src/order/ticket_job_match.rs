//! Links In Progress Manufacturing/Reaction tickets to the ESI industry jobs
//! their assignee runs for them. A link is stored once made, so it outlives
//! the ticket leaving In Progress and the job leaving ESI on delivery; the
//! link keeps the job's last known state, including what installing it cost.
//! Links never change a ticket's workflow `status`, which stays
//! user-controlled.
//!
//! Users can also link and unlink jobs by hand: a manual link is checked by
//! [`job_fits_ticket`], and an unlink is remembered as an exclusion the
//! matcher honours.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;

use super::*;
use crate::IndustryActivity;

/// The status a linked job takes once it has left ESI after its end date.
pub const DELIVERED_JOB_STATUS: &str = "delivered";

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

/// What a manual link checks a job against: any Manufacturing/Reaction
/// ticket, whatever its status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketJobTarget {
    pub ticket_id: TicketId,
    pub assignee: Option<ConnectedCharacterId>,
    pub activity: IndustryActivity,
    pub product_type_id: i64,
}

/// Whether `job` may be linked to the ticket by hand: run by its assignee,
/// for the same activity and exactly its product. Unlike automatic matching
/// it ignores when the job started, the ticket's status and its runs -- the
/// cases a manual link exists for.
pub fn job_fits_ticket(
    target: &TicketJobTarget,
    job: &ObservedIndustryJob,
) -> Result<(), OrderError> {
    let Some(assignee) = target.assignee else {
        return Err(OrderError::TicketJobNeedsAssignee);
    };
    if job.connection_id == assignee
        && job.activity() == target.activity
        && job.product_type_id == Some(target.product_type_id)
    {
        Ok(())
    } else {
        Err(OrderError::TicketJobMismatch)
    }
}

/// A job the user could link to a ticket by hand, and the ticket it is on
/// now, if any (linking moves it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketJobChoice {
    pub job: ObservedIndustryJob,
    pub linked_ticket_id: Option<TicketId>,
}

/// The jobs ESI reports now that fit the ticket ([`job_fits_ticket`]) and
/// aren't already linked to it, earliest-started first.
#[must_use]
pub fn ticket_job_choices(
    target: &TicketJobTarget,
    observed: &[ObservedIndustryJob],
    linked: &[TicketIndustryJob],
) -> Vec<TicketJobChoice> {
    let linked_to: HashMap<i64, TicketId> = linked
        .iter()
        .map(|link| (link.job.job_id, link.ticket_id))
        .collect();
    let mut choices: Vec<TicketJobChoice> = observed
        .iter()
        .filter(|job| job_fits_ticket(target, job).is_ok())
        .filter(|job| linked_to.get(&job.job_id) != Some(&target.ticket_id))
        .map(|job| TicketJobChoice {
            job: job.clone(),
            linked_ticket_id: linked_to.get(&job.job_id).copied(),
        })
        .collect();
    choices.sort_by_key(|choice| (choice.job.start_date, choice.job.job_id));
    choices
}

/// One ESI industry job as last observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedIndustryJob {
    pub connection_id: ConnectedCharacterId,
    pub job_id: i64,
    /// ESI's raw `activity_id`; see [`IndustryActivity::from_activity_id`].
    pub activity_id: i64,
    pub blueprint_type_id: i64,
    pub product_type_id: Option<i64>,
    pub facility_id: i64,
    pub runs: u64,
    /// What installing the job cost: ESI's installation fee plus facility
    /// tax. `None` when ESI omitted it.
    pub cost: Option<Money>,
    /// ESI's status verbatim, or [`DELIVERED_JOB_STATUS`] once it has left
    /// ESI.
    pub status: String,
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub pause_date: Option<DateTime<Utc>>,
}

impl ObservedIndustryJob {
    #[must_use]
    pub fn activity(&self) -> IndustryActivity {
        IndustryActivity::from_activity_id(self.activity_id)
    }
}

/// A stored ticket -> job link, with the job's last known state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketIndustryJob {
    pub ticket_id: TicketId,
    pub job: ObservedIndustryJob,
    pub last_seen_at: DateTime<Utc>,
}

/// What one sync writes: links to insert or refresh, and links to drop.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TicketJobSyncPlan {
    /// New links, and existing links with their job's refreshed state. An
    /// existing link keeps its ticket.
    pub upserts: Vec<TicketIndustryJob>,
    /// Jobs that left ESI before their end date -- cancelled -- so their
    /// tickets can match a replacement.
    pub removed_job_ids: Vec<i64>,
}

#[async_trait]
pub trait TicketJobRepository: Send + Sync {
    /// In Progress, assigned, unarchived Manufacturing/Reaction tickets.
    async fn ticket_job_candidates(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<TicketJobCandidate>, OrderError>;

    async fn list_ticket_jobs(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<TicketIndustryJob>, OrderError>;

    /// Applies a plan in one transaction. A job already linked keeps its
    /// ticket even if `upserts` names another (a concurrent sync linked it
    /// first).
    async fn apply_ticket_job_sync(
        &self,
        workspace_id: WorkspaceId,
        plan: &TicketJobSyncPlan,
    ) -> Result<(), OrderError>;

    /// What a manual link checks against. `TicketNotFound` for no such
    /// ticket, `TicketJobMismatch` for one that runs no job (not
    /// Manufacturing/Reaction).
    async fn ticket_job_target(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
    ) -> Result<TicketJobTarget, OrderError>;

    /// `(ticket, job)` pairs the user unlinked, which the matcher skips.
    async fn list_ticket_job_exclusions(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<HashSet<(TicketId, i64)>, OrderError>;

    /// Links `job` to the ticket by hand (checked by the caller with
    /// [`job_fits_ticket`]), moving it from any other ticket, and clears the
    /// pair's exclusion -- in one transaction.
    async fn link_ticket_job(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        job: &ObservedIndustryJob,
    ) -> Result<(), OrderError>;

    /// Unlinks a job from the ticket and remembers the pair so the matcher
    /// never links it back. `TicketJobNotFound` if the ticket doesn't have
    /// that job.
    async fn unlink_ticket_job(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        job_id: i64,
    ) -> Result<(), OrderError>;
}

/// One sync of stored links against what ESI reports now.
///
/// - A linked job still in ESI is refreshed.
/// - A linked job gone from ESI, for a character whose jobs were observed,
///   becomes [`DELIVERED_JOB_STATUS`] once past its end date, or is dropped
///   if it vanished early (cancelled).
/// - Unlinked jobs are matched to `candidates`: same assignee, activity and
///   product, started no earlier than the ticket was created, and not a
///   `(ticket, job)` pair the user unlinked (`excluded`). Oldest ticket
///   first, each claiming earliest-started jobs until its runs (counting
///   jobs already linked to it) are covered, so a job counts toward one
///   ticket at most.
#[must_use]
pub fn plan_ticket_job_sync(
    candidates: &[TicketJobCandidate],
    linked: &[TicketIndustryJob],
    excluded: &HashSet<(TicketId, i64)>,
    observed: &[ObservedIndustryJob],
    observed_connections: &HashSet<ConnectedCharacterId>,
    now: DateTime<Utc>,
) -> TicketJobSyncPlan {
    let observed_by_id: HashMap<i64, &ObservedIndustryJob> =
        observed.iter().map(|job| (job.job_id, job)).collect();
    let mut plan = TicketJobSyncPlan::default();
    let mut covered: HashMap<TicketId, u64> = HashMap::new();
    let mut claimed: HashSet<i64> = HashSet::new();

    for link in linked {
        claimed.insert(link.job.job_id);
        if let Some(job) = observed_by_id.get(&link.job.job_id) {
            plan.upserts.push(TicketIndustryJob {
                ticket_id: link.ticket_id,
                job: (*job).clone(),
                last_seen_at: now,
            });
        } else if observed_connections.contains(&link.job.connection_id)
            && link.job.status != DELIVERED_JOB_STATUS
        {
            if link.job.end_date <= now {
                let mut delivered = link.clone();
                delivered.job.status = DELIVERED_JOB_STATUS.to_string();
                plan.upserts.push(delivered);
            } else {
                plan.removed_job_ids.push(link.job.job_id);
                continue;
            }
        }
        *covered.entry(link.ticket_id).or_default() += link.job.runs;
    }

    let mut ordered_tickets: Vec<&TicketJobCandidate> = candidates.iter().collect();
    ordered_tickets.sort_by_key(|ticket| (ticket.created_at, ticket.ticket_id.0));
    let mut ordered_jobs: Vec<&ObservedIndustryJob> = observed
        .iter()
        .filter(|job| !claimed.contains(&job.job_id))
        .collect();
    ordered_jobs.sort_by_key(|job| (job.start_date, job.job_id));

    for ticket in ordered_tickets {
        let mut covered_runs = covered.get(&ticket.ticket_id).copied().unwrap_or(0);
        for job in &ordered_jobs {
            if ticket.runs.is_some_and(|runs| covered_runs >= runs) {
                break;
            }
            let fits = job.connection_id == ticket.assignee
                && job.activity() == ticket.activity
                && job.product_type_id == Some(ticket.product_type_id)
                && job.start_date >= ticket.created_at
                && !claimed.contains(&job.job_id)
                && !excluded.contains(&(ticket.ticket_id, job.job_id));
            if fits {
                claimed.insert(job.job_id);
                covered_runs += job.runs;
                plan.upserts.push(TicketIndustryJob {
                    ticket_id: ticket.ticket_id,
                    job: (*job).clone(),
                    last_seen_at: now,
                });
            }
        }
    }
    plan
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

    fn ticket_id(n: u128) -> TicketId {
        TicketId(Uuid::from_u128(n))
    }

    fn ticket(n: u128, created: i64, runs: Option<u64>) -> TicketJobCandidate {
        TicketJobCandidate {
            ticket_id: ticket_id(n),
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
            activity_id: 1,
            blueprint_type_id: 1,
            product_type_id: Some(PLATE),
            facility_id: 1,
            runs,
            cost: Money::parse("1000").ok(),
            status: "active".to_string(),
            start_date: at(started),
            end_date: at(started + 10),
            pause_date: None,
        }
    }

    fn link(ticket_n: u128, job: ObservedIndustryJob) -> TicketIndustryJob {
        TicketIndustryJob {
            ticket_id: ticket_id(ticket_n),
            job,
            last_seen_at: at(0),
        }
    }

    fn observed_all() -> HashSet<ConnectedCharacterId> {
        HashSet::from([character(1)])
    }

    /// New links only, as `(ticket n, job id)`.
    fn new_links(plan: &TicketJobSyncPlan, linked: &[TicketIndustryJob]) -> Vec<(TicketId, i64)> {
        plan.upserts
            .iter()
            .filter(|upsert| linked.iter().all(|old| old.job.job_id != upsert.job.job_id))
            .map(|upsert| (upsert.ticket_id, upsert.job.job_id))
            .collect()
    }

    fn sync(
        candidates: &[TicketJobCandidate],
        linked: &[TicketIndustryJob],
        observed: &[ObservedIndustryJob],
        now: i64,
    ) -> TicketJobSyncPlan {
        plan_ticket_job_sync(
            candidates,
            linked,
            &HashSet::new(),
            observed,
            &observed_all(),
            at(now),
        )
    }

    #[test]
    fn matches_a_job_on_assignee_activity_and_product() {
        let plan = sync(&[ticket(1, 0, Some(10))], &[], &[job(100, 1, 10)], 2);
        assert_eq!(new_links(&plan, &[]), vec![(ticket_id(1), 100)]);
    }

    #[test]
    fn ignores_jobs_for_another_character_activity_or_product() {
        let other_character = ObservedIndustryJob {
            connection_id: character(2),
            ..job(100, 1, 10)
        };
        let reaction = ObservedIndustryJob {
            activity_id: 9,
            ..job(101, 1, 10)
        };
        let other_product = ObservedIndustryJob {
            product_type_id: Some(CARBIDE),
            ..job(102, 1, 10)
        };
        let plan = sync(
            &[ticket(1, 0, Some(10))],
            &[],
            &[other_character, reaction, other_product],
            2,
        );
        assert!(plan.upserts.is_empty());
    }

    #[test]
    fn ignores_jobs_started_before_the_ticket_was_created() {
        let plan = sync(&[ticket(1, 5, Some(10))], &[], &[job(100, 4, 10)], 6);
        assert!(plan.upserts.is_empty());
    }

    #[test]
    fn a_split_ticket_claims_several_jobs_until_its_runs_are_covered() {
        let plan = sync(
            &[ticket(1, 0, Some(20))],
            &[],
            &[job(102, 3, 10), job(100, 1, 10), job(101, 2, 10)],
            4,
        );
        assert_eq!(
            new_links(&plan, &[]),
            vec![(ticket_id(1), 100), (ticket_id(1), 101)]
        );
    }

    #[test]
    fn the_oldest_ticket_claims_first_and_a_job_is_never_shared() {
        let plan = sync(
            &[ticket(2, 1, Some(10)), ticket(1, 0, Some(10))],
            &[],
            &[job(100, 2, 10), job(101, 3, 10)],
            4,
        );
        assert_eq!(
            new_links(&plan, &[]),
            vec![(ticket_id(1), 100), (ticket_id(2), 101)]
        );
    }

    #[test]
    fn a_ticket_without_frozen_runs_claims_every_matching_job() {
        let plan = sync(
            &[ticket(1, 0, None)],
            &[],
            &[job(100, 1, 5), job(101, 2, 5)],
            3,
        );
        assert_eq!(plan.upserts.len(), 2);
    }

    #[test]
    fn jobs_already_linked_count_toward_the_tickets_runs() {
        let linked = [link(1, job(100, 1, 10))];
        let plan = sync(
            &[ticket(1, 0, Some(10)), ticket(2, 0, Some(10))],
            &linked,
            &[job(100, 1, 10), job(101, 2, 10)],
            3,
        );
        // Ticket 1 is covered by its existing link; the new job goes to 2.
        assert_eq!(new_links(&plan, &linked), vec![(ticket_id(2), 101)]);
    }

    #[test]
    fn a_linked_job_still_in_esi_is_refreshed_and_keeps_its_ticket() {
        let linked = [link(1, job(100, 1, 10))];
        let paused = ObservedIndustryJob {
            status: "paused".to_string(),
            pause_date: Some(at(3)),
            ..job(100, 1, 10)
        };
        // The ticket is no longer a candidate (moved to Complete).
        let plan = sync(&[], &linked, std::slice::from_ref(&paused), 4);
        assert_eq!(plan.upserts.len(), 1);
        assert_eq!(plan.upserts[0].ticket_id, ticket_id(1));
        assert_eq!(plan.upserts[0].job, paused);
        assert_eq!(plan.upserts[0].last_seen_at, at(4));
    }

    #[test]
    fn a_linked_job_gone_after_its_end_is_delivered_and_keeps_its_cost() {
        let linked = [link(1, job(100, 1, 10))];
        let plan = sync(&[], &linked, &[], 12);
        assert_eq!(plan.upserts.len(), 1);
        assert_eq!(plan.upserts[0].job.status, DELIVERED_JOB_STATUS);
        assert_eq!(plan.upserts[0].job.cost, Money::parse("1000").ok());
        assert!(plan.removed_job_ids.is_empty());

        // Once delivered, a later sync leaves it alone.
        let delivered = [plan.upserts[0].clone()];
        assert_eq!(sync(&[], &delivered, &[], 20), TicketJobSyncPlan::default());
    }

    #[test]
    fn a_linked_job_gone_before_its_end_was_cancelled_and_frees_the_ticket() {
        let linked = [link(1, job(100, 1, 10))];
        let plan = sync(&[ticket(1, 0, Some(10))], &linked, &[job(101, 5, 10)], 6);
        assert_eq!(plan.removed_job_ids, vec![100]);
        assert_eq!(new_links(&plan, &linked), vec![(ticket_id(1), 101)]);
    }

    #[test]
    fn a_linked_job_of_an_unobserved_character_is_left_alone() {
        let linked = [link(1, job(100, 1, 10))];
        let plan =
            plan_ticket_job_sync(&[], &linked, &HashSet::new(), &[], &HashSet::new(), at(20));
        assert_eq!(plan, TicketJobSyncPlan::default());
    }

    #[test]
    fn a_job_unlinked_from_a_ticket_never_relinks_to_it_but_may_go_to_another() {
        let excluded = HashSet::from([(ticket_id(1), 100)]);
        let plan = plan_ticket_job_sync(
            &[ticket(1, 0, Some(10)), ticket(2, 0, Some(10))],
            &[],
            &excluded,
            &[job(100, 1, 10)],
            &observed_all(),
            at(2),
        );
        assert_eq!(new_links(&plan, &[]), vec![(ticket_id(2), 100)]);
    }

    fn target(assignee: Option<ConnectedCharacterId>) -> TicketJobTarget {
        TicketJobTarget {
            ticket_id: ticket_id(1),
            assignee,
            activity: IndustryActivity::Manufacturing,
            product_type_id: PLATE,
        }
    }

    #[test]
    fn a_job_fits_a_ticket_by_hand_on_assignee_activity_and_product_only() {
        // Started long before the ticket, and more runs than it needs: still
        // fits -- those are the cases a manual link is for.
        let early = job(100, -50, 500);
        assert!(job_fits_ticket(&target(Some(character(1))), &early).is_ok());
    }

    #[test]
    fn a_job_for_another_character_activity_or_product_does_not_fit() {
        let target = target(Some(character(1)));
        let other_character = ObservedIndustryJob {
            connection_id: character(2),
            ..job(100, 1, 10)
        };
        let reaction = ObservedIndustryJob {
            activity_id: 9,
            ..job(100, 1, 10)
        };
        let other_product = ObservedIndustryJob {
            product_type_id: Some(CARBIDE),
            ..job(100, 1, 10)
        };
        let no_product = ObservedIndustryJob {
            product_type_id: None,
            ..job(100, 1, 10)
        };
        for job in [other_character, reaction, other_product, no_product] {
            assert!(matches!(
                job_fits_ticket(&target, &job),
                Err(OrderError::TicketJobMismatch)
            ));
        }
    }

    #[test]
    fn choices_are_the_fitting_jobs_not_already_on_the_ticket_with_their_current_ticket() {
        let target = target(Some(character(1)));
        let on_this = job(100, 1, 10);
        let on_other = job(101, 2, 10);
        let free = job(102, 3, 10);
        let other_product = ObservedIndustryJob {
            product_type_id: Some(CARBIDE),
            ..job(103, 4, 10)
        };
        let linked = [link(1, on_this.clone()), link(2, on_other.clone())];
        let choices = ticket_job_choices(
            &target,
            &[free.clone(), other_product, on_this, on_other.clone()],
            &linked,
        );
        // Earliest-started first.
        assert_eq!(
            choices,
            vec![
                TicketJobChoice {
                    job: on_other,
                    linked_ticket_id: Some(ticket_id(2)),
                },
                TicketJobChoice {
                    job: free,
                    linked_ticket_id: None,
                },
            ]
        );
    }

    #[test]
    fn a_ticket_without_an_assignee_fits_no_job() {
        assert!(matches!(
            job_fits_ticket(&target(None), &job(100, 1, 10)),
            Err(OrderError::TicketJobNeedsAssignee)
        ));
    }
}
