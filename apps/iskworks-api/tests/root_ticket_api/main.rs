//! Root Manufacturing Ticket / final assembly.
//!
//! Proves that `POST /api/builds/:id/orders` also mints a *canonical root
//! Manufacturing ticket* -- an ordinary `TicketKind::Manufacturing` ticket
//! whose `source_build_id` is the root Build -- and that this ticket can post
//! the finished product through the normal `record-production` path with **no
//! `POST /api/orders/:id/complete` involved** -- Order completion is purely
//! organizational and posts no production output.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use iskworks_api::{build_router, AppState};
use iskworks_core::order::{OrderId, OrderRepository};
use iskworks_core::{
    Build, BuildId, BuildRecipe, CapturedReactionFormula, CapturedRecipe, CapturedRecipeLine,
    ComponentResolution, DraftPlanningInput, DraftPlanningSnapshot, DraftUpdate, FulfillmentScope,
    FulfillmentScopeOverride, IndustryRepository, InventoryRepository, MarketPricingPolicy,
    NewBuild, OwnerId, PriceSourceId, ProductionRepository, RecipeCurrency, RecipeSelection,
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
    /// A manual price list covering every fixture type -- `create_order`
    /// persists a build-plan snapshot whose `captured_source_revision` must
    /// be > 0, and only a manual price list populates that (an ESI scope
    /// leaves it 0). Frozen at revision 1.
    price_list_id: PriceSourceId,
    industry: PgIndustryRepository,
    pool: PgPool,
}

async fn fixture(pool: &PgPool) -> Fixture {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    let workspace_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let import_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Root Ticket Test',$2,$3,$3)",
    )
    .bind(workspace_id).bind(owner_id).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Root Ticket Test',true,$3,$3)",
    )
    .bind(owner_id).bind(workspace_id).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO sde_imports (id,source_version,source_label,source_checksum,status,active,started_at,completed_at) VALUES ($1,'test','fixture',$2,'active',true,$3,$3)",
    )
    .bind(import_id).bind(format!("root-ticket-{}", Uuid::new_v4())).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query(
        r#"INSERT INTO sde_types (import_id,type_id,name_en,published) VALUES
             ($1,34,'Tritanium',true),
             ($1,35,'Pyerite',true),
             ($1,5876,'Rifter',true),
             ($1,6830,'Rifter Blueprint',true),
             ($1,6001,'Antimatter Charge S',true),
             ($1,6002,'Charge Blueprint',true),
             -- Recursive-netting fixture: Assembly (90111) build-resolves
             -- Fabricated Component (90100, mfg from Pyerite) and/or Reacted
             -- Part (90200, rxn from Pyerite), and also uses Tritanium raw.
             ($1,90100,'Fabricated Component',true),
             ($1,90101,'Fabricated Component Blueprint',true),
             ($1,90200,'Reacted Part',true),
             ($1,90201,'Reacted Part Formula',true),
             ($1,90111,'Assembly',true),
             ($1,90110,'Assembly Blueprint',true),
             ($1,90113,'Reacted Assembly',true),
             ($1,90112,'Reacted Assembly Blueprint',true)"#,
    )
    .bind(import_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO sde_blueprints (import_id,blueprint_type_id,name_en,duration_seconds) VALUES
             ($1,6830,'Rifter Blueprint',600),
             ($1,6002,'Charge Blueprint',180),
             ($1,90101,'Fabricated Component Blueprint',120),
             ($1,90110,'Assembly Blueprint',300),
             ($1,90112,'Reacted Assembly Blueprint',300)"#,
    )
    .bind(import_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO sde_blueprint_materials (import_id,blueprint_type_id,material_type_id,quantity,position) VALUES
             ($1,6830,34,1000,0),($1,6830,35,200,1),
             ($1,6002,34,100,0),
             -- Fabricated Component: 60 Pyerite + 100 Tritanium -> 1 unit per run.
             -- (Tritanium is also consumed by the Assembly parent directly --
             -- used to exercise within-one-Epic shared-raw-material planning.)
             ($1,90101,35,60,0),($1,90101,34,100,1),
             -- Assembly: 5 Fabricated Component + 2000 Tritanium raw -> 1 unit per run.
             ($1,90110,90100,5,0),($1,90110,34,2000,1),
             -- Reacted Assembly: 5 Reacted Part + 2000 Tritanium raw -> 1 unit per run.
             ($1,90112,90200,5,0),($1,90112,34,2000,1)"#,
    )
    .bind(import_id).execute(&mut *tx).await.unwrap();
    sqlx::query(
        r#"INSERT INTO sde_blueprint_products (import_id,blueprint_type_id,product_type_id,quantity,position) VALUES
             ($1,6830,5876,1,0),
             ($1,6002,6001,5,0),
             ($1,90101,90100,1,0),
             ($1,90110,90111,1,0),
             ($1,90112,90113,1,0)"#,
    )
    .bind(import_id).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO sde_reaction_formulas (import_id,reaction_formula_type_id,name_en,duration_seconds) VALUES ($1,90201,'Reacted Part Formula',90)",
    )
    .bind(import_id).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO sde_reaction_formula_materials (import_id,reaction_formula_type_id,material_type_id,quantity,position) VALUES ($1,90201,35,40,0)",
    )
    .bind(import_id).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO sde_reaction_formula_products (import_id,reaction_formula_type_id,product_type_id,quantity,position) VALUES ($1,90201,90200,1,0)",
    )
    .bind(import_id).execute(&mut *tx).await.unwrap();

    // A manual price list covering every fixture type (see `Fixture`).
    let price_list_id = PriceSourceId::new();
    sqlx::query(
        "INSERT INTO price_sources (id,workspace_id,display_name,source_kind,revision,created_at,updated_at) VALUES ($1,$2,'Fixture prices','manual',1,$3,$3)",
    )
    .bind(price_list_id.0).bind(workspace_id).bind(now).execute(&mut *tx).await.unwrap();
    for (type_id, name, price) in [
        (34_i64, "Tritanium", "5.0000"),
        (35, "Pyerite", "10.0000"),
        (5876, "Rifter", "500000.0000"),
        (6001, "Antimatter Charge S", "120.0000"),
        (90100, "Fabricated Component", "5000.0000"),
        (90200, "Reacted Part", "5000.0000"),
        (90111, "Assembly", "100000.0000"),
        (90113, "Reacted Assembly", "100000.0000"),
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
        pool: pool.clone(),
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

/// A standalone, manufacturable root Build with `draft_planning` set so
/// `calculate_build_snapshot_with_coverage` (and therefore `create_order`) works.
#[allow(clippy::too_many_arguments)]
async fn manufacturable_build(
    fx: &Fixture,
    name: &str,
    blueprint_type_id: i64,
    blueprint_name: &str,
    product_type_id: i64,
    product_name: &str,
    product_per_run: u64,
    materials: Vec<CapturedRecipeLine>,
    runs: u64,
) -> Build {
    let now = chrono::Utc::now();
    fx.industry
        .create_root_build(NewBuild {
            build: Build {
                id: BuildId::new(),
                workspace_id: fx.workspace_id,
                owner_id: fx.owner_id,
                name: name.to_string(),
                recipe: BuildRecipe::Manufacturing(CapturedRecipe {
                    source_sde_dataset_id: fx.import_id,
                    source_sde_version: "test".to_string(),
                    blueprint_type_id,
                    blueprint_name: blueprint_name.to_string(),
                    duration_seconds_per_run: Some(600),
                    materials,
                    products: vec![CapturedRecipeLine {
                        type_id: product_type_id,
                        type_name: product_name.to_string(),
                        quantity_per_run: product_per_run,
                        sort_order: 0,
                    }],
                    fingerprint: format!("{name}-fixture"),
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

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

/// `POST /api/builds/:id/orders` now requires the live
/// `PreviewBuildPlanCommand` overlay body (matching Materials/Graph/
/// cost-projection) instead of taking no body. Every fixture Build in this
/// file has `draft_planning` set (see `draft_planning`/`manufacturable_build`
/// above), so "the live overlay" and "the saved configuration" are
/// identical here -- this mirrors `build.recipe`/`build.runs`/
/// `build.draft_planning.input` verbatim rather than sending a default/
/// empty command, so the whole-tree freeze sees the same sourcing
/// (component resolutions, facility, pricing) the Build was actually
/// saved with.
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

async fn create_order(app: &axum::Router, build: &Build) -> (StatusCode, Value) {
    let command = command_json_for_build(build);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/builds/{}/orders", build.id.0))
                .header("content-type", "application/json")
                .body(Body::from(command.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = body_json(response).await;
    if status != StatusCode::CREATED {
        eprintln!("create_order failed: {status} {body}");
    }
    (status, body)
}

async fn list_tickets(app: &axum::Router) -> Vec<Value> {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tickets")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    body_json(response)
        .await
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn root_ticket_for(tickets: &[Value], build_id: BuildId) -> &Value {
    tickets
        .iter()
        .find(|t| {
            t["sourceBuildId"] == build_id.0.to_string()
                && (t["kind"] == "manufacturing" || t["kind"] == "reaction")
        })
        .expect("a root manufacturing ticket exists for the build")
}

/// Scoped by explicit Epic membership (`orderId`), not `sourceBuildId` --
/// the only way to tell apart two Orders' independent root tickets when
/// they share a `sourceBuildId`.
fn root_ticket_for_order<'a>(tickets: &'a [Value], order_id: &str) -> &'a Value {
    tickets
        .iter()
        .find(|t| {
            t["orderId"] == order_id && (t["kind"] == "manufacturing" || t["kind"] == "reaction")
        })
        .expect("a root manufacturing ticket exists for the order")
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
    (status, body_json(response).await)
}

mod epic_creation;
mod epic_reservations;
mod inventory_snapshot;
mod recursive_netting;
mod whole_tree_freeze;

// ---------------------------------------------------------------------------
// Inventory snapshot helpers
// ---------------------------------------------------------------------------

async fn allocation_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*)::bigint FROM inventory_allocations")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// `(event_count, sorted [(type_id, quantity, total_historical_cost, revision)])`
/// -- the whole observable inventory state, for a before/after no-side-effect
/// assertion.
async fn inventory_fingerprint(pool: &PgPool) -> (i64, Vec<(i64, i64, i64, i64)>) {
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM inventory_events")
        .fetch_one(pool)
        .await
        .unwrap();
    let balances: Vec<(i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT type_id, quantity, total_historical_cost::bigint, revision \
         FROM inventory_balances ORDER BY type_id",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    (events, balances)
}

/// Seed a raw `inventory_balances` row (whole-ISK carrying cost, so the
/// weighted average is `total_cost / qty`).
fn money_eq(actual: &Value, expected: &str) {
    let a: f64 = actual
        .as_str()
        .unwrap_or_default()
        .parse()
        .unwrap_or(f64::NAN);
    let e: f64 = expected.parse().unwrap();
    assert!(
        (a - e).abs() < 0.01,
        "money mismatch: got {actual}, want {expected}"
    );
}

/// Frozen requirement view for `type_id`, regardless of Buy/Build kind.
fn requirement_of(order_body: &Value, type_id: i64) -> &Value {
    order_body["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["typeId"] == type_id)
        .unwrap_or_else(|| panic!("no requirement for type {type_id}"))
}

async fn rifter(fx: &Fixture) -> Build {
    manufacturable_build(
        fx,
        "Rifter",
        6830,
        "Rifter Blueprint",
        5876,
        "Rifter",
        1,
        rifter_materials(),
        1,
    )
    .await
}

async fn seed_balance(
    pool: &PgPool,
    fx: &Fixture,
    type_id: i64,
    name: &str,
    qty: i64,
    total_cost: i64,
) {
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
    .execute(pool)
    .await
    .unwrap();
}

// ---------------------------------------------------------------------------
// Recursive netting helpers
// ---------------------------------------------------------------------------

/// Create the parent "Assembly" Build (Build-resolving `component_type_id`
/// to its recipe) plus its persisted linked child Build. `component_type_id`
/// is `90100` (child = manufacturing, 60 Pyerite + 100 Tritanium/run) or
/// `90200` (child = reaction, 40 Pyerite/run). `child_full_scoped` marks
/// those type_ids `Full` on the *child's* own planning input. Returns the
/// parent Build; the child is the root plan's producer of the component.
async fn assembly_with_built_component(
    fx: &Fixture,
    parent_runs: u64,
    component_type_id: i64,
    child_full_scoped: &[i64],
) -> Build {
    let now = chrono::Utc::now();
    let (parent_bp, parent_product, parent_name, child_recipe, child_recipe_sel) =
        match component_type_id {
            90100 => (
                90110_i64,
                90111_i64,
                "Assembly",
                BuildRecipe::Manufacturing(CapturedRecipe {
                    source_sde_dataset_id: fx.import_id,
                    source_sde_version: "test".to_string(),
                    blueprint_type_id: 90101,
                    blueprint_name: "Fabricated Component Blueprint".to_string(),
                    duration_seconds_per_run: Some(120),
                    materials: vec![
                        CapturedRecipeLine {
                            type_id: 35,
                            type_name: "Pyerite".to_string(),
                            quantity_per_run: 60,
                            sort_order: 0,
                        },
                        CapturedRecipeLine {
                            type_id: 34,
                            type_name: "Tritanium".to_string(),
                            quantity_per_run: 100,
                            sort_order: 1,
                        },
                    ],
                    products: vec![CapturedRecipeLine {
                        type_id: 90100,
                        type_name: "Fabricated Component".to_string(),
                        quantity_per_run: 1,
                        sort_order: 0,
                    }],
                    fingerprint: "fab-component-fixture".to_string(),
                }),
                RecipeSelection::Manufacturing {
                    blueprint_type_id: 90101,
                },
            ),
            90200 => (
                90112,
                90113,
                "Reacted Assembly",
                BuildRecipe::Reaction(CapturedReactionFormula {
                    source_sde_dataset_id: fx.import_id,
                    source_sde_version: "test".to_string(),
                    reaction_formula_type_id: 90201,
                    reaction_formula_name: "Reacted Part Formula".to_string(),
                    duration_seconds_per_run: Some(90),
                    materials: vec![CapturedRecipeLine {
                        type_id: 35,
                        type_name: "Pyerite".to_string(),
                        quantity_per_run: 40,
                        sort_order: 0,
                    }],
                    products: vec![CapturedRecipeLine {
                        type_id: 90200,
                        type_name: "Reacted Part".to_string(),
                        quantity_per_run: 1,
                        sort_order: 0,
                    }],
                    fingerprint: "reacted-part-fixture".to_string(),
                }),
                RecipeSelection::Reaction {
                    reaction_formula_type_id: 90201,
                },
            ),
            other => panic!("unknown component type {other}"),
        };

    let mut parent_planning = draft_planning(now, fx.price_list_id);
    parent_planning.input.component_resolutions = vec![ComponentResolution {
        type_id: component_type_id,
        recipe: child_recipe_sel,
        facility_override: None,
        blueprint_selection: None,
    }];

    let parent = fx
        .industry
        .create_root_build(NewBuild {
            build: Build {
                id: BuildId::new(),
                workspace_id: fx.workspace_id,
                owner_id: fx.owner_id,
                name: parent_name.to_string(),
                recipe: BuildRecipe::Manufacturing(CapturedRecipe {
                    source_sde_dataset_id: fx.import_id,
                    source_sde_version: "test".to_string(),
                    blueprint_type_id: parent_bp,
                    blueprint_name: format!("{parent_name} Blueprint"),
                    duration_seconds_per_run: Some(300),
                    materials: vec![
                        CapturedRecipeLine {
                            type_id: component_type_id,
                            type_name: if component_type_id == 90100 {
                                "Fabricated Component".to_string()
                            } else {
                                "Reacted Part".to_string()
                            },
                            quantity_per_run: 5,
                            sort_order: 0,
                        },
                        CapturedRecipeLine {
                            type_id: 34,
                            type_name: "Tritanium".to_string(),
                            quantity_per_run: 2000,
                            sort_order: 1,
                        },
                    ],
                    products: vec![CapturedRecipeLine {
                        type_id: parent_product,
                        type_name: parent_name.to_string(),
                        quantity_per_run: 1,
                        sort_order: 0,
                    }],
                    fingerprint: format!("{parent_name}-fixture"),
                }),
                runs: parent_runs,
                notes: String::new(),
                revision: 1,
                created_at: now,
                updated_at: now,
                draft_planning: Some(parent_planning),
                recipe_currency: RecipeCurrency::Current,
                active_sde_version: None,
                product_category_name: None,
                product_group_name: None,
                selected_blueprint_origin: None,
                has_owned_blueprint: false,
            },
        })
        .await
        .unwrap();

    let mut child_planning = draft_planning(now, fx.price_list_id);
    child_planning.input.fulfillment_scopes = child_full_scoped
        .iter()
        .map(|&type_id| FulfillmentScopeOverride {
            type_id,
            scope: FulfillmentScope::Full,
        })
        .collect();
    let child = fx
        .industry
        .create_build(NewBuild {
            build: Build {
                id: BuildId::new(),
                workspace_id: fx.workspace_id,
                owner_id: fx.owner_id,
                name: format!("{parent_name} :: component"),
                recipe: child_recipe,
                runs: 1,
                notes: String::new(),
                revision: 1,
                created_at: now,
                updated_at: now,
                draft_planning: Some(child_planning),
                recipe_currency: RecipeCurrency::Current,
                active_sde_version: None,
                product_category_name: None,
                product_group_name: None,
                selected_blueprint_origin: None,
                has_owned_blueprint: false,
            },
        })
        .await
        .unwrap();
    attach_producer(fx, parent.id, component_type_id, &child, child_full_scoped).await;

    parent
}

/// Insert a live manual manufacturing facility profile.
async fn insert_facility_profile(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    id: Uuid,
    revision: i64,
    material_reduction_percent: &str,
) {
    let now = chrono::Utc::now();
    sqlx::query(
        r#"INSERT INTO industry_facility_profiles
             (id, workspace_id, display_name, facility_kind, security_class, role,
              material_reduction_percent, manual_system_cost_index, revision,
              created_at, updated_at)
           VALUES ($1, $2, 'Home Raitaru', 'manual', 'high_sec', 'manufacturing',
                   $3::numeric, 0.05, $4, $5, $5)"#,
    )
    .bind(id)
    .bind(workspace_id.0)
    .bind(material_reduction_percent)
    .bind(revision)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
}

/// Make `producer` the root plan's producer of `component_type_id` for the
/// root's demand edge, as the canonical sourcing write would: the producer
/// joins the plan with its own Buy edges (`full_scoped` ones Full), and the
/// root's edge becomes Produce by the producer's recipe.
async fn attach_producer(
    fx: &Fixture,
    root: BuildId,
    component_type_id: i64,
    producer: &Build,
    full_scoped: &[i64],
) {
    sqlx::query("UPDATE builds SET plan_root_build_id = $1 WHERE id = $2")
        .bind(root.0)
        .bind(producer.id.0)
        .execute(&fx.pool)
        .await
        .unwrap();
    sqlx::query(
        r#"
        INSERT INTO production_dependencies (
          workspace_id, plan_root_build_id, consumer_build_id, component_type_id,
          sourcing, fulfillment_scope
        )
        SELECT DISTINCT b.workspace_id, $1::uuid, b.id, m.type_id, 'buy',
          CASE WHEN m.type_id = ANY($3) THEN 'full' ELSE 'missing' END
        FROM builds b JOIN build_recipe_materials m ON m.build_id = b.id
        WHERE b.id = $2
        "#,
    )
    .bind(root.0)
    .bind(producer.id.0)
    .bind(full_scoped)
    .execute(&fx.pool)
    .await
    .unwrap();
    let (method_kind, method_type_id) = match &producer.recipe {
        BuildRecipe::Manufacturing(recipe) => ("manufacturing", recipe.blueprint_type_id),
        BuildRecipe::Reaction(formula) => ("reaction", formula.reaction_formula_type_id),
    };
    let updated = sqlx::query(
        r#"
        UPDATE production_dependencies
        SET sourcing = 'produce', method_kind = $1, method_type_id = $2, producer_build_id = $3
        WHERE consumer_build_id = $4 AND component_type_id = $5
        "#,
    )
    .bind(method_kind)
    .bind(method_type_id)
    .bind(producer.id.0)
    .bind(root.0)
    .bind(component_type_id)
    .execute(&fx.pool)
    .await
    .unwrap();
    assert_eq!(
        updated.rows_affected(),
        1,
        "the root has a demand edge for the component"
    );
}
