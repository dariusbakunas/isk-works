use super::*;

fn order_row(started: bool, completed: bool, canceled: bool) -> ReservationRow {
    let now = crate::db_now();
    ReservationRow {
        id: Uuid::new_v4(),
        quantity: 400,
        created_at: now,
        order_id: Some(Uuid::new_v4()),
        order_display_name: Some("Manufacture Ishtar".to_string()),
        order_started_at: started.then_some(now),
        order_completed_at: completed.then_some(now),
        order_canceled_at: canceled.then_some(now),
        ticket_id: None,
        ticket_display_id: None,
        ticket_status: None,
    }
}

fn ticket_row(status: &str) -> ReservationRow {
    ReservationRow {
        id: Uuid::new_v4(),
        quantity: 800,
        created_at: crate::db_now(),
        order_id: None,
        order_display_name: None,
        order_started_at: None,
        order_completed_at: None,
        order_canceled_at: None,
        ticket_id: Some(Uuid::new_v4()),
        ticket_display_id: Some("ISK-1852".to_string()),
        ticket_status: Some(status.to_string()),
    }
}

#[test]
fn an_order_reservation_with_no_lifecycle_timestamps_is_not_started() {
    assert_eq!(
        order_reservation_status(None, None, None),
        OrderReservationStatus::NotStarted
    );
}

#[test]
fn started_without_completed_or_canceled_is_in_progress() {
    let now = crate::db_now();
    assert_eq!(
        order_reservation_status(Some(now), None, None),
        OrderReservationStatus::InProgress
    );
}

#[test]
fn completed_wins_over_started() {
    let now = crate::db_now();
    assert_eq!(
        order_reservation_status(Some(now), Some(now), None),
        OrderReservationStatus::Complete
    );
}

#[test]
fn canceled_wins_over_completed_and_started() {
    let now = crate::db_now();
    assert_eq!(
        order_reservation_status(Some(now), Some(now), Some(now)),
        OrderReservationStatus::Canceled
    );
}

#[test]
fn every_stored_ticket_status_string_round_trips() {
    for (text, status) in [
        ("todo", TicketStatus::Todo),
        ("in_progress", TicketStatus::InProgress),
        ("complete", TicketStatus::Complete),
        ("canceled", TicketStatus::Canceled),
    ] {
        assert_eq!(ticket_status_from_str(text).unwrap(), status);
    }
}

#[test]
fn the_retired_blocked_and_ready_status_strings_are_rejected() {
    assert!(ticket_status_from_str("blocked").is_err());
    assert!(ticket_status_from_str("ready").is_err());
}

#[test]
fn an_unknown_ticket_status_string_is_a_persistence_error() {
    assert!(ticket_status_from_str("archived").is_err());
}

#[test]
fn an_order_requirement_allocation_resolves_to_its_order() {
    let reservation = reservation_from_row(order_row(true, false, false)).unwrap();
    assert_eq!(reservation.quantity, 400);
    match reservation.source {
        InventoryReservationSource::Order {
            display_name,
            status,
            ..
        } => {
            assert_eq!(display_name, "Manufacture Ishtar");
            assert_eq!(status, OrderReservationStatus::InProgress);
        }
        InventoryReservationSource::Ticket { .. } => panic!("expected an Order reservation"),
    }
}

#[test]
fn a_ticket_prerequisite_allocation_resolves_to_its_ticket() {
    let reservation = reservation_from_row(ticket_row("todo")).unwrap();
    assert_eq!(reservation.quantity, 800);
    match reservation.source {
        InventoryReservationSource::Ticket {
            display_id, status, ..
        } => {
            assert_eq!(display_id, "ISK-1852");
            assert_eq!(status, TicketStatus::Todo);
        }
        InventoryReservationSource::Order { .. } => panic!("expected a Ticket reservation"),
    }
}

#[test]
fn a_row_resolving_to_neither_order_nor_ticket_is_a_persistence_error() {
    let mut row = order_row(false, false, false);
    row.order_id = None;
    row.order_display_name = None;
    assert!(reservation_from_row(row).is_err());
}
