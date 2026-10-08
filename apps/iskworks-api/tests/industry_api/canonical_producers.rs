use super::*;
use iskworks_core::production_dependency::PersistedProductionDependency;

// ---- SDE fixture (97_xxx) ------------------------------------------

struct CanonicalSde;

fn ln(type_id: i64, name: &str, quantity: i64) -> RecipeLine {
    RecipeLine {
        type_id,
        type_name: name.to_string(),
        quantity,
    }
}

const ROOT_BP: i64 = 97_000;
const A_PRODUCT: i64 = 97_010;
const A_BP: i64 = 97_011;
const B_PRODUCT: i64 = 97_020;
const B_BP: i64 = 97_021;
const X: i64 = 97_100;
const X_BP: i64 = 97_101;
const X_ALT_BP: i64 = 97_102;
const Y: i64 = 97_200;
const Y_BP: i64 = 97_201;
const RAW_Y: i64 = 97_900;
const RAW_X: i64 = 97_910;
const F: i64 = 97_300;
const F_FORMULA: i64 = 97_301;
const RAW_F: i64 = 97_920;
const ROOT2_BP: i64 = 97_050;
const C_PRODUCT: i64 = 97_030;
const C_BP: i64 = 97_031;
const D_PRODUCT: i64 = 97_040;
const D_BP: i64 = 97_041;
const ROOT3_BP: i64 = 97_070;
const E_PRODUCT: i64 = 97_060;
const E_BP: i64 = 97_061;
const ROOT4_BP: i64 = 97_080;
const G_PRODUCT: i64 = 97_085;
const G_BP: i64 = 97_086;

fn sde_recipe(blueprint_type_id: i64) -> Option<(Vec<RecipeLine>, RecipeLine)> {
    Some(match blueprint_type_id {
        ROOT_BP => (
            vec![ln(A_PRODUCT, "A Product", 1), ln(B_PRODUCT, "B Product", 1)],
            ln(97_001, "Root Product", 1),
        ),
        A_BP => (vec![ln(X, "X", 24)], ln(A_PRODUCT, "A Product", 1)),
        B_BP => (vec![ln(X, "X", 41)], ln(B_PRODUCT, "B Product", 1)),
        X_BP => (vec![ln(Y, "Y", 2)], ln(X, "X", 10)),
        X_ALT_BP => (vec![ln(RAW_X, "Raw X", 3)], ln(X, "X", 10)),
        Y_BP => (vec![ln(RAW_Y, "Raw Y", 1)], ln(Y, "Y", 1)),
        ROOT2_BP => (
            vec![ln(C_PRODUCT, "C Product", 1), ln(D_PRODUCT, "D Product", 1)],
            ln(97_051, "Root2 Product", 1),
        ),
        C_BP => (vec![ln(F, "F", 90)], ln(C_PRODUCT, "C Product", 1)),
        D_BP => (vec![ln(F, "F", 90)], ln(D_PRODUCT, "D Product", 1)),
        ROOT3_BP => (
            vec![ln(A_PRODUCT, "A Product", 1), ln(E_PRODUCT, "E Product", 1)],
            ln(97_071, "Root3 Product", 1),
        ),
        E_BP => (vec![ln(X, "X", 7)], ln(E_PRODUCT, "E Product", 1)),
        ROOT4_BP => (
            vec![ln(A_PRODUCT, "A Product", 1), ln(G_PRODUCT, "G Product", 1)],
            ln(97_081, "Root4 Product", 1),
        ),
        G_BP => (
            vec![ln(B_PRODUCT, "B Product", 1)],
            ln(G_PRODUCT, "G Product", 1),
        ),
        _ => return None,
    })
}

#[async_trait]
impl SdeReadRepository for CanonicalSde {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        Ok(Some(ActiveSde {
            import_id: uuid::Uuid::nil(),
            source_version: "test".to_string(),
            source_label: "fixture.zip".to_string(),
            source_checksum: "abc123".to_string(),
            completed_at: chrono::Utc::now(),
            counts: iskworks_sde::ImportCounts::default(),
        }))
    }
    async fn search_manufacturing_blueprints(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<BlueprintSearchResult>, SdeError> {
        Ok(Vec::new())
    }
    async fn search_types(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        Ok(Vec::new())
    }
    async fn manufacturing_recipe(
        &self,
        blueprint_type_id: i64,
    ) -> Result<Option<ManufacturingRecipe>, SdeError> {
        Ok(
            sde_recipe(blueprint_type_id).map(|(materials, product)| ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: format!("bp {blueprint_type_id}"),
                duration_seconds: Some(300),
                materials,
                products: vec![product],
            }),
        )
    }
    async fn manufacturing_blueprint_for_product(
        &self,
        product_type_id: i64,
    ) -> Result<Option<i64>, SdeError> {
        Ok([
            ROOT_BP, A_BP, B_BP, X_BP, Y_BP, ROOT2_BP, C_BP, D_BP, ROOT3_BP, E_BP, ROOT4_BP, G_BP,
        ]
        .into_iter()
        .find(|bp| sde_recipe(*bp).is_some_and(|(_, product)| product.type_id == product_type_id)))
    }
    async fn reaction_formula_for_product(
        &self,
        product_type_id: i64,
    ) -> Result<Option<i64>, SdeError> {
        Ok((product_type_id == F).then_some(F_FORMULA))
    }
    async fn reaction_formula(
        &self,
        reaction_formula_type_id: i64,
    ) -> Result<Option<ReactionFormulaRecipe>, SdeError> {
        Ok(
            (reaction_formula_type_id == F_FORMULA).then(|| ReactionFormulaRecipe {
                reaction_formula_type_id,
                reaction_formula_name: "F Reaction Formula".to_string(),
                duration_seconds: Some(3_600),
                materials: vec![ln(RAW_F, "Raw F", 100)],
                products: vec![ln(F, "F", 200)],
            }),
        )
    }
}

// ---- Builds ----------------------------------------------------------

fn captured(blueprint_type_id: i64) -> iskworks_core::BuildRecipe {
    let (materials, product) = sde_recipe(blueprint_type_id).expect("fixture recipe");
    iskworks_core::BuildRecipe::Manufacturing(captured_recipe(
        blueprint_type_id,
        &materials
            .iter()
            .map(|line| (line.type_id, line.type_name.as_str(), line.quantity as u64))
            .collect::<Vec<_>>(),
        (product.type_id, &product.type_name, product.quantity as u64),
    ))
}

fn f_reaction() -> iskworks_core::BuildRecipe {
    iskworks_core::BuildRecipe::Reaction(
        iskworks_core::CapturedReactionFormula::capture(
            uuid::Uuid::new_v4(),
            "test".to_string(),
            ReactionFormulaRecipe {
                reaction_formula_type_id: F_FORMULA,
                reaction_formula_name: "F Reaction Formula".to_string(),
                duration_seconds: Some(3_600),
                materials: vec![ln(RAW_F, "Raw F", 100)],
                products: vec![ln(F, "F", 200)],
            },
        )
        .unwrap(),
    )
}

fn mfg(type_id: i64, blueprint_type_id: i64) -> iskworks_core::ComponentResolution {
    resolution(type_id, blueprint_type_id)
}

fn rxn(type_id: i64, formula: i64) -> iskworks_core::ComponentResolution {
    iskworks_core::ComponentResolution {
        type_id,
        recipe: iskworks_core::RecipeSelection::Reaction {
            reaction_formula_type_id: formula,
        },
        facility_override: None,
        blueprint_selection: None,
    }
}

struct Plan {
    workspace_id: iskworks_core::WorkspaceId,
    owner_id: OwnerId,
    root: Build,
    builds: Vec<Build>,
}

impl Plan {
    fn new(root_bp: i64, root_resolutions: Vec<iskworks_core::ComponentResolution>) -> Self {
        let workspace_id = iskworks_core::WorkspaceId::new();
        let owner_id = OwnerId::new();
        let mut root = linked_child_with_draft(
            workspace_id,
            owner_id,
            BuildId::new(),
            0,
            1,
            captured_recipe(0, &[], (0, "", 1)),
            root_resolutions,
        );
        root.recipe = captured(root_bp);
        fixture_unlink(root.id);
        root.name = "Root".to_string();
        Self {
            workspace_id,
            owner_id,
            root,
            builds: Vec::new(),
        }
    }

    /// A producer Build of this plan in `parent`'s `slot` (a fixture-only
    /// producer slot that `edges` derives canonical demand edges from; the
    /// planner itself reads edges only).
    fn add(
        &mut self,
        parent: BuildId,
        slot: i64,
        recipe: iskworks_core::BuildRecipe,
        resolutions: Vec<iskworks_core::ComponentResolution>,
    ) -> BuildId {
        let mut build = linked_child_with_draft(
            self.workspace_id,
            self.owner_id,
            parent,
            slot,
            1,
            captured_recipe(0, &[], (0, "", 1)),
            resolutions,
        );
        build.recipe = recipe;
        let id = build.id;
        self.builds.push(build);
        id
    }

    fn build_mut(&mut self, id: BuildId) -> &mut Build {
        if self.root.id == id {
            return &mut self.root;
        }
        self.builds.iter_mut().find(|b| b.id == id).unwrap()
    }

    fn set_me(&mut self, id: BuildId, me: u8) {
        let draft = self.build_mut(id).draft_planning.as_mut().unwrap();
        draft.input.blueprint_selection = Some(manual_bpc(me, 0));
    }

    fn set_full(&mut self, id: BuildId, type_id: i64) {
        let draft = self.build_mut(id).draft_planning.as_mut().unwrap();
        draft
            .input
            .fulfillment_scopes
            .push(FulfillmentScopeOverride {
                type_id,
                scope: FulfillmentScope::Full,
            });
    }

    /// The persisted edges exactly as the canonical derivation / canonical
    /// writes would hold them, then `shared` retargets: each
    /// `(consumer, component, producer)` makes that consumer's edge
    /// reference `producer` (one producer, many consumers).
    fn edges(&self, shared: &[(BuildId, i64, BuildId)]) -> Vec<PersistedProductionDependency> {
        super::derived_canonical_edges(&self.root, &self.builds, shared)
    }

    fn overlay(&self) -> serde_json::Value {
        let draft = &self.root.draft_planning.as_ref().unwrap().input;
        serde_json::json!({
            "recipe": {"mode": "manufacturing", "blueprintTypeId": self.root.recipe.blueprint_type_id()},
            "runs": self.root.runs,
            "pricingSelections": [],
            "componentResolutions": draft.component_resolutions,
            "fulfillmentScopes": draft.fulfillment_scopes,
        })
    }
}

struct App {
    router: axum::Router,
    repository: Arc<FixtureIndustryRepository>,
    root: BuildId,
    overlay: serde_json::Value,
}

/// `canonical: Some(shared)` -> a plan with those shared-producer edges;
/// `None` -> edges derived from the fixture's Builds.
fn app(
    plan: &Plan,
    canonical: Option<&[(BuildId, i64, BuildId)]>,
    balances: Vec<(i64, u64)>,
    retired: Vec<BuildId>,
) -> App {
    app_with_production(
        plan,
        canonical,
        balances,
        retired,
        Arc::new(NeverCalledProductionRepository),
    )
}

fn app_with_production(
    plan: &Plan,
    canonical: Option<&[(BuildId, i64, BuildId)]>,
    balances: Vec<(i64, u64)>,
    retired: Vec<BuildId>,
    production: Arc<dyn ProductionRepository>,
) -> App {
    // Every material has a fresh market price so cost is complete
    // enough to compare (installation stays incomplete: no facility).
    let mut market_items = std::collections::BTreeMap::new();
    for type_id in [
        RAW_Y, RAW_X, RAW_F, X, Y, F, A_PRODUCT, B_PRODUCT, C_PRODUCT, D_PRODUCT, E_PRODUCT,
        G_PRODUCT, 97_001, 97_051, 97_071, 97_081,
    ] {
        market_items.insert(
            type_id,
            iskworks_core::PriceSourceItem {
                type_id,
                type_name: format!("type {type_id}"),
                price: Money::parse(&format!("{}.25", type_id % 1000 + 3)).unwrap(),
                note: "fixture".to_string(),
                updated_at: chrono::Utc::now(),
            },
        );
    }
    let repository = Arc::new(FixtureIndustryRepository {
        build: Some(plan.root.clone()),
        linked_builds: std::sync::Mutex::new(plan.builds.clone()),
        market_items,
        canonical: std::sync::Mutex::new(canonical.map(|shared| CanonicalFixture {
            edges: plan.edges(shared),
            retired,
        })),
        ..Default::default()
    });
    let inventory = Arc::new(support::inventory::SeededInventoryRepository::new(balances));
    let mut new_workspace = NewWorkspace::manual("Industry".to_string());
    new_workspace.workspace.owner_id = plan.owner_id;
    new_workspace.owner.id = plan.owner_id;
    let router = build_router(
        AppState::new(Arc::new(ConfiguredWorkspaceRepository {
            state: WorkspaceState::configured(new_workspace.workspace, new_workspace.owner),
        }))
        .with_industry_repository(repository.clone())
        .with_sde_repository(Arc::new(CanonicalSde))
        .with_inventory_repository(inventory)
        .with_production_repository(production),
    );
    App {
        router,
        repository,
        root: plan.root.id,
        overlay: plan.overlay(),
    }
}

impl App {
    async fn call(
        &self,
        method: &str,
        uri: String,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value, Vec<u8>) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
            .unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json, bytes)
    }

    async fn project(&self, path: &str) -> serde_json::Value {
        let (status, json, _) = self
            .call(
                "POST",
                format!("/api/builds/{}/{path}", self.root.0),
                Some(self.overlay.clone()),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{path}: {json}");
        json
    }

    async fn materials(&self) -> serde_json::Value {
        self.project("materials").await
    }

    async fn stages(&self) -> serde_json::Value {
        self.project("execution-plan").await
    }

    async fn cost(&self) -> serde_json::Value {
        self.project("cost-projection").await
    }
}

fn stage_node(stages: &serde_json::Value, type_id: i64) -> Vec<&serde_json::Value> {
    stages["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|node| node["outputTypeId"] == type_id)
        .collect()
}

fn ops_for(cost: &serde_json::Value, type_id: i64) -> Vec<&serde_json::Value> {
    cost["operations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|op| op["productTypeId"] == type_id)
        .collect()
}

/// Root -> {A -> X, B -> X}: in the canonical plan ONE X producer
/// (`x`) serves both consumers; X -> Y -> Raw Y.
struct Shared {
    plan: Plan,
    a: BuildId,
    b: BuildId,
    x: BuildId,
    y: BuildId,
}

fn shared_x() -> Shared {
    let mut plan = Plan::new(ROOT_BP, vec![mfg(A_PRODUCT, A_BP), mfg(B_PRODUCT, B_BP)]);
    let root = plan.root.id;
    let a = plan.add(root, A_PRODUCT, captured(A_BP), vec![mfg(X, X_BP)]);
    let b = plan.add(root, B_PRODUCT, captured(B_BP), vec![mfg(X, X_BP)]);
    let x = plan.add(a, X, captured(X_BP), vec![mfg(Y, Y_BP)]);
    plan.set_me(x, 10);
    let y = plan.add(x, Y, captured(Y_BP), Vec::new());
    Shared { plan, a, b, x, y }
}

impl Shared {
    fn edges(&self) -> Vec<(BuildId, i64, BuildId)> {
        vec![(self.b, X, self.x)]
    }
}

// ---- sizing ---------------------------------------------------------

/// X serves A (24) and B (41): ONE producer sized ONCE from 65 ->
/// ceil(65/10) = 7 runs (never 3 + 5 = 8), previewed ONCE at 7 runs, so
/// Y = ceil(max(7, 2*7*0.9)) = 13 -- never the linear 7*ceil(2*0.9) =
/// 14, never ceil(2*3*0.9)+ceil(2*5*0.9) = 15. No occurrence
/// fingerprinting, no structural runs=1 previews.
#[tokio::test]
async fn a_shared_producer_is_sized_once_and_previewed_once_at_the_aggregate_runs() {
    let fixture = shared_x();
    let app = app(&fixture.plan, Some(&fixture.edges()), vec![], vec![]);
    let json = app.materials().await;

    let x_row = row(&json, X).unwrap();
    assert_eq!(x_row["requiredQuantity"], 65);
    for (consumer, own) in [(fixture.a, 24), (fixture.b, 41)] {
        let alloc = node_alloc(&json, consumer, X).unwrap();
        assert_eq!(alloc["shortageQuantity"], own);
        assert_eq!(alloc["childRuns"], 7, "ceil(65/10), never 3 or 5");
        assert_eq!(alloc["producedQuantity"], 70);
        assert_eq!(alloc["producerBuildId"], fixture.x.0.to_string());
        assert!(alloc["dependencyId"].as_str().unwrap().starts_with("pd:"));
    }
    let surpluses: Vec<u64> = [fixture.a, fixture.b]
        .iter()
        .map(|consumer| {
            node_alloc(&json, *consumer, X).unwrap()["surplusQuantity"]
                .as_u64()
                .unwrap()
        })
        .collect();
    assert_eq!(surpluses.iter().sum::<u64>(), 5, "70 - 65, once");
    assert!(surpluses.contains(&0), "never repeated per consumer");

    assert_eq!(row(&json, Y).unwrap()["requiredQuantity"], 13);
    assert_eq!(row(&json, RAW_Y).unwrap()["requiredQuantity"], 13);
    // The producer's own requirement is recorded once, by the producer.
    assert!(node_alloc(&json, fixture.x, Y).is_some());

    // One X operation with two incoming demand edges; one preview each.
    let cost = app.cost().await;
    let x_ops = ops_for(&cost, X);
    assert_eq!(
        x_ops.len(),
        1,
        "one ProductionOperation per canonical producer"
    );
    assert_eq!(x_ops[0]["nodeRuns"], 7);
    assert_eq!(x_ops[0]["graphNodeId"], format!("build:{}", fixture.x.0));
    assert_eq!(
        app.repository
            .root_plan_loads
            .load(std::sync::atomic::Ordering::Relaxed),
        2,
        "one bounded plan load per projection (materials + cost)"
    );
}

/// Cross-depth: Root4 -> {A -> X (depth 2), G -> B -> X (depth 3)}:
/// X is one producer regardless of depth; sized once from 24 + 41.
#[tokio::test]
async fn a_producer_reached_at_different_depths_is_sized_once() {
    let mut plan = Plan::new(ROOT4_BP, vec![mfg(A_PRODUCT, A_BP), mfg(G_PRODUCT, G_BP)]);
    let root = plan.root.id;
    let a = plan.add(root, A_PRODUCT, captured(A_BP), vec![mfg(X, X_BP)]);
    let g = plan.add(root, G_PRODUCT, captured(G_BP), vec![mfg(B_PRODUCT, B_BP)]);
    let b = plan.add(g, B_PRODUCT, captured(B_BP), vec![mfg(X, X_BP)]);
    let x = plan.add(a, X, captured(X_BP), vec![mfg(Y, Y_BP)]);
    plan.set_me(x, 10);
    plan.add(x, Y, captured(Y_BP), Vec::new());
    let app = app(&plan, Some(&[(b, X, x)]), vec![], vec![]);
    let json = app.materials().await;
    for consumer in [a, b] {
        assert_eq!(node_alloc(&json, consumer, X).unwrap()["childRuns"], 7);
    }
    assert_eq!(row(&json, Y).unwrap()["requiredQuantity"], 13);
    let cost = app.cost().await;
    assert_eq!(ops_for(&cost, X).len(), 1);
}

/// Reaction pooled sizing: F (200/run) serves C (90) and D (90):
/// 180 -> ONE run (never 1 + 1), Raw F = 100 (never 200).
#[tokio::test]
async fn a_shared_reaction_producer_is_sized_once() {
    let mut plan = Plan::new(ROOT2_BP, vec![mfg(C_PRODUCT, C_BP), mfg(D_PRODUCT, D_BP)]);
    let root = plan.root.id;
    let c = plan.add(root, C_PRODUCT, captured(C_BP), vec![rxn(F, F_FORMULA)]);
    let d = plan.add(root, D_PRODUCT, captured(D_BP), vec![rxn(F, F_FORMULA)]);
    let f = plan.add(c, F, f_reaction(), Vec::new());
    let app = app(&plan, Some(&[(d, F, f)]), vec![], vec![]);
    let json = app.materials().await;
    for consumer in [c, d] {
        let alloc = node_alloc(&json, consumer, F).unwrap();
        assert_eq!(alloc["resolution"], "reaction");
        assert_eq!(alloc["childRuns"], 1, "ceil(180/200) = 1, never 1 + 1");
        assert_eq!(alloc["producedQuantity"], 200);
    }
    assert_eq!(row(&json, RAW_F).unwrap()["requiredQuantity"], 100);
    let stages = app.stages().await;
    let f_rows = stage_node(&stages, F);
    assert_eq!(f_rows.len(), 1, "one Stages row per operation");
    assert_eq!(f_rows[0]["projectedRuns"], 1);
    assert_eq!(f_rows[0]["productionDemand"], 180);
    assert_eq!(f_rows[0]["retainedSurplusQuantity"], 20);
    assert_eq!(f_rows[0]["consumers"].as_array().unwrap().len(), 2);
}

// ---- inventory ------------------------------------------------------

/// Inventory is allocated PER EDGE before aggregation, in deterministic
/// order (A before B): 30 X in stock -> A draws 24, B draws 6 -> demand
/// 35 -> 4 runs, 40 output, 5 surplus.
#[tokio::test]
async fn inventory_is_allocated_per_edge_in_order_before_aggregation() {
    let fixture = shared_x();
    let app = app(&fixture.plan, Some(&fixture.edges()), vec![(X, 30)], vec![]);
    let json = app.materials().await;
    let a = node_alloc(&json, fixture.a, X).unwrap();
    let b = node_alloc(&json, fixture.b, X).unwrap();
    assert_eq!(
        (
            a["allocatedQuantity"].as_u64(),
            a["shortageQuantity"].as_u64()
        ),
        (Some(24), Some(0))
    );
    assert_eq!(
        (
            b["allocatedQuantity"].as_u64(),
            b["shortageQuantity"].as_u64()
        ),
        (Some(6), Some(35))
    );
    assert_eq!(b["childRuns"], 4);
    assert_eq!(b["surplusQuantity"], 5);
    let x_row = row(&json, X).unwrap();
    assert_eq!(x_row["allocatedQuantity"], 30, "the stock is counted once");
    assert_eq!(x_row["shortageQuantity"], 35);
}

/// A Full consumer ignores inventory for its own production demand while
/// a Missing consumer of the same producer draws it (Missing + Full).
#[tokio::test]
async fn missing_and_full_consumers_of_one_producer_aggregate_post_scope_demand() {
    let mut fixture = shared_x();
    fixture.plan.set_full(fixture.b, X);
    let app = app(&fixture.plan, Some(&fixture.edges()), vec![(X, 30)], vec![]);
    let json = app.materials().await;
    let a = node_alloc(&json, fixture.a, X).unwrap();
    let b = node_alloc(&json, fixture.b, X).unwrap();
    assert_eq!(a["allocatedQuantity"], 24);
    assert_eq!(a["shortageQuantity"], 0);
    assert_eq!(b["scope"], "full");
    assert_eq!(b["allocatedQuantity"], 0, "Full never draws");
    assert_eq!(b["shortageQuantity"], 41);
    assert_eq!(b["childRuns"], 5, "ceil(41/10)");

    // Full + Full: abundant inventory is ignored by both.
    let mut fixture = shared_x();
    fixture.plan.set_full(fixture.a, X);
    fixture.plan.set_full(fixture.b, X);
    let app = super::canonical_producers::app(
        &fixture.plan,
        Some(&fixture.edges()),
        vec![(X, 1_000)],
        vec![],
    );
    let json = app.materials().await;
    assert_eq!(node_alloc(&json, fixture.a, X).unwrap()["childRuns"], 7);
    assert_eq!(row(&json, X).unwrap()["allocatedQuantity"], 0);
}

/// Every incoming Missing edge fully covered: the producer is not an
/// active operation and its descendants are not expanded -- but it stays
/// configured (planning pruning, not deletion).
#[tokio::test]
async fn a_fully_covered_producer_is_pruned_with_its_descendants() {
    let fixture = shared_x();
    let app = app(
        &fixture.plan,
        Some(&fixture.edges()),
        vec![(X, 100)],
        vec![],
    );
    let json = app.materials().await;
    for consumer in [fixture.a, fixture.b] {
        let alloc = node_alloc(&json, consumer, X).unwrap();
        assert_eq!(alloc["shortageQuantity"], 0);
        assert_eq!(alloc["childRuns"], 0);
    }
    assert!(row(&json, Y).is_none(), "descendants are not expanded");
    assert!(row(&json, RAW_Y).is_none());
    let cost = app.cost().await;
    assert!(ops_for(&cost, X).is_empty(), "no active operation");
    assert!(app
        .repository
        .get_build(app.root_workspace(), fixture.x)
        .await
        .is_ok());
}

impl App {
    fn root_workspace(&self) -> iskworks_core::WorkspaceId {
        self.repository.build.as_ref().unwrap().workspace_id
    }
}

/// Inventory at a nested producer boundary (Y below the shared X) is
/// allocated at Y's own edge: X needs 13 Y, 5 in stock -> Y demand 8.
#[tokio::test]
async fn nested_producer_inventory_is_allocated_at_its_own_edge() {
    let fixture = shared_x();
    let app = app(&fixture.plan, Some(&fixture.edges()), vec![(Y, 5)], vec![]);
    let json = app.materials().await;
    let y = node_alloc(&json, fixture.x, Y).unwrap();
    assert_eq!(y["requiredQuantity"], 13);
    assert_eq!(y["allocatedQuantity"], 5);
    assert_eq!(y["shortageQuantity"], 8);
    assert_eq!(y["childRuns"], 8);
    assert_eq!(row(&json, RAW_Y).unwrap()["requiredQuantity"], 8);
    let _ = fixture.y;
}

/// A producer's discrete-run surplus never covers another branch: E buys
/// 7 X while A's X producer overproduces -- E's shortage stays 7.
#[tokio::test]
async fn producer_surplus_is_never_reused_by_another_branch() {
    let mut plan = Plan::new(ROOT3_BP, vec![mfg(A_PRODUCT, A_BP), mfg(E_PRODUCT, E_BP)]);
    let root = plan.root.id;
    let a = plan.add(root, A_PRODUCT, captured(A_BP), vec![mfg(X, X_BP)]);
    let e = plan.add(root, E_PRODUCT, captured(E_BP), Vec::new());
    let x = plan.add(a, X, captured(X_BP), vec![mfg(Y, Y_BP)]);
    plan.add(x, Y, captured(Y_BP), Vec::new());
    let app = app(&plan, Some(&[]), vec![], vec![]);
    let json = app.materials().await;
    let a_x = node_alloc(&json, a, X).unwrap();
    assert_eq!(a_x["childRuns"], 3);
    assert_eq!(a_x["surplusQuantity"], 6, "30 - 24");
    let e_x = node_alloc(&json, e, X).unwrap();
    assert_eq!(e_x["resolution"], "buy");
    assert_eq!(
        e_x["shortageQuantity"], 7,
        "surplus is evidence, never returned to the pool"
    );
}

// ---- Stages and Graph -----------------------------------------------

#[tokio::test]
async fn stages_has_one_row_per_operation_and_graph_one_producer_identity() {
    let fixture = shared_x();
    let app = app(&fixture.plan, Some(&fixture.edges()), vec![], vec![]);
    let stages = app.stages().await;
    let x_rows = stage_node(&stages, X);
    assert_eq!(x_rows.len(), 1);
    let row = x_rows[0];
    assert_eq!(row["occurrenceIds"].as_array().unwrap().len(), 1);
    assert_eq!(row["productionDemand"], 65);
    assert_eq!(row["projectedRuns"], 7, "one operation's runs, never a sum");
    assert_eq!(row["projectedOutput"], 70);
    assert_eq!(row["retainedSurplusQuantity"], 5);
    let consumers: Vec<(String, u64)> = row["consumers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["occurrenceId"].as_str().unwrap().to_string(),
                c["quantity"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(consumers.len(), 2);
    assert!(consumers.contains(&(format!("build:{}", fixture.a.0), 24)));
    assert!(consumers.contains(&(format!("build:{}", fixture.b.0), 41)));

    let graph = app.project("graph").await;
    let mut full = Vec::new();
    let mut aliases = Vec::new();
    fn walk<'a>(
        node: &'a serde_json::Value,
        full: &mut Vec<&'a serde_json::Value>,
        aliases: &mut Vec<&'a serde_json::Value>,
    ) {
        for child in node["children"].as_array().unwrap() {
            match child["nodeKind"].as_str().unwrap() {
                "production" => {
                    if child["typeId"] == X {
                        full.push(child);
                    }
                    walk(child, full, aliases);
                }
                "producerReference" => aliases.push(child),
                _ => {}
            }
        }
    }
    walk(&graph["root"], &mut full, &mut aliases);
    assert_eq!(full.len(), 1, "one full producer node");
    assert_eq!(aliases.len(), 1, "one alias under the second consumer");
    let id = format!("build:{}", fixture.x.0);
    assert_eq!(full[0]["graphNodeId"], id);
    assert_eq!(
        aliases[0]["graphNodeId"], id,
        "the alias is the SAME producer"
    );
    assert_eq!(full[0]["runs"], 7);
    assert_eq!(full[0]["netRequiredQuantity"], 65, "aggregate sizing basis");
    assert_eq!(full[0]["surplus"], 5);
    assert!(aliases[0].get("runs").is_none() && aliases[0].get("estimatedCost").is_none());
    assert_eq!(full[0]["incomingDemands"].as_array().unwrap().len(), 2);
}

// ---- Stages editing -------------------------------------------------

#[tokio::test]
async fn stages_edits_one_canonical_producer_and_rejects_stale_membership() {
    let fixture = shared_x();
    let app = app(&fixture.plan, Some(&fixture.edges()), vec![], vec![]);
    let body = |members: Vec<(BuildId, u64)>| {
        serde_json::json!({
            "command": app.overlay,
            "members": members.into_iter().map(|(build_id, revision)| {
                serde_json::json!({"buildId": build_id.0, "expectedRevision": revision})
            }).collect::<Vec<_>>(),
            "kind": "blueprintSelection",
            "blueprintSelection": {
                "mode": "manual", "kind": "original", "materialEfficiency": 0,
                "timeEfficiency": 0, "licensedRuns": null, "notes": "",
            },
        })
    };
    let uri = format!(
        "/api/builds/{}/descendant-production-configuration",
        app.root.0
    );
    // The operation is ONE producer: requesting it plus a consumer is not
    // the current operation.
    let (status, json, _) = app
        .call(
            "PATCH",
            uri.clone(),
            Some(body(vec![(fixture.x, 1), (fixture.a, 1)])),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{json}");
    assert_eq!(
        json["error"]["code"],
        "descendant_operation_membership_stale"
    );

    let (status, json, _) = app
        .call("PATCH", uri, Some(body(vec![(fixture.x, 1)])))
        .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    // ME 10 -> 0 on the one producer: Y = max(7, 2*7) = 14 now.
    let json = app.materials().await;
    assert_eq!(row(&json, Y).unwrap()["requiredQuantity"], 14);
}

// ---- workbook -------------------------------------------------------

#[tokio::test]
async fn a_canonical_shared_producer_plan_is_exportable() {
    let fixture = shared_x();
    let app = app(&fixture.plan, Some(&fixture.edges()), vec![], vec![]);
    let (status, _, bytes) = app
        .call(
            "POST",
            format!("/api/builds/{}/export-verification", app.root.0),
            Some(app.overlay.clone()),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes[..bytes.len().min(300)])
    );
    assert!(bytes.starts_with(b"PK"), "an xlsx");
}

// ---- typed invalid graph --------------------------------------------

#[tokio::test]
async fn a_canonical_graph_referencing_a_retired_producer_is_a_typed_conflict() {
    let fixture = shared_x();
    let app = app(
        &fixture.plan,
        Some(&fixture.edges()),
        vec![],
        vec![fixture.x],
    );
    let (status, json, _) = app
        .call(
            "POST",
            format!("/api/builds/{}/materials", app.root.0),
            Some(app.overlay.clone()),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{json}");
    assert_eq!(json["error"]["code"], "canonical_graph_invalid");
}

// ---- canonical writes through the routes ----------------------------

#[tokio::test]
async fn buy_to_build_references_the_canonical_producer_and_build_to_buy_keeps_it() {
    // Canonical plan where B currently BUYS X.
    let mut fixture = shared_x();
    fixture
        .plan
        .build_mut(fixture.b)
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .component_resolutions
        .clear();
    let app = app(&fixture.plan, Some(&[]), vec![], vec![]);
    let builds_before = app.repository.linked_builds.lock().unwrap().len();

    // Buy -> Build on B: the edge references the existing X producer.
    let (status, json, _) = app
        .call(
            "POST",
            format!("/api/builds/{}/component-resolutions", fixture.b.0),
            Some(serde_json::json!({
                "componentTypeId": X,
                "recipe": {"mode": "manufacturing", "blueprintTypeId": X_BP},
                "expectedRevision": 1,
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let (status, json, _) = app
        .call(
            "POST",
            format!("/api/builds/{}/linked-builds", fixture.b.0),
            Some(serde_json::json!({"componentTypeId": X})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(
        json["id"],
        fixture.x.0.to_string(),
        "the SAME producer, never a clone"
    );
    assert_eq!(
        app.repository.linked_builds.lock().unwrap().len(),
        builds_before
    );
    let materials = app.materials().await;
    assert_eq!(
        node_alloc(&materials, fixture.b, X).unwrap()["childRuns"],
        7
    );

    // A stale revision is rejected (409), nothing written.
    let (status, json, _) = app
        .call(
            "DELETE",
            format!(
                "/api/builds/{}/component-resolutions/{X}?expectedRevision=1",
                fixture.b.0
            ),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{json}");

    // Build -> Buy on B: only that edge changes; X stays for A.
    let (status, json, _) = app
        .call(
            "DELETE",
            format!(
                "/api/builds/{}/component-resolutions/{X}?expectedRevision=2",
                fixture.b.0
            ),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let materials = app.materials().await;
    assert_eq!(
        node_alloc(&materials, fixture.b, X).unwrap()["resolution"],
        "buy"
    );
    assert_eq!(
        node_alloc(&materials, fixture.a, X).unwrap()["childRuns"],
        3,
        "ceil(24/10)"
    );
    assert!(app
        .repository
        .get_build(app.root_workspace(), fixture.x)
        .await
        .is_ok());
}

#[tokio::test]
async fn a_method_change_creates_the_new_producer_once_and_never_mutates_the_old_one() {
    let fixture = shared_x();
    let app = app(&fixture.plan, Some(&fixture.edges()), vec![], vec![]);
    let before = app
        .repository
        .get_build(app.root_workspace(), fixture.x)
        .await
        .unwrap();
    let (status, json, _) = app
        .call(
            "POST",
            format!("/api/builds/{}/component-resolutions", fixture.b.0),
            Some(serde_json::json!({
                "componentTypeId": X,
                "recipe": {"mode": "manufacturing", "blueprintTypeId": X_ALT_BP},
                "expectedRevision": 1,
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let after = app
        .repository
        .get_build(app.root_workspace(), fixture.x)
        .await
        .unwrap();
    assert_eq!(
        after.recipe, before.recipe,
        "the incompatible producer is never re-recipe'd"
    );
    assert_eq!(after.revision, before.revision);
    let builds = app.repository.linked_builds.lock().unwrap().clone();
    let alternatives: Vec<&Build> = builds
        .iter()
        .filter(|b| b.recipe.blueprint_type_id() == Some(X_ALT_BP))
        .collect();
    assert_eq!(alternatives.len(), 1, "created exactly once");
    assert_eq!(
        producer_of(alternatives[0].id),
        None,
        "a canonical producer, not a per-consumer child"
    );
    drop(builds);
    let materials = app.materials().await;
    // A keeps the X_BP producer (24 -> 3 runs); B uses the new one (41 -> 5).
    assert_eq!(
        node_alloc(&materials, fixture.a, X).unwrap()["childRuns"],
        3
    );
    let b_x = node_alloc(&materials, fixture.b, X).unwrap();
    assert_eq!(b_x["childRuns"], 5);
    assert_eq!(b_x["producerBuildId"], alternatives_id(&app));
    assert_eq!(
        row(&materials, RAW_X).unwrap()["requiredQuantity"],
        15,
        "3 * 5 runs"
    );
}

fn alternatives_id(app: &App) -> String {
    app.repository
        .linked_builds
        .lock()
        .unwrap()
        .iter()
        .find(|b| b.recipe.blueprint_type_id() == Some(X_ALT_BP))
        .unwrap()
        .id
        .0
        .to_string()
}

// ---- Plan inspector sourcing -----------------------------------------

fn acquisition(stages: &serde_json::Value, type_id: i64) -> Option<&serde_json::Value> {
    stages["acquisitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|line| line["typeId"] == type_id)
}

fn logistics_line(stages: &serde_json::Value, type_id: i64) -> &serde_json::Value {
    stages["logistics"]["destinations"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|destination| destination["lines"].as_array().unwrap())
        .find(|line| line["typeId"] == type_id)
        .unwrap_or_else(|| panic!("logistics line {type_id}"))
}

/// The Plan (execution-plan) response carries everything the inspector
/// needs to change sourcing -- the demand edge's consumer Build and its
/// own requirement evidence, plus the valid production methods -- and a
/// Buy -> Build -> Buy round trip through the canonical write path
/// re-plans Plan, Logistics and cost with no new producer.
#[tokio::test]
async fn the_plan_offers_sourcing_per_edge_and_recomputes_after_a_canonical_change() {
    let mut fixture = shared_x();
    fixture
        .plan
        .build_mut(fixture.b)
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .component_resolutions
        .clear();
    let app = app(&fixture.plan, Some(&[]), vec![], vec![]);
    let builds_before = app.repository.linked_builds.lock().unwrap().len();

    let stages = app.stages().await;
    let x_buy = acquisition(&stages, X).expect("B buys X");
    assert_eq!(
        x_buy["productionMethods"],
        serde_json::json!([{"mode": "manufacturing", "blueprintTypeId": X_BP}])
    );
    let consumer = &x_buy["consumers"][0];
    assert_eq!(consumer["buildId"], fixture.b.0.to_string());
    assert_eq!(consumer["requiredQuantity"], 41);
    assert_eq!(consumer["plannedInventoryQuantity"], 0);
    assert_eq!(consumer["fulfillmentScope"], "missing");
    assert!(consumer["dependencyId"]
        .as_str()
        .unwrap()
        .starts_with("pd:"));
    assert_eq!(
        stage_node(&stages, X)[0]["consumers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let x_logistics = logistics_line(&stages, X);
    assert_eq!(x_logistics["acquireQuantity"], 41);
    assert_eq!(x_logistics["producedQuantity"], 24);
    let x_boundary_kinds = |cost: &serde_json::Value| -> Vec<String> {
        let mut kinds: Vec<String> = cost["boundaries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|boundary| boundary["typeId"] == X)
            .map(|boundary| boundary["kind"].as_str().unwrap().to_string())
            .collect();
        kinds.sort();
        kinds
    };
    assert_eq!(x_boundary_kinds(&app.cost().await), vec!["build", "buy"]);

    // Buy -> Build (the Plan inspector's call): the SAME canonical producer.
    let (status, json, _) = app
        .call(
            "POST",
            format!("/api/builds/{}/component-resolutions", fixture.b.0),
            Some(serde_json::json!({
                "componentTypeId": X,
                "recipe": {"mode": "manufacturing", "blueprintTypeId": X_BP},
                "expectedRevision": 1,
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(
        app.repository.linked_builds.lock().unwrap().len(),
        builds_before
    );

    let stages = app.stages().await;
    assert!(
        acquisition(&stages, X).is_none(),
        "X left external acquisition"
    );
    let x_node = stage_node(&stages, X);
    assert_eq!(x_node.len(), 1, "one operation, never one per consumer");
    assert_eq!(x_node[0]["consumers"].as_array().unwrap().len(), 2);
    assert_producers_precede_consumers(&stages);
    let x_logistics = logistics_line(&stages, X);
    assert_eq!(x_logistics["acquireQuantity"], 0);
    assert_eq!(x_logistics["producedQuantity"], 65);
    assert_eq!(x_logistics["consumers"].as_array().unwrap().len(), 2);
    assert_eq!(
        x_boundary_kinds(&app.cost().await),
        vec!["build", "build"],
        "cost re-planned: B now consumes the X operation"
    );

    // Build -> Buy: only B's edge changes; X remains for A.
    let (status, json, _) = app
        .call(
            "DELETE",
            format!(
                "/api/builds/{}/component-resolutions/{X}?expectedRevision=2",
                fixture.b.0
            ),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let stages = app.stages().await;
    assert_eq!(acquisition(&stages, X).unwrap()["shortageQuantity"], 41);
    assert_eq!(
        stage_node(&stages, X).len(),
        1,
        "the producer is kept for A"
    );
    assert_eq!(
        app.repository.linked_builds.lock().unwrap().len(),
        builds_before
    );
}

// ---- Per-edge evidence and Plan/Graph parity --------------------------

/// Every Graph production node (or producer-reference alias) for
/// `type_id`, walking the graph tree -- one per consuming edge.
fn graph_nodes_for(graph: &serde_json::Value, type_id: i64) -> usize {
    fn walk(value: &serde_json::Value, type_id: i64) -> usize {
        match value {
            serde_json::Value::Object(map) => {
                // The full producer node, plus a producer-reference
                // alias under every other consuming edge.
                let own = usize::from(
                    map.get("typeId") == Some(&serde_json::json!(type_id))
                        && matches!(
                            map.get("nodeKind").and_then(|kind| kind.as_str()),
                            Some("production" | "producerReference")
                        ),
                );
                own + map
                    .values()
                    .map(|child| walk(child, type_id))
                    .sum::<usize>()
            }
            serde_json::Value::Array(items) => items.iter().map(|child| walk(child, type_id)).sum(),
            _ => 0,
        }
    }
    walk(&graph["root"], type_id)
}

/// A shared producer's Plan consumers each carry their own demand edge
/// (consumer Build, dependency id, scope, required, planned use,
/// production demand); a sourcing change through the one write path
/// both Plan and Graph use leaves Plan, Graph and cost agreeing.
#[tokio::test]
async fn plan_edges_are_per_consumer_and_plan_and_graph_agree_after_a_change() {
    let fixture = shared_x();
    let app = app(&fixture.plan, Some(&fixture.edges()), vec![(X, 10)], vec![]);

    let stages = app.stages().await;
    let x = stage_node(&stages, X)[0];
    let edges = x["consumers"].as_array().unwrap();
    assert_eq!(edges.len(), 2);
    for (consumer, required, planned) in [(fixture.a, 24, 10), (fixture.b, 41, 0)] {
        let edge = edges
            .iter()
            .find(|edge| edge["buildId"] == consumer.0.to_string())
            .unwrap_or_else(|| panic!("edge for {consumer:?}: {edges:?}"));
        assert_eq!(edge["requiredQuantity"], required);
        assert_eq!(edge["plannedInventoryQuantity"], planned);
        assert_eq!(edge["quantity"], required - planned, "production demand");
        assert_eq!(edge["fulfillmentScope"], "missing");
        assert!(edge["dependencyId"].as_str().unwrap().starts_with("pd:"));
    }
    assert_eq!(x["availableQuantity"], 10);
    assert!(x.get("unitProductionCost").is_some());
    assert_eq!(
        graph_nodes_for(&app.project("graph").await, X),
        2,
        "Graph shows the producer under each consuming edge"
    );

    // B's edge -> Buy (the same component-resolutions write Graph uses).
    let (status, json, _) = app
        .call(
            "DELETE",
            format!(
                "/api/builds/{}/component-resolutions/{X}?expectedRevision=1",
                fixture.b.0
            ),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let stages = app.stages().await;
    let x = stage_node(&stages, X)[0];
    assert_eq!(
        x["consumers"].as_array().unwrap().len(),
        1,
        "only A still produces"
    );
    assert_eq!(x["consumers"][0]["buildId"], fixture.a.0.to_string());
    let x_input = stages["acquisitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|line| line["typeId"] == X)
        .expect("B now buys X");
    assert_eq!(x_input["consumers"][0]["buildId"], fixture.b.0.to_string());
    assert!(
        x_input["freshCost"].is_string(),
        "priced fixture: {x_input}"
    );
    let graph = app.project("graph").await;
    assert_eq!(
        graph_nodes_for(&graph, X),
        1,
        "Graph agrees: X produced for A only"
    );
}

// ---- Stage order on the operation DAG ---------------------------------

/// For every Stages edge P -> C: stage(P) < stage(C).
fn assert_producers_precede_consumers(stages: &serde_json::Value) {
    let stage_of: std::collections::BTreeMap<String, u64> = stages["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| {
            (
                node["id"].as_str().unwrap().to_string(),
                node["stage"].as_u64().unwrap(),
            )
        })
        .collect();
    let edges = stages["edges"].as_array().unwrap();
    assert!(!edges.is_empty());
    for edge in edges {
        let (from, to) = (edge["from"].as_str().unwrap(), edge["to"].as_str().unwrap());
        assert!(
            stage_of[from] < stage_of[to],
            "stage({from}) = {} must be < stage({to}) = {}",
            stage_of[from],
            stage_of[to]
        );
    }
    let listed: u64 = stages["stages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|stage| stage["nodeIds"].as_array().unwrap().len() as u64)
        .sum();
    assert_eq!(
        listed as usize,
        stage_of.len(),
        "every node in exactly one stage"
    );
}

fn stage_of_type(stages: &serde_json::Value, type_id: i64) -> u64 {
    let nodes = stage_node(stages, type_id);
    assert_eq!(nodes.len(), 1, "one row for {type_id}");
    nodes[0]["stage"].as_u64().unwrap()
}

/// Squall shape: Root -> {A, B}, both consuming ONE X (Reinforced
/// Carbon Fiber consumed by Life Support Backup Unit and
/// Auto-Integrity Preservation Seal). The plan has one canonical producer
/// for X, and it must be staged strictly earlier than both consumers --
/// never in the same stage as B.
#[tokio::test]
async fn a_shared_producer_is_staged_before_every_consumer() {
    let mut fixture = shared_x();
    let xb = fixture
        .plan
        .add(fixture.b, X, captured(X_BP), vec![mfg(Y, Y_BP)]);
    fixture.plan.set_me(xb, 10);
    fixture.plan.add(xb, Y, captured(Y_BP), Vec::new());
    let edges = fixture.edges();
    let app = app(&fixture.plan, Some(&edges[..]), vec![], vec![xb]);
    let stages = app.stages().await;
    assert_producers_precede_consumers(&stages);
    let (y, x, a, b, root) = (
        stage_of_type(&stages, Y),
        stage_of_type(&stages, X),
        stage_of_type(&stages, A_PRODUCT),
        stage_of_type(&stages, B_PRODUCT),
        stage_of_type(&stages, 97_001),
    );
    assert_eq!((y, x, a, b, root), (0, 1, 2, 2, 3));
}

/// Fan-out at different depths: X -> A -> Root and X -> B -> G -> Root.
/// X appears once, before BOTH A and B.
#[tokio::test]
async fn fan_out_at_different_depths_orders_the_producer_before_all_consumers() {
    let mut plan = Plan::new(ROOT4_BP, vec![mfg(A_PRODUCT, A_BP), mfg(G_PRODUCT, G_BP)]);
    let root = plan.root.id;
    let a = plan.add(root, A_PRODUCT, captured(A_BP), vec![mfg(X, X_BP)]);
    let g = plan.add(root, G_PRODUCT, captured(G_BP), vec![mfg(B_PRODUCT, B_BP)]);
    let b = plan.add(g, B_PRODUCT, captured(B_BP), vec![mfg(X, X_BP)]);
    let x = plan.add(a, X, captured(X_BP), vec![mfg(Y, Y_BP)]);
    plan.set_me(x, 10);
    plan.add(x, Y, captured(Y_BP), Vec::new());
    let xb = plan.add(b, X, captured(X_BP), vec![mfg(Y, Y_BP)]);
    plan.set_me(xb, 10);
    plan.add(xb, Y, captured(Y_BP), Vec::new());
    let shared = [(b, X, x)];
    let app = app(&plan, Some(&shared[..]), vec![], vec![xb]);
    let stages = app.stages().await;
    assert_producers_precede_consumers(&stages);
    let x_stage = stage_of_type(&stages, X);
    assert!(x_stage < stage_of_type(&stages, A_PRODUCT));
    assert!(x_stage < stage_of_type(&stages, B_PRODUCT));
    assert!(stage_of_type(&stages, B_PRODUCT) < stage_of_type(&stages, G_PRODUCT));
}

// ---- ticket plan freezing --------------------------------------------

/// Freezing a ticket plan for the root (two build-resolved rows, A and B)
/// resolves each row's frozen Buy/Build sourcing from ONE bounded plan
/// load, never one `load_root_plan` per material line.
#[tokio::test]
async fn ticket_plan_preview_resolves_every_rows_sourcing_from_one_plan_load() {
    let fixture = shared_x();
    let app = app_with_production(
        &fixture.plan,
        Some(&fixture.edges()),
        vec![],
        vec![],
        Arc::new(FixtureProductionRepository {
            type_id: RAW_Y,
            available_to_this_build: 0,
            average_historical_unit_cost: None,
        }),
    );
    let loads_before = app
        .repository
        .root_plan_loads
        .load(std::sync::atomic::Ordering::Relaxed);
    let (status, json, _) = app
        .call(
            "GET",
            format!("/api/builds/{}/ticket-preview", app.root.0),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let kinds: Vec<(i64, String)> = json["prerequisites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["typeId"].as_i64().unwrap(),
                row["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            (A_PRODUCT, "build".to_string()),
            (B_PRODUCT, "build".to_string())
        ]
    );
    let loads = app
        .repository
        .root_plan_loads
        .load(std::sync::atomic::Ordering::Relaxed)
        - loads_before;
    assert_eq!(
        loads, 2,
        "one load for the snapshot + one for every row's sourcing, never one per row"
    );
}
