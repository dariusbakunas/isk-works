use super::*;
use chrono::Utc;
use iskworks_core::order::{OrderId, OrderRequirementId};
use iskworks_core::{FulfillmentScope, Money, OwnerId, PriceSnapshotId, WorkspaceId};
use uuid::Uuid;

fn order() -> Order {
    let now = Utc::now();
    Order {
        id: OrderId::new(),
        workspace_id: WorkspaceId(Uuid::new_v4()),
        owner_id: OwnerId(Uuid::new_v4()),
        source_build_id: Some(BuildId::new()),
        source_build_revision: 1,
        display_name: "Manufacture Ishtar".to_string(),
        runs: 1,
        recipe_fingerprint: "fp".to_string(),
        price_snapshot_id: PriceSnapshotId(Uuid::new_v4()),
        estimated_material_cost: Money::zero(),
        expected_revenue: None,
        estimated_margin: None,
        missing_price_count: 0,
        created_at: now,
        updated_at: now,
        started_at: None,
        completed_at: None,
        canceled_at: None,
        archived_at: None,
        planning_snapshot_version: 1,
    }
}

fn requirement(order_id: OrderId, fresh_quantity: u64) -> OrderRequirement {
    OrderRequirement {
        id: OrderRequirementId::new(),
        order_id,
        type_id: 34,
        captured_name: "Tritanium".to_string(),
        kind: RequirementKind::Buy,
        source_build_id: None,
        required_quantity: 100,
        fulfillment_scope: if fresh_quantity == 100 {
            FulfillmentScope::Full
        } else {
            FulfillmentScope::Missing
        },
        reused_quantity: 100 - fresh_quantity,
        fresh_quantity,
        estimated_unit_cost: None,
        estimated_line_total: None,
        reused_line_total: None,
        operation_occurrence_key: None,
        child_occurrence_key: None,
        inventory_unit_basis: None,
        child_produced_quantity: None,
        child_consumed_quantity: None,
        child_surplus_quantity: None,
        child_surplus_retained_basis: None,
        child_consumed_cost: None,
        dependency_id: None,
        price_evidence: None,
    }
}

#[test]
fn a_freshly_created_orders_requirements_are_needs_action_or_inventory_satisfied() {
    let order = order();
    let requirements = vec![requirement(order.id, 50), requirement(order.id, 0)];
    let response = order_detail_response(&order, requirements, vec![Vec::new(), Vec::new()]);

    assert_eq!(response.status, OrderStatus::Blocked);
    assert_eq!(response.rollup.satisfied, 1);
    assert_eq!(response.rollup.needs_action, 1);
    assert_eq!(response.rollup.in_progress, 0);
    assert_eq!(
        response.requirements[0].state,
        RequirementFulfillmentState::NeedsAction
    );
    assert_eq!(
        response.requirements[1].state,
        RequirementFulfillmentState::InventorySatisfied
    );
}

#[test]
fn every_requirement_satisfied_reports_the_order_ready() {
    let order = order();
    let requirements = vec![requirement(order.id, 0)];
    let response = order_detail_response(&order, requirements, vec![Vec::new()]);

    assert_eq!(response.status, OrderStatus::Ready);
    assert_eq!(response.rollup.satisfied, 1);
    assert_eq!(response.rollup.total, 1);
}

#[test]
fn a_linked_in_progress_ticket_moves_a_requirement_out_of_needs_action() {
    let order = order();
    let requirements = vec![requirement(order.id, 50)];
    let fulfillments = vec![vec![LinkedTicketRef {
        id: TicketId::new(),
        display_id: "ISK-1000".to_string(),
        status: TicketStatus::InProgress,
        allocated_quantity: 50,
    }]];
    let response = order_detail_response(&order, requirements, fulfillments);

    assert_eq!(response.status, OrderStatus::Blocked);
    assert_eq!(response.requirements[0].linked_tickets.len(), 1);
    assert_eq!(
        response.requirements[0].state,
        RequirementFulfillmentState::Linked
    );
    assert_eq!(response.rollup.in_progress, 1);
    assert_eq!(response.rollup.needs_action, 0);
}
