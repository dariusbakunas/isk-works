use super::*;

// ─────────────────────────────────────────────────────────────────────────
// `POST /api/orders/:id/operations/:occurrence_key/ticket`: a frozen
// production step's ticket, created on demand.
// ─────────────────────────────────────────────────────────────────────────

/// An Assembly Epic: root (5 Fabricated Component + Tritanium) and the
/// Fabricated Component child operation (5 runs).
async fn assembly_epic(fx: &Fixture) -> Value {
    let parent = assembly_with_built_component(fx, 1, 90100, &[]).await;
    let (status, order) = create_order(&fx.app, &parent).await;
    assert_eq!(status, StatusCode::CREATED, "body: {order}");
    order
}

fn operation_key(order: &Value, product_type_id: i64) -> String {
    order["productionPlan"]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|operation| operation["productTypeId"] == product_type_id)
        .unwrap_or_else(|| panic!("no operation for {product_type_id}"))["occurrenceKey"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn ticket_for(fx: &Fixture, order: &Value, product_type_id: i64) -> Option<Value> {
    list_tickets(&fx.app).await.into_iter().find(|ticket| {
        ticket["orderId"] == order["id"]
            && ticket["typeId"] == product_type_id
            && ticket["status"] != "canceled"
    })
}

async fn create_step_ticket(fx: &Fixture, order: &Value, key: &str) -> (StatusCode, Value) {
    post_json(
        &fx.app,
        &format!(
            "/api/orders/{}/operations/{}/ticket",
            order["id"].as_str().unwrap(),
            key
        ),
        Value::Null,
    )
    .await
}

async fn delete_ticket(fx: &Fixture, ticket_id: &str) {
    let response = fx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/tickets/{ticket_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "delete: {}",
        response.status()
    );
}

/// The fields that make a step's ticket the same work, prerequisites
/// included, minus identity.
fn shape(ticket: &Value) -> Value {
    let mut prerequisites: Vec<Value> = ticket["prerequisites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            serde_json::json!([
                p["typeId"],
                p["requiredQuantity"],
                p["reusedQuantity"],
                p["fulfillmentScope"]
            ])
        })
        .collect();
    prerequisites.sort_by_key(|p| p[0].as_i64());
    serde_json::json!({
        "kind": ticket["kind"],
        "typeId": ticket["typeId"],
        "capturedName": ticket["capturedName"],
        "quantity": ticket["quantity"],
        "producedQuantity": ticket["producedQuantity"],
        "sourceBuildId": ticket["sourceBuildId"],
        "occurrenceKey": ticket["occurrenceKey"],
        "parentTicketId": ticket["parentTicketId"],
        "totalProductionCost": ticket["totalProductionCost"],
        "prerequisites": prerequisites,
    })
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_deleted_step_ticket_is_recreated_from_the_frozen_plan(pool: PgPool) {
    let fx = fixture(&pool).await;
    let order = assembly_epic(&fx).await;
    let key = operation_key(&order, 90100);
    let original = ticket_for(&fx, &order, 90100)
        .await
        .expect("eager child ticket");
    delete_ticket(&fx, original["id"].as_str().unwrap()).await;
    assert!(ticket_for(&fx, &order, 90100).await.is_none());

    let (status, created) = create_step_ticket(&fx, &order, &key).await;

    assert_eq!(status, StatusCode::CREATED, "body: {created}");
    let recreated = ticket_for(&fx, &order, 90100)
        .await
        .expect("recreated ticket");
    assert_ne!(recreated["id"], original["id"]);
    assert_eq!(
        shape(&recreated),
        shape(&original),
        "the same work as Create Epic's ticket"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_step_with_an_active_ticket_returns_it_instead_of_a_second(pool: PgPool) {
    let fx = fixture(&pool).await;
    let order = assembly_epic(&fx).await;
    let key = operation_key(&order, 90100);
    let existing = ticket_for(&fx, &order, 90100).await.unwrap();

    let (status, body) = create_step_ticket(&fx, &order, &key).await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["id"], existing["id"]);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_creates_make_one_ticket(pool: PgPool) {
    let fx = fixture(&pool).await;
    let order = assembly_epic(&fx).await;
    let key = operation_key(&order, 90100);
    let original = ticket_for(&fx, &order, 90100).await.unwrap();
    delete_ticket(&fx, original["id"].as_str().unwrap()).await;

    let (a, b) = tokio::join!(
        create_step_ticket(&fx, &order, &key),
        create_step_ticket(&fx, &order, &key)
    );

    let statuses = [a.0, b.0];
    assert!(
        statuses.contains(&StatusCode::CREATED) && statuses.contains(&StatusCode::OK),
        "{statuses:?}"
    );
    assert_eq!(a.1["id"], b.1["id"], "the loser gets the winner's ticket");
    let count = list_tickets(&fx.app)
        .await
        .iter()
        .filter(|ticket| ticket["orderId"] == order["id"] && ticket["typeId"] == 90100)
        .count();
    assert_eq!(count, 1);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_canceled_step_ticket_frees_the_step_and_can_still_be_restored(pool: PgPool) {
    let fx = fixture(&pool).await;
    let order = assembly_epic(&fx).await;
    let key = operation_key(&order, 90100);
    let original = ticket_for(&fx, &order, 90100).await.unwrap();
    let original_id = original["id"].as_str().unwrap().to_string();
    let (status, _) = post_json(
        &fx.app,
        &format!("/api/tickets/{original_id}/cancel"),
        Value::Null,
    )
    .await;
    assert!(status.is_success());

    let (status, created) = create_step_ticket(&fx, &order, &key).await;
    assert_eq!(status, StatusCode::CREATED, "body: {created}");
    assert_ne!(created["id"], original["id"]);
    // The claim moved: asking again returns the new ticket.
    let (status, again) = create_step_ticket(&fx, &order, &key).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again["id"], created["id"]);

    // Un-canceling the old ticket is still allowed (no constraint on it).
    let response = fx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/tickets/{original_id}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"status": "todo"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "restore: {}",
        response.status()
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn parent_and_child_link_whichever_is_created_first(pool: PgPool) {
    let fx = fixture(&pool).await;
    let order = assembly_epic(&fx).await;
    let root_key = order["productionPlan"]["rootOccurrenceKey"]
        .as_str()
        .unwrap()
        .to_string();
    let child_key = operation_key(&order, 90100);
    let root = ticket_for(&fx, &order, 90111).await.expect("root ticket");
    let child = ticket_for(&fx, &order, 90100).await.unwrap();
    delete_ticket(&fx, child["id"].as_str().unwrap()).await;
    delete_ticket(&fx, root["id"].as_str().unwrap()).await;

    // Child first: no parent yet.
    let (status, new_child) = create_step_ticket(&fx, &order, &child_key).await;
    assert_eq!(status, StatusCode::CREATED, "body: {new_child}");
    assert!(new_child["parentTicketId"].is_null());

    // Then the root adopts it.
    let (status, new_root) = create_step_ticket(&fx, &order, &root_key).await;
    assert_eq!(status, StatusCode::CREATED, "body: {new_root}");
    let child_now = ticket_for(&fx, &order, 90100).await.unwrap();
    assert_eq!(child_now["parentTicketId"], new_root["id"]);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn an_unknown_step_is_not_found(pool: PgPool) {
    let fx = fixture(&pool).await;
    let order = assembly_epic(&fx).await;

    let (status, body) = create_step_ticket(&fx, &order, "build:nope").await;

    assert_eq!(status, StatusCode::NOT_FOUND, "body: {body}");
    assert_eq!(body["error"]["code"], "operation_not_found");
}
