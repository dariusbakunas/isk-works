use super::*;

// ─────────────────────────────────────────────────────────────────────────
// `POST /api/tickets/combine` and `POST /api/tickets/:id/split`: one
// ticket doing the same production step for several Epics.
// ─────────────────────────────────────────────────────────────────────────

/// Two Epics from one Assembly Build, each with its Fabricated Component
/// step ticketed. Returns (Epics, step tickets).
async fn two_epics_with_component_steps(fx: &Fixture) -> (Vec<Value>, Vec<Value>) {
    let parent = assembly_with_built_component(fx, 1, 90100, &[]).await;
    let mut orders = Vec::new();
    let mut steps = Vec::new();
    for _ in 0..2 {
        let (status, order) = create_order(&fx.app, &parent).await;
        assert_eq!(status, StatusCode::CREATED, "body: {order}");
        let key = order["productionPlan"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|operation| operation["productTypeId"] == 90100)
            .unwrap()["occurrenceKey"]
            .as_str()
            .unwrap()
            .to_string();
        let (status, step) = post_json(
            &fx.app,
            &format!(
                "/api/orders/{}/operations/{}/ticket",
                order["id"].as_str().unwrap(),
                key
            ),
            Value::Null,
        )
        .await;
        assert!(status.is_success(), "body: {step}");
        orders.push(order);
        steps.push(step);
    }
    (orders, steps)
}

async fn combine(fx: &Fixture, tickets: &[&Value]) -> (StatusCode, Value) {
    let ids: Vec<&Value> = tickets.iter().map(|ticket| &ticket["id"]).collect();
    post_json(
        &fx.app,
        "/api/tickets/combine",
        serde_json::json!({ "ticketIds": ids }),
    )
    .await
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn combine_then_split_round_trips_through_the_api(pool: PgPool) {
    let fx = fixture(&pool).await;
    let (orders, steps) = two_epics_with_component_steps(&fx).await;

    let (status, combined) = combine(&fx, &[&steps[0], &steps[1]]).await;
    assert_eq!(status, StatusCode::CREATED, "body: {combined}");
    assert_eq!(combined["orderId"], Value::Null);
    assert_eq!(
        combined["quantity"].as_u64().unwrap(),
        steps[0]["quantity"].as_u64().unwrap() + steps[1]["quantity"].as_u64().unwrap()
    );
    let mut epics: Vec<&Value> = combined["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|operation| &operation["orderId"])
        .collect();
    epics.sort_by_key(|id| id.as_str().unwrap().to_string());
    let mut expected: Vec<&Value> = orders.iter().map(|order| &order["id"]).collect();
    expected.sort_by_key(|id| id.as_str().unwrap().to_string());
    assert_eq!(epics, expected);

    // The Board lists the members as canceled, merged into it.
    let tickets = list_tickets(&fx.app).await;
    for step in &steps {
        let member = tickets
            .iter()
            .find(|ticket| ticket["id"] == step["id"])
            .unwrap();
        assert_eq!(member["status"], "canceled");
        assert_eq!(member["mergedIntoTicketId"], combined["id"]);
    }
    // Each Epic's plan shows the combined ticket on its step.
    for order in &orders {
        let response = fx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/orders/{}", order["id"].as_str().unwrap()))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let detail = body_json(response).await;
        let step = detail["productionPlan"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|operation| operation["productTypeId"] == 90100)
            .unwrap();
        assert_eq!(step["ticketId"], combined["id"], "detail: {detail}");
    }

    let (status, restored) = post_json(
        &fx.app,
        &format!("/api/tickets/{}/split", combined["id"].as_str().unwrap()),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {restored}");
    assert_eq!(restored.as_array().unwrap().len(), 2);
    assert!(restored
        .as_array()
        .unwrap()
        .iter()
        .all(|ticket| ticket["status"] == "todo" && ticket["mergedIntoTicketId"].is_null()));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn refusals_are_conflicts_naming_the_ticket(pool: PgPool) {
    let fx = fixture(&pool).await;
    let (_, steps) = two_epics_with_component_steps(&fx).await;

    let (status, body) = combine(&fx, &[&steps[0]]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body: {body}");
    assert_eq!(body["error"]["code"], "combine_needs_two_tickets");

    let (status, _) = post_json(
        &fx.app,
        &format!("/api/tickets/{}/start", steps[1]["id"].as_str().unwrap()),
        Value::Null,
    )
    .await;
    assert!(status.is_success());
    let (status, body) = combine(&fx, &[&steps[0], &steps[1]]).await;
    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
    assert_eq!(body["error"]["code"], "ticket_not_combinable");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with(steps[1]["displayId"].as_str().unwrap()),
        "body: {body}"
    );

    let (status, body) = post_json(
        &fx.app,
        &format!("/api/tickets/{}/split", steps[0]["id"].as_str().unwrap()),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
    assert_eq!(body["error"]["code"], "ticket_not_combined");

    let (status, body) = combine(
        &fx,
        &[&steps[0], &serde_json::json!({ "id": Uuid::new_v4() })],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body: {body}");
}
