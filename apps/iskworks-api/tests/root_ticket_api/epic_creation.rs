use super::*;

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_order_mints_a_canonical_root_manufacturing_ticket(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = manufacturable_build(
        &fx,
        "Rifter",
        6830,
        "Rifter Blueprint",
        5876,
        "Rifter",
        1,
        rifter_materials(),
        1,
    )
    .await;

    let (status, order_body) = create_order(&fx.app, &build).await;
    assert_eq!(status, StatusCode::CREATED);

    let tickets = list_tickets(&fx.app).await;
    let root = root_ticket_for(&tickets, build.id);

    assert_eq!(root["kind"], "manufacturing");
    assert_eq!(root["typeId"], 5876);
    assert_eq!(root["capturedName"], "Rifter");
    assert_eq!(root["quantity"], 1, "runs 1 x 1/run = 1");
    assert_eq!(root["sourceBuildId"], build.id.0.to_string());
    // Explicit organizational Epic membership -- never inferred later from
    // `sourceBuildId` equality.
    assert_eq!(root["orderId"], order_body["id"]);
    // A frozen execution snapshot, from the same canonical builder an
    // ordinary Manufacturing ticket uses.
    assert!(
        root["executionSnapshot"].is_object(),
        "root ticket carries a frozen snapshot"
    );
    assert_eq!(root["executionSnapshot"]["runs"], 1);
    // Prerequisites mirror the effective root recipe materials.
    let prereq_types: Vec<i64> = root["prerequisites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["typeId"].as_i64().unwrap())
        .collect();
    assert!(prereq_types.contains(&34));
    assert!(prereq_types.contains(&35));
    // Every generated ticket starts `todo` -- its unmet materials show only
    // in the derived `blockedBy` list, never in its workflow status.
    assert_eq!(root["status"], "todo");
    assert!(
        !root["blockedBy"].as_array().unwrap().is_empty(),
        "the root ticket's unmet prerequisites are surfaced as derived blockers"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn root_ticket_output_is_runs_times_output_per_run(pool: PgPool) {
    let fx = fixture(&pool).await;
    // 3 runs of a recipe that yields 5 per run -> output 15, discrete runs.
    let build = manufacturable_build(
        &fx,
        "Antimatter Charge S",
        6002,
        "Charge Blueprint",
        6001,
        "Antimatter Charge S",
        5,
        vec![CapturedRecipeLine {
            type_id: 34,
            type_name: "Tritanium".to_string(),
            quantity_per_run: 100,
            sort_order: 0,
        }],
        3,
    )
    .await;

    let (status, _) = create_order(&fx.app, &build).await;
    assert_eq!(status, StatusCode::CREATED);

    let tickets = list_tickets(&fx.app).await;
    let root = root_ticket_for(&tickets, build.id);
    assert_eq!(root["quantity"], 15);
    assert_eq!(root["executionSnapshot"]["runs"], 3);
}

/// Critical acceptance test for explicit Ticket -> Epic membership: two
/// separate Orders/Epics created from the *same* root Build are
/// independent execution work and must each get their own root ticket --
/// the pre-`order_id` behavior (one shared root ticket, keyed only by
/// `sourceBuildId`) was exactly the transitional weakness this
/// removes. `sourceBuildId` stays Build *context* on both; `orderId` is
/// what actually tells them apart.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn two_epics_created_from_the_same_build_get_independent_root_tickets(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = manufacturable_build(
        &fx,
        "Rifter",
        6830,
        "Rifter Blueprint",
        5876,
        "Rifter",
        1,
        rifter_materials(),
        1,
    )
    .await;

    let (status_a, order_a) = create_order(&fx.app, &build).await;
    assert_eq!(status_a, StatusCode::CREATED);
    let (status_b, order_b) = create_order(&fx.app, &build).await;
    assert_eq!(status_b, StatusCode::CREATED);
    assert_ne!(order_a["id"], order_b["id"], "two independent Orders/Epics");

    let tickets = list_tickets(&fx.app).await;
    let root_count = tickets
        .iter()
        .filter(|t| t["sourceBuildId"] == build.id.0.to_string() && t["kind"] == "manufacturing")
        .count();
    assert_eq!(
        root_count, 2,
        "each Epic gets its own root ticket, even sharing one source Build"
    );

    let root_a = root_ticket_for_order(&tickets, order_a["id"].as_str().unwrap());
    let root_b = root_ticket_for_order(&tickets, order_b["id"].as_str().unwrap());
    assert_ne!(root_a["id"], root_b["id"]);
    assert_eq!(root_a["sourceBuildId"], build.id.0.to_string());
    assert_eq!(root_b["sourceBuildId"], build.id.0.to_string());
    assert_eq!(root_a["orderId"], order_a["id"]);
    assert_eq!(root_b["orderId"], order_b["id"]);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn root_ticket_records_final_production_without_order_completion(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = manufacturable_build(
        &fx,
        "Rifter",
        6830,
        "Rifter Blueprint",
        5876,
        "Rifter",
        1,
        rifter_materials(),
        1,
    )
    .await;
    assert_eq!(create_order(&fx.app, &build).await.0, StatusCode::CREATED);

    let root_id = root_ticket_for(&list_tickets(&fx.app).await, build.id)["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Stock the root ticket's inputs (whole-ISK carrying cost).
    for (type_id, name, qty, total_cost) in [
        (34_i64, "Tritanium", 1_000_i64, 1_000_i64),
        (35, "Pyerite", 200, 400),
    ] {
        sqlx::query(
            r#"INSERT INTO inventory_balances (workspace_id,owner_id,type_id,captured_name,quantity,total_historical_cost,revision,last_activity_at)
               VALUES ($1,$2,$3,$4,$5,$6,1,now())"#,
        )
        .bind(fx.workspace_id.0)
        .bind(fx.owner_id.0)
        .bind(type_id)
        .bind(name)
        .bind(qty)
        .bind(total_cost)
        .execute(&pool)
        .await
        .unwrap();
    }

    let key = Uuid::new_v4().to_string();
    let record = serde_json::json!({
        "idempotencyKey": key,
        "runsCompleted": 1,
        "output": { "typeId": 5876, "quantity": 1 },
        "inputs": [ { "typeId": 34, "quantity": 1000 }, { "typeId": 35, "quantity": 200 } ],
        "installationCost": "0",
    });
    let (status, first) = post_json(
        &fx.app,
        &format!("/api/tickets/{root_id}/record-production"),
        record.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body: {first}");
    assert_eq!(first["recording"]["outputTypeId"], 5876);
    assert_eq!(first["recording"]["outputQuantity"], 1);

    // The finished product now exists in inventory -- posted by the ticket
    // recording, NOT by complete_order.
    let product_qty: Option<i64> = sqlx::query_scalar(
        "SELECT quantity FROM inventory_balances WHERE workspace_id=$1 AND owner_id=$2 AND type_id=5876",
    )
    .bind(fx.workspace_id.0)
    .bind(fx.owner_id.0)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(product_qty, Some(1));

    let output_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM inventory_events WHERE event_kind='production_output' AND ticket_inventory_recording_id IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(output_events, 1);

    // Nothing came from the Order-completion path.
    let order_sourced: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM inventory_events WHERE source_reference LIKE 'order:%'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(order_sourced, 0, "no complete_order posting was involved");

    // Idempotent replay: 200, nothing new.
    let (status, replay) = post_json(
        &fx.app,
        &format!("/api/tickets/{root_id}/record-production"),
        record,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["recording"]["id"], first["recording"]["id"]);
    let product_qty_after: Option<i64> = sqlx::query_scalar(
        "SELECT quantity FROM inventory_balances WHERE workspace_id=$1 AND owner_id=$2 AND type_id=5876",
    )
    .bind(fx.workspace_id.0)
    .bind(fx.owner_id.0)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(product_qty_after, Some(1), "replay posted nothing");

    // The derived recording summary reflects the completed root run.
    let root = root_ticket_for(&list_tickets(&fx.app).await, build.id).clone();
    assert_eq!(root["recording"]["state"], "recorded");
    assert_eq!(root["recording"]["recordedQuantity"], 1);
}

/// An Order can be completed with its root Manufacturing
/// ticket still unrecorded, and completion posts nothing to inventory --
/// organizational lifecycle and execution/accounting are independent.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn completing_an_order_posts_nothing_and_leaves_the_root_ticket_unrecorded(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = manufacturable_build(
        &fx,
        "Rifter",
        6830,
        "Rifter Blueprint",
        5876,
        "Rifter",
        1,
        rifter_materials(),
        1,
    )
    .await;

    let events_before: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM inventory_events")
        .fetch_one(&pool)
        .await
        .unwrap();

    let (status, order_body) = create_order(&fx.app, &build).await;
    assert_eq!(status, StatusCode::CREATED);
    let order_id = OrderId(order_body["id"].as_str().unwrap().parse().unwrap());

    // The routes guard on derived Ready/InProgress, which a fresh Blocked
    // Order can't reach; this test is about the *storage* lifecycle
    // transitions' inventory-neutrality, so drive them directly.
    let repo = PgOrderRepository::new(pool.clone());
    repo.start_order(fx.workspace_id, order_id).await.unwrap();
    let completed = repo
        .complete_order(fx.workspace_id, order_id)
        .await
        .unwrap();
    assert!(completed.completed_at.is_some());

    // Nothing was posted to inventory by the Order lifecycle.
    let events_after: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM inventory_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        events_after, events_before,
        "Order completion posts no inventory event"
    );
    let alloc_count: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM inventory_allocations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(alloc_count, 0, "Order lifecycle created no allocation row");
    let product_qty: Option<i64> =
        sqlx::query_scalar("SELECT quantity FROM inventory_balances WHERE type_id = 5876")
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert_eq!(
        product_qty, None,
        "the finished Rifter never entered inventory"
    );

    // The root ticket exists but has recorded nothing.
    let root = root_ticket_for(&list_tickets(&fx.app).await, build.id).clone();
    assert_eq!(root["recording"]["state"], "notRecorded");
    assert_eq!(root["recording"]["recordedQuantity"], 0);
}

/// A requirement whose `create_ticket_for_requirement` route resolves it
/// to `RequirementKind::Buy` (no linked build for the raw material) --
/// `needsAction` in a freshly-created Order until a ticket is created for
/// it.
fn buy_requirement(order_body: &Value, type_id: i64) -> &Value {
    order_body["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["typeId"] == type_id && r["kind"] == "buy")
        .unwrap_or_else(|| panic!("no Buy requirement for type {type_id}"))
}

/// Explicit organizational Epic membership for a ticket generated from an
/// Order requirement (not the root ticket) -- `POST
/// /api/orders/:id/requirements/:reqId/tickets` must stamp `orderId` on
/// the created Acquisition ticket.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_ticket_for_requirement_sets_explicit_epic_membership(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = manufacturable_build(
        &fx,
        "Rifter",
        6830,
        "Rifter Blueprint",
        5876,
        "Rifter",
        1,
        rifter_materials(),
        1,
    )
    .await;

    let (status, order_body) = create_order(&fx.app, &build).await;
    assert_eq!(status, StatusCode::CREATED);
    let order_id = order_body["id"].as_str().unwrap();
    let requirement_id = buy_requirement(&order_body, 34)["id"].as_str().unwrap();

    let (status, ticket_body) = post_json(
        &fx.app,
        &format!("/api/orders/{order_id}/requirements/{requirement_id}/tickets"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(ticket_body["kind"], "acquisition");
    assert_eq!(ticket_body["orderId"], order_id);
}

/// Critical acceptance test: two Epics created from the same Build each
/// have their own frozen `OrderRequirement` row for the same material
/// (Tritanium) -- creating a ticket for each must never reuse one
/// ticket across the two Epics merely because the requirements are
/// equivalent. `create_ticket_for_requirement`'s idempotency guard is
/// scoped to one `OrderRequirement`'s own fulfillment links, which are
/// already per-Order by construction -- this proves that holds with
/// explicit `orderId` membership too.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn two_epics_with_equivalent_buy_requirements_get_independent_tickets(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = manufacturable_build(
        &fx,
        "Rifter",
        6830,
        "Rifter Blueprint",
        5876,
        "Rifter",
        1,
        rifter_materials(),
        1,
    )
    .await;

    let (status_a, order_a) = create_order(&fx.app, &build).await;
    assert_eq!(status_a, StatusCode::CREATED);
    let (status_b, order_b) = create_order(&fx.app, &build).await;
    assert_eq!(status_b, StatusCode::CREATED);
    let order_a_id = order_a["id"].as_str().unwrap();
    let order_b_id = order_b["id"].as_str().unwrap();

    let requirement_a_id = buy_requirement(&order_a, 34)["id"].as_str().unwrap();
    let requirement_b_id = buy_requirement(&order_b, 34)["id"].as_str().unwrap();
    assert_ne!(
        requirement_a_id, requirement_b_id,
        "each Epic freezes its own OrderRequirement row, even for the same material"
    );

    let (status, ticket_a) = post_json(
        &fx.app,
        &format!("/api/orders/{order_a_id}/requirements/{requirement_a_id}/tickets"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, ticket_b) = post_json(
        &fx.app,
        &format!("/api/orders/{order_b_id}/requirements/{requirement_b_id}/tickets"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    assert_ne!(
        ticket_a["id"], ticket_b["id"],
        "equivalent Buy requirements in two Epics never share one ticket"
    );
    assert_eq!(ticket_a["orderId"], order_a_id);
    assert_eq!(ticket_b["orderId"], order_b_id);
}

/// `POST /api/orders/:id/requirements/:reqId/link` must not reach another
/// workspace's requirement. `list_order_requirements` is scoped only by
/// `order_id`, so the route has to check the Order's workspace itself, the
/// same way `create_ticket_for_requirement_route` does.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn linking_a_ticket_to_another_workspaces_requirement_is_not_found(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = manufacturable_build(
        &fx,
        "Rifter",
        6830,
        "Rifter Blueprint",
        5876,
        "Rifter",
        1,
        rifter_materials(),
        1,
    )
    .await;

    let (status, own_order) = create_order(&fx.app, &build).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, victim_order) = create_order(&fx.app, &build).await;
    assert_eq!(status, StatusCode::CREATED);
    let own_order_id = own_order["id"].as_str().unwrap();
    let own_requirement_id = buy_requirement(&own_order, 34)["id"].as_str().unwrap();
    let victim_order_id = victim_order["id"].as_str().unwrap();
    let victim_requirement_id = buy_requirement(&victim_order, 34)["id"].as_str().unwrap();

    // A Tritanium ticket in the caller's own workspace.
    let (status, ticket) = post_json(
        &fx.app,
        &format!("/api/orders/{own_order_id}/requirements/{own_requirement_id}/tickets"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Hand the second Epic to a newer workspace. The app keeps resolving to
    // the fixture workspace (oldest), so from here on it acts as the attacker.
    let victim_workspace_id = Uuid::new_v4();
    let victim_owner_id = Uuid::new_v4();
    let later = chrono::Utc::now() + chrono::Duration::hours(1);
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Victim',$2,$3,$3)",
    )
    .bind(victim_workspace_id).bind(victim_owner_id).bind(later).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Victim',true,$3,$3)",
    )
    .bind(victim_owner_id).bind(victim_workspace_id).bind(later).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE orders SET workspace_id = $1 WHERE id = $2")
        .bind(victim_workspace_id)
        .bind(Uuid::parse_str(victim_order_id).unwrap())
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let (status, body) = post_json(
        &fx.app,
        &format!("/api/orders/{victim_order_id}/requirements/{victim_requirement_id}/link"),
        serde_json::json!({ "ticketId": ticket["id"] }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    let linked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM order_requirement_fulfillments WHERE order_requirement_id = $1",
    )
    .bind(Uuid::parse_str(victim_requirement_id).unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        linked, 0,
        "nothing may be written against the other workspace's requirement"
    );
}
