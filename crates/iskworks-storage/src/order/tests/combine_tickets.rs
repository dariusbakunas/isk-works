use super::*;
use iskworks_core::order::CombineRefusalReason;

// ─── Combining tickets: persistence groundwork ────────────────────────────
// A combined ticket's members point at it through `merged_into_ticket_id`;
// its copies of their prerequisites carry the Epic each one serves.

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn merged_into_ticket_id_round_trips_and_clears_on_delete(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());
    let new_ticket = |name: &str| {
        mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Reaction,
            16_658,
            name,
            build_id,
            Vec::new(),
            Some(1),
        )
    };
    let (member, _) = repository
        .create_ticket(new_ticket("Hexite"))
        .await
        .unwrap();
    let (combined, _) = repository
        .create_ticket(new_ticket("Hexite"))
        .await
        .unwrap();
    assert_eq!(member.merged_into_ticket_id, None);

    sqlx::query("UPDATE tickets SET merged_into_ticket_id = $1 WHERE id = $2")
        .bind(combined.id.0)
        .bind(member.id.0)
        .execute(&pool)
        .await
        .unwrap();
    let reloaded = repository
        .get_ticket(workspace_id, member.id)
        .await
        .unwrap();
    assert_eq!(reloaded.merged_into_ticket_id, Some(combined.id));

    repository
        .delete_ticket(workspace_id, combined.id)
        .await
        .unwrap();
    let reloaded = repository
        .get_ticket(workspace_id, member.id)
        .await
        .unwrap();
    assert_eq!(reloaded.merged_into_ticket_id, None);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn prerequisite_epic_round_trips(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());
    let snapshot = empty_price_snapshot();
    let (order, _) = repository
        .create_order(NewOrder {
            order: draft_order(workspace_id, owner_id, build_id, snapshot.id),
            price_snapshot: snapshot,
            requirements: vec![requirement(34, RequirementKind::Buy, 100)],
        })
        .await
        .unwrap();

    let mut tagged = prod_prereq(34, "Tritanium");
    tagged.order_id = Some(order.id);
    let untagged = prod_prereq(35, "Pyerite");
    let (ticket, prerequisites) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Manufacturing,
            587,
            "Rifter",
            build_id,
            vec![tagged, untagged],
            Some(1),
        ))
        .await
        .unwrap();
    let created: Vec<_> = prerequisites
        .iter()
        .map(|p| (p.type_id, p.order_id))
        .collect();
    assert_eq!(created, vec![(34, Some(order.id)), (35, None)]);

    let reloaded = repository
        .list_ticket_prerequisites(ticket.id)
        .await
        .unwrap();
    let reloaded: Vec<_> = reloaded.iter().map(|p| (p.type_id, p.order_id)).collect();
    assert_eq!(reloaded, vec![(34, Some(order.id)), (35, None)]);
}

async fn claim(pool: &PgPool, order_id: OrderId, occurrence_key: &str, ticket_id: TicketId) {
    sqlx::query(
        "INSERT INTO order_operation_ticket_claims (order_id, occurrence_key, ticket_id, claimed_at) \
         VALUES ($1, $2, $3, now())",
    )
    .bind(order_id.0)
    .bind(occurrence_key)
    .bind(ticket_id.0)
    .execute(pool)
    .await
    .unwrap();
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn ticket_operations_come_from_claims_with_an_unclaimed_fallback(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());
    let mut orders = Vec::new();
    for _ in 0..2 {
        let snapshot = empty_price_snapshot();
        let (order, _) = repository
            .create_order(NewOrder {
                order: draft_order(workspace_id, owner_id, build_id, snapshot.id),
                price_snapshot: snapshot,
                requirements: vec![requirement(34, RequirementKind::Buy, 100)],
            })
            .await
            .unwrap();
        orders.push(order.id);
    }
    let mut orders_sorted = orders.clone();
    orders_sorted.sort();
    let step_ticket = |order_id: OrderId| {
        let mut ticket = mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Reaction,
            16_658,
            "Hexite",
            build_id,
            Vec::new(),
            Some(1),
        );
        ticket.order_id = Some(order_id);
        ticket.occurrence_key = Some("hexite".to_string());
        ticket
    };
    let (claimed, _) = repository
        .create_ticket(step_ticket(orders[0]))
        .await
        .unwrap();
    claim(&pool, orders[0], "hexite", claimed.id).await;
    let (unclaimed, _) = repository
        .create_ticket(step_ticket(orders[1]))
        .await
        .unwrap();
    let (combined, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Reaction,
            16_658,
            "Hexite",
            build_id,
            Vec::new(),
            Some(2),
        ))
        .await
        .unwrap();
    claim(&pool, orders[0], "hexite-2", combined.id).await;
    claim(&pool, orders[1], "hexite-2", combined.id).await;
    let (acquisition, _) = repository
        .create_ticket(new_acquisition_ticket(
            workspace_id,
            owner_id,
            34,
            "Tritanium",
            100,
            None,
            None,
        ))
        .await
        .unwrap();

    let operations = repository
        .list_ticket_operations(&[claimed.id, unclaimed.id, combined.id, acquisition.id])
        .await
        .unwrap();

    let op = |order_id: OrderId, key: &str| TicketOperation {
        order_id,
        occurrence_key: key.to_string(),
    };
    assert_eq!(operations[&claimed.id], vec![op(orders[0], "hexite")]);
    assert_eq!(operations[&unclaimed.id], vec![op(orders[1], "hexite")]);
    assert_eq!(
        operations[&combined.id],
        vec![
            op(orders_sorted[0], "hexite-2"),
            op(orders_sorted[1], "hexite-2")
        ]
    );
    assert!(!operations.contains_key(&acquisition.id));
}

/// An Epic with one 100-unit requirement of `type_id` for its `hexite`
/// operation, holding a 100-unit reservation for it. Returns the Epic and
/// the reservation id.
async fn epic_holding(
    pool: &PgPool,
    repository: &PgOrderRepository,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    build_id: BuildId,
    type_id: i64,
) -> (OrderId, Uuid) {
    let snapshot = empty_price_snapshot();
    let mut need = requirement(type_id, RequirementKind::Buy, 100);
    need.operation_occurrence_key = Some("hexite".to_string());
    let (order, _) = repository
        .create_order(NewOrder {
            order: draft_order(workspace_id, owner_id, build_id, snapshot.id),
            price_snapshot: snapshot,
            requirements: vec![need],
        })
        .await
        .unwrap();
    let requirement_id: Uuid =
        sqlx::query_scalar("SELECT id FROM order_requirements WHERE order_id = $1")
            .bind(order.id.0)
            .fetch_one(pool)
            .await
            .unwrap();
    let allocation_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO inventory_allocations \
         (id, workspace_id, owner_id, type_id, quantity, order_requirement_id, created_at, reason) \
         VALUES ($1, $2, $3, $4, 100, $5, now(), 'epic_create')",
    )
    .bind(allocation_id)
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(type_id)
    .bind(requirement_id)
    .execute(pool)
    .await
    .unwrap();
    (order.id, allocation_id)
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn recording_draws_from_every_epic_the_ticket_executes(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_sde_type(&pool, import_id, 16_658, "Hexite").await;
    seed_sde_type(&pool, import_id, 34, "Tritanium").await;
    let repository = PgOrderRepository::new(pool.clone());
    seed_inventory_balance_with_cost(&pool, workspace_id, owner_id, 34, "Tritanium", 300, "1500")
        .await;
    let mut epics = Vec::new();
    for _ in 0..3 {
        epics.push(epic_holding(&pool, &repository, workspace_id, owner_id, build_id, 34).await);
    }
    let (ticket, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Reaction,
            16_658,
            "Hexite",
            build_id,
            vec![prod_prereq(34, "Tritanium")],
            Some(2),
        ))
        .await
        .unwrap();
    claim(&pool, epics[0].0, "hexite", ticket.id).await;
    claim(&pool, epics[1].0, "hexite", ticket.id).await;

    // Free stock is 0: all 300 units are reserved. Only the two claimed
    // Epics' reservations are the ticket's own.
    repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(2, 16_658, 0, &[(34, 200)], "0"),
        )
        .await
        .unwrap();

    let consumed: Vec<(Uuid, bool)> = sqlx::query_as(
        "SELECT id, consumed_at IS NOT NULL FROM inventory_allocations \
         WHERE id = ANY($1) AND released_at IS NULL",
    )
    .bind(
        epics
            .iter()
            .map(|(_, allocation)| *allocation)
            .collect::<Vec<_>>(),
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let consumed: std::collections::HashMap<Uuid, bool> = consumed.into_iter().collect();
    assert!(consumed[&epics[0].1]);
    assert!(consumed[&epics[1].1]);
    assert!(!consumed[&epics[2].1], "an unclaimed Epic keeps its stock");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn recorded_output_is_reserved_to_every_epic_the_ticket_executes(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    seed_sde_type(&pool, import_id, 16_658, "Hexite").await;
    let repository = PgOrderRepository::new(pool.clone());
    // Two live Epics, each needing 100 Hexite from its `hexite` step.
    let mut needs = Vec::new();
    for _ in 0..2 {
        let snapshot = empty_price_snapshot();
        let mut order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
        order.planning_snapshot_version = 3;
        let mut consumer = requirement(16_658, RequirementKind::React, 100);
        consumer.operation_occurrence_key = Some("root".to_string());
        consumer.child_occurrence_key = Some("hexite".to_string());
        let (order, _) = repository
            .create_order(NewOrder {
                order,
                price_snapshot: snapshot,
                requirements: vec![consumer],
            })
            .await
            .unwrap();
        for (key, parent) in [("root", None), ("hexite", Some("root"))] {
            sqlx::query(
                "INSERT INTO order_plan_operations \
                 (id, order_id, occurrence_key, parent_occurrence_key, activity, runs, \
                  persisted_runs, product_type_id, product_name, output_per_run, \
                  produced_quantity, blueprint_or_formula_type_id, complete, evidence, created_at) \
                 VALUES ($1, $2, $3, $4, 'reaction', 1, 1, 16658, 'Hexite', 200, 200, 1, true, \
                         '{}'::jsonb, now())",
            )
            .bind(Uuid::new_v4())
            .bind(order.id.0)
            .bind(key)
            .bind(parent)
            .execute(&pool)
            .await
            .unwrap();
        }
        let requirement_id: Uuid =
            sqlx::query_scalar("SELECT id FROM order_requirements WHERE order_id = $1")
                .bind(order.id.0)
                .fetch_one(&pool)
                .await
                .unwrap();
        needs.push((order.id, requirement_id));
    }
    let (ticket, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Reaction,
            16_658,
            "Hexite",
            build_id,
            Vec::new(),
            Some(2),
        ))
        .await
        .unwrap();
    for (order_id, _) in &needs {
        claim(&pool, *order_id, "hexite", ticket.id).await;
    }

    repository
        .record_ticket_production(
            workspace_id,
            ticket.id,
            production_input(2, 16_658, 250, &[], "0"),
        )
        .await
        .unwrap();

    let reserved: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT order_requirement_id, quantity FROM inventory_allocations \
         WHERE reason = 'recorded_output' ORDER BY order_requirement_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let mut expected: Vec<(Uuid, i64)> = needs.iter().map(|(_, id)| (*id, 100)).collect();
    expected.sort();
    assert_eq!(reserved, expected, "50 overshoot stays free");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_ticket_is_listed_under_every_epic_it_executes(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
    let repository = PgOrderRepository::new(pool.clone());
    let mut orders = Vec::new();
    for _ in 0..3 {
        let snapshot = empty_price_snapshot();
        let (order, _) = repository
            .create_order(NewOrder {
                order: draft_order(workspace_id, owner_id, build_id, snapshot.id),
                price_snapshot: snapshot,
                requirements: vec![requirement(34, RequirementKind::Buy, 100)],
            })
            .await
            .unwrap();
        orders.push(order.id);
    }
    let (combined, _) = repository
        .create_ticket(mfg_ticket(
            workspace_id,
            owner_id,
            TicketKind::Reaction,
            16_658,
            "Hexite",
            build_id,
            Vec::new(),
            Some(2),
        ))
        .await
        .unwrap();
    assert!(combined.operations.is_empty());
    claim(&pool, orders[0], "hexite", combined.id).await;
    claim(&pool, orders[1], "hexite", combined.id).await;

    for order_id in &orders[..2] {
        let listed = repository
            .list_tickets_for_order(workspace_id, *order_id)
            .await
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].executes(*order_id, "hexite"));
    }
    assert!(repository
        .list_tickets_for_order(workspace_id, orders[2])
        .await
        .unwrap()
        .is_empty());
    // One row per ticket, even when it serves several of the asked Epics.
    assert_eq!(
        repository
            .list_tickets_for_orders(workspace_id, &orders)
            .await
            .unwrap()
            .len(),
        1
    );

    let reloaded = repository
        .get_ticket(workspace_id, combined.id)
        .await
        .unwrap();
    assert_eq!(reloaded.order_id, None);
    assert_eq!(
        reloaded.epics(),
        orders[..2]
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
    );

    // A ticket created for one step carries that step from the start.
    let mut step = mfg_ticket(
        workspace_id,
        owner_id,
        TicketKind::Reaction,
        16_658,
        "Hexite",
        build_id,
        Vec::new(),
        Some(1),
    );
    step.order_id = Some(orders[2]);
    step.occurrence_key = Some("hexite".to_string());
    let (step, _) = repository.create_ticket(step).await.unwrap();
    assert!(step.executes(orders[2], "hexite"));
}

// ─── Combine ──────────────────────────────────────────────────────────────

const HEXITE: i64 = 16_658;
const INPUT: i64 = 16_640;

async fn insert_operations(pool: &PgPool, order_id: OrderId, operations: &[(&str, Option<&str>)]) {
    for (key, parent) in operations {
        sqlx::query(
            "INSERT INTO order_plan_operations \
             (id, order_id, occurrence_key, parent_occurrence_key, activity, runs, \
              persisted_runs, product_type_id, product_name, output_per_run, \
              produced_quantity, blueprint_or_formula_type_id, complete, evidence, created_at) \
             VALUES ($1, $2, $3, $4, 'reaction', 2, 2, $5, 'Hexite', 200, 400, 1, true, \
                     '{\"effectiveMe\":null,\"effectiveTe\":null,\"recipeCurrency\":\"current\", \
                       \"installation\":null,\"warnings\":[]}'::jsonb, now())",
        )
        .bind(Uuid::new_v4())
        .bind(order_id.0)
        .bind(key)
        .bind(parent)
        .bind(HEXITE)
        .execute(pool)
        .await
        .unwrap();
    }
}

/// A version-3 Epic whose root consumes Hexite from step `hexite` (which,
/// when `second_step`, consumes Hexite from step `hexite-2`), each step
/// needing 200 of `INPUT`.
async fn hexite_epic(
    pool: &PgPool,
    repository: &PgOrderRepository,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    build_id: BuildId,
    second_step: bool,
) -> OrderId {
    let steps: &[(&str, &str)] = if second_step {
        &[("hexite", "root"), ("hexite-2", "hexite")]
    } else {
        &[("hexite", "root")]
    };
    let mut requirements = Vec::new();
    for (step, parent) in steps {
        let mut consumer = requirement(HEXITE, RequirementKind::React, 400);
        consumer.operation_occurrence_key = Some((*parent).to_string());
        consumer.child_occurrence_key = Some((*step).to_string());
        requirements.push(consumer);
        let mut input = requirement(INPUT, RequirementKind::Buy, 200);
        input.operation_occurrence_key = Some((*step).to_string());
        requirements.push(input);
    }
    let snapshot = empty_price_snapshot();
    let mut order = draft_order(workspace_id, owner_id, build_id, snapshot.id);
    order.planning_snapshot_version = 3;
    let (order, _) = repository
        .create_order(NewOrder {
            order,
            price_snapshot: snapshot,
            requirements,
        })
        .await
        .unwrap();
    let mut operations = vec![("root", None)];
    operations.extend(steps.iter().map(|(step, parent)| (*step, Some(*parent))));
    insert_operations(pool, order.id, &operations).await;
    order.id
}

async fn step_ticket(
    repository: &PgOrderRepository,
    workspace_id: WorkspaceId,
    order_id: OrderId,
    key: &str,
) -> Ticket {
    match repository
        .create_operation_ticket(workspace_id, order_id, key)
        .await
        .unwrap()
    {
        OperationTicketCreation::Created(ticket) => *ticket,
        OperationTicketCreation::AlreadyExists(id) => {
            repository.get_ticket(workspace_id, id).await.unwrap()
        }
    }
}

struct CombineFixture {
    pool: PgPool,
    repository: PgOrderRepository,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    build_id: BuildId,
}

impl CombineFixture {
    async fn new(pool: PgPool) -> Self {
        let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
        let build_id = fixture_build(&pool, workspace_id, owner_id, import_id).await;
        Self {
            repository: PgOrderRepository::new(pool.clone()),
            pool,
            workspace_id,
            owner_id,
            build_id,
        }
    }

    async fn epic(&self, second_step: bool) -> OrderId {
        hexite_epic(
            &self.pool,
            &self.repository,
            self.workspace_id,
            self.owner_id,
            self.build_id,
            second_step,
        )
        .await
    }

    async fn step(&self, order_id: OrderId, key: &str) -> Ticket {
        step_ticket(&self.repository, self.workspace_id, order_id, key).await
    }

    async fn combine(&self, tickets: &[&Ticket]) -> Result<Ticket, OrderError> {
        let ids: Vec<TicketId> = tickets.iter().map(|ticket| ticket.id).collect();
        self.repository
            .combine_tickets(self.workspace_id, &ids)
            .await
    }

    async fn ticket(&self, id: TicketId) -> Ticket {
        self.repository
            .get_ticket(self.workspace_id, id)
            .await
            .unwrap()
    }
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn combining_two_epics_steps_moves_their_work_onto_one_ticket(pool: PgPool) {
    let fx = CombineFixture::new(pool).await;
    let first_epic = fx.epic(false).await;
    let second_epic = fx.epic(false).await;
    let a = fx.step(first_epic, "hexite").await;
    let b = fx.step(second_epic, "hexite").await;
    let root = fx.step(first_epic, "root").await;
    // The root ticket was created after its step: it adopted `a` as child.
    assert_eq!(fx.ticket(a.id).await.parent_ticket_id, Some(root.id));
    // A consumer's prerequisite fulfilled by `a`.
    let consumer_prerequisite = fx
        .repository
        .list_ticket_prerequisites(root.id)
        .await
        .unwrap()
        .into_iter()
        .find(|prerequisite| prerequisite.type_id == HEXITE)
        .unwrap();
    fx.repository
        .link_ticket_prerequisite_to_ticket(consumer_prerequisite.id, a.id, 400)
        .await
        .unwrap();

    let combined = fx.combine(&[&a, &b]).await.unwrap();

    assert_eq!(combined.order_id, None);
    assert_eq!(combined.status, TicketStatus::Todo);
    assert_eq!(combined.quantity, Some(800));
    assert_eq!(combined.execution_snapshot.as_ref().unwrap().runs, 4);
    assert_eq!(
        combined.epics(),
        [first_epic, second_epic].into_iter().collect()
    );
    assert!(combined.executes(first_epic, "hexite"));
    assert!(combined.executes(second_epic, "hexite"));
    for member in [&a, &b] {
        let member = fx.ticket(member.id).await;
        assert_eq!(member.status, TicketStatus::Canceled);
        assert_eq!(member.merged_into_ticket_id, Some(combined.id));
    }
    // The combined ticket is now each Epic's step ticket.
    for epic in [first_epic, second_epic] {
        let listed = fx
            .repository
            .list_tickets_for_order(fx.workspace_id, epic)
            .await
            .unwrap();
        assert!(listed.iter().any(|ticket| ticket.id == combined.id));
        match fx
            .repository
            .create_operation_ticket(fx.workspace_id, epic, "hexite")
            .await
            .unwrap()
        {
            OperationTicketCreation::AlreadyExists(id) => assert_eq!(id, combined.id),
            OperationTicketCreation::Created(_) => panic!("the step already has a ticket"),
        }
    }
    // Prerequisites: one copy per member, each tagged with its Epic.
    let copies = fx
        .repository
        .list_ticket_prerequisites(combined.id)
        .await
        .unwrap();
    let mut tagged: Vec<_> = copies
        .iter()
        .map(|copy| (copy.type_id, copy.required_quantity, copy.order_id))
        .collect();
    tagged.sort();
    let mut expected = vec![
        (INPUT, 200, Some(first_epic)),
        (INPUT, 200, Some(second_epic)),
    ];
    expected.sort();
    assert_eq!(tagged, expected);
    // The consumer's link now names the combined ticket too, and children
    // follow it.
    let links = fx
        .repository
        .list_ticket_prerequisite_fulfillments(consumer_prerequisite.id)
        .await
        .unwrap();
    assert!(links
        .iter()
        .any(|link| link.fulfilling_ticket_id == combined.id && link.allocated_quantity == 400));
    assert_eq!(
        fx.ticket(root.id).await.parent_ticket_id,
        None,
        "the root is the step's parent, not its child"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn combining_adopts_the_members_children(pool: PgPool) {
    let fx = CombineFixture::new(pool).await;
    let first_epic = fx.epic(false).await;
    let second_epic = fx.epic(false).await;
    let a = fx.step(first_epic, "hexite").await;
    let b = fx.step(second_epic, "hexite").await;
    let (child, _) = fx
        .repository
        .create_ticket(mfg_ticket(
            fx.workspace_id,
            fx.owner_id,
            TicketKind::Reaction,
            INPUT,
            "Input",
            fx.build_id,
            Vec::new(),
            Some(1),
        ))
        .await
        .unwrap();
    sqlx::query("UPDATE tickets SET parent_ticket_id = $1 WHERE id = $2")
        .bind(a.id.0)
        .bind(child.id.0)
        .execute(&fx.pool)
        .await
        .unwrap();

    let combined = fx.combine(&[&a, &b]).await.unwrap();

    assert_eq!(
        fx.ticket(child.id).await.parent_ticket_id,
        Some(combined.id)
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn two_steps_of_one_epic_and_a_third_combine(pool: PgPool) {
    let fx = CombineFixture::new(pool).await;
    let first_epic = fx.epic(true).await;
    let second_epic = fx.epic(false).await;
    let a = fx.step(first_epic, "hexite").await;
    let a2 = fx.step(first_epic, "hexite-2").await;
    let b = fx.step(second_epic, "hexite").await;

    let combined = fx.combine(&[&a, &a2, &b]).await.unwrap();

    assert_eq!(combined.operations.len(), 3);
    assert_eq!(combined.quantity, Some(1_200));
    assert_eq!(combined.epics().len(), 2);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_refused_combine_changes_nothing(pool: PgPool) {
    let fx = CombineFixture::new(pool).await;
    let first_epic = fx.epic(false).await;
    let second_epic = fx.epic(false).await;
    let a = fx.step(first_epic, "hexite").await;
    let b = fx.step(second_epic, "hexite").await;
    let root = fx.step(second_epic, "root").await;
    let ticket_count = || async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM tickets")
            .fetch_one(&fx.pool)
            .await
            .unwrap()
    };
    let before = ticket_count().await;

    // The root step stays alone (the fixture's root makes Hexite too).
    match fx.combine(&[&a, &root]).await {
        Err(OrderError::TicketNotCombinable { display_id, reason }) => {
            assert_eq!(display_id, root.display_id);
            assert_eq!(reason, CombineRefusalReason::RootOperation);
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    // A started ticket.
    fx.repository
        .set_ticket_status(fx.workspace_id, b.id, TicketStatus::InProgress)
        .await
        .unwrap();
    assert!(matches!(
        fx.combine(&[&a, &b]).await,
        Err(OrderError::TicketNotCombinable {
            reason: CombineRefusalReason::NotTodo,
            ..
        })
    ));
    // An unknown ticket.
    let ghost = Ticket {
        id: TicketId::new(),
        ..a.clone()
    };
    assert!(matches!(
        fx.combine(&[&a, &ghost]).await,
        Err(OrderError::TicketNotFound)
    ));

    assert_eq!(ticket_count().await, before);
    let a = fx.ticket(a.id).await;
    assert_eq!(a.status, TicketStatus::Todo);
    assert_eq!(a.merged_into_ticket_id, None);
    assert!(a.executes(first_epic, "hexite"));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_legacy_epics_step_is_refused(pool: PgPool) {
    let fx = CombineFixture::new(pool).await;
    let current = fx.epic(false).await;
    let a = fx.step(current, "hexite").await;
    let legacy = fx.epic(false).await;
    let b = fx.step(legacy, "hexite").await;
    sqlx::query("UPDATE orders SET planning_snapshot_version = 2 WHERE id = $1")
        .bind(legacy.0)
        .execute(&fx.pool)
        .await
        .unwrap();

    assert!(matches!(
        fx.combine(&[&a, &b]).await,
        Err(OrderError::TicketNotCombinable {
            reason: CombineRefusalReason::LegacyEpic,
            ..
        })
    ));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_combined_ticket_records_into_both_epics(pool: PgPool) {
    let fx = CombineFixture::new(pool.clone()).await;
    let import_id: Uuid = sqlx::query_scalar("SELECT id FROM sde_imports WHERE active")
        .fetch_one(&pool)
        .await
        .unwrap();
    seed_sde_type(&pool, import_id, HEXITE, "Hexite").await;
    // The requirement fixture names every input "Tritanium".
    seed_sde_type(&pool, import_id, INPUT, "Tritanium").await;
    seed_inventory_balance_with_cost(
        &pool,
        fx.workspace_id,
        fx.owner_id,
        INPUT,
        "Tritanium",
        400,
        "400",
    )
    .await;
    let first_epic = fx.epic(false).await;
    let second_epic = fx.epic(false).await;
    let a = fx.step(first_epic, "hexite").await;
    let b = fx.step(second_epic, "hexite").await;
    let combined = fx.combine(&[&a, &b]).await.unwrap();

    fx.repository
        .record_ticket_production(
            fx.workspace_id,
            combined.id,
            production_input(4, HEXITE, 800, &[(INPUT, 400)], "0"),
        )
        .await
        .unwrap();

    let reserved: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT r.order_id, SUM(a.quantity)::bigint FROM inventory_allocations a \
         JOIN order_requirements r ON r.id = a.order_requirement_id \
         WHERE a.reason = 'recorded_output' GROUP BY r.order_id ORDER BY r.order_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let mut expected = vec![(first_epic.0, 400), (second_epic.0, 400)];
    expected.sort();
    assert_eq!(reserved, expected);
}

// ─── Split ────────────────────────────────────────────────────────────────

async fn claims(pool: &PgPool) -> Vec<(Uuid, String, Uuid)> {
    sqlx::query_as(
        "SELECT order_id, occurrence_key, ticket_id FROM order_operation_ticket_claims \
         ORDER BY order_id, occurrence_key",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

/// A ticket as split must restore it: everything but `updated_at`.
fn comparable(mut ticket: Ticket) -> Ticket {
    ticket.updated_at = ticket.created_at;
    ticket
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn split_restores_the_members_exactly(pool: PgPool) {
    let fx = CombineFixture::new(pool.clone()).await;
    let first_epic = fx.epic(true).await;
    let second_epic = fx.epic(false).await;
    let a = fx.step(first_epic, "hexite").await;
    let child = fx.step(first_epic, "hexite-2").await;
    let b = fx.step(second_epic, "hexite").await;
    let root = fx.step(second_epic, "root").await;
    let consumer_prerequisite = fx
        .repository
        .list_ticket_prerequisites(root.id)
        .await
        .unwrap()
        .into_iter()
        .find(|prerequisite| prerequisite.type_id == HEXITE)
        .unwrap();
    fx.repository
        .link_ticket_prerequisite_to_ticket(consumer_prerequisite.id, b.id, 400)
        .await
        .unwrap();
    let before_tickets = [
        fx.ticket(a.id).await,
        fx.ticket(b.id).await,
        fx.ticket(child.id).await,
    ];
    assert_eq!(before_tickets[2].parent_ticket_id, Some(a.id));
    let before_claims = claims(&pool).await;

    let combined = fx.combine(&[&a, &b]).await.unwrap();
    assert_eq!(
        fx.ticket(child.id).await.parent_ticket_id,
        Some(combined.id)
    );
    let restored = fx
        .repository
        .split_ticket(fx.workspace_id, combined.id)
        .await
        .unwrap();

    let mut restored_ids: Vec<TicketId> = restored.iter().map(|ticket| ticket.id).collect();
    restored_ids.sort_by_key(|id| id.0);
    let mut expected_ids = vec![a.id, b.id];
    expected_ids.sort_by_key(|id| id.0);
    assert_eq!(restored_ids, expected_ids);
    for before in before_tickets {
        assert_eq!(
            comparable(fx.ticket(before.id).await),
            comparable(before.clone()),
            "{} is back as it was",
            before.display_id
        );
    }
    assert_eq!(claims(&pool).await, before_claims);
    assert!(matches!(
        fx.repository.get_ticket(fx.workspace_id, combined.id).await,
        Err(OrderError::TicketNotFound)
    ));
    let links = fx
        .repository
        .list_ticket_prerequisite_fulfillments(consumer_prerequisite.id)
        .await
        .unwrap();
    assert_eq!(
        links
            .iter()
            .map(|link| link.fulfilling_ticket_id)
            .collect::<Vec<_>>(),
        vec![b.id]
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_merged_member_cannot_be_reopened_on_its_own(pool: PgPool) {
    let fx = CombineFixture::new(pool).await;
    let first_epic = fx.epic(false).await;
    let second_epic = fx.epic(false).await;
    let a = fx.step(first_epic, "hexite").await;
    let b = fx.step(second_epic, "hexite").await;
    let combined = fx.combine(&[&a, &b]).await.unwrap();

    match fx
        .repository
        .set_ticket_status(fx.workspace_id, a.id, TicketStatus::Todo)
        .await
    {
        Err(OrderError::TicketMerged { into_display_id }) => {
            assert_eq!(into_display_id, combined.display_id)
        }
        other => panic!("expected TicketMerged, got {other:?}"),
    }
    // Writing Canceled again is harmless.
    fx.repository
        .set_ticket_status(fx.workspace_id, a.id, TicketStatus::Canceled)
        .await
        .unwrap();
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn split_waits_until_recordings_are_reverted(pool: PgPool) {
    let fx = CombineFixture::new(pool.clone()).await;
    let import_id: Uuid = sqlx::query_scalar("SELECT id FROM sde_imports WHERE active")
        .fetch_one(&pool)
        .await
        .unwrap();
    seed_sde_type(&pool, import_id, HEXITE, "Hexite").await;
    let first_epic = fx.epic(false).await;
    let second_epic = fx.epic(false).await;
    let a = fx.step(first_epic, "hexite").await;
    let b = fx.step(second_epic, "hexite").await;
    let combined = fx.combine(&[&a, &b]).await.unwrap();
    let outcome = fx
        .repository
        .record_ticket_production(
            fx.workspace_id,
            combined.id,
            production_input(1, HEXITE, 0, &[], "0"),
        )
        .await
        .unwrap();

    assert!(matches!(
        fx.repository
            .split_ticket(fx.workspace_id, combined.id)
            .await,
        Err(OrderError::TicketNotSplittable)
    ));

    fx.repository
        .revert_ticket_inventory_recording(fx.workspace_id, combined.id, outcome.recording.id)
        .await
        .unwrap();
    assert_eq!(
        fx.repository
            .split_ticket(fx.workspace_id, combined.id)
            .await
            .unwrap()
            .len(),
        2
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_canceled_combined_ticket_still_splits(pool: PgPool) {
    let fx = CombineFixture::new(pool).await;
    let first_epic = fx.epic(false).await;
    let second_epic = fx.epic(false).await;
    let a = fx.step(first_epic, "hexite").await;
    let b = fx.step(second_epic, "hexite").await;
    let combined = fx.combine(&[&a, &b]).await.unwrap();
    fx.repository
        .set_ticket_status(fx.workspace_id, combined.id, TicketStatus::Canceled)
        .await
        .unwrap();

    let restored = fx
        .repository
        .split_ticket(fx.workspace_id, combined.id)
        .await
        .unwrap();

    assert!(restored
        .iter()
        .all(|ticket| ticket.status == TicketStatus::Todo));
    // A started combined ticket doesn't split; neither does a plain one.
    assert!(matches!(
        fx.repository.split_ticket(fx.workspace_id, a.id).await,
        Err(OrderError::TicketNotCombined)
    ));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn split_after_an_epic_is_deleted_restores_the_rest(pool: PgPool) {
    let fx = CombineFixture::new(pool.clone()).await;
    let first_epic = fx.epic(false).await;
    let second_epic = fx.epic(false).await;
    let a = fx.step(first_epic, "hexite").await;
    let b = fx.step(second_epic, "hexite").await;
    let combined = fx.combine(&[&a, &b]).await.unwrap();
    fx.repository
        .delete_order(fx.workspace_id, second_epic)
        .await
        .unwrap();
    let combined = fx.ticket(combined.id).await;
    assert_eq!(
        combined.epics(),
        [first_epic].into_iter().collect(),
        "the deleted Epic's claim is gone"
    );

    let restored = fx
        .repository
        .split_ticket(fx.workspace_id, combined.id)
        .await
        .unwrap();

    assert!(restored.iter().any(|ticket| ticket.id == a.id));
    assert!(fx.ticket(a.id).await.executes(first_epic, "hexite"));
    assert_eq!(
        claims(&pool).await,
        vec![(first_epic.0, "hexite".to_string(), a.id.0)]
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_combined_ticket_frees_its_members(pool: PgPool) {
    let fx = CombineFixture::new(pool.clone()).await;
    let first_epic = fx.epic(false).await;
    let second_epic = fx.epic(false).await;
    let a = fx.step(first_epic, "hexite").await;
    let b = fx.step(second_epic, "hexite").await;
    let combined = fx.combine(&[&a, &b]).await.unwrap();

    fx.repository
        .delete_ticket(fx.workspace_id, combined.id)
        .await
        .unwrap();

    for member in [&a, &b] {
        let member = fx.ticket(member.id).await;
        assert_eq!(member.status, TicketStatus::Canceled);
        assert_eq!(member.merged_into_ticket_id, None);
    }
    assert!(claims(&pool).await.is_empty());
    // The step can be ticketed again on demand.
    let again = fx.step(first_epic, "hexite").await;
    assert_ne!(again.id, a.id);
}
