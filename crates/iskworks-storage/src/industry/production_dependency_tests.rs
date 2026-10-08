//! Persistent `production_dependencies` edges,
//! `plan_root_build_id`, backfill, dual writes, and the bounded root-plan
//! loader -- against a real PostgreSQL database.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::*;
use iskworks_core::production_dependency::{DependencySourcing, ProductionMethod};
use iskworks_core::{
    ComponentResolution, FulfillmentScope, FulfillmentScopeOverride, IndustryService,
    MarketPricingPolicy, NewBuild, OwnerId, RecipeSelection,
};
use iskworks_sde::{ManufacturingRecipe, ReactionFormulaRecipe, RecipeLine};

// ---- fixture helpers ---------------------------------------------------------

struct Fixture {
    pool: PgPool,
    repository: PgIndustryRepository,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    import_id: Uuid,
}

async fn fixture(pool: PgPool) -> Fixture {
    let workspace_id = WorkspaceId(Uuid::new_v4());
    let owner_id = OwnerId(Uuid::new_v4());
    let import_id = Uuid::new_v4();
    let now = Utc::now();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, display_name, owner_id, created_at, updated_at) \
         VALUES ($1, 'Producers', $2, $3, $3)",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO owners (id, workspace_id, owner_kind, display_name, hidden, created_at, updated_at) \
         VALUES ($1, $2, 'manual', 'Producers', false, $3, $3)",
    )
    .bind(owner_id.0)
    .bind(workspace_id.0)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_imports (id, source_version, source_label, source_checksum, status, active, started_at, completed_at) \
         VALUES ($1, 'test', 'fixture', $2, 'active', true, $3, $3)",
    )
    .bind(import_id)
    .bind(format!("producers-{}", Uuid::new_v4()))
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    Fixture {
        repository: PgIndustryRepository::new(pool.clone()),
        pool,
        workspace_id,
        owner_id,
        import_id,
    }
}

fn ln(type_id: i64, name: &str, quantity: i64) -> RecipeLine {
    RecipeLine {
        type_id,
        type_name: name.to_string(),
        quantity,
    }
}

fn rxn_sel(reaction_formula_type_id: i64) -> RecipeSelection {
    RecipeSelection::Reaction {
        reaction_formula_type_id,
    }
}

fn mfg_sel(blueprint_type_id: i64) -> RecipeSelection {
    RecipeSelection::Manufacturing { blueprint_type_id }
}

fn draft(resolutions: &[(i64, RecipeSelection)], full: &[i64]) -> Option<DraftPlanningSnapshot> {
    Some(DraftPlanningSnapshot {
        input: DraftPlanningInput {
            material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            manual_price_list_id: None,
            expected_manual_price_list_revision: None,
            material_pricing_policy: MarketPricingPolicy::HighestBuy,
            output_pricing_policy: MarketPricingPolicy::LowestSell,
            pricing_selections: Vec::new(),
            blueprint_selection: None,
            manufacturing_facility: None,
            reaction_facility: None,
            facility_eiv_manual: false,
            component_resolutions: resolutions
                .iter()
                .map(|(type_id, recipe)| ComponentResolution {
                    type_id: *type_id,
                    recipe: *recipe,
                    facility_override: None,
                    blueprint_selection: None,
                })
                .collect(),
            fulfillment_scopes: full
                .iter()
                .map(|type_id| FulfillmentScopeOverride {
                    type_id: *type_id,
                    scope: FulfillmentScope::Full,
                })
                .collect(),
        },
        updated_at: Utc::now(),
    })
}

impl Fixture {
    fn mfg(&self, blueprint: i64, product: RecipeLine, materials: Vec<RecipeLine>) -> BuildRecipe {
        BuildRecipe::Manufacturing(
            CapturedRecipe::capture(
                self.import_id,
                "test".into(),
                ManufacturingRecipe {
                    blueprint_type_id: blueprint,
                    blueprint_name: format!("{} Blueprint", product.type_name),
                    duration_seconds: Some(60),
                    materials,
                    products: vec![product],
                },
            )
            .unwrap(),
        )
    }

    fn rxn(&self, formula: i64, product: RecipeLine, materials: Vec<RecipeLine>) -> BuildRecipe {
        BuildRecipe::Reaction(
            CapturedReactionFormula::capture(
                self.import_id,
                "test".into(),
                ReactionFormulaRecipe {
                    reaction_formula_type_id: formula,
                    reaction_formula_name: format!("{} Reaction Formula", product.type_name),
                    duration_seconds: Some(3600),
                    materials,
                    products: vec![product],
                },
            )
            .unwrap(),
        )
    }

    /// A new root Build, born a canonical plan (`create_root_build`).
    async fn create_root(
        &self,
        recipe: BuildRecipe,
        draft_planning: Option<DraftPlanningSnapshot>,
    ) -> Build {
        let now = Utc::now();
        self.repository
            .create_root_build(NewBuild {
                build: Build {
                    id: BuildId::new(),
                    workspace_id: self.workspace_id,
                    owner_id: self.owner_id,
                    name: "fixture".into(),
                    recipe,
                    runs: 1,
                    notes: String::new(),
                    revision: 1,
                    created_at: now,
                    updated_at: now,
                    draft_planning,
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

    /// A Build in a canonical plan, seeded the way the canonical write
    /// leaves it: no `parent` -> a new root; `Some((consumer, component))`
    /// -> a producer of `consumer`'s plan. Its own edges follow its draft
    /// (a resolution is Produce with no producer yet, a scope is Full), and
    /// the consumer's edge for `component` references it only when that
    /// edge is Produce by this producer's recipe -- never a mismatched one.
    async fn create(
        &self,
        recipe: BuildRecipe,
        parent: Option<(BuildId, i64)>,
        draft_planning: Option<DraftPlanningSnapshot>,
    ) -> Build {
        let Some((consumer, component)) = parent else {
            let root = self.create_root(recipe, draft_planning).await;
            self.seed_draft_edges(&root).await;
            return root;
        };
        let now = Utc::now();
        let producer = self
            .repository
            .create_build(NewBuild {
                build: Build {
                    id: BuildId::new(),
                    workspace_id: self.workspace_id,
                    owner_id: self.owner_id,
                    name: "fixture".into(),
                    recipe,
                    runs: 1,
                    notes: String::new(),
                    revision: 1,
                    created_at: now,
                    updated_at: now,
                    draft_planning,
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
        let root = self
            .plan_root(consumer)
            .await
            .expect("consumer is in a plan");
        sqlx::query("UPDATE builds SET plan_root_build_id = $1 WHERE id = $2")
            .bind(root.0)
            .bind(producer.id.0)
            .execute(&self.pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO production_dependencies \
               (workspace_id, plan_root_build_id, consumer_build_id, component_type_id, sourcing, fulfillment_scope) \
             SELECT DISTINCT b.workspace_id, $1::uuid, b.id, m.type_id, 'buy', 'missing' \
             FROM builds b JOIN build_recipe_materials m ON m.build_id = b.id WHERE b.id = $2",
        )
        .bind(root.0)
        .bind(producer.id.0)
        .execute(&self.pool)
        .await
        .unwrap();
        self.seed_draft_edges(&producer).await;
        let (kind, method_type_id) = match &producer.recipe {
            BuildRecipe::Manufacturing(recipe) => ("manufacturing", recipe.blueprint_type_id),
            BuildRecipe::Reaction(formula) => ("reaction", formula.reaction_formula_type_id),
        };
        sqlx::query(
            "UPDATE production_dependencies SET producer_build_id = $1 \
             WHERE consumer_build_id = $2 AND component_type_id = $3 \
               AND sourcing = 'produce' AND method_kind = $4 AND method_type_id = $5",
        )
        .bind(producer.id.0)
        .bind(consumer.0)
        .bind(component)
        .bind(kind)
        .bind(method_type_id)
        .execute(&self.pool)
        .await
        .unwrap();
        self.repository
            .get_build(self.workspace_id, producer.id)
            .await
            .unwrap()
    }

    /// Apply `build`'s draft sourcing to its existing (Buy) edges.
    async fn seed_draft_edges(&self, build: &Build) {
        let Some(input) = build.draft_planning.as_ref().map(|d| &d.input) else {
            return;
        };
        for resolution in &input.component_resolutions {
            let (kind, method_type_id) = match resolution.recipe {
                RecipeSelection::Manufacturing { blueprint_type_id } => {
                    ("manufacturing", blueprint_type_id)
                }
                RecipeSelection::Reaction {
                    reaction_formula_type_id,
                } => ("reaction", reaction_formula_type_id),
            };
            sqlx::query(
                "UPDATE production_dependencies SET sourcing = 'produce', method_kind = $1, \
                   method_type_id = $2 WHERE consumer_build_id = $3 AND component_type_id = $4",
            )
            .bind(kind)
            .bind(method_type_id)
            .bind(build.id.0)
            .bind(resolution.type_id)
            .execute(&self.pool)
            .await
            .unwrap();
        }
        for scope in &input.fulfillment_scopes {
            if scope.scope != iskworks_core::FulfillmentScope::Full {
                continue;
            }
            sqlx::query(
                "UPDATE production_dependencies SET fulfillment_scope = 'full' \
                 WHERE consumer_build_id = $1 AND component_type_id = $2",
            )
            .bind(build.id.0)
            .bind(scope.type_id)
            .execute(&self.pool)
            .await
            .unwrap();
        }
    }

    fn service(&self) -> IndustryService {
        IndustryService::new(
            Arc::new(self.repository.clone()),
            Arc::new(PgSdeRepository::new(self.pool.clone())),
        )
    }

    async fn edges(&self) -> BTreeMap<(Uuid, i64), EdgeRow> {
        sqlx::query_as::<_, EdgeRow>(
            "SELECT id, plan_root_build_id, consumer_build_id, component_type_id, sourcing, \
                    method_kind, method_type_id, producer_build_id, fulfillment_scope, revision \
             FROM production_dependencies WHERE workspace_id = $1",
        )
        .bind(self.workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| ((row.consumer_build_id, row.component_type_id), row))
        .collect()
    }

    async fn plan_root(&self, build: BuildId) -> Option<BuildId> {
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT plan_root_build_id FROM builds WHERE id = $1")
            .bind(build.0)
            .fetch_one(&self.pool)
            .await
            .unwrap()
            .map(BuildId)
    }

    async fn edge(&self, consumer: BuildId, component_type_id: i64) -> EdgeRow {
        self.edges()
            .await
            .remove(&(consumer.0, component_type_id))
            .expect("edge exists")
    }
}

#[derive(sqlx::FromRow, Debug, Clone, PartialEq)]
struct EdgeRow {
    id: Uuid,
    plan_root_build_id: Uuid,
    consumer_build_id: Uuid,
    component_type_id: i64,
    sourcing: String,
    method_kind: Option<String>,
    method_type_id: Option<i64>,
    producer_build_id: Option<Uuid>,
    fulfillment_scope: String,
    revision: i64,
}

// Real Muninn type / formula ids.
const PLASMA_THRUSTER: i64 = 11_530;
const DEFLECTION_SHIELD_EMITTER: i64 = 11_555;
const LADAR_SENSOR_CLUSTER: i64 = 11_538;
const FERROGEL: i64 = 16_683;
const FERROGEL_FORMULA: i64 = 46_213;
const FERNITE_CARBIDE: i64 = 16_673;
const FERNITE_FORMULA: i64 = 46_206;
const FUEL: i64 = 4_246;
const HEXITE: i64 = 16_665;
const HYPERFLURITE: i64 = 16_666;
const FERROFLUID: i64 = 16_669;
const PROMETIUM: i64 = 17_960;

struct Muninn {
    root: Build,
    plasma_thruster: Build,
    deflection: Build,
    ladar: Build,
    ferrogel_a: Build,
    ferrogel_b: Build,
    fernite: [Build; 3],
    ferrogel_b_inputs: [Build; 3],
}

/// The profiled Muninn shape: Ferrogel twice (divergent sourcing) and
/// Fernite Carbide three times (identical), all created through the real
/// write path (so dual writes run).
async fn muninn(fx: &Fixture) -> Muninn {
    let root = fx
        .create(
            fx.mfg(
                12_016,
                ln(12_003, "Muninn", 1),
                vec![
                    ln(PLASMA_THRUSTER, "Plasma Thruster", 67),
                    ln(DEFLECTION_SHIELD_EMITTER, "Deflection Shield Emitter", 402),
                    ln(LADAR_SENSOR_CLUSTER, "Ladar Sensor Cluster", 134),
                ],
            ),
            None,
            draft(
                &[
                    (PLASMA_THRUSTER, mfg_sel(17_324)),
                    (DEFLECTION_SHIELD_EMITTER, mfg_sel(17_346)),
                    (LADAR_SENSOR_CLUSTER, mfg_sel(17_325)),
                ],
                &[],
            ),
        )
        .await;
    let component = |bp: i64, product: i64, name: &str, with_ferrogel: bool| {
        let mut materials = vec![ln(FERNITE_CARBIDE, "Fernite Carbide", 13)];
        let mut resolutions = vec![(FERNITE_CARBIDE, rxn_sel(FERNITE_FORMULA))];
        if with_ferrogel {
            materials.push(ln(FERROGEL, "Ferrogel", 1));
            resolutions.push((FERROGEL, rxn_sel(FERROGEL_FORMULA)));
        }
        (
            fx.mfg(bp, ln(product, name, 1), materials),
            draft(&resolutions, &[]),
        )
    };
    let (recipe, d) = component(17_324, PLASMA_THRUSTER, "Plasma Thruster", true);
    let plasma_thruster = fx.create(recipe, Some((root.id, PLASMA_THRUSTER)), d).await;
    let (recipe, d) = component(
        17_346,
        DEFLECTION_SHIELD_EMITTER,
        "Deflection Shield Emitter",
        true,
    );
    let deflection = fx
        .create(recipe, Some((root.id, DEFLECTION_SHIELD_EMITTER)), d)
        .await;
    let (recipe, d) = component(17_325, LADAR_SENSOR_CLUSTER, "Ladar Sensor Cluster", false);
    let ladar = fx
        .create(recipe, Some((root.id, LADAR_SENSOR_CLUSTER)), d)
        .await;

    let ferrogel = || {
        fx.rxn(
            FERROGEL_FORMULA,
            ln(FERROGEL, "Ferrogel", 400),
            vec![
                ln(FUEL, "Hydrogen Fuel Block", 5),
                ln(HEXITE, "Hexite", 100),
                ln(HYPERFLURITE, "Hyperflurite", 100),
                ln(FERROFLUID, "Ferrofluid", 100),
                ln(PROMETIUM, "Prometium", 100),
            ],
        )
    };
    let ferrogel_a = fx
        .create(
            ferrogel(),
            Some((plasma_thruster.id, FERROGEL)),
            draft(&[], &[]),
        )
        .await;
    let ferrogel_b = fx
        .create(
            ferrogel(),
            Some((deflection.id, FERROGEL)),
            draft(
                &[
                    (HEXITE, rxn_sel(46_174)),
                    (FERROFLUID, rxn_sel(46_172)),
                    (PROMETIUM, rxn_sel(46_184)),
                ],
                &[],
            ),
        )
        .await;
    let mut inputs = Vec::new();
    for (formula, type_id, name) in [
        (46_174, HEXITE, "Hexite"),
        (46_172, FERROFLUID, "Ferrofluid"),
        (46_184, PROMETIUM, "Prometium"),
    ] {
        inputs.push(
            fx.create(
                fx.rxn(
                    formula,
                    ln(type_id, name, 200),
                    vec![ln(90_000 + type_id, "Goo", 100)],
                ),
                Some((ferrogel_b.id, type_id)),
                draft(&[], &[]),
            )
            .await,
        );
    }
    let fernite_recipe = || {
        fx.rxn(
            FERNITE_FORMULA,
            ln(FERNITE_CARBIDE, "Fernite Carbide", 10_000),
            vec![
                ln(FUEL, "Hydrogen Fuel Block", 5),
                ln(16_640, "Fernite Alloy", 100),
            ],
        )
    };
    let mut fernite = Vec::new();
    for consumer in [&plasma_thruster, &deflection, &ladar] {
        fernite.push(
            fx.create(
                fernite_recipe(),
                Some((consumer.id, FERNITE_CARBIDE)),
                draft(&[], &[]),
            )
            .await,
        );
    }
    // Reload consumers: creating a child re-derived their edges.
    Muninn {
        root,
        plasma_thruster,
        deflection,
        ladar,
        ferrogel_a,
        ferrogel_b,
        fernite: fernite.try_into().unwrap(),
        ferrogel_b_inputs: inputs.try_into().unwrap(),
    }
}

async fn checksums(pool: &PgPool) -> Vec<(String, Option<String>)> {
    sqlx::query_as(
        r#"
        SELECT 'builds', md5(string_agg(row(id, workspace_id, owner_id, display_name, recipe_kind,
            blueprint_type_id, reaction_formula_type_id, product_type_id, runs, notes, revision,
            updated_at, plan_root_build_id)::text, '|' ORDER BY id)) FROM builds
        UNION ALL SELECT 'drafts', md5(string_agg(row(build_id, planning_input, updated_at)::text,
            '|' ORDER BY build_id)) FROM build_draft_planning
        UNION ALL SELECT 'materials', md5(string_agg(row(build_id, type_id, quantity_per_run,
            sort_order)::text, '|' ORDER BY build_id, sort_order, type_id)) FROM build_recipe_materials
        UNION ALL SELECT 'inventory_events', count(*)::text FROM inventory_events
        UNION ALL SELECT 'inventory_balances', count(*)::text FROM inventory_balances
        "#,
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

// ---- backfill ----------------------------------------------------------------

// ---- loader parity / duplicates / root identity ---------------------------------

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn root_plan_identity_never_crosses_roots(pool: PgPool) {
    let fx = fixture(pool.clone()).await;
    let plan = muninn(&fx).await;
    // A separate root with its own Ferrogel producer.
    let other_root = fx
        .create(
            fx.mfg(
                12_006,
                ln(12_005, "Ishtar", 1),
                vec![ln(FERROGEL, "Ferrogel", 10)],
            ),
            None,
            draft(&[(FERROGEL, rxn_sel(FERROGEL_FORMULA))], &[]),
        )
        .await;
    let other_ferrogel = fx
        .create(
            fx.rxn(
                FERROGEL_FORMULA,
                ln(FERROGEL, "Ferrogel", 400),
                vec![ln(FUEL, "Hydrogen Fuel Block", 5)],
            ),
            Some((other_root.id, FERROGEL)),
            draft(&[], &[]),
        )
        .await;

    assert_eq!(fx.plan_root(plan.root.id).await, Some(plan.root.id));
    for build in [
        &plan.plasma_thruster,
        &plan.ferrogel_a,
        &plan.ferrogel_b,
        &plan.ferrogel_b_inputs[2],
        &plan.fernite[0],
    ] {
        assert_eq!(fx.plan_root(build.id).await, Some(plan.root.id));
    }
    assert_eq!(fx.plan_root(other_root.id).await, Some(other_root.id));
    assert_eq!(fx.plan_root(other_ferrogel.id).await, Some(other_root.id));

    let records = fx
        .repository
        .load_root_plan(fx.workspace_id, plan.root.id)
        .await
        .unwrap();
    assert!(records.producers.iter().all(|b| b.id != other_ferrogel.id));
    let ferrogel_producers = |records: &iskworks_core::production_dependency::RootPlanRecords| {
        records
            .producers
            .iter()
            .filter(|build| build.recipe.primary_product().type_id == FERROGEL)
            .count()
    };
    assert_eq!(
        ferrogel_producers(&records),
        2,
        "Ishtar's Ferrogel is another plan"
    );
    let ishtar = fx
        .repository
        .load_root_plan(fx.workspace_id, other_root.id)
        .await
        .unwrap();
    assert_eq!(ferrogel_producers(&ishtar), 1);

    let not_a_root = fx
        .repository
        .load_root_plan(fx.workspace_id, plan.ferrogel_a.id)
        .await;
    assert!(matches!(not_a_root, Err(IndustryError::Validation(_))));
}

// ---- edge shapes ---------------------------------------------------------------

// ---- dual writes: Build <-> Buy, rollback, revisions --------------------------------

// ---- delete semantics ------------------------------------------------------------

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_referenced_producer_is_refused_orphans_and_consumers_follow_policy(
    pool: PgPool,
) {
    let fx = fixture(pool.clone()).await;
    let plan = muninn(&fx).await;

    // An active producer cannot be deleted.
    let refused = fx
        .repository
        .delete_build(
            fx.workspace_id,
            plan.ferrogel_a.id,
            plan.ferrogel_a.revision,
            false,
        )
        .await;
    match refused {
        Err(IndustryError::ProducerInUse {
            producer,
            consumers,
        }) => {
            assert_eq!(producer, plan.ferrogel_a.id);
            assert_eq!(consumers, vec![plan.plasma_thruster.id]);
        }
        other => panic!("expected ProducerInUse, got {other:?}"),
    }

    // Once its consumer switches Ferrogel to Buy it is an orphan and follows
    // the ordinary deletion policy.
    let pt = fx
        .repository
        .get_build(fx.workspace_id, plan.plasma_thruster.id)
        .await
        .unwrap();
    fx.service()
        .clear_component_resolution(fx.workspace_id, pt.id, FERROGEL, pt.revision)
        .await
        .unwrap();
    fx.repository
        .delete_build(
            fx.workspace_id,
            plan.ferrogel_a.id,
            plan.ferrogel_a.revision,
            false,
        )
        .await
        .expect("an orphaned producer may be deleted");

    // Deleting a consumer removes its own outgoing edges; its producers stay
    // in the plan, detached: switch the root's Ladar row to Buy, then delete
    // Ladar.
    let root = fx
        .repository
        .get_build(fx.workspace_id, plan.root.id)
        .await
        .unwrap();
    fx.service()
        .clear_component_resolution(
            fx.workspace_id,
            root.id,
            LADAR_SENSOR_CLUSTER,
            root.revision,
        )
        .await
        .unwrap();
    fx.repository
        .delete_build(fx.workspace_id, plan.ladar.id, plan.ladar.revision, false)
        .await
        .unwrap();
    let edges = fx.edges().await;
    assert!(edges
        .keys()
        .all(|(consumer, _)| *consumer != plan.ladar.id.0));

    // Deleting the root deletes the whole plan and every edge.
    let root = fx
        .repository
        .get_build(fx.workspace_id, plan.root.id)
        .await
        .unwrap();
    fx.repository
        .delete_build(fx.workspace_id, root.id, root.revision, false)
        .await
        .unwrap();
    assert!(fx.edges().await.is_empty());
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM builds WHERE workspace_id = $1")
        .bind(fx.workspace_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0);
}

// ---- bounded loading ---------------------------------------------------------------

#[derive(Clone, Default)]
struct QueryCounter(Arc<AtomicUsize>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for QueryCounter {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if event.metadata().target() == "sqlx::query" {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// sqlx logs each statement through the `log` crate (target
/// `sqlx::query`); bridge `log` into `tracing` once so a scoped counting
/// subscriber sees them.
fn bridge_log_to_tracing() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = tracing_log::LogTracer::init();
    });
}

async fn count_queries<F: std::future::Future>(future: F) -> (F::Output, usize) {
    use tracing::instrument::WithSubscriber;
    use tracing_subscriber::layer::SubscriberExt;
    bridge_log_to_tracing();
    let counter = QueryCounter::default();
    let subscriber = tracing_subscriber::registry().with(counter.clone());
    let output = future.with_subscriber(subscriber).await;
    (output, counter.0.load(Ordering::SeqCst))
}

/// A root with `size - 1` linked children (a chain, so depth grows too).
async fn chain_plan(fx: &Fixture, size: usize) -> Build {
    let type_for = |depth: usize| 70_000 + i64::try_from(depth).unwrap();
    let recipe_for = |depth: usize| {
        let materials = if depth + 1 < size {
            vec![ln(type_for(depth + 1), "Next", 2), ln(34, "Tritanium", 10)]
        } else {
            vec![ln(34, "Tritanium", 10)]
        };
        fx.mfg(
            80_000 + i64::try_from(depth).unwrap(),
            ln(type_for(depth), "Level", 1),
            materials,
        )
    };
    let resolution_for = |depth: usize| {
        if depth + 1 < size {
            draft(
                &[(
                    type_for(depth + 1),
                    mfg_sel(80_000 + i64::try_from(depth + 1).unwrap()),
                )],
                &[],
            )
        } else {
            draft(&[], &[])
        }
    };
    let root = fx.create(recipe_for(0), None, resolution_for(0)).await;
    let mut parent = root.id;
    for depth in 1..size {
        parent = fx
            .create(
                recipe_for(depth),
                Some((parent, type_for(depth))),
                resolution_for(depth),
            )
            .await
            .id;
    }
    root
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn load_root_plan_issues_a_bounded_number_of_queries_regardless_of_plan_size(pool: PgPool) {
    let fx = fixture(pool.clone()).await;
    let mut persisted_counts = Vec::new();
    for size in [1_usize, 10, 25, 50] {
        let root = chain_plan(&fx, size).await;
        let (records, queries) =
            count_queries(fx.repository.load_root_plan(fx.workspace_id, root.id)).await;
        let records = records.unwrap();
        assert_eq!(records.producers.len(), size - 1);
        assert_eq!(records.dependencies.len(), 2 * size - 1);
        persisted_counts.push(queries);

        let graph = RootPlanDependencyGraphExt::load(&fx, root.id).await;
        assert_eq!(graph.producers().len(), size);
        assert!(graph.validate_acyclic().is_ok());
    }
    assert!(persisted_counts[0] > 0, "the counter observes queries");
    assert!(
        persisted_counts
            .iter()
            .all(|count| *count == persisted_counts[0]),
        "persisted loader query count must not grow with plan size: {persisted_counts:?}"
    );
    eprintln!("load_root_plan queries by size [1,10,25,50]: {persisted_counts:?}");
}

struct RootPlanDependencyGraphExt;

impl RootPlanDependencyGraphExt {
    async fn load(
        fx: &Fixture,
        root: BuildId,
    ) -> iskworks_core::production_dependency::RootPlanDependencyGraph {
        fx.service()
            .persisted_root_plan_dependency_graph(fx.workspace_id, root)
            .await
            .unwrap()
    }
}

// ---- method sanity ---------------------------------------------------------------

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn persisted_rows_decode_to_the_domain_method(pool: PgPool) {
    let fx = fixture(pool.clone()).await;
    let plan = muninn(&fx).await;
    let records = fx
        .repository
        .load_root_plan(fx.workspace_id, plan.root.id)
        .await
        .unwrap();
    let edge = records
        .dependencies
        .iter()
        .find(|e| e.consumer_build_id == plan.deflection.id && e.component_type_id == FERROGEL)
        .unwrap();
    assert_eq!(
        edge.sourcing,
        DependencySourcing::Produce {
            method: ProductionMethod::Reaction {
                reaction_formula_type_id: FERROGEL_FORMULA
            }
        }
    );
    assert_eq!(edge.producer_build_id, Some(plan.ferrogel_b.id));
    assert_eq!(edge.plan_root_build_id, plan.root.id);
    // 3 components + 2 Ferrogels + 3 Ferrogel inputs + 3 Fernite Carbides.
    assert_eq!(records.producers.len(), 11);
}

/// Everything a calculation or the accounting model reads, plus edges.
async fn planner_and_accounting_checksums(pool: &PgPool) -> Vec<(String, Option<String>)> {
    let mut sums = checksums(pool).await;
    sums.extend(
        sqlx::query_as::<_, (String, Option<String>)>(
            r#"
            SELECT 'edges', md5(string_agg(row(id, consumer_build_id, component_type_id, sourcing,
                method_kind, method_type_id, producer_build_id, fulfillment_scope, revision)::text,
                '|' ORDER BY id)) FROM production_dependencies
            UNION ALL SELECT 'plan_roots', md5(string_agg(row(id, plan_root_build_id)::text, '|'
                ORDER BY id)) FROM builds
            UNION ALL SELECT 'orders', count(*)::text FROM orders
            UNION ALL SELECT 'tickets', count(*)::text FROM tickets
            UNION ALL SELECT 'order_requirements', count(*)::text FROM order_requirements
            UNION ALL SELECT 'inventory_allocations', count(*)::text FROM inventory_allocations
            "#,
        )
        .fetch_all(pool)
        .await
        .unwrap(),
    );
    sums
}

// ---- canonical writes -------------------------------------------------------

use iskworks_core::canonical_planner::{
    CanonicalConsumerWrite, CanonicalWriteError, PlannedEdgeWrite, PlannedProducer,
};

/// An SDE fake for the canonical write path's recipe capture (the fixture
/// has no SDE rows): every recipe the canonical-producer fixtures create.
struct CanonicalWriteSde {
    import_id: Uuid,
}

const CW_ROOT_BP: i64 = 64_000;
const CW_A: i64 = 64_010;
const CW_A_BP: i64 = 64_011;
const CW_B: i64 = 64_020;
const CW_B_BP: i64 = 64_021;
const CW_X: i64 = 64_100;
const CW_X_FORMULA: i64 = 64_101;
const CW_X_ALT_BP: i64 = 64_102;
const CW_GOO: i64 = 64_900;

fn s4_recipe(blueprint_type_id: i64) -> Option<ManufacturingRecipe> {
    let (materials, product) = match blueprint_type_id {
        CW_ROOT_BP => (
            vec![ln(CW_A, "A", 1), ln(CW_B, "B", 1)],
            ln(64_001, "Root", 1),
        ),
        // A's recipe also takes its own product and X takes "A": the cycle
        // probes (never Build-resolved unless a test asks for it).
        CW_A_BP => (vec![ln(CW_X, "X", 24)], ln(CW_A, "A", 1)),
        CW_B_BP => (vec![ln(CW_X, "X", 41)], ln(CW_B, "B", 1)),
        CW_X_ALT_BP => (vec![ln(34, "Tritanium", 3)], ln(CW_X, "X", 10)),
        _ => return None,
    };
    Some(ManufacturingRecipe {
        blueprint_type_id,
        blueprint_name: format!("bp {blueprint_type_id}"),
        duration_seconds: Some(60),
        materials,
        products: vec![product],
    })
}

#[async_trait]
impl SdeReadRepository for CanonicalWriteSde {
    async fn active_sde(&self) -> Result<Option<iskworks_sde::ActiveSde>, iskworks_sde::SdeError> {
        Ok(Some(iskworks_sde::ActiveSde {
            import_id: self.import_id,
            source_version: "test".to_string(),
            source_label: "fixture".to_string(),
            source_checksum: "fixture".to_string(),
            completed_at: Utc::now(),
            counts: iskworks_sde::ImportCounts::default(),
        }))
    }
    async fn search_manufacturing_blueprints(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<iskworks_sde::BlueprintSearchResult>, iskworks_sde::SdeError> {
        Ok(Vec::new())
    }
    async fn search_types(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<iskworks_sde::TypeSearchResult>, iskworks_sde::SdeError> {
        Ok(Vec::new())
    }
    async fn manufacturing_recipe(
        &self,
        blueprint_type_id: i64,
    ) -> Result<Option<ManufacturingRecipe>, iskworks_sde::SdeError> {
        Ok(s4_recipe(blueprint_type_id))
    }
    async fn reaction_formula(
        &self,
        reaction_formula_type_id: i64,
    ) -> Result<Option<ReactionFormulaRecipe>, iskworks_sde::SdeError> {
        Ok(
            (reaction_formula_type_id == CW_X_FORMULA).then(|| ReactionFormulaRecipe {
                reaction_formula_type_id,
                reaction_formula_name: "X Reaction Formula".to_string(),
                duration_seconds: Some(3600),
                materials: vec![ln(CW_GOO, "Goo", 100), ln(CW_A, "A", 1)],
                products: vec![ln(CW_X, "X", 10)],
            }),
        )
    }
}

impl Fixture {
    fn canonical_service(&self) -> IndustryService {
        IndustryService::new(
            Arc::new(self.repository.clone()),
            Arc::new(CanonicalWriteSde {
                import_id: self.import_id,
            }),
        )
    }

    async fn plan_build_count(&self, root: BuildId) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM builds WHERE plan_root_build_id = $1")
            .bind(root.0)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn fresh(&self, build: BuildId) -> Build {
        self.repository
            .get_build(self.workspace_id, build)
            .await
            .unwrap()
    }
}

/// Root -> {A -> X (reaction producer), B buying X}; cut over (clean).
struct Shared {
    root: Build,
    a: Build,
    b: Build,
    x: Build,
}

/// Root -> A (Build), B (Build); A makes X by reaction, B buys X. Built the
/// way the app builds a plan: a canonical root, then canonical sourcing
/// writes that create each producer exactly once.
async fn shared_plan(fx: &Fixture) -> Shared {
    let service = fx.canonical_service();
    let root_recipe = service.capture_recipe(&mfg_sel(CW_ROOT_BP)).await.unwrap();
    let root = fx.create_root(root_recipe, draft(&[], &[])).await;
    let mut revision = root.revision;
    for (component, blueprint) in [(CW_A, CW_A_BP), (CW_B, CW_B_BP)] {
        revision = service
            .set_component_resolution(
                fx.workspace_id,
                root.id,
                component,
                mfg_sel(blueprint),
                revision,
            )
            .await
            .unwrap()
            .revision;
    }
    let producer_of = |consumer: BuildId, component: i64| async move {
        BuildId(
            fx.edge(consumer, component)
                .await
                .producer_build_id
                .expect("a produced edge references its producer"),
        )
    };
    let a = producer_of(root.id, CW_A).await;
    let b = producer_of(root.id, CW_B).await;
    let a_revision = fx.fresh(a).await.revision;
    service
        .set_component_resolution(fx.workspace_id, a, CW_X, rxn_sel(CW_X_FORMULA), a_revision)
        .await
        .unwrap();
    let x = producer_of(a, CW_X).await;
    Shared {
        root: fx.fresh(root.id).await,
        a: fx.fresh(a).await,
        b: fx.fresh(b).await,
        x: fx.fresh(x).await,
    }
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn canonical_writes_share_one_producer_detach_reuse_and_replace_methods(pool: PgPool) {
    let fx = fixture(pool.clone()).await;
    let plan = shared_plan(&fx).await;
    let service = fx.canonical_service();
    let builds = fx.plan_build_count(plan.root.id).await;

    // First consumer already references X; second consumer Buy -> Build
    // references the SAME producer (no per-consumer child).
    let b = service
        .set_component_resolution(
            fx.workspace_id,
            plan.b.id,
            CW_X,
            rxn_sel(CW_X_FORMULA),
            plan.b.revision,
        )
        .await
        .unwrap();
    assert_eq!(
        fx.edge(plan.b.id, CW_X).await.producer_build_id,
        Some(plan.x.id.0)
    );
    assert_eq!(fx.plan_build_count(plan.root.id).await, builds);
    let linked = service
        .create_or_reuse_linked_build(fx.workspace_id, fx.owner_id, plan.b.id, CW_X)
        .await
        .unwrap();
    assert_eq!(linked.id, plan.x.id, "the canonical producer, not a clone");
    assert_eq!(fx.plan_build_count(plan.root.id).await, builds);
    assert_eq!(
        fx.fresh(plan.x.id).await.runs,
        plan.x.runs,
        "descendant runs are never resynced"
    );

    // A stale revision is rejected; nothing written.
    let edges = fx.edges().await;
    assert!(matches!(
        service
            .clear_component_resolution(fx.workspace_id, plan.b.id, CW_X, plan.b.revision)
            .await,
        Err(IndustryError::RevisionConflict)
    ));
    assert_eq!(fx.edges().await, edges);

    // Build -> Buy on B: only B's edge; X stays for A.
    service
        .clear_component_resolution(fx.workspace_id, plan.b.id, CW_X, b.revision)
        .await
        .unwrap();
    assert_eq!(fx.edge(plan.b.id, CW_X).await.sourcing, "buy");
    assert_eq!(
        fx.edge(plan.a.id, CW_X).await.producer_build_id,
        Some(plan.x.id.0)
    );

    // Last consumer -> Buy: X detached, never deleted.
    let a = service
        .clear_component_resolution(fx.workspace_id, plan.a.id, CW_X, plan.a.revision)
        .await
        .unwrap();
    let referenced: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM production_dependencies WHERE producer_build_id = $1",
    )
    .bind(plan.x.id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(referenced, 0);
    fx.fresh(plan.x.id).await;

    // Re-enable Build: the detached producer is reused.
    let a = service
        .set_component_resolution(
            fx.workspace_id,
            plan.a.id,
            CW_X,
            rxn_sel(CW_X_FORMULA),
            a.revision,
        )
        .await
        .unwrap();
    assert_eq!(
        fx.edge(plan.a.id, CW_X).await.producer_build_id,
        Some(plan.x.id.0)
    );
    assert_eq!(fx.plan_build_count(plan.root.id).await, builds);

    // Method change: a new producer of the new identity is created exactly
    // once; the reaction producer is never re-recipe'd.
    let x_before = fx.fresh(plan.x.id).await;
    let a = service
        .set_component_resolution(
            fx.workspace_id,
            plan.a.id,
            CW_X,
            mfg_sel(CW_X_ALT_BP),
            a.revision,
        )
        .await
        .unwrap();
    let new_producer = fx.edge(plan.a.id, CW_X).await.producer_build_id.unwrap();
    assert_ne!(new_producer, plan.x.id.0);
    assert_eq!(fx.plan_build_count(plan.root.id).await, builds + 1);
    let created = fx.fresh(BuildId(new_producer)).await;
    assert_eq!(created.recipe.blueprint_type_id(), Some(CW_X_ALT_BP));
    assert_eq!(fx.plan_root(created.id).await, Some(plan.root.id));
    assert_eq!(
        fx.edge(created.id, 34).await.sourcing,
        "buy",
        "its own requirements start as Buy"
    );
    let x_after = fx.fresh(plan.x.id).await;
    assert_eq!(x_after.recipe, x_before.recipe);
    assert_eq!(x_after.revision, x_before.revision);
    // Never listed as a root.
    let roots = fx.repository.list_builds(fx.workspace_id).await.unwrap();
    assert_eq!(
        roots.iter().map(|b| b.id).collect::<Vec<_>>(),
        vec![plan.root.id]
    );

    // B switching to the same alternative method reuses it (once).
    let b = fx.fresh(plan.b.id).await;
    service
        .set_component_resolution(
            fx.workspace_id,
            plan.b.id,
            CW_X,
            mfg_sel(CW_X_ALT_BP),
            b.revision,
        )
        .await
        .unwrap();
    assert_eq!(
        fx.edge(plan.b.id, CW_X).await.producer_build_id,
        Some(new_producer)
    );
    assert_eq!(fx.plan_build_count(plan.root.id).await, builds + 1);
    let _ = a;
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn cycles_stale_plans_and_legacy_writes_are_rejected_transactionally(pool: PgPool) {
    let fx = fixture(pool.clone()).await;
    let plan = shared_plan(&fx).await;
    let service = fx.canonical_service();

    // Indirect cycle: X's own requirement "A" produced by reusing A would
    // close A -> X -> A. Rejected; nothing written.
    let before = planner_and_accounting_checksums(&pool).await;
    let x = fx.fresh(plan.x.id).await;
    assert!(matches!(
        service
            .set_component_resolution(
                fx.workspace_id,
                plan.x.id,
                CW_A,
                mfg_sel(CW_A_BP),
                x.revision
            )
            .await,
        Err(IndustryError::CanonicalWrite(
            CanonicalWriteError::WouldCreateCycle { .. }
        ))
    ));
    assert_eq!(planner_and_accounting_checksums(&pool).await, before);

    // The in-transaction guard catches a cycle even when a write bypasses
    // the planner: the whole write rolls back.
    let records = fx
        .repository
        .load_root_plan(fx.workspace_id, plan.root.id)
        .await
        .unwrap();
    let edge = fx.edge(plan.x.id, CW_A).await;
    let forged = CanonicalConsumerWrite {
        root: plan.root.id,
        expected_plan_state: iskworks_core::plan_state::plan_state(&records),
        consumer: plan.x.id,
        update: DraftUpdate {
            expected_revision: x.revision,
            name: x.name.clone(),
            runs: x.runs,
            notes: x.notes.clone(),
            replacement_recipe: None,
            draft_planning: x.draft_planning.clone(),
        },
        new_producers: Vec::new(),
        edge_writes: vec![PlannedEdgeWrite {
            dependency_id: edge.id,
            component_type_id: CW_A,
            sourcing: DependencySourcing::Produce {
                method: ProductionMethod::Manufacturing {
                    blueprint_type_id: CW_A_BP,
                },
            },
            producer: PlannedProducer::Existing {
                producer: plan.a.id,
            },
            fulfillment_scope: FulfillmentScope::Missing,
            previous_producer: None,
            previous_producer_still_referenced: false,
        }],
    };
    assert!(matches!(
        fx.repository
            .apply_canonical_consumer_write(fx.workspace_id, forged.clone())
            .await,
        Err(IndustryError::CanonicalWrite(
            CanonicalWriteError::CanonicalGraphCorrupt { .. }
        ))
    ));
    assert_eq!(planner_and_accounting_checksums(&pool).await, before);

    // A write planned against a plan that changed since: PlanChanged.
    let a = fx.fresh(plan.a.id).await;
    fx.repository
        .update_draft_planning_batch(
            fx.workspace_id,
            vec![DraftPlanningBatchUpdate {
                build_id: a.id,
                expected_revision: a.revision,
                input: a.draft_planning.clone().unwrap().input,
            }],
        )
        .await
        .unwrap();
    let before = planner_and_accounting_checksums(&pool).await;
    let mut stale = forged;
    stale.edge_writes.clear();
    assert!(matches!(
        fx.repository
            .apply_canonical_consumer_write(fx.workspace_id, stale)
            .await,
        Err(IndustryError::CanonicalWrite(
            CanonicalWriteError::PlanChanged
        ))
    ));
    assert_eq!(planner_and_accounting_checksums(&pool).await, before);

    // The plain repository `update_draft` cannot change a plan's sourcing...
    let b = fx.fresh(plan.b.id).await;
    let mut input = b.draft_planning.clone().unwrap().input;
    input.component_resolutions.push(ComponentResolution {
        type_id: CW_X,
        recipe: rxn_sel(CW_X_FORMULA),
        facility_override: None,
        blueprint_selection: None,
    });
    assert!(matches!(
        fx.repository
            .update_draft(
                fx.workspace_id,
                b.id,
                DraftUpdate {
                    expected_revision: b.revision,
                    name: b.name.clone(),
                    runs: b.runs,
                    notes: b.notes.clone(),
                    replacement_recipe: None,
                    draft_planning: Some(DraftPlanningSnapshot {
                        input,
                        updated_at: Utc::now(),
                    }),
                },
            )
            .await,
        Err(IndustryError::CanonicalWrite(
            CanonicalWriteError::CanonicalWriteRequired { .. }
        ))
    ));
    // A non-sourcing edit (rename/notes) still works and re-derives nothing.
    let edges = fx.edges().await;
    fx.repository
        .update_draft(
            fx.workspace_id,
            b.id,
            DraftUpdate {
                expected_revision: b.revision,
                name: "renamed".into(),
                runs: b.runs,
                notes: "note".into(),
                replacement_recipe: None,
                draft_planning: b.draft_planning.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(fx.edges().await, edges);

    // The database refuses an edge to a retired producer.
    sqlx::query(
        "INSERT INTO canonical_planner_cutovers (workspace_id, plan_root_build_id, \
         plan_state_token, applied_decisions, retargeted_edges, cut_over_at) \
         VALUES ($1, $2, 'x', '[]', '[]', now()) ON CONFLICT DO NOTHING",
    )
    .bind(fx.workspace_id.0)
    .bind(plan.root.id.0)
    .execute(&pool)
    .await
    .ok();
    let detached = fx.create_detached_producer(plan.root.id, &plan.x).await;
    sqlx::query(
        "INSERT INTO retired_producers (producer_build_id, workspace_id, plan_root_build_id, \
         cutover_id, retired_at) SELECT $1, $2, $3, id, now() FROM canonical_planner_cutovers \
         WHERE plan_root_build_id = $3 LIMIT 1",
    )
    .bind(detached.0)
    .bind(fx.workspace_id.0)
    .bind(plan.root.id.0)
    .execute(&pool)
    .await
    .unwrap();
    let error = sqlx::query(
        "UPDATE production_dependencies SET producer_build_id = $1 \
         WHERE consumer_build_id = $2 AND component_type_id = $3",
    )
    .bind(detached.0)
    .bind(plan.a.id.0)
    .bind(CW_X)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(error.to_string().contains("retired producer"), "{error}");
}

impl Fixture {
    /// A second X producer in `root`'s plan with no incoming edge (a
    /// detached duplicate) -- inserted directly, since no canonical write
    /// produces one.
    async fn create_detached_producer(&self, root: BuildId, like: &Build) -> BuildId {
        let id = BuildId::new();
        sqlx::query(
            "INSERT INTO builds (id, workspace_id, owner_id, display_name, recipe_kind, \
             reaction_formula_type_id, reaction_formula_name, product_type_id, product_name, \
             product_quantity_per_run, source_sde_dataset_id, source_sde_version, \
             recipe_fingerprint, runs, notes, revision, created_at, updated_at, plan_root_build_id) \
             SELECT $1, workspace_id, owner_id, display_name, recipe_kind, reaction_formula_type_id, \
             reaction_formula_name, product_type_id, product_name, product_quantity_per_run, \
             source_sde_dataset_id, source_sde_version, recipe_fingerprint, runs, notes, 1, now(), \
             now(), $2 FROM builds WHERE id = $3",
        )
        .bind(id.0)
        .bind(root.0)
        .bind(like.id.0)
        .execute(&self.pool)
        .await
        .unwrap();
        id
    }
}
