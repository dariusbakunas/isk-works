//! When a finished root ticket completes its Epic.
//!
//! A version-3 Epic completes once its root ticket is finished: Complete
//! *and* its output fully recorded, whichever happens last. Completing first
//! and recording later doesn't free the Epic's stock before the recording
//! uses it. Storage applies this rule in the same transaction as the ticket
//! write that may finish the root, so the two commit or roll back together.

use super::*;

/// A Manufacturing/Reaction ticket's recording progress: `recorded_runs`
/// (reverted recordings excluded) against the runs frozen in its execution
/// snapshot. A ticket without a snapshot (legacy, or a recreated root)
/// counts whatever it recorded as the whole.
#[must_use]
pub fn production_recording_summary(ticket: &Ticket, recorded_runs: u64) -> TicketRecordingSummary {
    let requested = ticket
        .execution_snapshot
        .as_ref()
        .map_or(recorded_runs, |snapshot| snapshot.runs);
    derive_recording_summary(requested, recorded_runs)
}

/// Whether `ticket` belongs to an Epic and could finish it: the Epic's root
/// Manufacturing/Reaction ticket, now Complete. Cheap enough to check before
/// loading the Epic and the recordings.
#[must_use]
pub fn may_complete_epic(ticket: &Ticket) -> bool {
    ticket.order_id.is_some()
        && ticket.status == TicketStatus::Complete
        && matches!(
            ticket.kind,
            TicketKind::Manufacturing | TicketKind::Reaction
        )
        && ticket
            .occurrence_key
            .as_deref()
            .is_some_and(|key| key.starts_with(ROOT_OCCURRENCE_PREFIX))
}

/// Whether `order` completes now that `ticket` is in its current state,
/// with `recorded_runs` of production recorded against it. `false` for any
/// other ticket, a pre-version-3 Epic, or an Epic already completed or
/// canceled.
#[must_use]
pub fn epic_completes_after(ticket: &Ticket, order: &Order, recorded_runs: u64) -> bool {
    may_complete_epic(ticket)
        && ticket.order_id == Some(order.id)
        && order.planning_snapshot_version >= 3
        && order.completed_at.is_none()
        && order.canceled_at.is_none()
        && production_recording_summary(ticket, recorded_runs).state == RecordingState::Recorded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuildId, Money, OwnerId, PriceSnapshotId, WorkspaceId};

    fn order() -> Order {
        Order {
            id: OrderId::new(),
            workspace_id: WorkspaceId::new(),
            owner_id: OwnerId::new(),
            source_build_id: Some(BuildId::new()),
            source_build_revision: 1,
            display_name: "Manufacture Rifter".into(),
            runs: 1,
            recipe_fingerprint: "fp".into(),
            price_snapshot_id: PriceSnapshotId(Uuid::new_v4()),
            estimated_material_cost: Money::zero(),
            expected_revenue: None,
            estimated_margin: None,
            missing_price_count: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            started_at: None,
            completed_at: None,
            canceled_at: None,
            archived_at: None,
            planning_snapshot_version: 3,
        }
    }

    fn root_ticket(order: &Order, runs: Option<u64>) -> Ticket {
        let now = Utc::now();
        Ticket {
            id: TicketId::new(),
            workspace_id: order.workspace_id,
            owner_id: order.owner_id,
            display_id: "ISK-1".into(),
            order_id: Some(order.id),
            kind: TicketKind::Manufacturing,
            type_id: Some(587),
            captured_name: "Rifter".into(),
            quantity: Some(1),
            source_build_id: None,
            notes: String::new(),
            assignee_character_id: None,
            status: TicketStatus::Complete,
            estimated_unit_cost: None,
            estimated_line_total: None,
            actual_unit_cost: None,
            actual_line_total: None,
            market_region_id: None,
            market_location_id: None,
            price_source_id: None,
            acquisition_run_id: None,
            acquired_quantity: None,
            execution_snapshot: runs.map(|runs| crate::TaskExecutionSnapshot {
                runs,
                blueprint: None,
                facility: None,
                duration_seconds: None,
                installation_cost: None,
                material_value: None,
            }),
            created_at: now,
            updated_at: now,
            completed_at: Some(now),
            archived_at: None,
            occurrence_key: Some(format!("{ROOT_OCCURRENCE_PREFIX}587")),
            parent_ticket_id: None,
            produced_quantity: None,
            material_component_cost: None,
            own_installation_cost: None,
            total_production_cost: None,
            plan_evidence: None,
            merged_into_ticket_id: None,
            operations: Vec::new(),
        }
    }

    #[test]
    fn a_complete_fully_recorded_root_completes_its_epic() {
        let order = order();
        let ticket = root_ticket(&order, Some(2));
        assert!(epic_completes_after(&ticket, &order, 2));
        assert!(epic_completes_after(&ticket, &order, 3), "over-recorded");
    }

    #[test]
    fn a_partly_recorded_or_unfinished_root_does_not() {
        let order = order();
        let ticket = root_ticket(&order, Some(2));
        assert!(!epic_completes_after(&ticket, &order, 0));
        assert!(!epic_completes_after(&ticket, &order, 1));
        for status in [
            TicketStatus::Todo,
            TicketStatus::InProgress,
            TicketStatus::Canceled,
        ] {
            let ticket = Ticket {
                status,
                ..ticket.clone()
            };
            assert!(!epic_completes_after(&ticket, &order, 2), "{status:?}");
        }
    }

    #[test]
    fn a_root_without_a_snapshot_needs_any_recording() {
        let order = order();
        let ticket = root_ticket(&order, None);
        assert!(!epic_completes_after(&ticket, &order, 0));
        assert!(epic_completes_after(&ticket, &order, 1));
    }

    #[test]
    fn only_the_root_production_ticket_of_that_epic_counts() {
        let order = order();
        let root = root_ticket(&order, Some(1));
        let step = Ticket {
            occurrence_key: Some("node:34".into()),
            ..root.clone()
        };
        assert!(!epic_completes_after(&step, &order, 1));
        let purchase = Ticket {
            kind: TicketKind::Acquisition,
            ..root.clone()
        };
        assert!(!epic_completes_after(&purchase, &order, 1));
        let reaction = Ticket {
            kind: TicketKind::Reaction,
            ..root.clone()
        };
        assert!(epic_completes_after(&reaction, &order, 1));
        let other_epic = Ticket {
            order_id: Some(OrderId::new()),
            ..root.clone()
        };
        assert!(!epic_completes_after(&other_epic, &order, 1));
        let standalone = Ticket {
            order_id: None,
            ..root
        };
        assert!(!epic_completes_after(&standalone, &order, 1));
    }

    #[test]
    fn a_closed_or_older_epic_stays_as_it_is() {
        let ticket = root_ticket(&order(), Some(1));
        let open = Order {
            id: ticket.order_id.unwrap(),
            ..order()
        };
        assert!(epic_completes_after(&ticket, &open, 1));
        for closed in [
            Order {
                completed_at: Some(Utc::now()),
                ..open.clone()
            },
            Order {
                canceled_at: Some(Utc::now()),
                ..open.clone()
            },
            Order {
                planning_snapshot_version: 2,
                ..open.clone()
            },
        ] {
            assert!(!epic_completes_after(&ticket, &closed, 1));
        }
    }
}
