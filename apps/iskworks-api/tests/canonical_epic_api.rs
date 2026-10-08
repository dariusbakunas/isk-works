//! Epic freeze for a canonical operation that
//! serves several demand requirements, end to end through the HTTP routes
//! against a real Postgres database.
//!
//! Topology (97_1xx SDE range, inserted by the fixture):
//! Root -> {A, B}; A needs 20 X, B needs 30 X; X yields 10/run from 5
//! Tritanium/run. Built through the canonical sourcing writes, so the
//! planner sees ONE X producer serving two demand edges.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use iskworks_api::{build_router, AppState};
use iskworks_core::{
    Build, BuildId, BuildRecipe, ComponentResolution, DraftPlanningInput, DraftPlanningSnapshot,
    FacilityPreviewCommand, FacilityProfileId, IndustryRepository, InventoryRepository,
    MarketPricingPolicy, OwnerId, PriceSourceId, ProductionRepository, RecipeSelection,
    WorkspaceId, WorkspaceRepository, DEFAULT_MARKET_SCOPE,
};
use iskworks_sde::SdeReadRepository;
use iskworks_storage::{
    PgIndustryRepository, PgInventoryRepository, PgOrderRepository, PgProductionRepository,
    PgSdeRepository, PgWorkspaceRepository,
};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const ROOT_BP: i64 = 97_100;
const ROOT_PRODUCT: i64 = 97_101;
const A: i64 = 97_110;
const A_BP: i64 = 97_111;
const B: i64 = 97_120;
const B_BP: i64 = 97_121;
const X: i64 = 97_130;
const X_BP: i64 = 97_131;
const TRITANIUM: i64 = 34;

struct Fixture {
    app: axum::Router,
    pool: PgPool,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    price_list_id: PriceSourceId,
    facility_id: Uuid,
    industry: PgIndustryRepository,
}

struct FixtureAdjustedPrices;

#[async_trait]
impl iskworks_core::AdjustedPriceRepository for FixtureAdjustedPrices {
    async fn latest_adjusted_prices(
        &self,
        type_ids: &[i64],
        _as_of: chrono::DateTime<chrono::Utc>,
    ) -> Result<BTreeMap<i64, Decimal>, iskworks_core::InventoryError> {
        Ok(type_ids
            .iter()
            .map(|type_id| (*type_id, Decimal::from(10)))
            .collect())
    }
}

async fn fixture(pool: &PgPool) -> Fixture {
    let workspace_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let import_id = Uuid::new_v4();
    let price_list_id = PriceSourceId::new();
    let facility_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Canonical Epic Test',$2,$3,$3)",
    )
    .bind(workspace_id).bind(owner_id).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Canonical Epic Test',true,$3,$3)",
    )
    .bind(owner_id).bind(workspace_id).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO sde_imports (id,source_version,source_label,source_checksum,status,active,started_at,completed_at) VALUES ($1,'test','fixture',$2,'active',true,$3,$3)",
    )
    .bind(import_id).bind(format!("canonical-epic-{}", Uuid::new_v4())).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query(
        r#"INSERT INTO sde_types (import_id,type_id,name_en,published) VALUES
             ($1,34,'Tritanium',true),
             ($1,97100,'Root Blueprint',true),($1,97101,'Root Product',true),
             ($1,97110,'A Product',true),($1,97111,'A Blueprint',true),
             ($1,97120,'B Product',true),($1,97121,'B Blueprint',true),
             ($1,97130,'X',true),($1,97131,'X Blueprint',true)"#,
    )
    .bind(import_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO sde_blueprints (import_id,blueprint_type_id,name_en,duration_seconds) VALUES
             ($1,97100,'Root Blueprint',600),($1,97111,'A Blueprint',300),
             ($1,97121,'B Blueprint',300),($1,97131,'X Blueprint',300)"#,
    )
    .bind(import_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO sde_blueprint_materials (import_id,blueprint_type_id,material_type_id,quantity,position) VALUES
             ($1,97100,97110,1,0),($1,97100,97120,1,1),
             ($1,97111,97130,20,0),($1,97121,97130,30,0),
             ($1,97131,34,5,0)"#,
    )
    .bind(import_id).execute(&mut *tx).await.unwrap();
    sqlx::query(
        r#"INSERT INTO sde_blueprint_products (import_id,blueprint_type_id,product_type_id,quantity,position) VALUES
             ($1,97100,97101,1,0),($1,97111,97110,1,0),
             ($1,97121,97120,1,0),($1,97131,97130,10,0)"#,
    )
    .bind(import_id).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO price_sources (id,workspace_id,display_name,source_kind,revision,created_at,updated_at) VALUES ($1,$2,'Fixture prices','manual',1,$3,$3)",
    )
    .bind(price_list_id.0).bind(workspace_id).bind(now).execute(&mut *tx).await.unwrap();
    for (type_id, name, price) in [
        (TRITANIUM, "Tritanium", "5.0000"),
        (ROOT_PRODUCT, "Root Product", "1000000.0000"),
    ] {
        sqlx::query(
            "INSERT INTO price_source_items (price_source_id,type_id,captured_name,price,updated_at) VALUES ($1,$2,$3,$4::numeric,$5)",
        )
        .bind(price_list_id.0).bind(type_id).bind(name).bind(price).bind(now).execute(&mut *tx).await.unwrap();
    }
    sqlx::query(
        r#"INSERT INTO industry_facility_profiles
             (id, workspace_id, display_name, facility_kind, security_class, role,
              material_reduction_percent, manual_system_cost_index, revision,
              created_at, updated_at)
           VALUES ($1, $2, 'Home Raitaru', 'manual', 'high_sec', 'manufacturing',
                   0, 0.05, 1, $3, $3)"#,
    )
    .bind(facility_id)
    .bind(workspace_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let app = build_router(
        AppState::new(
            Arc::new(PgWorkspaceRepository::new(pool.clone())) as Arc<dyn WorkspaceRepository>
        )
        .with_sde_repository(
            Arc::new(PgSdeRepository::new(pool.clone())) as Arc<dyn SdeReadRepository>
        )
        .with_industry_repository(
            Arc::new(PgIndustryRepository::new(pool.clone())) as Arc<dyn IndustryRepository>
        )
        .with_inventory_repository(
            Arc::new(PgInventoryRepository::new(pool.clone())) as Arc<dyn InventoryRepository>
        )
        .with_production_repository(
            Arc::new(PgProductionRepository::new(pool.clone())) as Arc<dyn ProductionRepository>
        )
        .with_order_repository(Arc::new(PgOrderRepository::new(pool.clone())))
        .with_adjusted_price_repository(Arc::new(FixtureAdjustedPrices)),
    );

    Fixture {
        app,
        pool: pool.clone(),
        workspace_id: WorkspaceId(workspace_id),
        owner_id: OwnerId(owner_id),
        price_list_id,
        facility_id,
        industry: PgIndustryRepository::new(pool.clone()),
    }
}

impl Fixture {
    fn draft(&self, resolutions: &[(i64, i64)]) -> DraftPlanningSnapshot {
        DraftPlanningSnapshot {
            input: DraftPlanningInput {
                material_scope: DEFAULT_MARKET_SCOPE,
                output_scope: DEFAULT_MARKET_SCOPE,
                manual_price_list_id: Some(self.price_list_id),
                expected_manual_price_list_revision: Some(1),
                material_pricing_policy: MarketPricingPolicy::HighestBuy,
                output_pricing_policy: MarketPricingPolicy::LowestSell,
                pricing_selections: Vec::new(),
                blueprint_selection: None,
                manufacturing_facility: Some(FacilityPreviewCommand {
                    facility_profile_id: FacilityProfileId(self.facility_id),
                    blueprint_me: 0,
                    blueprint_te: 0,
                    estimated_item_value: None,
                }),
                reaction_facility: None,
                facility_eiv_manual: false,
                component_resolutions: resolutions
                    .iter()
                    .map(|&(type_id, blueprint_type_id)| ComponentResolution {
                        type_id,
                        recipe: RecipeSelection::Manufacturing { blueprint_type_id },
                        facility_override: None,
                        blueprint_selection: None,
                    })
                    .collect(),
                fulfillment_scopes: Vec::new(),
            },
            updated_at: chrono::Utc::now(),
        }
    }

    async fn fresh(&self, id: BuildId) -> Build {
        self.industry
            .get_build(self.workspace_id, id)
            .await
            .unwrap()
    }

    async fn plan_root(&self, build: BuildId) -> BuildId {
        BuildId(
            sqlx::query_scalar::<_, Uuid>("SELECT plan_root_build_id FROM builds WHERE id = $1")
                .bind(build.0)
                .fetch_one(&self.pool)
                .await
                .unwrap(),
        )
    }
}

struct Plan {
    root: Build,
}

/// Root -> A (20 X), B (30 X), both built, both building X -- one shared X
/// producer. Built the way a user builds it: create the root, then switch
/// each component to Build through the canonical sourcing write.
async fn canonical_plan(fx: &Fixture) -> Plan {
    let (status, created) = send(
        &fx.app,
        "POST",
        "/api/builds",
        Some(serde_json::json!({
            "name": "Root",
            "recipe": { "mode": "manufacturing", "blueprintTypeId": ROOT_BP },
            "runs": 1,
            "draftPlanning": fx.draft(&[]).input,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let root = BuildId(Uuid::parse_str(created["id"].as_str().unwrap()).unwrap());
    let mut producers = Vec::new();
    for (component, blueprint) in [(A, A_BP), (B, B_BP)] {
        let (status, producer) = produce_edge(fx, root, component, blueprint).await;
        assert!(status.is_success(), "{status} {producer}");
        producers.push(BuildId(
            Uuid::parse_str(producer["id"].as_str().unwrap()).unwrap(),
        ));
    }
    let mut x_producers = std::collections::BTreeSet::new();
    for consumer in producers {
        let (status, x) = produce_edge(fx, consumer, X, X_BP).await;
        assert!(status.is_success(), "{status} {x}");
        x_producers.insert(x["id"].as_str().unwrap().to_string());
    }
    assert_eq!(x_producers.len(), 1, "A and B share one X producer");
    assert_eq!(fx.plan_root(root).await, root);
    Plan {
        root: fx.fresh(root).await,
    }
}

// ---- HTTP helpers ----------------------------------------------------------

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

async fn send(
    app: &axum::Router,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let builder = Request::builder().method(method).uri(path);
    let request = match body {
        Some(body) => builder
            .header("content-type", "application/json")
            .body(Body::from(body.to_string())),
        None => builder.body(Body::empty()),
    }
    .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    (status, body_json(response).await)
}

fn command_json(build: &Build) -> Value {
    let BuildRecipe::Manufacturing(recipe) = &build.recipe else {
        panic!("manufacturing fixture");
    };
    let draft = &build.draft_planning.as_ref().unwrap().input;
    serde_json::json!({
        "recipe": { "mode": "manufacturing", "blueprintTypeId": recipe.blueprint_type_id },
        "runs": build.runs,
        "materialScope": draft.material_scope,
        "outputScope": draft.output_scope,
        "manualPriceListId": draft.manual_price_list_id.map(|id| id.0),
        "expectedManualPriceListRevision": draft.expected_manual_price_list_revision,
        "pricingSelections": draft.pricing_selections,
        "blueprintSelection": draft.blueprint_selection,
        "manufacturingFacility": draft.manufacturing_facility,
        "reactionFacility": draft.reaction_facility,
        "componentResolutions": draft.component_resolutions,
        "fulfillmentScopes": draft.fulfillment_scopes,
        "buildId": build.id.0,
    })
}

async fn create_epic(fx: &Fixture, root: &Build) -> (StatusCode, Value) {
    send(
        &fx.app,
        "POST",
        &format!("/api/builds/{}/orders", root.id.0),
        Some(command_json(root)),
    )
    .await
}

async fn order_tickets(fx: &Fixture, order_id: &str) -> Vec<Value> {
    let (status, body) = send(&fx.app, "GET", "/api/tickets", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body.as_array()
        .unwrap()
        .iter()
        .filter(|ticket| ticket["orderId"] == order_id)
        .cloned()
        .collect()
}

fn plan_operation(order: &Value, type_id: i64) -> Vec<&Value> {
    order["productionPlan"]["operations"]
        .as_array()
        .expect("productionPlan.operations")
        .iter()
        .filter(|op| op["productTypeId"] == type_id)
        .collect()
}

fn requirements_of(order: &Value, type_id: i64) -> Vec<&Value> {
    order["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["typeId"] == type_id)
        .collect()
}

fn dec(value: &Value) -> Decimal {
    value
        .as_str()
        .unwrap_or_else(|| panic!("money string, got {value}"))
        .parse()
        .unwrap()
}

async fn seed_balance(fx: &Fixture, type_id: i64, name: &str, qty: i64, total_cost: i64) {
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
    .execute(&fx.pool)
    .await
    .unwrap();
}

async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

/// Exact conservation from the persisted rows: for every frozen producer
/// with a known cost, `total = sum(consumer shares) + retained surplus`.
async fn assert_persisted_conservation(pool: &PgPool, order_id: Uuid) {
    type Row = (String, Option<Decimal>, Option<Decimal>, Option<Decimal>);
    let rows: Vec<Row> = sqlx::query_as(
        r#"SELECT op.occurrence_key, op.total_production_cost, op.surplus_retained_basis,
                  (SELECT sum(r.child_consumed_cost) FROM order_requirements r
                    WHERE r.order_id = op.order_id AND r.child_occurrence_key = op.occurrence_key)
             FROM order_plan_operations op
            WHERE op.order_id = $1 AND op.occurrence_key NOT LIKE 'root:%'"#,
    )
    .bind(order_id)
    .fetch_all(pool)
    .await
    .unwrap();
    assert!(!rows.is_empty());
    for (key, total, retained, shares) in rows {
        let total = total.unwrap_or_else(|| panic!("{key} has a complete frozen cost"));
        assert_eq!(
            shares.unwrap() + retained.unwrap(),
            total,
            "{key}: sum(shares) + retained == total"
        );
    }
}

/// The detail read re-derives the create response's frozen plan exactly
/// (timestamps and money scale aside: the create response carries the
/// in-memory values, the read the stored ones).
fn assert_same_plan(detail: &Value, created: &Value) {
    fn strip(value: &Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.iter()
                    .filter(|(key, _)| key.as_str() != "createdAt")
                    .map(|(key, value)| (key.clone(), strip(value)))
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.iter().map(strip).collect()),
            // Money compares by value: Postgres reads a stored zero back
            // without its scale ("0" for "0.0000").
            Value::String(text) => match text.parse::<Decimal>() {
                Ok(number) => Value::String(number.normalize().to_string()),
                Err(_) => value.clone(),
            },
            other => other.clone(),
        }
    }
    let (detail, created) = (
        strip(&detail["productionPlan"]),
        strip(&created["productionPlan"]),
    );
    assert_eq!(
        detail,
        created,
        "detail:\n{}\ncreated:\n{}",
        serde_json::to_string_pretty(&detail).unwrap(),
        serde_json::to_string_pretty(&created).unwrap()
    );
}

// ---- Tests -----------------------------------------------------------------

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_canonical_epic_freezes_one_shared_operation_with_one_ticket(pool: PgPool) {
    let fx = fixture(&pool).await;
    let plan = canonical_plan(&fx).await;

    let (status, order) = create_epic(&fx, &plan.root).await;
    assert_eq!(status, StatusCode::CREATED, "{order}");
    assert_eq!(order["planningSnapshotVersion"], 3);
    let order_id = order["id"].as_str().unwrap().to_string();

    // Operation level: X frozen ONCE -- 5 runs, 50 out, 50 consumed.
    let operations = order["productionPlan"]["operations"].as_array().unwrap();
    assert_eq!(operations.len(), 4, "root, A, B, X: {order}");
    let x_ops = plan_operation(&order, X);
    assert_eq!(
        x_ops.len(),
        1,
        "one ProductionOperation, never one per consumer"
    );
    let x = x_ops[0];
    assert_eq!(x["runs"], 5);
    assert_eq!(x["producedQuantity"], 50);
    assert_eq!(x["consumedQuantity"], 50);
    assert_eq!(x["surplusQuantity"], 0);
    assert!(
        x["parentOccurrenceKey"].is_null(),
        "fan-in keeps no arbitrary parent"
    );
    assert_eq!(x["stage"], 0);
    assert_eq!(x["servedRequirementIds"].as_array().unwrap().len(), 2);
    let root_op = operations.last().unwrap();
    assert!(root_op["occurrenceKey"]
        .as_str()
        .unwrap()
        .starts_with("root:"));
    assert_eq!(root_op["stage"], 2);
    assert_eq!(
        order["productionPlan"]["dependencies"]
            .as_array()
            .unwrap()
            .len(),
        4
    );

    // Requirement level: two X requirements, one per demand edge, sharing
    // X's occurrence; each with its own consumed share and dependency id.
    let x_requirements = requirements_of(&order, X);
    assert_eq!(x_requirements.len(), 2);
    let mut consumed: Vec<u64> = Vec::new();
    for requirement in &x_requirements {
        assert_eq!(requirement["childOccurrenceKey"], x["occurrenceKey"]);
        assert!(requirement["dependencyId"]
            .as_str()
            .unwrap()
            .starts_with("pd:"));
        assert!(!requirement["childConsumedCost"].is_null());
        consumed.push(requirement["childConsumedQuantity"].as_u64().unwrap());
    }
    consumed.sort_unstable();
    assert_eq!(consumed, vec![20, 30]);
    let shares: Decimal = x_requirements
        .iter()
        .map(|r| dec(&r["childConsumedCost"]))
        .sum();
    assert_eq!(
        shares + dec(&x["surplusRetainedBasis"]),
        dec(&x["totalProductionCost"]),
        "exact conservation"
    );

    // ONE production ticket per operation.
    let tickets = order_tickets(&fx, &order_id).await;
    assert_eq!(tickets.len(), 4, "{tickets:?}");
    let x_tickets: Vec<&Value> = tickets.iter().filter(|t| t["typeId"] == X).collect();
    assert_eq!(x_tickets.len(), 1);
    assert_eq!(x_tickets[0]["quantity"], 50);
    assert_eq!(x_tickets[0]["occurrenceKey"], x["occurrenceKey"]);
    assert!(x_tickets[0]["parentTicketId"].is_null());
    assert_eq!(x["ticketId"], x_tickets[0]["id"]);
    let root_ticket = tickets
        .iter()
        .find(|t| t["occurrenceKey"].as_str().unwrap().starts_with("root:"))
        .unwrap();
    let a_ticket = tickets.iter().find(|t| t["typeId"] == A).unwrap();
    assert_eq!(a_ticket["parentTicketId"], root_ticket["id"]);
    assert!(!root_ticket["executionSnapshot"].is_null());

    let order_uuid: Uuid = order_id.parse().unwrap();
    assert_persisted_conservation(&pool, order_uuid).await;
    assert_eq!(
        count(&pool, "SELECT count(*) FROM order_requirement_fulfillments").await,
        0,
        "no status-bearing producer links"
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM inventory_allocations").await,
        0
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM inventory_events").await,
        0
    );

    // The detail read re-derives the same plan.
    let (status, detail) = send(&fx.app, "GET", &format!("/api/orders/{order_id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_same_plan(&detail, &order);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_canonical_epic_nets_x_inventory_on_its_edges_before_sizing_the_operation(pool: PgPool) {
    let fx = fixture(&pool).await;
    let plan = canonical_plan(&fx).await;
    seed_balance(&fx, X, "X", 15, 45).await;

    let (status, order) = create_epic(&fx, &plan.root).await;
    assert_eq!(status, StatusCode::CREATED, "{order}");
    let x = plan_operation(&order, X)[0];
    assert_eq!(x["runs"], 4, "35 short -> 4 runs");
    assert_eq!(x["producedQuantity"], 40);
    assert_eq!(x["consumedQuantity"], 35);
    assert_eq!(x["surplusQuantity"], 5);

    let x_requirements = requirements_of(&order, X);
    let reused: u64 = x_requirements
        .iter()
        .map(|r| r["reusedQuantity"].as_u64().unwrap())
        .sum();
    assert_eq!(reused, 15, "the one inventory snapshot, drawn once");
    for requirement in &x_requirements {
        assert_eq!(
            requirement["reusedQuantity"].as_u64().unwrap()
                + requirement["childConsumedQuantity"].as_u64().unwrap(),
            requirement["requiredQuantity"].as_u64().unwrap()
        );
    }
    assert_persisted_conservation(&pool, order["id"].as_str().unwrap().parse().unwrap()).await;
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn delayed_tickets_use_frozen_evidence_and_reuse_the_one_operation_ticket(pool: PgPool) {
    let fx = fixture(&pool).await;
    let plan = canonical_plan(&fx).await;
    let (status, order) = create_epic(&fx, &plan.root).await;
    assert_eq!(status, StatusCode::CREATED, "{order}");
    let order_id = order["id"].as_str().unwrap().to_string();
    let x_ticket_id = plan_operation(&order, X)[0]["ticketId"].clone();
    let tickets_before = order_tickets(&fx, &order_id).await.len();

    // Every requirement X serves resolves to the SAME operation ticket --
    // no per-consumer ticket, no status-bearing fulfillment link.
    for requirement in requirements_of(&order, X) {
        let (status, ticket) = send(
            &fx.app,
            "POST",
            &format!(
                "/api/orders/{order_id}/requirements/{}/tickets",
                requirement["id"].as_str().unwrap()
            ),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{ticket}");
        assert_eq!(ticket["id"], x_ticket_id);
    }
    assert_eq!(order_tickets(&fx, &order_id).await.len(), tickets_before);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM order_requirement_fulfillments").await,
        0
    );

    // Live prices and inventory change after the freeze...
    sqlx::query("UPDATE price_source_items SET price = 999 WHERE type_id = 34")
        .execute(&pool)
        .await
        .unwrap();
    seed_balance(&fx, TRITANIUM, "Tritanium", 1_000_000, 1_000_000).await;

    // ...the delayed Acquisition ticket still uses the frozen quantity/cost.
    let tritanium = requirements_of(&order, TRITANIUM)[0];
    assert_eq!(tritanium["freshQuantity"], 25, "5 runs x 5");
    let (status, acquisition) = send(
        &fx.app,
        "POST",
        &format!(
            "/api/orders/{order_id}/requirements/{}/tickets",
            tritanium["id"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{acquisition}");
    assert_eq!(acquisition["kind"], "acquisition");
    assert_eq!(acquisition["quantity"], 25);
    assert_eq!(
        dec(&acquisition["estimatedLineTotal"]),
        dec(&tritanium["estimatedLineTotal"]),
        "frozen 25 x 5, never re-priced or re-netted"
    );
    assert_eq!(dec(&acquisition["estimatedLineTotal"]), Decimal::from(125));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn workflow_status_has_no_side_effects_and_record_production_posts_once(pool: PgPool) {
    let fx = fixture(&pool).await;
    let plan = canonical_plan(&fx).await;
    let (status, order) = create_epic(&fx, &plan.root).await;
    assert_eq!(status, StatusCode::CREATED, "{order}");
    let order_id = order["id"].as_str().unwrap().to_string();
    let x_ticket = plan_operation(&order, X)[0]["ticketId"]
        .as_str()
        .unwrap()
        .to_string();

    // Completing the operation's ticket by workflow status consumes nothing,
    // satisfies nothing, and posts nothing.
    let (status, body) = send(
        &fx.app,
        "POST",
        &format!("/api/tickets/{x_ticket}/start"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = send(
        &fx.app,
        "POST",
        &format!("/api/tickets/{x_ticket}/complete"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        count(&pool, "SELECT count(*) FROM inventory_events").await,
        0
    );
    let (_, detail) = send(&fx.app, "GET", &format!("/api/orders/{order_id}"), None).await;
    for requirement in requirements_of(&detail, X) {
        assert_ne!(
            requirement["state"], "satisfied",
            "status never satisfies a frozen requirement: {requirement}"
        );
        assert_eq!(requirement["state"], "needsAction", "{requirement}");
    }

    // Recording the one physical operation posts its output exactly once.
    seed_balance(&fx, TRITANIUM, "Tritanium", 25, 125).await;
    let record = serde_json::json!({
        "idempotencyKey": Uuid::new_v4().to_string(),
        "runsCompleted": 5,
        "output": { "typeId": X, "quantity": 50 },
        "inputs": [ { "typeId": TRITANIUM, "quantity": 25 } ],
        "installationCost": "0",
    });
    let path = format!("/api/tickets/{x_ticket}/record-production");
    let (status, first) = send(&fx.app, "POST", &path, Some(record.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let (status, replay) = send(&fx.app, "POST", &path, Some(record)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["recording"]["id"], first["recording"]["id"]);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM inventory_events WHERE event_kind = 'production_output'"
        )
        .await,
        1,
        "one operation, one output posting -- not one per consumer"
    );
    let x_on_hand: i64 = sqlx::query_scalar(
        "SELECT quantity FROM inventory_balances WHERE workspace_id = $1 AND type_id = $2",
    )
    .bind(fx.workspace_id.0)
    .bind(X)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(x_on_hand, 50);
}

/// A direct Plan ticket for one canonical production
/// operation -- the existing standalone Build-backed production ticket,
/// created for the operation's producer Build at the Plan's projected runs.
/// A shared producer gets ONE ticket (not one per consumer); creating it
/// writes no inventory, and Create Epic still freezes version 3 afterwards.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_direct_plan_ticket_is_one_standalone_ticket_per_operation(pool: PgPool) {
    let fx = fixture(&pool).await;
    let plan = canonical_plan(&fx).await;

    let (status, stages) = send(
        &fx.app,
        "POST",
        &format!("/api/builds/{}/execution-plan", plan.root.id.0),
        Some(command_json(&plan.root)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{stages}");
    let x_nodes: Vec<&Value> = stages["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|node| node["outputTypeId"] == X)
        .collect();
    assert_eq!(x_nodes.len(), 1);
    let x_node = x_nodes[0];
    assert_eq!(x_node["consumers"].as_array().unwrap().len(), 2);
    let occurrence = stages["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|occurrence| occurrence["id"] == x_node["occurrenceIds"][0])
        .unwrap();
    assert_eq!(x_node["projectedRuns"], 5);

    // The Plan response also carries Logistics: every operation runs at
    // the one fixture facility, so there is one destination.
    let destinations = stages["logistics"]["destinations"].as_array().unwrap();
    assert_eq!(
        destinations.len(),
        1,
        "every operation runs at Home Raitaru"
    );
    assert_eq!(destinations[0]["facilityName"], "Home Raitaru");

    let (status, ticket) = send(
        &fx.app,
        "POST",
        "/api/tickets",
        Some(serde_json::json!({
            "kind": "manufacturing",
            "buildId": occurrence["buildId"],
            "runs": x_node["projectedRuns"],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{ticket}");
    assert_eq!(ticket["kind"], "manufacturing");
    assert_eq!(ticket["typeId"], X);
    assert_eq!(ticket["quantity"], 50, "5 runs x 10, the projected output");
    assert_eq!(ticket["sourceBuildId"], occurrence["buildId"]);
    assert!(
        ticket["orderId"].is_null(),
        "standalone -- no Epic required"
    );
    assert!(ticket["occurrenceKey"].is_null(), "not an Epic snapshot");
    assert_eq!(
        ticket["executionSnapshot"]["facility"]["id"],
        fx.facility_id.to_string(),
        "the producer's facility, captured at creation"
    );
    assert_eq!(ticket["executionSnapshot"]["runs"], 5);

    let x_tickets: i64 = sqlx::query_scalar("SELECT count(*) FROM tickets WHERE type_id = $1")
        .bind(X)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(x_tickets, 1, "one ticket for the one operation");
    assert_eq!(
        count(&pool, "SELECT count(*) FROM inventory_events").await,
        0
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM inventory_allocations").await,
        0
    );

    // Create Epic from the same plan is untouched by the direct ticket.
    let (status, order) = create_epic(&fx, &plan.root).await;
    assert_eq!(status, StatusCode::CREATED, "{order}");
    assert_eq!(order["planningSnapshotVersion"], 3);
    assert_eq!(plan_operation(&order, X).len(), 1);
    assert_eq!(
        order_tickets(&fx, order["id"].as_str().unwrap())
            .await
            .len(),
        4
    );
    assert_persisted_conservation(&pool, order["id"].as_str().unwrap().parse().unwrap()).await;
}

// ---- Worksheet capability parity -------------------------------------------

async fn execution_plan(fx: &Fixture, root: &Build) -> Value {
    let (status, body) = send(
        &fx.app,
        "POST",
        &format!("/api/builds/{}/execution-plan", root.id.0),
        Some(command_json(root)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

async fn build_economics(fx: &Fixture, root: &Build) -> Value {
    let (status, body) = send(
        &fx.app,
        "POST",
        "/api/build-plans/candidate-preview",
        Some(command_json(root)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["worksheet"]["summary"].clone()
}

fn x_producer(plan: &Value) -> (&Value, &Value) {
    let nodes: Vec<&Value> = plan["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|node| node["outputTypeId"] == X)
        .collect();
    assert_eq!(nodes.len(), 1, "one canonical X producer");
    let occurrence = plan["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|occurrence| occurrence["id"] == nodes[0]["occurrenceIds"][0])
        .unwrap();
    (nodes[0], occurrence)
}

/// Where Logistics sends `type_id`, and how many units.
fn logistics_for(plan: &Value, type_id: i64) -> Vec<(String, i64)> {
    let mut found = Vec::new();
    for destination in plan["logistics"]["destinations"].as_array().unwrap() {
        let name = destination["facilityName"]
            .as_str()
            .unwrap_or("")
            .to_string();
        for line in destination["lines"].as_array().into_iter().flatten() {
            if line["typeId"] == type_id {
                found.push((name.clone(), line["quantity"].as_i64().unwrap()));
            }
        }
    }
    found
}

async fn patch_producer(fx: &Fixture, root: &Build, producer: &Value, body: Value) {
    let mut request = body;
    request["command"] = command_json(root);
    request["members"] = serde_json::json!([{
        "buildId": producer["buildId"],
        "expectedRevision": producer["revision"],
    }]);
    let (status, response) = send(
        &fx.app,
        "PATCH",
        &format!(
            "/api/builds/{}/descendant-production-configuration",
            root.id.0
        ),
        Some(request),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
}

/// The producer inspector's facility and ME/TE edits (the Worksheet's
/// linked-Build settings, re-homed in Plan) go through the existing
/// descendant-configuration write, and the re-projected Plan, Logistics and
/// Build economics all move: a facility change moves the producer's
/// Logistics destination and its installation (folded into the Build's
/// material/total/margin); an ME change moves input quantities, Logistics
/// and cost. The shared producer stays ONE producer throughout.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn producer_facility_and_me_edits_refresh_plan_logistics_and_economics(pool: PgPool) {
    let fx = fixture(&pool).await;
    let plan = canonical_plan(&fx).await;
    let far_facility = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO industry_facility_profiles
             (id, workspace_id, display_name, facility_kind, security_class, role,
              material_reduction_percent, manual_system_cost_index, revision,
              created_at, updated_at)
           VALUES ($1, $2, 'Far Azbel', 'manual', 'high_sec', 'manufacturing',
                   0, 0.10, 1, now(), now())"#,
    )
    .bind(far_facility)
    .bind(fx.workspace_id.0)
    .execute(&pool)
    .await
    .unwrap();

    // Baseline: X at Home Raitaru, 5 runs x 5 Tritanium = 25.
    let before = execution_plan(&fx, &plan.root).await;
    let (x_node, x_occurrence) = x_producer(&before);
    assert_eq!(x_node["facilityName"], "Home Raitaru");
    assert_eq!(x_node["consumers"].as_array().unwrap().len(), 2);
    assert_eq!(
        logistics_for(&before, TRITANIUM),
        vec![("Home Raitaru".to_string(), 25)]
    );
    let install_before = dec(&x_node["ownInstallationCost"]);
    let economics_before = build_economics(&fx, &plan.root).await;

    // Facility: Home Raitaru -> Far Azbel (twice the cost index).
    patch_producer(
        &fx,
        &plan.root,
        x_occurrence,
        serde_json::json!({ "kind": "facility", "facilityProfileId": far_facility }),
    )
    .await;
    let after_facility = execution_plan(&fx, &plan.root).await;
    let (x_node, x_occurrence) = x_producer(&after_facility);
    assert_eq!(
        x_node["facilityName"], "Far Azbel",
        "Plan shows the new facility"
    );
    assert_eq!(
        x_node["consumers"].as_array().unwrap().len(),
        2,
        "still ONE producer serving both edges -- no duplicate"
    );
    assert_eq!(
        logistics_for(&after_facility, TRITANIUM),
        vec![("Far Azbel".to_string(), 25)],
        "X's inputs now go to the new facility"
    );
    assert!(
        logistics_for(&after_facility, X)
            .iter()
            .all(|(facility, _)| facility == "Home Raitaru"),
        "X itself still goes to its consumers' facility"
    );
    let install_after = dec(&x_node["ownInstallationCost"]);
    assert_eq!(
        install_after,
        install_before * Decimal::from(2),
        "installation follows the facility's cost index"
    );
    let economics_facility = build_economics(&fx, &plan.root).await;
    let delta = install_after - install_before;
    assert_eq!(
        dec(&economics_facility["totalCost"]) - dec(&economics_before["totalCost"]),
        delta,
        "the Build total moves by exactly the producer's installation change"
    );
    assert_eq!(
        dec(&economics_before["estimatedMargin"]) - dec(&economics_facility["estimatedMargin"]),
        delta,
        "and profit by the same amount"
    );

    // ME 0 -> 10: X needs ceil(25 x 0.9) = 23 Tritanium.
    patch_producer(
        &fx,
        &plan.root,
        x_occurrence,
        serde_json::json!({
            "kind": "blueprintSelection",
            "blueprintSelection": {
                "mode": "manual", "kind": "original",
                "materialEfficiency": 10, "timeEfficiency": 0,
                "licensedRuns": null, "notes": ""
            }
        }),
    )
    .await;
    let after_me = execution_plan(&fx, &plan.root).await;
    let (x_node, _) = x_producer(&after_me);
    assert_eq!(x_node["effectiveMe"], 10);
    let trit = after_me["acquisitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|line| line["typeId"] == TRITANIUM)
        .unwrap();
    assert_eq!(trit["requiredQuantity"], 23, "ME updates quantities");
    assert_eq!(
        logistics_for(&after_me, TRITANIUM),
        vec![("Far Azbel".to_string(), 23)],
        "ME updates Logistics"
    );
    let economics_me = build_economics(&fx, &plan.root).await;
    assert_eq!(
        dec(&economics_facility["totalCost"]) - dec(&economics_me["totalCost"]),
        Decimal::from(10),
        "2 fewer Tritanium at 5 ISK -- ME updates Build economics"
    );
}

// ---- One planner ----------------------------------------------------------

async fn build_json(fx: &Fixture, id: BuildId) -> Value {
    let (status, body) = send(&fx.app, "GET", &format!("/api/builds/{}", id.0), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

/// Plan sourcing Buy -> Produce on one demand edge, exactly as the Plan
/// inspector does it (persist the resolution, then create-or-reuse).
async fn produce_edge(
    fx: &Fixture,
    consumer: BuildId,
    component: i64,
    blueprint: i64,
) -> (StatusCode, Value) {
    let revision = build_json(fx, consumer).await["revision"].clone();
    let (status, body) = send(
        &fx.app,
        "POST",
        &format!("/api/builds/{}/component-resolutions", consumer.0),
        Some(serde_json::json!({
            "componentTypeId": component,
            "recipe": { "mode": "manufacturing", "blueprintTypeId": blueprint },
            "expectedRevision": revision,
        })),
    )
    .await;
    if !status.is_success() {
        return (status, body);
    }
    send(
        &fx.app,
        "POST",
        &format!("/api/builds/{}/linked-builds", consumer.0),
        Some(serde_json::json!({ "componentTypeId": component })),
    )
    .await
}

/// A brand-new Build starts on shared producers; nested sourcing
/// works immediately and never creates a consumer-private producer.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_new_build_starts_on_shared_producers(pool: PgPool) {
    let fx = fixture(&pool).await;
    let (status, created) = send(
        &fx.app,
        "POST",
        "/api/builds",
        Some(serde_json::json!({
            "name": "Fresh Root",
            "recipe": { "mode": "manufacturing", "blueprintTypeId": ROOT_BP },
            "runs": 1,
            "draftPlanning": fx.draft(&[]).input,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let root_id = BuildId(Uuid::parse_str(created["id"].as_str().unwrap()).unwrap());
    assert_eq!(fx.plan_root(root_id).await, root_id, "a root owns its plan");
    assert_eq!(
        created["revision"],
        build_json(&fx, root_id).await["revision"]
    );

    // Root edge A -> Produce, then A's X -> Produce.
    let (status, a) = produce_edge(&fx, root_id, A, A_BP).await;
    assert!(status.is_success(), "{status} {a}");
    let a_id = BuildId(Uuid::parse_str(a["id"].as_str().unwrap()).unwrap());
    let (status, x) = produce_edge(&fx, a_id, X, X_BP).await;
    assert!(status.is_success(), "{status} {x}");
    let x_id = BuildId(Uuid::parse_str(x["id"].as_str().unwrap()).unwrap());
    assert_eq!(
        fx.plan_root(x_id).await,
        root_id,
        "a shared producer of the root's plan, not private to A"
    );
}

/// A Build created with sourcing already in its draft starts with demand
/// edges that match it: Produce for a resolved component, the scope it was
/// given for the rest. The canonical write path, not a SQL sync, writes them.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_new_build_created_with_sourcing_starts_with_matching_edges(pool: PgPool) {
    let fx = fixture(&pool).await;
    let mut draft = fx.draft(&[(A, A_BP)]).input;
    draft.fulfillment_scopes = vec![iskworks_core::FulfillmentScopeOverride {
        type_id: B,
        scope: iskworks_core::FulfillmentScope::Full,
    }];
    let (status, created) = send(
        &fx.app,
        "POST",
        "/api/builds",
        Some(serde_json::json!({
            "name": "Sourced Root",
            "recipe": { "mode": "manufacturing", "blueprintTypeId": ROOT_BP },
            "runs": 1,
            "draftPlanning": draft,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let root_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    let edges: Vec<(i64, String, Option<i64>, String)> = sqlx::query_as(
        "SELECT component_type_id, sourcing, method_type_id, fulfillment_scope \
         FROM production_dependencies WHERE consumer_build_id = $1 ORDER BY component_type_id",
    )
    .bind(root_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        edges,
        vec![
            (A, "produce".to_string(), Some(A_BP), "missing".to_string()),
            (B, "buy".to_string(), None, "full".to_string()),
        ]
    );

    // Its producer resolves through the canonical path.
    let (status, producer) = send(
        &fx.app,
        "POST",
        &format!("/api/builds/{root_id}/linked-builds"),
        Some(serde_json::json!({ "componentTypeId": A })),
    )
    .await;
    assert!(status.is_success(), "{status} {producer}");
    assert_eq!(producer["recipe"]["products"][0]["typeId"], A);
}

/// A manual Build-backed ticket freezes each prerequisite's sourcing. For a
/// component produced by a shared producer (no parent link -- canonical
/// writes never create one), it must still resolve to `build` with that
/// producer's id, not fall back to `build` with no source Build.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_manual_ticket_freezes_a_shared_producers_prerequisite_with_its_build(pool: PgPool) {
    let fx = fixture(&pool).await;
    let (status, created) = send(
        &fx.app,
        "POST",
        "/api/builds",
        Some(serde_json::json!({
            "name": "Fresh Root",
            "recipe": { "mode": "manufacturing", "blueprintTypeId": ROOT_BP },
            "runs": 1,
            "draftPlanning": fx.draft(&[]).input,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let root_id = BuildId(Uuid::parse_str(created["id"].as_str().unwrap()).unwrap());
    let (status, a) = produce_edge(&fx, root_id, A, A_BP).await;
    assert!(status.is_success(), "{status} {a}");
    let a_id = BuildId(Uuid::parse_str(a["id"].as_str().unwrap()).unwrap());
    assert_eq!(
        fx.plan_root(a_id).await,
        root_id,
        "a producer of the root's plan"
    );
    // A's X is produced from priced Tritanium, so A's cost is complete.
    let (status, x) = produce_edge(&fx, a_id, X, X_BP).await;
    assert!(status.is_success(), "{status} {x}");

    let (status, ticket) = send(
        &fx.app,
        "POST",
        "/api/tickets",
        Some(serde_json::json!({
            "kind": "manufacturing",
            "buildId": root_id.0,
            "runs": 1,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{ticket}");
    let (status, tickets) = send(&fx.app, "GET", "/api/tickets", None).await;
    assert_eq!(status, StatusCode::OK);
    let enriched = tickets
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == ticket["id"])
        .unwrap();
    let prerequisites = enriched["prerequisites"]
        .as_array()
        .unwrap_or_else(|| panic!("no prerequisites on {enriched}"));
    let a_prerequisite = prerequisites
        .iter()
        .find(|p| p["typeId"] == A)
        .unwrap_or_else(|| panic!("no prerequisite for A in {ticket}"));
    assert_eq!(a_prerequisite["kind"], "build", "{a_prerequisite}");
    assert_eq!(a_prerequisite["sourceBuildId"], a["id"], "{a_prerequisite}");
    let (status, epic) = create_epic(&fx, &fx.fresh(root_id).await).await;
    assert_eq!(status, StatusCode::CREATED, "{epic}");
    // Priced by the plan's cost projection: the produced component costs
    // what its producer's share costs -- the same figure the Epic freezes.
    assert!(
        !a_prerequisite["estimatedLineTotal"].is_null(),
        "a produced prerequisite is priced: {a_prerequisite}"
    );
    let epic_a = requirements_of(&epic, A);
    assert_eq!(epic_a.len(), 1, "{epic}");
    assert_eq!(
        dec(&a_prerequisite["estimatedLineTotal"]),
        dec(&epic_a[0]["estimatedLineTotal"]),
        "manual ticket {a_prerequisite} vs Epic {}",
        epic_a[0]
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_ticket_creates_for_one_requirement_yield_one_ticket(pool: PgPool) {
    let fx = fixture(&pool).await;
    let plan = canonical_plan(&fx).await;
    let (status, order) = create_epic(&fx, &plan.root).await;
    assert_eq!(status, StatusCode::CREATED, "{order}");
    let order_id = order["id"].as_str().unwrap().to_string();
    let tritanium = requirements_of(&order, TRITANIUM)[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let tickets_before = order_tickets(&fx, &order_id).await.len();

    // A double-click on "create ticket" racing a bulk create of the same
    // requirement.
    let single_uri = format!("/api/orders/{order_id}/requirements/{tritanium}/tickets");
    let bulk_uri = format!("/api/orders/{order_id}/tickets/bulk");
    let bulk_body = serde_json::json!({ "requirementIds": [tritanium] });
    let (first, second, bulk) = tokio::join!(
        send(&fx.app, "POST", &single_uri, None),
        send(&fx.app, "POST", &single_uri, None),
        send(&fx.app, "POST", &bulk_uri, Some(bulk_body)),
    );
    assert_eq!(first.0, StatusCode::CREATED, "{}", first.1);
    assert_eq!(second.0, StatusCode::CREATED, "{}", second.1);
    assert_eq!(bulk.0, StatusCode::OK, "{}", bulk.1);
    assert_eq!(
        first.1["id"], second.1["id"],
        "the loser gets the winner's ticket"
    );
    assert_eq!(bulk.1[0]["id"], first.1["id"]);

    assert_eq!(
        order_tickets(&fx, &order_id).await.len(),
        tickets_before + 1
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM order_requirement_fulfillments").await,
        1
    );
}
