use super::*;

fn ticket(kind: TicketKind, type_id: i64, order_id: OrderId, key: &str) -> Ticket {
    let now = Utc::now();
    Ticket {
        id: TicketId::new(),
        workspace_id: WorkspaceId(Uuid::nil()),
        owner_id: OwnerId(Uuid::nil()),
        display_id: format!("ISK-{}", rand_suffix()),
        order_id: Some(order_id),
        kind,
        type_id: Some(type_id),
        captured_name: "Hexite".to_string(),
        quantity: Some(400),
        source_build_id: None,
        notes: String::new(),
        assignee_character_id: None,
        status: TicketStatus::Todo,
        estimated_unit_cost: None,
        estimated_line_total: Some(money("100")),
        actual_unit_cost: None,
        actual_line_total: None,
        market_region_id: None,
        market_location_id: None,
        price_source_id: None,
        acquisition_run_id: None,
        acquired_quantity: None,
        execution_snapshot: Some(TaskExecutionSnapshot {
            runs: 2,
            blueprint: None,
            facility: None,
            duration_seconds: Some(3_600),
            installation_cost: Some(installation("10", Some("0.05"))),
            material_value: Some(money("90")),
        }),
        created_at: now,
        updated_at: now,
        completed_at: None,
        archived_at: None,
        occurrence_key: Some(key.to_string()),
        parent_ticket_id: None,
        produced_quantity: Some(400),
        material_component_cost: Some(money("90")),
        own_installation_cost: Some(money("10")),
        total_production_cost: Some(money("100")),
        plan_evidence: None,
        merged_into_ticket_id: None,
        operations: vec![TicketOperation {
            order_id,
            occurrence_key: key.to_string(),
        }],
    }
}

fn rand_suffix() -> u32 {
    Uuid::new_v4().as_u128() as u32 % 10_000
}

fn money(value: &str) -> Money {
    Money::parse(value).unwrap()
}

fn installation(total: &str, index: Option<&str>) -> InstallationCostBreakdown {
    InstallationCostBreakdown {
        complete: true,
        estimated_item_value: Some(money("200")),
        system_cost_index: index.map(|value| value.parse().unwrap()),
        unmodified_system_index_cost: Some(money("10")),
        job_cost_reduction_percent: Decimal::ZERO,
        system_index_cost: Some(money("10")),
        facility_tax: Some(money("0")),
        scc_surcharge: Some(money("0")),
        alliance_surcharge: Some(money("0")),
        fixed_supplemental_cost: Money::zero(),
        total: Some(money(total)),
        warnings: Vec::new(),
        formula_version: "v1".to_string(),
    }
}

fn prerequisite(ticket: &Ticket, type_id: i64, quantity: u64) -> TicketPrerequisite {
    TicketPrerequisite {
        id: TicketPrerequisiteId::new(),
        ticket_id: ticket.id,
        type_id,
        captured_name: "Input".to_string(),
        kind: RequirementKind::Buy,
        source_build_id: None,
        required_quantity: quantity,
        fulfillment_scope: FulfillmentScope::Full,
        reused_quantity: 0,
        fresh_quantity: quantity,
        estimated_unit_cost: None,
        estimated_line_total: None,
        reused_line_total: None,
        operation_occurrence_key: ticket.occurrence_key.clone(),
        child_occurrence_key: None,
        inventory_unit_basis: None,
        child_produced_quantity: None,
        child_consumed_quantity: None,
        child_surplus_quantity: None,
        child_surplus_retained_basis: None,
        child_consumed_cost: None,
        dependency_id: None,
        price_evidence: None,
        order_id: None,
    }
}

fn candidate(ticket: &Ticket) -> CombineCandidate<'_> {
    CombineCandidate {
        ticket,
        legacy_epic: false,
        root_operation: false,
        has_live_recording: false,
        has_linked_jobs: false,
    }
}

fn refusal(result: Result<(), OrderError>) -> (String, CombineRefusalReason) {
    match result {
        Err(OrderError::TicketNotCombinable { display_id, reason }) => (display_id, reason),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn two_unstarted_steps_of_the_same_item_combine() {
    let a = ticket(TicketKind::Reaction, 16_658, OrderId::new(), "hexite");
    let b = ticket(TicketKind::Reaction, 16_658, OrderId::new(), "hexite");
    assert!(check_combinable(&[candidate(&a), candidate(&b)]).is_ok());
}

#[test]
fn fewer_than_two_tickets_is_refused() {
    let a = ticket(TicketKind::Reaction, 16_658, OrderId::new(), "hexite");
    assert!(matches!(
        check_combinable(&[candidate(&a)]),
        Err(OrderError::CombineNeedsTwoTickets)
    ));
}

#[test]
fn each_rule_names_the_offending_ticket() {
    let a = ticket(TicketKind::Reaction, 16_658, OrderId::new(), "hexite");
    let check = |b: &Ticket, adjust: fn(&mut CombineCandidate<'_>)| {
        let mut second = candidate(b);
        adjust(&mut second);
        refusal(check_combinable(&[candidate(&a), second]))
    };
    let base = || ticket(TicketKind::Reaction, 16_658, OrderId::new(), "hexite");
    type Case = (Ticket, fn(&mut CombineCandidate<'_>), CombineRefusalReason);
    let cases: Vec<Case> = vec![
        (
            ticket(TicketKind::Manufacturing, 16_658, OrderId::new(), "x"),
            |_| {},
            CombineRefusalReason::DifferentItem,
        ),
        (
            ticket(TicketKind::Reaction, 16_659, OrderId::new(), "x"),
            |_| {},
            CombineRefusalReason::DifferentItem,
        ),
        (
            Ticket {
                status: TicketStatus::InProgress,
                ..base()
            },
            |_| {},
            CombineRefusalReason::NotTodo,
        ),
        (
            Ticket {
                archived_at: Some(Utc::now()),
                ..base()
            },
            |_| {},
            CombineRefusalReason::Archived,
        ),
        (
            Ticket {
                status: TicketStatus::Canceled,
                merged_into_ticket_id: Some(TicketId::new()),
                ..base()
            },
            |_| {},
            CombineRefusalReason::AlreadyMerged,
        ),
        (
            Ticket {
                operations: Vec::new(),
                ..base()
            },
            |_| {},
            CombineRefusalReason::NoOperation,
        ),
        (
            Ticket {
                owner_id: OwnerId(Uuid::new_v4()),
                ..base()
            },
            |_| {},
            CombineRefusalReason::DifferentOwner,
        ),
        (
            base(),
            |c| c.legacy_epic = true,
            CombineRefusalReason::LegacyEpic,
        ),
        (
            base(),
            |c| c.root_operation = true,
            CombineRefusalReason::RootOperation,
        ),
        (
            base(),
            |c| c.has_live_recording = true,
            CombineRefusalReason::Recorded,
        ),
        (
            base(),
            |c| c.has_linked_jobs = true,
            CombineRefusalReason::JobsLinked,
        ),
    ];
    for (b, adjust, expected) in cases {
        assert_eq!(check(&b, adjust), (b.display_id.clone(), expected));
    }

    let mut combined = base();
    combined.operations.push(TicketOperation {
        order_id: OrderId::new(),
        occurrence_key: "hexite".to_string(),
    });
    assert_eq!(
        check(&combined, |_| {}).1,
        CombineRefusalReason::AlreadyCombined
    );
    assert_eq!(
        refusal(check_combinable(&[candidate(&a), candidate(&a)])).1,
        CombineRefusalReason::Duplicate
    );
    let acquisition = Ticket {
        kind: TicketKind::Acquisition,
        ..base()
    };
    assert_eq!(
        refusal(check_combinable(&[candidate(&acquisition), candidate(&a)])).1,
        CombineRefusalReason::NotProduction
    );
}

#[test]
fn the_combined_ticket_adds_up_its_members() {
    let first_epic = OrderId::new();
    let second_epic = OrderId::new();
    let a = ticket(TicketKind::Reaction, 16_658, first_epic, "hexite");
    let mut b = ticket(TicketKind::Reaction, 16_658, second_epic, "hexite");
    b.quantity = Some(1_200);
    b.produced_quantity = Some(1_200);
    b.execution_snapshot.as_mut().unwrap().runs = 6;
    b.total_production_cost = Some(money("300"));
    let a_prerequisites = vec![prerequisite(&a, 16_640, 200)];
    let b_prerequisites = vec![prerequisite(&b, 16_640, 600), prerequisite(&b, 16_641, 600)];

    let combined = combined_ticket(&[
        CombineMember {
            ticket: &a,
            prerequisites: &a_prerequisites,
        },
        CombineMember {
            ticket: &b,
            prerequisites: &b_prerequisites,
        },
    ])
    .unwrap();

    assert_eq!(combined.order_id, None);
    assert_eq!(combined.occurrence_key, None);
    assert_eq!(combined.parent_ticket_id, None);
    assert_eq!(combined.kind, TicketKind::Reaction);
    assert_eq!(combined.type_id, Some(16_658));
    assert_eq!(combined.quantity, Some(1_600));
    assert_eq!(combined.produced_quantity, Some(1_600));
    assert_eq!(combined.total_production_cost, Some(money("400")));
    assert_eq!(combined.material_component_cost, Some(money("180")));
    let snapshot = combined.execution_snapshot.unwrap();
    assert_eq!(snapshot.runs, 8);
    assert_eq!(snapshot.duration_seconds, Some(3_600));
    assert_eq!(snapshot.material_value, Some(money("180")));
    let installation = snapshot.installation_cost.unwrap();
    assert_eq!(installation.total, Some(money("20")));
    assert_eq!(installation.estimated_item_value, Some(money("400")));
    assert_eq!(
        installation.system_cost_index,
        Some("0.05".parse().unwrap())
    );
    assert!(installation.warnings.is_empty());
    let copies: Vec<_> = combined
        .prerequisites
        .iter()
        .map(|p| (p.type_id, p.required_quantity, p.order_id))
        .collect();
    assert_eq!(
        copies,
        vec![
            (16_640, 200, Some(first_epic)),
            (16_640, 600, Some(second_epic)),
            (16_641, 600, Some(second_epic)),
        ]
    );
    assert!(combined.prerequisites.iter().all(|copy| a_prerequisites
        .iter()
        .chain(&b_prerequisites)
        .all(|p| p.id != copy.id)));
}

#[test]
fn mixed_members_drop_what_they_do_not_share() {
    let a = ticket(TicketKind::Reaction, 16_658, OrderId::new(), "hexite");
    let mut b = ticket(TicketKind::Reaction, 16_658, OrderId::new(), "hexite");
    {
        let snapshot = b.execution_snapshot.as_mut().unwrap();
        snapshot.duration_seconds = Some(7_200);
        snapshot.installation_cost = Some(installation("10", Some("0.07")));
    }
    b.material_component_cost = None;

    let combined = combined_ticket(&[
        CombineMember {
            ticket: &a,
            prerequisites: &[],
        },
        CombineMember {
            ticket: &b,
            prerequisites: &[],
        },
    ])
    .unwrap();

    assert_eq!(
        combined.material_component_cost, None,
        "an unknown makes the sum unknown"
    );
    let snapshot = combined.execution_snapshot.unwrap();
    assert_eq!(snapshot.duration_seconds, None);
    let installation = snapshot.installation_cost.unwrap();
    assert_eq!(installation.system_cost_index, None);
    assert_eq!(installation.total, Some(money("20")));
    assert_eq!(installation.warnings.len(), 1);
}
