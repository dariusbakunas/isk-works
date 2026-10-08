use super::*;

// ─── Explicit inventory recording -- acquisition ──────────────────────────
//
// Invariant: `record_ticket_acquisition` posts exactly one Purchase +
// exactly one immutable recording row, in one transaction, and touches
// nothing organizational (status, archived_at, prerequisites, dependents,
// Build). Idempotent on `(ticket_id, idempotency_key)`.

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_acquisition_posts_one_purchase_linked_to_the_recording(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let repository = PgOrderRepository::new(pool.clone());
    let (ticket, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
        ))
        .await
        .unwrap();
    let before = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();

    let outcome = repository
        .record_ticket_acquisition(
            workspace_id,
            ticket.id,
            RecordAcquisitionInput {
                idempotency_key: Uuid::new_v4(),
                quantity: 100,
                unit_cost: Some(Money::parse("4.25").unwrap()),
                location_note: "Jita 4-4".to_string(),
                note: "first trip".to_string(),
                effective_at: crate::db_now(),
            },
        )
        .await
        .unwrap();

    assert!(outcome.created);
    assert_eq!(outcome.recording.recorded_quantity, Some(100));
    assert_eq!(outcome.recording.location_note, "Jita 4-4");
    assert_eq!(outcome.summary.state, RecordingState::Recorded);
    assert_eq!(outcome.summary.recorded_quantity, 100);
    assert_eq!(outcome.summary.remaining_quantity, 0);
    assert_eq!(outcome.summary.surplus_quantity, 0);

    assert_eq!(recording_count(&pool).await, 1);
    let events = purchase_events(&pool).await;
    assert_eq!(events.len(), 1);
    let (quantity_delta, total_cost_delta, cost_quality, recording_id, source_reference, avg) =
        &events[0];
    assert_eq!(*quantity_delta, 100);
    assert_eq!(*total_cost_delta, "425.0000".parse::<Decimal>().unwrap());
    assert_eq!(cost_quality, "known");
    assert_eq!(*recording_id, Some(outcome.recording.id.0));
    assert_eq!(source_reference, &ticket.display_id);
    assert_eq!(*avg, Some("4.2500".parse::<Decimal>().unwrap()));

    assert_eq!(
        inventory_balance(&pool, workspace_id, owner_id, 34).await,
        Some((100, "425.0000".parse::<Decimal>().unwrap(), 1))
    );

    // Nothing organizational moved.
    let after = repository
        .get_ticket(workspace_id, ticket.id)
        .await
        .unwrap();
    assert_eq!(after.status, before.status);
    assert_eq!(after.updated_at, before.updated_at);
    assert_eq!(after.archived_at, before.archived_at);
}
