use super::*;

/// The `*_for_*` reads used by list views return exactly what the per-id
/// reads do, concatenated in id order, so a list view can swap one for the
/// other without changing a response.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn batched_ticket_reads_match_the_per_ticket_reads(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    for (type_id, name) in [(34, "Tritanium"), (35, "Pyerite"), (20185, "Product")] {
        seed_sde_type(&pool, import_id, type_id, name).await;
    }
    let repository = PgOrderRepository::new(pool.clone());
    let (acquisition, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
        ))
        .await
        .unwrap();
    for quantity in [60, 40] {
        repository
            .record_ticket_acquisition(
                workspace_id,
                acquisition.id,
                acquisition_input(quantity, Some("4")),
            )
            .await
            .unwrap();
    }
    let (producer, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            20185,
            "Product",
            build_id,
            vec![prod_prereq(34, "Tritanium"), prod_prereq(35, "Pyerite")],
            None,
        ))
        .await
        .unwrap();
    repository
        .record_ticket_production(
            workspace_id,
            producer.id,
            production_input(1, 20185, 10, &[(34, 50)], "0"),
        )
        .await
        .unwrap();
    let (untouched, _) = repository
        .create_ticket(new_acquisition_ticket_no_estimate(
            workspace_id,
            owner_id,
            35,
            "Pyerite",
            5,
        ))
        .await
        .unwrap();
    let producer_prerequisites = repository
        .list_ticket_prerequisites(producer.id)
        .await
        .unwrap();
    for prerequisite in &producer_prerequisites {
        let fulfilling = if prerequisite.type_id == 34 {
            acquisition.id
        } else {
            untouched.id
        };
        repository
            .link_ticket_prerequisite_to_ticket(prerequisite.id, fulfilling, 5)
            .await
            .unwrap();
    }

    let mut ticket_ids = vec![acquisition.id, producer.id, untouched.id];
    ticket_ids.sort_by_key(|id| id.0);

    let mut prerequisites = Vec::new();
    let mut recordings = Vec::new();
    for ticket_id in &ticket_ids {
        prerequisites.extend(
            repository
                .list_ticket_prerequisites(*ticket_id)
                .await
                .unwrap(),
        );
        recordings.extend(
            repository
                .list_ticket_inventory_recordings(*ticket_id)
                .await
                .unwrap(),
        );
    }
    assert_eq!(prerequisites.len(), 2);
    assert_eq!(recordings.len(), 3);
    assert!(recordings
        .iter()
        .all(|recording| !recording.effects.is_empty()));
    assert_eq!(
        repository
            .list_ticket_prerequisites_for_tickets(&ticket_ids)
            .await
            .unwrap(),
        prerequisites
    );
    assert_eq!(
        repository
            .list_ticket_inventory_recordings_for_tickets(&ticket_ids)
            .await
            .unwrap(),
        recordings
    );

    let mut prerequisite_ids: Vec<_> = producer_prerequisites.iter().map(|p| p.id).collect();
    prerequisite_ids.sort_by_key(|id| id.0);
    let mut fulfillments = Vec::new();
    for prerequisite_id in &prerequisite_ids {
        fulfillments.extend(
            repository
                .list_ticket_prerequisite_fulfillments(*prerequisite_id)
                .await
                .unwrap(),
        );
    }
    assert_eq!(fulfillments.len(), 2);
    assert_eq!(
        repository
            .list_ticket_prerequisite_fulfillments_for_prerequisites(&prerequisite_ids)
            .await
            .unwrap(),
        fulfillments
    );

    assert!(repository
        .list_ticket_prerequisites_for_tickets(&[])
        .await
        .unwrap()
        .is_empty());
    assert!(repository
        .list_ticket_inventory_recordings_for_tickets(&[])
        .await
        .unwrap()
        .is_empty());
}
