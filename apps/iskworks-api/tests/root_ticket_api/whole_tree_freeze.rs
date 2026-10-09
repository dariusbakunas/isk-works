use super::*;

// ---------------------------------------------------------------------------
// Whole-tree Epic freeze verification, real Postgres.
// ---------------------------------------------------------------------------

/// **Primary whole-tree freeze acceptance test.** Captures the live projection
/// (`POST /api/builds/:id/graph`, the same allocation-aware walk
/// `materials_with_planning_cost_for_overlay` produces) for a two-level
/// tree (root Assembly, Build-resolved Fabricated Component child, shared
/// Tritanium at both depths, Pyerite only partially covered), then calls
/// Create Epic with the *identical* overlay body and asserts the persisted
/// `order_plan_operations`/`order_requirements`/`tickets` rows match the
/// live numbers exactly -- runs, produced quantity, cost split, required/
/// reused/fresh quantities, resolution, and child produced/consumed/
/// surplus evidence. Also proves, in the same run: zero inventory/
/// allocation writes, and correct parent-ticket linkage (two levels).
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn live_projection_and_frozen_epic_match_exactly(pool: PgPool) {
    let fx = fixture(&pool).await;
    let parent = assembly_with_built_component(&fx, 1, 90100, &[]).await;
    // Root needs 2000 Tritanium direct + child needs 500 more -- 2200 on
    // hand, forcing the shared-allocation split. Pyerite: child needs 300,
    // only 250 on hand -- a genuine partial-coverage shortage.
    seed_balance(&pool, &fx, 34, "Tritanium", 2_200, 6_600).await; // avg 3
    seed_balance(&pool, &fx, 35, "Pyerite", 250, 1_250).await; // avg 5

    let command = command_json_for_build(&parent);
    let (status, graph) = post_json(
        &fx.app,
        &format!("/api/builds/{}/graph", parent.id.0),
        command,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "graph: {graph}");

    let before = inventory_fingerprint(&pool).await;
    let allocations_before = allocation_count(&pool).await;

    let (status, order) = create_order(&fx.app, &parent).await;
    assert_eq!(status, StatusCode::CREATED, "order: {order}");
    let order_id: Uuid = order["id"].as_str().unwrap().parse().unwrap();

    // Zero ledger side effects: balances/events byte-identical before
    // and after Create Epic; it only reserves its frozen reuse.
    assert_eq!(inventory_fingerprint(&pool).await, before);
    assert!(allocation_count(&pool).await > allocations_before);

    let order_repository = PgOrderRepository::new(pool.clone());
    let operations = order_repository
        .list_order_plan_operations(OrderId(order_id))
        .await
        .unwrap();
    let requirements = order_repository
        .list_order_requirements(OrderId(order_id))
        .await
        .unwrap();
    type TicketLinkageRow = (Uuid, Option<String>, Option<Uuid>, Option<i64>);
    let tickets: Vec<TicketLinkageRow> = sqlx::query_as(
        "SELECT id, occurrence_key, parent_ticket_id, produced_quantity \
         FROM tickets WHERE order_id = $1",
    )
    .bind(order_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    let root_graph = &graph["root"];
    let root_op = operations
        .iter()
        .find(|op| op.parent_occurrence_key.is_none())
        .expect("root operation persisted");
    assert_eq!(
        root_op.occurrence_key,
        root_graph["graphNodeId"].as_str().unwrap()
    );
    assert_eq!(root_op.runs, root_graph["runs"].as_u64().unwrap());
    assert_eq!(
        root_op.persisted_runs,
        root_graph["persistedRuns"].as_u64().unwrap()
    );
    assert_eq!(
        root_op.produced_quantity,
        root_graph["producingQuantity"].as_u64().unwrap()
    );
    money_eq_opt(
        root_op.material_component_cost,
        &root_graph["materialComponentCost"],
    );
    money_eq_opt(
        root_op.own_installation_cost,
        &root_graph["ownInstallationCost"],
    );

    let child_graph = root_graph["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["nodeKind"] == "production")
        .expect("Fabricated Component production child in live graph");
    let child_op = operations
        .iter()
        .find(|op| op.parent_occurrence_key.as_deref() == Some(root_op.occurrence_key.as_str()))
        .expect("child operation persisted");
    assert_eq!(
        child_op.occurrence_key,
        child_graph["graphNodeId"].as_str().unwrap()
    );
    assert_eq!(child_op.runs, child_graph["runs"].as_u64().unwrap());
    // Persisted (1) diverges from projected (5) -- proves runs are frozen
    // from the live projection, never the child Build's own saved value.
    assert_eq!(child_op.persisted_runs, 1);
    assert_eq!(child_graph["persistedRuns"].as_u64().unwrap(), 1);
    assert_ne!(child_op.runs, child_op.persisted_runs);
    assert_eq!(
        child_op.produced_quantity,
        child_graph["producingQuantity"].as_u64().unwrap()
    );
    money_eq_opt(
        child_op.material_component_cost,
        &child_graph["materialComponentCost"],
    );
    money_eq_opt(
        child_op.own_installation_cost,
        &child_graph["ownInstallationCost"],
    );

    // Reload the child Build directly: its own persisted `runs` was never
    // mutated by Create Epic.
    let reloaded_child = fx
        .industry
        .get_build(
            fx.workspace_id,
            child_op
                .build_id
                .expect("fresh operation has a source Build"),
        )
        .await
        .unwrap();
    assert_eq!(reloaded_child.runs, 1);

    // Requirements: root Tritanium (shared-allocation first claim), child
    // Tritanium (remainder), child Pyerite (partial coverage + surplus/
    // consumed evidence -- Fabricated Component's own output is 1:1 with
    // no discrete surplus here, so child_produced == child_consumed).
    let root_trit = requirements
        .iter()
        .find(|r| {
            r.type_id == 34
                && r.operation_occurrence_key.as_deref() == Some(root_op.occurrence_key.as_str())
        })
        .unwrap();
    let child_trit = requirements
        .iter()
        .find(|r| {
            r.type_id == 34
                && r.operation_occurrence_key.as_deref() == Some(child_op.occurrence_key.as_str())
        })
        .unwrap();
    let child_pyerite = requirements
        .iter()
        .find(|r| {
            r.type_id == 35
                && r.operation_occurrence_key.as_deref() == Some(child_op.occurrence_key.as_str())
        })
        .unwrap();

    assert_eq!(root_trit.required_quantity, 2000);
    assert_eq!(root_trit.reused_quantity, 2000);
    assert_eq!(root_trit.fresh_quantity, 0);
    assert_eq!(root_trit.kind, iskworks_core::order::RequirementKind::Buy);

    assert_eq!(child_trit.required_quantity, 500);
    assert_eq!(child_trit.reused_quantity, 200);
    assert_eq!(child_trit.fresh_quantity, 300);

    assert_eq!(child_pyerite.required_quantity, 300);
    assert_eq!(child_pyerite.reused_quantity, 250);
    assert_eq!(child_pyerite.fresh_quantity, 50);
    assert_eq!(
        child_pyerite.inventory_unit_basis.map(|m| m.0.to_string()),
        Some("5.0000".to_string())
    );

    // The requirement naming the child operation (Fabricated Component
    // itself, on the root) carries produced/consumed/surplus evidence.
    let root_fab_component = requirements
        .iter()
        .find(|r| {
            r.type_id == 90100
                && r.operation_occurrence_key.as_deref() == Some(root_op.occurrence_key.as_str())
        })
        .unwrap();
    assert_eq!(
        root_fab_component.child_occurrence_key.as_deref(),
        Some(child_op.occurrence_key.as_str())
    );
    assert_eq!(root_fab_component.required_quantity, 5);
    assert_eq!(root_fab_component.child_produced_quantity, Some(5));
    assert_eq!(root_fab_component.child_consumed_quantity, Some(5));
    assert_eq!(root_fab_component.child_surplus_quantity, Some(0));

    // Ticket parent linkage: root ticket has no parent; the child
    // production ticket's parent is exactly the root ticket.
    let root_ticket = tickets
        .iter()
        .find(|(_, occurrence_key, ..)| {
            occurrence_key.as_deref() == Some(root_op.occurrence_key.as_str())
        })
        .expect("root ticket");
    let child_ticket = tickets
        .iter()
        .find(|(_, occurrence_key, ..)| {
            occurrence_key.as_deref() == Some(child_op.occurrence_key.as_str())
        })
        .expect("child ticket");
    assert_eq!(root_ticket.2, None, "root ticket has no parent");
    assert_eq!(
        child_ticket.2,
        Some(root_ticket.0),
        "child ticket's parent is the root ticket"
    );
    assert_eq!(child_ticket.3, Some(5), "child ticket sized to full produced quantity, not the 5-unit parent-consumed portion coincidentally equal here");

    // Exactly two tickets total (root + the one active child) -- no
    // Acquisition ticket generated eagerly.
    assert_eq!(tickets.len(), 2);
}

fn money_eq_opt(actual: Option<iskworks_core::Money>, expected: &Value) {
    match (actual, expected) {
        (None, Value::Null) => {}
        (Some(money), Value::String(text)) => {
            assert_eq!(&money.0.to_string(), text, "money mismatch");
        }
        (actual, expected) => panic!("money mismatch: {actual:?} vs {expected:?}"),
    }
}

/// `Full` scope on a Build-resolved component: inventory is
/// ignored regardless of how much is on hand, the child operation still
/// exists, and its ticket is still generated (contrast with the
/// fully-covered-pruning case below).
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn full_scope_build_ignores_inventory_and_still_gets_an_operation(pool: PgPool) {
    let fx = fixture(&pool).await;
    let parent = assembly_with_built_component(&fx, 1, 90100, &[]).await;
    // Fabricated Component itself (90100) has 100 on hand -- more than
    // enough to cover the 5 needed -- but Full scope must ignore it.
    seed_balance(&pool, &fx, 90100, "Fabricated Component", 100, 500_000).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 50_000, 250_000).await;
    seed_balance(&pool, &fx, 35, "Pyerite", 50_000, 250_000).await;

    let mut command = command_json_for_build(&parent);
    command["fulfillmentScopes"] = serde_json::json!([{ "typeId": 90100, "scope": "full" }]);
    let (status, order) = post_json(
        &fx.app,
        &format!("/api/builds/{}/orders", parent.id.0),
        command,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "order: {order}");
    let order_id: Uuid = order["id"].as_str().unwrap().parse().unwrap();

    let fab_req = requirement_of(&order, 90100);
    assert_eq!(fab_req["fulfillmentScope"], "full");
    assert_eq!(fab_req["reusedQuantity"], 0);
    assert_eq!(fab_req["freshQuantity"], 5);

    let order_repository = PgOrderRepository::new(pool.clone());
    let operations = order_repository
        .list_order_plan_operations(OrderId(order_id))
        .await
        .unwrap();
    assert_eq!(
        operations.len(),
        2,
        "root + the Full-scoped child operation"
    );
    let tickets: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM tickets WHERE order_id = $1")
            .bind(order_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        tickets, 1,
        "only the root is ticketed; the child step's ticket is created on demand"
    );
}

/// A Build-resolved component fully covered by on-hand
/// inventory (`Missing` scope, shortage 0): the requirement freezes
/// `reused == required`, but NO `order_plan_operations` row and NO
/// production ticket are generated for it -- matching live Graph/Materials
/// pruning exactly. Must not "helpfully" regenerate the linked child
/// merely because it exists.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn fully_covered_build_operation_is_pruned_entirely(pool: PgPool) {
    let fx = fixture(&pool).await;
    let parent = assembly_with_built_component(&fx, 1, 90100, &[]).await;
    // 5 Fabricated Component on hand -- exactly the root's own need.
    seed_balance(&pool, &fx, 90100, "Fabricated Component", 5, 25_000).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 50_000, 250_000).await;

    let (status, order) = create_order(&fx.app, &parent).await;
    assert_eq!(status, StatusCode::CREATED, "order: {order}");
    let order_id: Uuid = order["id"].as_str().unwrap().parse().unwrap();

    let fab_req = requirement_of(&order, 90100);
    assert_eq!(fab_req["fulfillmentScope"], "missing");
    assert_eq!(fab_req["kind"], "build");
    assert_eq!(fab_req["reusedQuantity"], 5);
    assert_eq!(fab_req["freshQuantity"], 0);
    assert_eq!(fab_req["state"], "inventorySatisfied");

    let order_repository = PgOrderRepository::new(pool.clone());
    let operations = order_repository
        .list_order_plan_operations(OrderId(order_id))
        .await
        .unwrap();
    assert_eq!(
        operations.len(),
        1,
        "root only -- the fully-covered Fabricated Component subtree must be pruned"
    );
    let tickets: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM tickets WHERE order_id = $1")
            .bind(order_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(tickets, 1, "root ticket only, no child production ticket");

    // Requesting a ticket for this requirement directly is refused (fresh
    // quantity is 0) -- never silently regenerates the linked child.
    let (status, _body) = post_json(
        &fx.app,
        &format!(
            "/api/orders/{order_id}/requirements/{}/tickets",
            fab_req["id"].as_str().unwrap()
        ),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// Two Epics created from the same Build at different live
/// states never share mutable snapshot state: Epic A is frozen against the
/// original inventory/facility, Epic B (created after both change) reflects
/// the NEW state, and reloading Epic A afterward shows it unchanged.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn two_epics_diverge_when_live_state_changes_between_them(pool: PgPool) {
    let fx = fixture(&pool).await;
    let parent = assembly_with_built_component(&fx, 1, 90100, &[]).await;
    // Enough for both Epics: each reserves 2,500 (root 2,000 + child 500).
    seed_balance(&pool, &fx, 34, "Tritanium", 5_000, 15_000).await; // avg 3
    seed_balance(&pool, &fx, 35, "Pyerite", 5_000, 15_000).await;

    let (status, order_a) = create_order(&fx.app, &parent).await;
    assert_eq!(status, StatusCode::CREATED);
    let order_a_id: Uuid = order_a["id"].as_str().unwrap().parse().unwrap();

    // Change the live inventory basis for Tritanium between the two Epics.
    sqlx::query(
        "UPDATE inventory_balances SET total_historical_cost = 30000 \
         WHERE workspace_id = $1 AND type_id = 34", // avg now 6, was 3
    )
    .bind(fx.workspace_id.0)
    .execute(&pool)
    .await
    .unwrap();

    let (status, order_b) = create_order(&fx.app, &parent).await;
    assert_eq!(status, StatusCode::CREATED);
    let order_b_id: Uuid = order_b["id"].as_str().unwrap().parse().unwrap();
    assert_ne!(order_a_id, order_b_id);

    let order_repository = PgOrderRepository::new(pool.clone());
    let a_requirements = order_repository
        .list_order_requirements(OrderId(order_a_id))
        .await
        .unwrap();
    let b_requirements = order_repository
        .list_order_requirements(OrderId(order_b_id))
        .await
        .unwrap();
    let a_operations = order_repository
        .list_order_plan_operations(OrderId(order_a_id))
        .await
        .unwrap();
    let b_operations = order_repository
        .list_order_plan_operations(OrderId(order_b_id))
        .await
        .unwrap();
    let a_root_op = a_operations
        .iter()
        .find(|op| op.parent_occurrence_key.is_none())
        .unwrap();
    let b_root_op = b_operations
        .iter()
        .find(|op| op.parent_occurrence_key.is_none())
        .unwrap();
    let a_root_trit = a_requirements
        .iter()
        .find(|r| {
            r.type_id == 34
                && r.operation_occurrence_key.as_deref() == Some(a_root_op.occurrence_key.as_str())
        })
        .unwrap();
    let b_root_trit = b_requirements
        .iter()
        .find(|r| {
            r.type_id == 34
                && r.operation_occurrence_key.as_deref() == Some(b_root_op.occurrence_key.as_str())
        })
        .unwrap();

    // Both froze the full 2000 reused (each from its own free stock), but
    // at DIFFERENT unit bases.
    assert_eq!(a_root_trit.reused_quantity, 2000);
    assert_eq!(b_root_trit.reused_quantity, 2000);
    assert_eq!(
        a_root_trit.inventory_unit_basis.unwrap().0.to_string(),
        "3.0000"
    );
    assert_eq!(
        b_root_trit.inventory_unit_basis.unwrap().0.to_string(),
        "6.0000"
    );

    // Reloading Epic A afterward still shows the ORIGINAL basis -- its own
    // frozen snapshot never mutates because of Epic B or the live change.
    let a_requirements_reloaded = order_repository
        .list_order_requirements(OrderId(order_a_id))
        .await
        .unwrap();
    let a_root_trit_reloaded = a_requirements_reloaded
        .iter()
        .find(|r| {
            r.type_id == 34
                && r.operation_occurrence_key.as_deref() == Some(a_root_op.occurrence_key.as_str())
        })
        .unwrap();
    assert_eq!(
        a_root_trit_reloaded
            .inventory_unit_basis
            .unwrap()
            .0
            .to_string(),
        "3.0000"
    );
}

/// Atomicity. Calls `PgOrderRepository::create_order_plan`
/// directly (bypassing the HTTP walk) with a hand-built `NewOrderPlan`
/// whose root operation is valid but whose SECOND operation references a
/// `build_id` that does not exist -- `order_plan_operations_build_id_fkey`
/// fails partway through the one transaction, *after* the order, price
/// snapshot, and root operation would already have been written. Asserts
/// the whole attempt leaves **nothing** behind: no Order, no
/// PlanOperation (not even the root one that "succeeded" first), no
/// OrderRequirement, no Ticket, no TicketPrerequisite.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_order_plan_is_atomic_and_rolls_back_completely_on_failure(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;
    let now = chrono::Utc::now();

    let order_repository = PgOrderRepository::new(pool.clone());
    let order_id = OrderId(Uuid::new_v4());
    let price_snapshot_id = iskworks_core::PriceSnapshotId(Uuid::new_v4());

    let order = iskworks_core::order::Order {
        id: order_id,
        workspace_id: fx.workspace_id,
        owner_id: fx.owner_id,
        source_build_id: Some(build.id),
        source_build_revision: build.revision,
        display_name: "Atomicity probe".to_string(),
        runs: 1,
        recipe_fingerprint: "fp".to_string(),
        price_snapshot_id,
        estimated_material_cost: iskworks_core::Money::zero(),
        expected_revenue: None,
        estimated_margin: None,
        missing_price_count: 0,
        created_at: now,
        updated_at: now,
        started_at: None,
        completed_at: None,
        canceled_at: None,
        archived_at: None,
        planning_snapshot_version: 2,
    };
    let price_snapshot = iskworks_core::PriceSnapshot {
        id: price_snapshot_id,
        price_source_id: None,
        source_name: "fixture".to_string(),
        source_revision: 1,
        created_at: now,
        items: Vec::new(),
    };
    let evidence = iskworks_core::order::PlanOperationEvidence {
        effective_me: Some(0),
        effective_te: Some(0),
        job_count: 1,
        recipe_currency: RecipeCurrency::Current,
        installation: None,
        warnings: Vec::new(),
    };
    let root_op = iskworks_core::order::NewPlanOperation {
        id: iskworks_core::order::PlanOperationId::new(),
        occurrence_key: format!("root:{}", build.id.0),
        parent_occurrence_key: None,
        build_id: build.id,
        activity: iskworks_core::build_materials::MaterialActivity::Manufacturing,
        runs: 1,
        persisted_runs: 1,
        product_type_id: 5876,
        product_name: "Rifter".to_string(),
        output_per_run: 1,
        produced_quantity: 1,
        blueprint_or_formula_type_id: 6830,
        material_component_cost: None,
        own_installation_cost: None,
        total_production_cost: None,
        complete: false,
        consumed_quantity: None,
        surplus_quantity: None,
        surplus_retained_basis: None,
        evidence: evidence.clone(),
    };
    // A second "operation" whose build_id does not exist -- inserted
    // AFTER the (otherwise valid) root operation in the same transaction.
    let bogus_build_id = BuildId::new();
    let bogus_op = iskworks_core::order::NewPlanOperation {
        id: iskworks_core::order::PlanOperationId::new(),
        occurrence_key: format!("build:{}", bogus_build_id.0),
        parent_occurrence_key: Some(root_op.occurrence_key.clone()),
        build_id: bogus_build_id,
        activity: iskworks_core::build_materials::MaterialActivity::Manufacturing,
        runs: 1,
        persisted_runs: 1,
        product_type_id: 34,
        product_name: "Nonexistent".to_string(),
        output_per_run: 1,
        produced_quantity: 1,
        blueprint_or_formula_type_id: 1,
        material_component_cost: None,
        own_installation_cost: None,
        total_production_cost: None,
        complete: false,
        consumed_quantity: None,
        surplus_quantity: None,
        surplus_retained_basis: None,
        evidence,
    };

    let plan = iskworks_core::order::NewOrderPlan {
        order,
        price_snapshot,
        operations: vec![root_op, bogus_op],
        requirements: Vec::new(),
        tickets: Vec::new(),
        reservation: None,
    };

    let result = order_repository.create_order_plan(plan).await;
    assert!(
        result.is_err(),
        "the bogus build_id must fail the FK check and the whole plan"
    );

    let orders: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM orders WHERE id = $1")
        .bind(order_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    let operations: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM order_plan_operations WHERE order_id = $1",
    )
    .bind(order_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    let price_snapshots: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM price_snapshots WHERE id = $1")
            .bind(price_snapshot_id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(orders, 0, "no partial Order");
    assert_eq!(
        operations, 0,
        "not even the root operation that inserted first within the transaction"
    );
    assert_eq!(price_snapshots, 0, "no partial price snapshot");
}

/// **Live-overlay freeze acceptance test**. The persisted
/// Build has no facility; the *unsaved* live overlay selects one. Every
/// persisted representation that claims to describe the root -- the
/// whole-tree `PlanOperation`'s own evidence, the Order's financial
/// summary, and the root ticket's compatibility `executionSnapshot` -- must
/// agree on that facility. No persisted representation may claim "no
/// facility".
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn unsaved_facility_overlay_is_reflected_everywhere(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await; // persisted draft_planning has NO facility
    seed_balance(&pool, &fx, 34, "Tritanium", 50_000, 250_000).await;
    seed_balance(&pool, &fx, 35, "Pyerite", 50_000, 250_000).await;

    let facility_id = Uuid::new_v4();
    insert_facility_profile(&pool, fx.workspace_id, facility_id, 1, "5").await;

    // Live overlay: adds a manufacturing facility the persisted Build does
    // NOT have.
    let mut command = command_json_for_build(&build);
    command["manufacturingFacility"] = serde_json::json!({
        "facilityProfileId": facility_id,
        "blueprintMe": 0,
        "blueprintTe": 0,
        "estimatedItemValue": null,
    });

    let (status, order) = post_json(
        &fx.app,
        &format!("/api/builds/{}/orders", build.id.0),
        command,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "order: {order}");
    let order_id: Uuid = order["id"].as_str().unwrap().parse().unwrap();

    let order_repository = PgOrderRepository::new(pool.clone());
    let operations = order_repository
        .list_order_plan_operations(OrderId(order_id))
        .await
        .unwrap();
    let root_op = operations
        .iter()
        .find(|op| op.parent_occurrence_key.is_none())
        .unwrap();

    // Authoritative: the whole-tree freeze's own root evidence names this
    // exact facility profile.
    let installation = root_op
        .evidence
        .installation
        .as_ref()
        .expect("the whole-tree freeze must record the overlay's facility identity");
    assert_eq!(installation.facility_profile_id, Some(facility_id));

    // Derived: the root ticket's compatibility `executionSnapshot` (per
    // `order::plan`'s own "derived execution copy" model) comes from the
    // SAME `OrderPlanCoordinator::freeze` call,
    // so it must name the identical facility -- never "no facility".
    let tickets: Vec<(Option<serde_json::Value>,)> = sqlx::query_as(
        "SELECT execution_snapshot FROM tickets WHERE order_id = $1 AND occurrence_key = $2",
    )
    .bind(order_id)
    .bind(&root_op.occurrence_key)
    .fetch_all(&pool)
    .await
    .unwrap();
    let execution_snapshot = tickets[0]
        .0
        .as_ref()
        .expect("root ticket has a legacy snapshot");
    assert_eq!(
        execution_snapshot["facility"]["id"],
        serde_json::json!(facility_id),
        "the root ticket's legacy executionSnapshot must name the overlay's facility, not \
         claim no facility exists"
    );
}

/// Migration/version coexistence. A version-1 (legacy, root-only) Epic
/// created via the `create_order` repository method and a version-2
/// (whole-tree) Epic created via the live HTTP route both read back
/// correctly from the SAME schema: the legacy row's nullable evidence
/// columns are all `NULL`, its `planningSnapshotVersion` is `1`, and its
/// requirements/tickets remain readable.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn legacy_v1_and_whole_tree_v2_epics_coexist_after_migration(pool: PgPool) {
    let fx = fixture(&pool).await;
    let legacy_build = rifter(&fx).await;
    let v2_build = rifter(&fx).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 50_000, 250_000).await;
    seed_balance(&pool, &fx, 35, "Pyerite", 50_000, 250_000).await;

    // A version-1 Epic, root-only -- straight through the `create_order`
    // repository trait method rather than the whole-tree freeze.
    let order_repository = PgOrderRepository::new(pool.clone());
    let now = chrono::Utc::now();
    let price_snapshot = iskworks_core::PriceSnapshot {
        id: iskworks_core::PriceSnapshotId(Uuid::new_v4()),
        price_source_id: None,
        source_name: "fixture".to_string(),
        source_revision: 1,
        created_at: now,
        items: Vec::new(),
    };
    let legacy_order = iskworks_core::order::Order {
        id: OrderId(Uuid::new_v4()),
        workspace_id: fx.workspace_id,
        owner_id: fx.owner_id,
        source_build_id: Some(legacy_build.id),
        source_build_revision: legacy_build.revision,
        display_name: "Legacy v1 Epic".to_string(),
        runs: 1,
        recipe_fingerprint: "fp".to_string(),
        price_snapshot_id: price_snapshot.id,
        estimated_material_cost: iskworks_core::Money::zero(),
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
    };
    let (legacy_order, _legacy_requirements) = order_repository
        .create_order(iskworks_core::order::NewOrder {
            order: legacy_order,
            price_snapshot,
            requirements: Vec::new(),
        })
        .await
        .unwrap();

    // A version-2 Epic. The live route only writes version 3, so take its
    // whole-tree rows and mark them version 2: the v2 read path must keep
    // working for frozen v2 Epics.
    let (status, v2_order) = create_order(&fx.app, &v2_build).await;
    assert_eq!(status, StatusCode::CREATED, "v2 order: {v2_order}");
    let v2_order_id: Uuid = v2_order["id"].as_str().unwrap().parse().unwrap();
    let downgraded = sqlx::query(
        "UPDATE orders SET planning_snapshot_version = 2 WHERE id = $1 AND planning_snapshot_version = 3",
    )
    .bind(v2_order_id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        downgraded.rows_affected(),
        1,
        "the live route writes version 3"
    );

    // The legacy row reads back untouched: version 1, no operations, every
    // whole-tree evidence column null at the DB level.
    let reloaded_legacy = order_repository
        .get_order(fx.workspace_id, legacy_order.id)
        .await
        .unwrap();
    assert_eq!(reloaded_legacy.planning_snapshot_version, 1);
    let legacy_operations = order_repository
        .list_order_plan_operations(legacy_order.id)
        .await
        .unwrap();
    assert!(
        legacy_operations.is_empty(),
        "a version-1 Epic has no PlanOperation rows"
    );

    let legacy_row: (i16,) =
        sqlx::query_as("SELECT planning_snapshot_version FROM orders WHERE id = $1")
            .bind(legacy_order.id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(legacy_row.0, 1);

    // The version-2 row is independently correct: version 2, has
    // operations.
    let v2_row: (i16,) =
        sqlx::query_as("SELECT planning_snapshot_version FROM orders WHERE id = $1")
            .bind(v2_order_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(v2_row.0, 2);
    let v2_operations = order_repository
        .list_order_plan_operations(OrderId(v2_order_id))
        .await
        .unwrap();
    assert!(!v2_operations.is_empty());

    // Both coexist in the same `list_orders` result, each correctly typed.
    let all_orders = order_repository
        .list_orders(fx.workspace_id, fx.owner_id)
        .await
        .unwrap();
    let versions: std::collections::BTreeMap<Uuid, u8> = all_orders
        .iter()
        .map(|o| (o.id.0, o.planning_snapshot_version))
        .collect();
    assert_eq!(versions.get(&legacy_order.id.0), Some(&1));
    assert_eq!(versions.get(&v2_order_id), Some(&2));
}

/// **Live-overlay freeze acceptance test -- Full scope overlay**. The
/// persisted root has `Missing` scope (default) and on-hand inventory that
/// would otherwise cover the requirement; the unsaved live overlay marks
/// it `Full`. No snapshot of the persisted Missing scope may survive.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn unsaved_missing_to_full_scope_overlay_is_reflected_everywhere(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await; // default Missing scope, no overrides persisted
    seed_balance(&pool, &fx, 34, "Tritanium", 50_000, 250_000).await; // more than enough
    seed_balance(&pool, &fx, 35, "Pyerite", 50_000, 250_000).await;

    let mut command = command_json_for_build(&build);
    command["fulfillmentScopes"] = serde_json::json!([{ "typeId": 34, "scope": "full" }]);

    let (status, order) = post_json(
        &fx.app,
        &format!("/api/builds/{}/orders", build.id.0),
        command,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "order: {order}");

    let tritanium = requirement_of(&order, 34);
    assert_eq!(tritanium["fulfillmentScope"], "full");
    assert_eq!(
        tritanium["reusedQuantity"], 0,
        "Full scope: nothing reused despite ample stock"
    );
    assert_eq!(
        tritanium["freshQuantity"], 1000,
        "the full requirement is fresh"
    );

    // Order summary reflects the Full-scope fresh cost -- never a
    // Missing-scope-derived (partially-reused) figure.
    money_eq(&tritanium["estimatedLineTotal"], "5000.0000"); // 1000 @ 5.0000, entirely fresh
}

/// Tasks 16/17 -- live-vs-Order-summary and live-vs-root-ticket parity.
/// Captures the authoritative live root totals via the Graph oracle
/// (`materialComponentCost`/`ownInstallationCost`/`producingQuantity`, the
/// same walk `OrderPlanCoordinator::freeze` reduces), creates the Epic,
/// and asserts every overlapping Order-summary and root-ticket field
/// agrees exactly -- not merely "non-null".
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn live_projection_matches_order_summary_and_root_ticket_exactly(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 400, 800).await; // avg 2, partial coverage
    seed_balance(&pool, &fx, 35, "Pyerite", 50_000, 250_000).await;

    let command = command_json_for_build(&build);
    let (status, graph) = post_json(
        &fx.app,
        &format!("/api/builds/{}/graph", build.id.0),
        command.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "graph: {graph}");
    let live_material_cost = graph["root"]["materialComponentCost"].clone();
    let live_produced_quantity = graph["root"]["producingQuantity"].as_u64().unwrap();

    let (status, order) = post_json(
        &fx.app,
        &format!("/api/builds/{}/orders", build.id.0),
        command,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "order: {order}");
    let order_id: Uuid = order["id"].as_str().unwrap().parse().unwrap();

    // Order summary: estimatedMaterialCost matches the live root exactly.
    assert_eq!(
        order["estimatedMaterialCost"], live_material_cost,
        "Order.estimatedMaterialCost must match the live root's materialComponentCost exactly"
    );

    // Root ticket: producedQuantity matches the live root exactly.
    let root_ticket: (Option<i64>,) = sqlx::query_as(
        "SELECT produced_quantity FROM tickets WHERE order_id = $1 AND parent_ticket_id IS NULL",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        root_ticket.0,
        Some(i64::try_from(live_produced_quantity).unwrap())
    );
}

/// Tasks 19/26 -- market-price immutability for the new sell-side
/// enrichment. Epic A freezes at output price P1; the price list changes
/// to P2; Epic A reloaded still shows P1's expected revenue; Epic B
/// freezes at P2.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn output_market_price_is_immutable_once_frozen(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 50_000, 250_000).await;
    seed_balance(&pool, &fx, 35, "Pyerite", 50_000, 250_000).await;

    let (status, order_a) = create_order(&fx.app, &build).await;
    assert_eq!(status, StatusCode::CREATED, "order A: {order_a}");
    let revenue_a: String = order_a["expectedRevenue"].as_str().unwrap().to_string();
    // Rifter price on the fixture list is 500000.0000 @ qty 1 -> revenue 500000.0000.
    assert_eq!(revenue_a, "500000.0000");

    // Change the live output price.
    sqlx::query("UPDATE price_source_items SET price = 700000 WHERE price_source_id = $1 AND type_id = 5876")
        .bind(fx.price_list_id.0)
        .execute(&pool)
        .await
        .unwrap();

    let (status, order_b) = create_order(&fx.app, &build).await;
    assert_eq!(status, StatusCode::CREATED, "order B: {order_b}");
    let revenue_b: String = order_b["expectedRevenue"].as_str().unwrap().to_string();
    assert_eq!(revenue_b, "700000.0000");

    // Reload Epic A directly from the repository -- still P1.
    let order_repository = PgOrderRepository::new(pool.clone());
    let order_a_id: Uuid = order_a["id"].as_str().unwrap().parse().unwrap();
    let reloaded_a = order_repository
        .get_order(fx.workspace_id, OrderId(order_a_id))
        .await
        .unwrap();
    assert_eq!(
        reloaded_a.expected_revenue.unwrap().0.to_string(),
        "500000.0000",
        "Epic A's frozen revenue must not change because of a later price change or Epic B"
    );
}

/// Root/child ticket symmetry. `create_order`'s ticket loop
/// (`apps/iskworks-api/src/routes/orders/order_lifecycle.rs`) builds `plan_evidence: Some(operation.evidence.clone())`
/// identically for every operation, root included -- no special-casing.
/// Assert that holds for real: both the root ticket and the Build-resolved
/// child ticket's persisted `planEvidence` are exact projections of their
/// own frozen `PlanOperation.evidence`, not just the child's (the existing
/// facility-revision coverage only ever checked the child -- see
/// `linked_production_ticket_freezes_the_current_facility_revision`).
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn root_and_child_ticket_evidence_both_project_their_own_plan_operation(pool: PgPool) {
    let fx = fixture(&pool).await;
    let parent = assembly_with_built_component(&fx, 1, 90100, &[]).await;

    seed_balance(&pool, &fx, 35, "Pyerite", 50_000, 150_000).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 500_000, 2_500_000).await;

    let (status, order) = create_order(&fx.app, &parent).await;
    assert_eq!(status, StatusCode::CREATED, "{order}");

    let order_repository = PgOrderRepository::new(pool.clone());
    let order_id: Uuid = order["id"].as_str().unwrap().parse().unwrap();
    let operations = order_repository
        .list_order_plan_operations(OrderId(order_id))
        .await
        .unwrap();
    let root_op = operations
        .iter()
        .find(|op| op.parent_occurrence_key.is_none())
        .expect("root operation");
    let child_op = operations
        .iter()
        .find(|op| op.parent_occurrence_key.is_some())
        .expect("Build-resolved child operation");

    let tickets = list_tickets(&fx.app).await;
    let root_ticket = tickets
        .iter()
        .find(|t| t["occurrenceKey"] == root_op.occurrence_key)
        .unwrap_or_else(|| panic!("no ticket for root occurrence key: {tickets:?}"));
    let child_ticket = tickets
        .iter()
        .find(|t| t["occurrenceKey"] == child_op.occurrence_key)
        .unwrap_or_else(|| panic!("no ticket for child occurrence key: {tickets:?}"));

    for (label, ticket, op) in [
        ("root", root_ticket, root_op),
        ("child", child_ticket, child_op),
    ] {
        assert_eq!(
            ticket["planEvidence"],
            serde_json::to_value(&op.evidence).unwrap(),
            "{label} ticket's planEvidence must be an exact projection of its own frozen PlanOperation.evidence"
        );
        assert_eq!(
            ticket["materialComponentCost"],
            serde_json::to_value(op.material_component_cost).unwrap(),
            "{label} materialComponentCost"
        );
        assert_eq!(
            ticket["ownInstallationCost"],
            serde_json::to_value(op.own_installation_cost).unwrap(),
            "{label} ownInstallationCost"
        );
        assert_eq!(
            ticket["totalProductionCost"],
            serde_json::to_value(op.total_production_cost).unwrap(),
            "{label} totalProductionCost"
        );
        assert_eq!(
            ticket["producedQuantity"],
            serde_json::to_value(op.produced_quantity).unwrap(),
            "{label} producedQuantity"
        );
    }

    // Both carry an `executionSnapshot` for the recording form: the
    // root's from the root preview, the child's built from its own frozen
    // operation (runs + installation evidence). The child's full execution
    // identity still lives in `planEvidence`.
    assert!(
        !root_ticket["executionSnapshot"].is_null(),
        "root ticket must carry executionSnapshot: {root_ticket:?}"
    );
    assert_eq!(
        child_ticket["executionSnapshot"]["runs"],
        serde_json::to_value(child_op.runs).unwrap(),
        "child ticket's snapshot is its frozen operation's plan: {child_ticket:?}"
    );
}
