//! Combining production tickets: several unstarted step tickets for the same
//! item, from one or more Epics, become one ticket executing all of their
//! frozen operations. Pure; storage checks eligibility under row locks,
//! inserts the combined ticket and moves the members' claims onto it.

use rust_decimal::Decimal;

use super::*;
use crate::InstallationCostBreakdown;

/// Why one ticket can't be combined.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CombineRefusalReason {
    /// Selected more than once.
    Duplicate,
    /// Not a manufacturing or reaction ticket.
    NotProduction,
    /// A different kind or item than the first selected ticket.
    DifferentItem,
    /// Not To Do (already started, complete or canceled).
    NotTodo,
    Archived,
    /// A canceled member of another combined ticket.
    AlreadyMerged,
    /// Already executes several operations -- split it first.
    AlreadyCombined,
    /// Executes no Epic operation (standalone, or its Epic is gone).
    NoOperation,
    /// Its Epic predates whole-tree plans.
    LegacyEpic,
    /// The Epic's own product -- it drives the Epic, so it stays alone.
    RootOperation,
    /// Has recorded inventory that hasn't been reverted.
    Recorded,
    /// Has linked industry jobs.
    JobsLinked,
}

impl CombineRefusalReason {
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Self::Duplicate => "it was selected more than once",
            Self::NotProduction => "only manufacturing and reaction tickets can be combined",
            Self::DifferentItem => "combined tickets must make the same item by the same activity",
            Self::NotTodo => "only To Do tickets can be combined",
            Self::Archived => "it is archived",
            Self::AlreadyMerged => "it was already merged into another ticket",
            Self::AlreadyCombined => "it is already a combined ticket -- split it first",
            Self::NoOperation => "it isn't a step of an Epic",
            Self::LegacyEpic => "its Epic was created before whole-tree plans",
            Self::RootOperation => "it is its Epic's final product",
            Self::Recorded => "it has recorded inventory",
            Self::JobsLinked => "it has linked industry jobs",
        }
    }
}

/// One selected ticket, with the facts storage reads under lock.
#[derive(Debug, Clone, Copy)]
pub struct CombineCandidate<'a> {
    pub ticket: &'a Ticket,
    /// Its Epic's `planning_snapshot_version < 3`.
    pub legacy_epic: bool,
    /// Its operation has no parent (the Epic's own product).
    pub root_operation: bool,
    /// Has a recording that isn't reverted.
    pub has_live_recording: bool,
    pub has_linked_jobs: bool,
}

/// Checks spec §2's rules, in selection order: the first offending ticket
/// is reported.
///
/// # Errors
///
/// [`OrderError::CombineNeedsTwoTickets`] for fewer than two tickets,
/// otherwise [`OrderError::TicketNotCombinable`] naming the first offender.
pub fn check_combinable(candidates: &[CombineCandidate<'_>]) -> Result<(), OrderError> {
    if candidates.len() < 2 {
        return Err(OrderError::CombineNeedsTwoTickets);
    }
    let first = candidates[0].ticket;
    let mut seen = std::collections::HashSet::new();
    for candidate in candidates {
        let ticket = candidate.ticket;
        let reason = if !seen.insert(ticket.id) {
            Some(CombineRefusalReason::Duplicate)
        } else if !matches!(
            ticket.kind,
            TicketKind::Manufacturing | TicketKind::Reaction
        ) {
            Some(CombineRefusalReason::NotProduction)
        } else if ticket.kind != first.kind || ticket.type_id != first.type_id {
            Some(CombineRefusalReason::DifferentItem)
        } else if ticket.merged_into_ticket_id.is_some() {
            Some(CombineRefusalReason::AlreadyMerged)
        } else if ticket.status != TicketStatus::Todo {
            Some(CombineRefusalReason::NotTodo)
        } else if ticket.archived_at.is_some() {
            Some(CombineRefusalReason::Archived)
        } else if ticket.operations.len() > 1 {
            Some(CombineRefusalReason::AlreadyCombined)
        } else if ticket.operations.is_empty() {
            Some(CombineRefusalReason::NoOperation)
        } else if candidate.legacy_epic {
            Some(CombineRefusalReason::LegacyEpic)
        } else if candidate.root_operation {
            Some(CombineRefusalReason::RootOperation)
        } else if candidate.has_live_recording {
            Some(CombineRefusalReason::Recorded)
        } else if candidate.has_linked_jobs {
            Some(CombineRefusalReason::JobsLinked)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(OrderError::TicketNotCombinable {
                display_id: ticket.display_id.clone(),
                reason,
            });
        }
    }
    Ok(())
}

/// One member of a combination: the ticket and its own prerequisites.
#[derive(Debug, Clone, Copy)]
pub struct CombineMember<'a> {
    pub ticket: &'a Ticket,
    pub prerequisites: &'a [TicketPrerequisite],
}

fn sum_money(values: impl IntoIterator<Item = Option<Money>>) -> Result<Option<Money>, OrderError> {
    let mut total = Money::zero();
    for value in values {
        let Some(value) = value else {
            return Ok(None);
        };
        total = total
            .checked_add(value)
            .map_err(|_| OrderError::ArithmeticOverflow)?;
    }
    Ok(Some(total))
}

fn sum_quantity(values: impl IntoIterator<Item = Option<u64>>) -> Result<Option<u64>, OrderError> {
    let mut total: u64 = 0;
    for value in values {
        let Some(value) = value else {
            return Ok(None);
        };
        total = total
            .checked_add(value)
            .ok_or(OrderError::ArithmeticOverflow)?;
    }
    Ok(Some(total))
}

/// `Some(value)` when every item is equal, `None` otherwise (or when empty).
fn shared<T: PartialEq + Clone>(values: impl IntoIterator<Item = T>) -> Option<T> {
    let mut values = values.into_iter();
    let first = values.next()?;
    values.all(|value| value == first).then_some(first)
}

/// The members' installation costs added up. Every money line scales with
/// the job, so the sums are the combined job's; the cost index, reduction
/// and formula are kept only when every member shares them.
fn combined_installation(
    breakdowns: &[&InstallationCostBreakdown],
) -> Result<InstallationCostBreakdown, OrderError> {
    let money = |pick: fn(&InstallationCostBreakdown) -> Option<Money>| {
        sum_money(breakdowns.iter().map(|breakdown| pick(breakdown)))
    };
    let shared_index = shared(
        breakdowns
            .iter()
            .map(|breakdown| breakdown.system_cost_index),
    );
    let shared_reduction = shared(
        breakdowns
            .iter()
            .map(|breakdown| breakdown.job_cost_reduction_percent),
    );
    let mut warnings: Vec<String> = breakdowns
        .iter()
        .flat_map(|breakdown| breakdown.warnings.iter().cloned())
        .collect();
    if shared_index.is_none() || shared_reduction.is_none() {
        warnings.push(
            "Combined from steps priced at different facilities: per-job rates are not shown."
                .to_string(),
        );
    }
    warnings.sort();
    warnings.dedup();
    Ok(InstallationCostBreakdown {
        complete: breakdowns.iter().all(|breakdown| breakdown.complete),
        estimated_item_value: money(|breakdown| breakdown.estimated_item_value)?,
        system_cost_index: shared_index.flatten(),
        unmodified_system_index_cost: money(|breakdown| breakdown.unmodified_system_index_cost)?,
        job_cost_reduction_percent: shared_reduction.unwrap_or(Decimal::ZERO),
        system_index_cost: money(|breakdown| breakdown.system_index_cost)?,
        facility_tax: money(|breakdown| breakdown.facility_tax)?,
        scc_surcharge: money(|breakdown| breakdown.scc_surcharge)?,
        alliance_surcharge: money(|breakdown| breakdown.alliance_surcharge)?,
        fixed_supplemental_cost: money(|breakdown| Some(breakdown.fixed_supplemental_cost))?
            .unwrap_or_else(Money::zero),
        total: money(|breakdown| breakdown.total)?,
        warnings,
        formula_version: shared(
            breakdowns
                .iter()
                .map(|breakdown| breakdown.formula_version.clone()),
        )
        .unwrap_or_default(),
    })
}

fn combined_snapshot(
    members: &[CombineMember<'_>],
) -> Result<Option<TaskExecutionSnapshot>, OrderError> {
    let Some(snapshots) = members
        .iter()
        .map(|member| member.ticket.execution_snapshot.as_ref())
        .collect::<Option<Vec<_>>>()
    else {
        return Ok(None);
    };
    let runs = sum_quantity(snapshots.iter().map(|snapshot| Some(snapshot.runs)))?
        .ok_or(OrderError::ArithmeticOverflow)?;
    let installation_cost = match snapshots
        .iter()
        .map(|snapshot| snapshot.installation_cost.as_ref())
        .collect::<Option<Vec<_>>>()
    {
        Some(breakdowns) => Some(combined_installation(&breakdowns)?),
        None => None,
    };
    Ok(Some(TaskExecutionSnapshot {
        runs,
        blueprint: shared(snapshots.iter().map(|snapshot| snapshot.blueprint.clone())).flatten(),
        facility: shared(snapshots.iter().map(|snapshot| snapshot.facility.clone())).flatten(),
        duration_seconds: shared(snapshots.iter().map(|snapshot| snapshot.duration_seconds))
            .flatten(),
        installation_cost,
        material_value: sum_money(snapshots.iter().map(|snapshot| snapshot.material_value))?,
    }))
}

/// A member's prerequisite, copied onto the combined ticket and tagged with
/// the Epic it serves.
fn prerequisite_copy(ticket: &Ticket, prerequisite: &TicketPrerequisite) -> NewTicketPrerequisite {
    NewTicketPrerequisite {
        id: TicketPrerequisiteId::new(),
        type_id: prerequisite.type_id,
        captured_name: prerequisite.captured_name.clone(),
        kind: prerequisite.kind,
        source_build_id: prerequisite.source_build_id,
        required_quantity: prerequisite.required_quantity,
        fulfillment_scope: prerequisite.fulfillment_scope,
        reused_quantity: prerequisite.reused_quantity,
        estimated_unit_cost: prerequisite.estimated_unit_cost,
        estimated_line_total: prerequisite.estimated_line_total,
        reused_line_total: prerequisite.reused_line_total,
        operation_occurrence_key: prerequisite.operation_occurrence_key.clone(),
        child_occurrence_key: prerequisite.child_occurrence_key.clone(),
        inventory_unit_basis: prerequisite.inventory_unit_basis,
        child_produced_quantity: prerequisite.child_produced_quantity,
        child_consumed_quantity: prerequisite.child_consumed_quantity,
        child_surplus_quantity: prerequisite.child_surplus_quantity,
        child_surplus_retained_basis: prerequisite.child_surplus_retained_basis,
        child_consumed_cost: prerequisite.child_consumed_cost,
        dependency_id: prerequisite.dependency_id.clone(),
        price_evidence: prerequisite.price_evidence.clone(),
        order_id: prerequisite.epic(ticket.order_id),
    }
}

/// The ticket that does the members' work together (spec D3/D4): their
/// quantities, runs and costs added up (unknown if any member's is), shared
/// blueprint/facility/duration kept, the first member's assignee and market
/// scope, and a copy of every member's prerequisites tagged with its Epic.
/// No Epic or occurrence key of its own: storage moves the members' claims
/// onto it.
///
/// # Errors
///
/// [`OrderError::CombineNeedsTwoTickets`] for an empty list;
/// [`OrderError::ArithmeticOverflow`] if a sum overflows.
pub fn combined_ticket(members: &[CombineMember<'_>]) -> Result<NewTicket, OrderError> {
    let first = members
        .first()
        .ok_or(OrderError::CombineNeedsTwoTickets)?
        .ticket;
    let tickets = || members.iter().map(|member| member.ticket);
    Ok(NewTicket {
        id: TicketId::new(),
        workspace_id: first.workspace_id,
        owner_id: first.owner_id,
        order_id: None,
        kind: first.kind,
        type_id: first.type_id,
        captured_name: first.captured_name.clone(),
        quantity: sum_quantity(tickets().map(|ticket| ticket.quantity))?,
        source_build_id: shared(tickets().map(|ticket| ticket.source_build_id)).flatten(),
        estimated_unit_cost: None,
        estimated_line_total: sum_money(tickets().map(|ticket| ticket.estimated_line_total))?,
        market_region_id: first.market_region_id,
        market_location_id: first.market_location_id,
        price_source_id: first.price_source_id,
        notes: String::new(),
        assignee_character_id: first.assignee_character_id,
        execution_snapshot: combined_snapshot(members)?,
        prerequisites: members
            .iter()
            .flat_map(|member| {
                member
                    .prerequisites
                    .iter()
                    .map(|prerequisite| prerequisite_copy(member.ticket, prerequisite))
            })
            .collect(),
        occurrence_key: None,
        parent_ticket_id: None,
        produced_quantity: sum_quantity(tickets().map(|ticket| ticket.produced_quantity))?,
        material_component_cost: sum_money(tickets().map(|ticket| ticket.material_component_cost))?,
        own_installation_cost: sum_money(tickets().map(|ticket| ticket.own_installation_cost))?,
        total_production_cost: sum_money(tickets().map(|ticket| ticket.total_production_cost))?,
        plan_evidence: None,
    })
}

#[cfg(test)]
mod tests;
