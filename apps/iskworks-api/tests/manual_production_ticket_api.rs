//! Build-backed manual Manufacturing/Reaction ticket creation (`POST
//! /api/tickets` kind = `manufacturing` | `reaction`) and its read-only
//! preview (`GET /api/builds/:build_id/ticket-preview`).
//!
//! Proves the product rule -- "Build owns the production plan; Ticket
//! freezes that plan as intended work" -- end to end: the server derives
//! title/quantity/execution snapshot/prerequisites from the selected Build
//! rather than accepting them from the client, an overridden `runs` is
//! frozen without mutating the Build, a kind/Build-recipe mismatch is
//! rejected in both directions, an out-of-bounds `runs` surfaces the
//! planner's own validation error rather than being silently clamped, a
//! later Build edit never changes an already-frozen ticket, the resulting
//! ticket works through the existing `record-production` path unmodified,
//! and a manually created ticket matches its generated (Order-rooted)
//! counterpart on every execution-relevant field.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use iskworks_api::{build_router, AppState};
use iskworks_core::{
    Build, BuildId, BuildRecipe, CapturedReactionFormula, CapturedRecipe, CapturedRecipeLine,
    DraftPlanningInput, DraftPlanningSnapshot, IndustryRepository, InventoryRepository,
    MarketPricingPolicy, NewBuild, OwnerId, PriceSourceId, ProductionRepository, RecipeCurrency,
    WorkspaceId, WorkspaceRepository, DEFAULT_MARKET_SCOPE,
};
use iskworks_sde::SdeReadRepository;
use iskworks_storage::{
    PgIndustryRepository, PgInventoryRepository, PgOrderRepository, PgProductionRepository,
    PgSdeRepository, PgWorkspaceRepository,
};
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

struct Fixture {
    app: axum::Router,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    import_id: Uuid,
    price_list_id: PriceSourceId,
    industry: PgIndustryRepository,
}

async fn fixture(pool: &PgPool) -> Fixture {
    let workspace_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let import_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Manual Production Ticket Test',$2,$3,$3)",
    )
    .bind(workspace_id).bind(owner_id).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Manual Production Ticket Test',true,$3,$3)",
    )
    .bind(owner_id).bind(workspace_id).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO sde_imports (id,source_version,source_label,source_checksum,status,active,started_at,completed_at) VALUES ($1,'test','fixture',$2,'active',true,$3,$3)",
    )
    .bind(import_id).bind(format!("manual-production-ticket-{}", Uuid::new_v4())).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query(
        r#"INSERT INTO sde_types (import_id,type_id,name_en,published) VALUES
             ($1,34,'Tritanium',true),
             ($1,35,'Pyerite',true),
             ($1,5876,'Rifter',true),
             ($1,6830,'Rifter Blueprint',true),
             ($1,90001,'Hull Section',true),
             ($1,90003,'Hull Section Reaction Formula',true)"#,
    )
    .bind(import_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_blueprints (import_id,blueprint_type_id,name_en,duration_seconds) VALUES ($1,6830,'Rifter Blueprint',600)",
    )
    .bind(import_id).execute(&mut *tx).await.unwrap();
    sqlx::query(
        r#"INSERT INTO sde_blueprint_materials (import_id,blueprint_type_id,material_type_id,quantity,position) VALUES
             ($1,6830,34,1000,0),($1,6830,35,200,1)"#,
    )
    .bind(import_id).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO sde_blueprint_products (import_id,blueprint_type_id,product_type_id,quantity,position) VALUES ($1,6830,5876,1,0)",
    )
    .bind(import_id).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO sde_reaction_formulas (import_id,reaction_formula_type_id,name_en,duration_seconds) VALUES ($1,90003,'Hull Section Reaction Formula',50)",
    )
    .bind(import_id).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO sde_reaction_formula_materials (import_id,reaction_formula_type_id,material_type_id,quantity,position) VALUES ($1,90003,35,50,0)",
    )
    .bind(import_id).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO sde_reaction_formula_products (import_id,reaction_formula_type_id,product_type_id,quantity,position) VALUES ($1,90003,90001,1,0)",
    )
    .bind(import_id).execute(&mut *tx).await.unwrap();

    // A manual price list covering every fixture type -- `create_order`
    // (used by the generated/manual parity test) persists a build-plan
    // snapshot whose `captured_source_revision` must be > 0.
    let price_list_id = PriceSourceId::new();
    sqlx::query(
        "INSERT INTO price_sources (id,workspace_id,display_name,source_kind,revision,created_at,updated_at) VALUES ($1,$2,'Fixture prices','manual',1,$3,$3)",
    )
    .bind(price_list_id.0).bind(workspace_id).bind(now).execute(&mut *tx).await.unwrap();
    for (type_id, name, price) in [
        (34_i64, "Tritanium", "5.0000"),
        (35, "Pyerite", "10.0000"),
        (5876, "Rifter", "500000.0000"),
        (90001, "Hull Section", "200.0000"),
    ] {
        sqlx::query(
            "INSERT INTO price_source_items (price_source_id,type_id,captured_name,price,updated_at) VALUES ($1,$2,$3,$4::numeric,$5)",
        )
        .bind(price_list_id.0).bind(type_id).bind(name).bind(price).bind(now).execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();

    let workspace = PgWorkspaceRepository::new(pool.clone());
    let app =
        build_router(
            AppState::new(Arc::new(workspace) as Arc<dyn WorkspaceRepository>)
                .with_sde_repository(
                    Arc::new(PgSdeRepository::new(pool.clone())) as Arc<dyn SdeReadRepository>
                )
                .with_industry_repository(Arc::new(PgIndustryRepository::new(pool.clone()))
                    as Arc<dyn IndustryRepository>)
                .with_inventory_repository(Arc::new(PgInventoryRepository::new(pool.clone()))
                    as Arc<dyn InventoryRepository>)
                .with_production_repository(Arc::new(PgProductionRepository::new(pool.clone()))
                    as Arc<dyn ProductionRepository>)
                .with_order_repository(Arc::new(PgOrderRepository::new(pool.clone()))),
        );

    Fixture {
        app,
        workspace_id: WorkspaceId(workspace_id),
        owner_id: OwnerId(owner_id),
        import_id,
        price_list_id,
        industry: PgIndustryRepository::new(pool.clone()),
    }
}

fn draft_planning(
    now: chrono::DateTime<chrono::Utc>,
    price_list_id: PriceSourceId,
) -> DraftPlanningSnapshot {
    DraftPlanningSnapshot {
        input: DraftPlanningInput {
            material_scope: DEFAULT_MARKET_SCOPE,
            output_scope: DEFAULT_MARKET_SCOPE,
            manual_price_list_id: Some(price_list_id),
            expected_manual_price_list_revision: Some(1),
            material_pricing_policy: MarketPricingPolicy::HighestBuy,
            output_pricing_policy: MarketPricingPolicy::LowestSell,
            pricing_selections: Vec::new(),
            blueprint_selection: None,
            manufacturing_facility: None,
            reaction_facility: None,
            facility_eiv_manual: false,
            component_resolutions: Vec::new(),
            fulfillment_scopes: Vec::new(),
        },
        updated_at: now,
    }
}

fn rifter_materials() -> Vec<CapturedRecipeLine> {
    vec![
        CapturedRecipeLine {
            type_id: 34,
            type_name: "Tritanium".to_string(),
            quantity_per_run: 1_000,
            sort_order: 0,
        },
        CapturedRecipeLine {
            type_id: 35,
            type_name: "Pyerite".to_string(),
            quantity_per_run: 200,
            sort_order: 1,
        },
    ]
}

/// A standalone, manufacturable Rifter Build with `draft_planning` set so
/// `calculate_build_snapshot_with_coverage` (and therefore both manual ticket creation
/// and `create_order`) works.
async fn manufacturing_build(fx: &Fixture, runs: u64) -> Build {
    let now = chrono::Utc::now();
    fx.industry
        .create_root_build(NewBuild {
            build: Build {
                id: BuildId::new(),
                workspace_id: fx.workspace_id,
                owner_id: fx.owner_id,
                name: "Rifter".to_string(),
                recipe: BuildRecipe::Manufacturing(CapturedRecipe {
                    source_sde_dataset_id: fx.import_id,
                    source_sde_version: "test".to_string(),
                    blueprint_type_id: 6830,
                    blueprint_name: "Rifter Blueprint".to_string(),
                    duration_seconds_per_run: Some(600),
                    materials: rifter_materials(),
                    products: vec![CapturedRecipeLine {
                        type_id: 5876,
                        type_name: "Rifter".to_string(),
                        quantity_per_run: 1,
                        sort_order: 0,
                    }],
                    fingerprint: "rifter-fixture".to_string(),
                }),
                runs,
                notes: String::new(),
                revision: 1,
                created_at: now,
                updated_at: now,
                draft_planning: Some(draft_planning(now, fx.price_list_id)),
                recipe_currency: RecipeCurrency::Current,
                active_sde_version: None,
                product_category_name: None,
                product_group_name: None,
                selected_blueprint_origin: None,
                has_owned_blueprint: false,
            },
        })
        .await
        .unwrap()
}

/// A standalone reaction Build (Hull Section, Pyerite -> Hull Section).
async fn reaction_build(fx: &Fixture, runs: u64) -> Build {
    let now = chrono::Utc::now();
    fx.industry
        .create_root_build(NewBuild {
            build: Build {
                id: BuildId::new(),
                workspace_id: fx.workspace_id,
                owner_id: fx.owner_id,
                name: "Hull Section".to_string(),
                recipe: BuildRecipe::Reaction(CapturedReactionFormula {
                    source_sde_dataset_id: fx.import_id,
                    source_sde_version: "test".to_string(),
                    reaction_formula_type_id: 90_003,
                    reaction_formula_name: "Hull Section Reaction Formula".to_string(),
                    duration_seconds_per_run: Some(50),
                    materials: vec![CapturedRecipeLine {
                        type_id: 35,
                        type_name: "Pyerite".to_string(),
                        quantity_per_run: 50,
                        sort_order: 0,
                    }],
                    products: vec![CapturedRecipeLine {
                        type_id: 90_001,
                        type_name: "Hull Section".to_string(),
                        quantity_per_run: 1,
                        sort_order: 0,
                    }],
                    fingerprint: "hull-section-fixture".to_string(),
                }),
                runs,
                notes: String::new(),
                revision: 1,
                created_at: now,
                updated_at: now,
                draft_planning: Some(draft_planning(now, fx.price_list_id)),
                recipe_currency: RecipeCurrency::Current,
                active_sde_version: None,
                product_category_name: None,
                product_group_name: None,
                selected_blueprint_origin: None,
                has_owned_blueprint: false,
            },
        })
        .await
        .unwrap()
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

/// `POST /api/builds/:id/orders` requires the live
/// `PreviewBuildPlanCommand` overlay body -- mirrors `build.recipe`/
/// `build.runs`/`build.draft_planning.input` verbatim (every fixture Build
/// in this file has `draft_planning` set, so "live overlay" and "saved
/// configuration" are identical here).
fn command_json_for_build(build: &Build) -> Value {
    let recipe = match &build.recipe {
        BuildRecipe::Manufacturing(recipe) => serde_json::json!({
            "mode": "manufacturing",
            "blueprintTypeId": recipe.blueprint_type_id,
        }),
        BuildRecipe::Reaction(formula) => serde_json::json!({
            "mode": "reaction",
            "reactionFormulaTypeId": formula.reaction_formula_type_id,
        }),
    };
    let draft = build
        .draft_planning
        .as_ref()
        .map(|snapshot| &snapshot.input);
    serde_json::json!({
        "recipe": recipe,
        "runs": build.runs,
        "materialScope": draft.map(|d| &d.material_scope),
        "outputScope": draft.map(|d| &d.output_scope),
        "manualPriceListId": draft.and_then(|d| d.manual_price_list_id).map(|id| id.0),
        "expectedManualPriceListRevision": draft.and_then(|d| d.expected_manual_price_list_revision),
        "pricingSelections": draft.map(|d| &d.pricing_selections).unwrap_or(&Vec::new()),
        "blueprintSelection": draft.and_then(|d| d.blueprint_selection.as_ref()),
        "manufacturingFacility": draft.and_then(|d| d.manufacturing_facility.as_ref()),
        "reactionFacility": draft.and_then(|d| d.reaction_facility.as_ref()),
        "componentResolutions": draft.map(|d| &d.component_resolutions).unwrap_or(&Vec::new()),
        "fulfillmentScopes": draft.map(|d| &d.fulfillment_scopes).unwrap_or(&Vec::new()),
        "buildId": build.id.0,
    })
}

async fn post_json(app: &axum::Router, path: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = body_json(response).await;
    if !status.is_success() {
        eprintln!("POST {path} failed: {status} {body}");
    }
    (status, body)
}

async fn put_json(app: &axum::Router, path: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (status, body_json(response).await)
}

async fn get_json(app: &axum::Router, path: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    (status, body_json(response).await)
}

async fn list_tickets(app: &axum::Router) -> Vec<Value> {
    let (status, body) = get_json(app, "/api/tickets").await;
    assert_eq!(status, StatusCode::OK);
    body.as_array().cloned().unwrap_or_default()
}

fn ticket_by_id<'a>(tickets: &'a [Value], id: &str) -> &'a Value {
    tickets
        .iter()
        .find(|t| t["id"] == id)
        .expect("ticket exists")
}

/// `POST /api/tickets`'s own response is a bare `Ticket` -- `prerequisites`/
/// `blockedBy`/`recording` only exist on `GET /api/tickets`'s enriched
/// `OrderTicketSummary` view. Fetches that enriched view for one ticket.
async fn fetch_enriched_ticket(app: &axum::Router, ticket_id: &str) -> Value {
    ticket_by_id(&list_tickets(app).await, ticket_id).clone()
}

/// A `BlueprintSnapshot` mints a fresh `id`/`buildId`/`capturedAt` on every
/// capture, even for the same underlying Build/blueprint -- strip those
/// before comparing two independently-captured snapshots (e.g. a preview
/// vs. the ticket it previewed, or two independently generated tickets)
/// for semantic equality. Never used for a *single* ticket's snapshot
/// compared against itself (a re-fetch of the same stored row), which is
/// byte-identical without normalizing.
fn normalized_snapshot(mut snapshot: Value) -> Value {
    if let Some(blueprint) = snapshot
        .get_mut("blueprint")
        .and_then(|blueprint| blueprint.as_object_mut())
    {
        blueprint.remove("id");
        blueprint.remove("buildId");
        blueprint.remove("capturedAt");
    }
    snapshot
}

fn prerequisite_quantities(prerequisites: &Value) -> std::collections::BTreeMap<i64, u64> {
    prerequisites
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["typeId"].as_i64().unwrap(),
                p["requiredQuantity"].as_u64().unwrap(),
            )
        })
        .collect()
}

async fn inventory_event_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*)::bigint FROM inventory_events")
        .fetch_one(pool)
        .await
        .unwrap()
}

// -- Manufacturing creation --------------------------------------------

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_manual_manufacturing_ticket_freezes_the_build_plan(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = manufacturing_build(&fx, 1).await;
    let events_before = inventory_event_count(&pool).await;

    let (status, ticket) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({ "kind": "manufacturing", "buildId": build.id.0 }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(ticket["kind"], "manufacturing");
    // Every new ticket starts in the To Do lane regardless of whether its
    // materials are on hand -- workflow status is user-controlled.
    assert_eq!(ticket["status"], "todo");
    assert_eq!(ticket["typeId"], 5876);
    // Title is derived from the Build's own product, never accepted from
    // the client.
    assert_eq!(ticket["capturedName"], "Rifter");
    assert_eq!(ticket["quantity"], 1, "1 run x 1/run");
    assert_eq!(ticket["sourceBuildId"], build.id.0.to_string());
    assert_eq!(ticket["orderId"], Value::Null);
    assert_eq!(ticket["assigneeCharacterId"], Value::Null);
    assert_eq!(ticket["notes"], "");
    assert_eq!(
        ticket["estimatedUnitCost"],
        Value::Null,
        "no per-ticket price snapshot -- pricing is a Build concern"
    );
    assert!(
        ticket["executionSnapshot"].is_object(),
        "a frozen execution snapshot"
    );
    assert_eq!(ticket["executionSnapshot"]["runs"], 1);
    let enriched = fetch_enriched_ticket(&fx.app, ticket["id"].as_str().unwrap()).await;
    let prereq_types: Vec<i64> = enriched["prerequisites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["typeId"].as_i64().unwrap())
        .collect();
    assert!(prereq_types.contains(&34));
    assert!(prereq_types.contains(&35));
    // Those unmet prerequisites surface as derived blockers -- a separate
    // axis from the `todo` workflow status asserted above.
    assert!(
        !enriched["blockedBy"].as_array().unwrap().is_empty(),
        "unmet prerequisites are reported as derived blockers"
    );

    // Fully inventory-neutral: creating this ticket posted nothing.
    let events_after = inventory_event_count(&pool).await;
    assert_eq!(events_after, events_before, "no inventory side effects");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_manual_reaction_ticket_freezes_the_build_plan(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = reaction_build(&fx, 1).await;

    let (status, ticket) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({ "kind": "reaction", "buildId": build.id.0 }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(ticket["kind"], "reaction");
    assert_eq!(ticket["status"], "todo");
    assert_eq!(ticket["typeId"], 90_001);
    assert_eq!(ticket["capturedName"], "Hull Section");
    assert_eq!(ticket["quantity"], 1);
    assert_eq!(ticket["sourceBuildId"], build.id.0.to_string());
    assert_eq!(
        ticket["executionSnapshot"]["blueprint"],
        Value::Null,
        "a reaction snapshot carries no blueprint/ME/TE"
    );
    let enriched = fetch_enriched_ticket(&fx.app, ticket["id"].as_str().unwrap()).await;
    let prereq_types: Vec<i64> = enriched["prerequisites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["typeId"].as_i64().unwrap())
        .collect();
    assert!(prereq_types.contains(&35));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn runs_override_freezes_intended_runs_without_mutating_the_build(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = manufacturing_build(&fx, 1).await;

    let (status, ticket) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({ "kind": "manufacturing", "buildId": build.id.0, "runs": 3 }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(ticket["quantity"], 3, "3 runs x 1/run");
    assert_eq!(ticket["executionSnapshot"]["runs"], 3);
    let enriched = fetch_enriched_ticket(&fx.app, ticket["id"].as_str().unwrap()).await;
    let tritanium = enriched["prerequisites"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["typeId"] == 34)
        .unwrap();
    assert_eq!(
        tritanium["requiredQuantity"], 3_000,
        "materials scale with the overridden runs, not the Build's own runs"
    );

    // The persisted Build itself is untouched.
    let (status, fetched_build) = get_json(&fx.app, &format!("/api/builds/{}", build.id.0)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched_build["runs"], 1, "Build.runs was never mutated");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn manufacturing_request_against_a_reaction_build_is_rejected(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = reaction_build(&fx, 1).await;

    let (status, body) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({ "kind": "manufacturing", "buildId": build.id.0 }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"]["code"],
        "ticket_kind_does_not_match_build_recipe"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reaction_request_against_a_manufacturing_build_is_rejected(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = manufacturing_build(&fx, 1).await;

    let (status, body) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({ "kind": "reaction", "buildId": build.id.0 }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"]["code"],
        "ticket_kind_does_not_match_build_recipe"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn runs_above_the_planner_limit_is_rejected_not_clamped(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = manufacturing_build(&fx, 1).await;

    let (status, body) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({ "kind": "manufacturing", "buildId": build.id.0, "runs": 1_000_001 }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "validation_failed");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn creating_a_ticket_for_an_unknown_build_returns_404(pool: PgPool) {
    let fx = fixture(&pool).await;

    let (status, body) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({ "kind": "manufacturing", "buildId": Uuid::new_v4() }),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "build_not_found");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn manual_ticket_honors_epic_and_assignee_independent_of_source_build(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = manufacturing_build(&fx, 1).await;

    // A real Epic, unrelated to this ticket's own creation.
    let (status, order_body) = post_json(
        &fx.app,
        &format!("/api/builds/{}/orders", build.id.0),
        command_json_for_build(&build),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let order_id = order_body["id"].as_str().unwrap();

    let (status, ticket) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({
            "kind": "manufacturing",
            "buildId": build.id.0,
            "orderId": order_id,
            "notes": "second Rifter run",
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(ticket["orderId"], order_id);
    assert_eq!(ticket["sourceBuildId"], build.id.0.to_string());
    assert_eq!(ticket["notes"], "second Rifter run");

    // Selecting an Epic never creates a requirement fulfillment -- Epic
    // membership is purely organizational for a manually created ticket.
    let fulfillment_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM order_requirement_fulfillments WHERE ticket_id = $1",
    )
    .bind(ticket["id"].as_str().unwrap().parse::<Uuid>().unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(fulfillment_count, 0);
}

// -- Preview -------------------------------------------------------------

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn ticket_plan_preview_matches_what_creation_freezes(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = manufacturing_build(&fx, 1).await;

    let (status, preview) = get_json(
        &fx.app,
        &format!("/api/builds/{}/ticket-preview?runs=3", build.id.0),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["kind"], "manufacturing");
    assert_eq!(preview["runs"], 3);
    assert_eq!(preview["quantity"], 3);
    assert_eq!(preview["typeId"], 5876);
    assert_eq!(preview["capturedName"], "Rifter");

    let (status, ticket) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({ "kind": "manufacturing", "buildId": build.id.0, "runs": 3 }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let enriched = fetch_enriched_ticket(&fx.app, ticket["id"].as_str().unwrap()).await;

    // The preview and the ticket it previewed are two independent
    // `calculate_build_snapshot_with_coverage` calls, so their captured blueprint
    // snapshot's own `id`/`buildId`/`capturedAt` mint fresh each time --
    // compare the planning content, not that synthetic identity.
    assert_eq!(
        normalized_snapshot(preview["executionSnapshot"].clone()),
        normalized_snapshot(enriched["executionSnapshot"].clone())
    );
    assert_eq!(preview["quantity"], ticket["quantity"]);
    assert_eq!(
        prerequisite_quantities(&preview["prerequisites"]),
        prerequisite_quantities(&enriched["prerequisites"])
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn ticket_plan_preview_defaults_runs_to_the_builds_own(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = manufacturing_build(&fx, 4).await;

    let (status, preview) = get_json(
        &fx.app,
        &format!("/api/builds/{}/ticket-preview", build.id.0),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["runs"], 4);
    assert_eq!(preview["quantity"], 4);
}

// -- Frozen after a later Build edit --------------------------------------

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn ticket_snapshot_survives_a_later_build_edit(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = manufacturing_build(&fx, 1).await;

    let (status, ticket) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({ "kind": "manufacturing", "buildId": build.id.0 }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let ticket_id = ticket["id"].as_str().unwrap().to_string();
    let frozen = fetch_enriched_ticket(&fx.app, &ticket_id).await;
    let frozen_snapshot = frozen["executionSnapshot"].clone();
    let frozen_prerequisites = frozen["prerequisites"].clone();
    let frozen_quantity = frozen["quantity"].clone();

    // Edit the Build afterward: bump runs from 1 to 5.
    let (status, _) = put_json(
        &fx.app,
        &format!("/api/builds/{}", build.id.0),
        serde_json::json!({
            "expectedRevision": build.revision,
            "name": "Rifter",
            "recipe": { "mode": "manufacturing", "blueprintTypeId": 6830 },
            "runs": 5,
            "notes": "",
            "draftPlanning": {
                "materialScope": {
                    "regionId": DEFAULT_MARKET_SCOPE.region_id,
                    "locationId": DEFAULT_MARKET_SCOPE.location_id,
                },
                "outputScope": {
                    "regionId": DEFAULT_MARKET_SCOPE.region_id,
                    "locationId": DEFAULT_MARKET_SCOPE.location_id,
                },
                "manualPriceListId": fx.price_list_id.0,
                "expectedManualPriceListRevision": 1,
                "materialPricingPolicy": "highestBuy",
                "outputPricingPolicy": "lowestSell",
                "pricingSelections": [],
                "blueprintSelection": null,
                "manufacturingFacility": null,
                "reactionFacility": null,
                "facilityEivManual": false,
                "componentResolutions": [],
                "fulfillmentScopes": [],
            },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let tickets = list_tickets(&fx.app).await;
    let refetched = ticket_by_id(&tickets, &ticket_id);
    assert_eq!(refetched["quantity"], frozen_quantity, "quantity unchanged");
    assert_eq!(
        refetched["executionSnapshot"], frozen_snapshot,
        "execution snapshot unchanged by the Build edit"
    );
    assert_eq!(
        refetched["prerequisites"], frozen_prerequisites,
        "prerequisites unchanged by the Build edit"
    );
}

// -- record-production compatibility --------------------------------------

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn manual_manufacturing_ticket_records_production_through_the_existing_path(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = manufacturing_build(&fx, 1).await;
    let (status, ticket) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({ "kind": "manufacturing", "buildId": build.id.0 }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let ticket_id = ticket["id"].as_str().unwrap();

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

    let (status, recorded) = post_json(
        &fx.app,
        &format!("/api/tickets/{ticket_id}/record-production"),
        serde_json::json!({
            "idempotencyKey": Uuid::new_v4(),
            "runsCompleted": 1,
            "output": { "typeId": 5876, "quantity": 1 },
            "inputs": [ { "typeId": 34, "quantity": 1000 }, { "typeId": 35, "quantity": 200 } ],
            "installationCost": "0",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body: {recorded}");
    assert_eq!(recorded["recording"]["outputTypeId"], 5876);
    assert_eq!(recorded["recording"]["outputQuantity"], 1);

    let product_qty: Option<i64> = sqlx::query_scalar(
        "SELECT quantity FROM inventory_balances WHERE workspace_id=$1 AND owner_id=$2 AND type_id=5876",
    )
    .bind(fx.workspace_id.0)
    .bind(fx.owner_id.0)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(product_qty, Some(1));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn manual_reaction_ticket_records_production_through_the_existing_path(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = reaction_build(&fx, 1).await;
    let (status, ticket) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({ "kind": "reaction", "buildId": build.id.0 }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let ticket_id = ticket["id"].as_str().unwrap();

    sqlx::query(
        r#"INSERT INTO inventory_balances (workspace_id,owner_id,type_id,captured_name,quantity,total_historical_cost,revision,last_activity_at)
           VALUES ($1,$2,35,'Pyerite',50,500,1,now())"#,
    )
    .bind(fx.workspace_id.0)
    .bind(fx.owner_id.0)
    .execute(&pool)
    .await
    .unwrap();

    let (status, recorded) = post_json(
        &fx.app,
        &format!("/api/tickets/{ticket_id}/record-production"),
        serde_json::json!({
            "idempotencyKey": Uuid::new_v4(),
            "runsCompleted": 1,
            "output": { "typeId": 90_001, "quantity": 1 },
            "inputs": [ { "typeId": 35, "quantity": 50 } ],
            "installationCost": "0",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body: {recorded}");
    assert_eq!(recorded["recording"]["outputTypeId"], 90_001);
    assert_eq!(recorded["recording"]["outputQuantity"], 1);

    let product_qty: Option<i64> = sqlx::query_scalar(
        "SELECT quantity FROM inventory_balances WHERE workspace_id=$1 AND owner_id=$2 AND type_id=90001",
    )
    .bind(fx.workspace_id.0)
    .bind(fx.owner_id.0)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(product_qty, Some(1));
}

// -- Generated vs manual parity --------------------------------------------

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn manual_manufacturing_ticket_matches_the_equivalent_generated_root_ticket(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = manufacturing_build(&fx, 1).await;

    // Generated: `create_order` mints its own canonical root ticket.
    let (status, order_body) = post_json(
        &fx.app,
        &format!("/api/builds/{}/orders", build.id.0),
        command_json_for_build(&build),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let tickets = list_tickets(&fx.app).await;
    let generated = tickets
        .iter()
        .find(|t| t["orderId"] == order_body["id"] && t["kind"] == "manufacturing")
        .expect("generated root ticket");

    // Manual: the same Build, standalone (no Epic).
    let (status, manual) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({ "kind": "manufacturing", "buildId": build.id.0 }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let manual = fetch_enriched_ticket(&fx.app, manual["id"].as_str().unwrap()).await;

    assert_eq!(manual["sourceBuildId"], generated["sourceBuildId"]);
    assert_eq!(manual["typeId"], generated["typeId"]);
    assert_eq!(manual["quantity"], generated["quantity"]);
    // Independently captured snapshots -- compare planning content, not
    // the blueprint snapshot's own synthetic identity (see
    // `normalized_snapshot`).
    assert_eq!(
        normalized_snapshot(manual["executionSnapshot"].clone()),
        normalized_snapshot(generated["executionSnapshot"].clone())
    );
    assert_eq!(
        prerequisite_quantities(&manual["prerequisites"]),
        prerequisite_quantities(&generated["prerequisites"])
    );
    // Provenance differs: only the generated ticket belongs to the Epic.
    assert_eq!(manual["orderId"], Value::Null);
    assert_eq!(generated["orderId"], order_body["id"]);
}
