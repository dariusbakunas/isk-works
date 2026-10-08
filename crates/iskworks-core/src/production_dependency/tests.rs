use chrono::Utc;
use iskworks_sde::{ManufacturingRecipe, ReactionFormulaRecipe, RecipeLine};
use uuid::Uuid;

use super::*;
use crate::industry::{
    BuildRecipe, CapturedReactionFormula, CapturedRecipe, DraftPlanningInput,
    DraftPlanningSnapshot, RecipeCurrency,
};
use crate::{
    ComponentResolution, FacilityPreviewCommand, FulfillmentScopeOverride, MarketPricingPolicy,
    OwnerId, ReactionFacilityPreviewCommand, WorkspaceId, DEFAULT_MARKET_SCOPE,
};

// ---- fixture helpers -------------------------------------------------------

thread_local! {
    /// Which consumer component each fixture producer was created for --
    /// what `derive_persisted` turns into Produce edges. Per test thread.
    static PRODUCER_OF: std::cell::RefCell<HashMap<BuildId, (BuildId, i64)>> =
        std::cell::RefCell::new(HashMap::new());
}

pub(crate) fn producer_of(build: BuildId) -> Option<(BuildId, i64)> {
    PRODUCER_OF.with(|links| links.borrow().get(&build).copied())
}

pub(crate) fn ln(type_id: i64, name: &str, quantity: i64) -> RecipeLine {
    RecipeLine {
        type_id,
        type_name: name.to_string(),
        quantity,
    }
}

pub(crate) fn mfg(
    blueprint_type_id: i64,
    product: RecipeLine,
    materials: Vec<RecipeLine>,
) -> BuildRecipe {
    BuildRecipe::Manufacturing(
        CapturedRecipe::capture(
            Uuid::nil(),
            "1".into(),
            ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: format!("{} Blueprint", product.type_name),
                duration_seconds: Some(60),
                materials,
                products: vec![product],
            },
        )
        .unwrap(),
    )
}

pub(crate) fn rxn(formula: i64, product: RecipeLine, materials: Vec<RecipeLine>) -> BuildRecipe {
    BuildRecipe::Reaction(
        CapturedReactionFormula::capture(
            Uuid::nil(),
            "1".into(),
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

pub(crate) fn mfg_sel(blueprint_type_id: i64) -> RecipeSelection {
    RecipeSelection::Manufacturing { blueprint_type_id }
}

pub(crate) fn rxn_sel(reaction_formula_type_id: i64) -> RecipeSelection {
    RecipeSelection::Reaction {
        reaction_formula_type_id,
    }
}

pub(crate) fn res(type_id: i64, recipe: RecipeSelection) -> ComponentResolution {
    ComponentResolution {
        type_id,
        recipe,
        facility_override: None,
        blueprint_selection: None,
    }
}

#[derive(Default, Clone)]
pub(crate) struct Draft {
    pub(crate) resolutions: Vec<ComponentResolution>,
    pub(crate) full_scoped: Vec<i64>,
    pub(crate) manufacturing_facility: Option<FacilityProfileId>,
    pub(crate) reaction_facility: Option<FacilityProfileId>,
    pub(crate) blueprint: Option<BlueprintSelection>,
}

impl Draft {
    fn snapshot(self) -> Option<DraftPlanningSnapshot> {
        Some(DraftPlanningSnapshot {
            input: DraftPlanningInput {
                material_scope: DEFAULT_MARKET_SCOPE,
                output_scope: DEFAULT_MARKET_SCOPE,
                manual_price_list_id: None,
                expected_manual_price_list_revision: None,
                material_pricing_policy: MarketPricingPolicy::HighestBuy,
                output_pricing_policy: MarketPricingPolicy::LowestSell,
                pricing_selections: Vec::new(),
                blueprint_selection: self.blueprint,
                manufacturing_facility: self.manufacturing_facility.map(|id| {
                    FacilityPreviewCommand {
                        facility_profile_id: id,
                        blueprint_me: 0,
                        blueprint_te: 0,
                        estimated_item_value: None,
                    }
                }),
                reaction_facility: self.reaction_facility.map(|id| {
                    ReactionFacilityPreviewCommand {
                        facility_profile_id: id,
                        estimated_item_value: None,
                    }
                }),
                facility_eiv_manual: false,
                component_resolutions: self.resolutions,
                fulfillment_scopes: self
                    .full_scoped
                    .into_iter()
                    .map(|type_id| FulfillmentScopeOverride {
                        type_id,
                        scope: FulfillmentScope::Full,
                    })
                    .collect(),
            },
            updated_at: Utc::now(),
        })
    }
}

pub(crate) fn build(
    id: BuildId,
    recipe: BuildRecipe,
    runs: u64,
    parent: Option<(BuildId, i64)>,
    draft: Draft,
) -> Build {
    let now = Utc::now();
    let build = Build {
        id,
        workspace_id: WorkspaceId(Uuid::nil()),
        owner_id: OwnerId(Uuid::nil()),
        name: "fixture".into(),
        recipe,
        runs,
        notes: String::new(),
        revision: 1,
        created_at: now,
        updated_at: now,
        draft_planning: draft.snapshot(),
        recipe_currency: RecipeCurrency::Current,
        active_sde_version: None,
        product_category_name: None,
        product_group_name: None,
        selected_blueprint_origin: None,
        has_owned_blueprint: false,
    };
    PRODUCER_OF.with(|links| match parent {
        Some(slot) => {
            links.borrow_mut().insert(id, slot);
        }
        None => {
            links.borrow_mut().remove(&id);
        }
    });
    build
}

pub(crate) fn id(n: u128) -> BuildId {
    BuildId(Uuid::from_u128(n))
}

pub(crate) fn manual_blueprint(me: u8, te: u8) -> BlueprintSelection {
    BlueprintSelection::Manual {
        kind: BlueprintKind::Original,
        material_efficiency: me,
        time_efficiency: te,
        licensed_runs: None,
        notes: String::new(),
    }
}

// ---- the real Muninn Ferrogel shape ----------------------------------------
//
// Type/recipe ids and quantities are taken verbatim from the profiled local
// Muninn plan (Build ids replaced with deterministic fixture ids).

pub(crate) const REACTIONS_FACILITY: Uuid = Uuid::from_u128(0x5df9_2fb5);
pub(crate) const MFG_FACILITY: Uuid = Uuid::from_u128(0x5106_9ce6);

pub(crate) const MUNINN: i64 = 12_003;
pub(crate) const PLASMA_THRUSTER: i64 = 11_530;
pub(crate) const DEFLECTION_SHIELD_EMITTER: i64 = 11_555;
pub(crate) const FERROGEL: i64 = 16_683;
pub(crate) const FERROGEL_FORMULA: i64 = 46_213;
pub(crate) const FUEL: i64 = 4_246;
pub(crate) const HEXITE: i64 = 16_665;
pub(crate) const HYPERFLURITE: i64 = 16_666;
pub(crate) const FERROFLUID: i64 = 16_669;
pub(crate) const PROMETIUM: i64 = 17_960;

pub(crate) const ROOT: u128 = 1;
pub(crate) const PT: u128 = 10;
pub(crate) const DSE: u128 = 11;
pub(crate) const FERROGEL_A: u128 = 20;
pub(crate) const FERROGEL_B: u128 = 21;
pub(crate) const HEXITE_B: u128 = 30;
pub(crate) const FERROFLUID_B: u128 = 31;
pub(crate) const PROMETIUM_B: u128 = 32;

pub(crate) fn ferrogel_recipe() -> BuildRecipe {
    rxn(
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
}

pub(crate) fn simple_reaction(formula: i64, product: i64, name: &str) -> BuildRecipe {
    rxn(
        formula,
        ln(product, name, 200),
        vec![ln(90_000 + product, "Moon goo", 100)],
    )
}

pub(crate) fn reactions_draft(resolutions: Vec<ComponentResolution>) -> Draft {
    Draft {
        resolutions,
        reaction_facility: Some(FacilityProfileId(REACTIONS_FACILITY)),
        ..Draft::default()
    }
}

pub(crate) fn muninn_fixture() -> (Build, Vec<Build>) {
    let root = build(
        id(ROOT),
        mfg(
            12_016,
            ln(MUNINN, "Muninn", 1),
            vec![
                ln(PLASMA_THRUSTER, "Plasma Thruster", 67),
                ln(DEFLECTION_SHIELD_EMITTER, "Deflection Shield Emitter", 402),
            ],
        ),
        3,
        None,
        Draft {
            resolutions: vec![
                res(PLASMA_THRUSTER, mfg_sel(17_324)),
                res(DEFLECTION_SHIELD_EMITTER, mfg_sel(17_346)),
            ],
            manufacturing_facility: Some(FacilityProfileId(MFG_FACILITY)),
            ..Draft::default()
        },
    );
    let consumer_draft = || Draft {
        resolutions: vec![res(FERROGEL, rxn_sel(FERROGEL_FORMULA))],
        manufacturing_facility: Some(FacilityProfileId(MFG_FACILITY)),
        blueprint: Some(manual_blueprint(10, 20)),
        ..Draft::default()
    };
    let plasma_thruster = build(
        id(PT),
        mfg(
            17_324,
            ln(PLASMA_THRUSTER, "Plasma Thruster", 1),
            vec![ln(FERROGEL, "Ferrogel", 1)],
        ),
        201,
        Some((id(ROOT), PLASMA_THRUSTER)),
        consumer_draft(),
    );
    let deflection = build(
        id(DSE),
        mfg(
            17_346,
            ln(DEFLECTION_SHIELD_EMITTER, "Deflection Shield Emitter", 1),
            vec![ln(FERROGEL, "Ferrogel", 1)],
        ),
        1_206,
        Some((id(ROOT), DEFLECTION_SHIELD_EMITTER)),
        consumer_draft(),
    );
    // Ferrogel A: every input bought.
    let ferrogel_a = build(
        id(FERROGEL_A),
        ferrogel_recipe(),
        1,
        Some((id(PT), FERROGEL)),
        reactions_draft(Vec::new()),
    );
    // Ferrogel B: Hexite / Ferrofluid / Prometium produced by linked reactions.
    let ferrogel_b = build(
        id(FERROGEL_B),
        ferrogel_recipe(),
        4,
        Some((id(DSE), FERROGEL)),
        reactions_draft(vec![
            res(HEXITE, rxn_sel(46_174)),
            res(FERROFLUID, rxn_sel(46_172)),
            res(PROMETIUM, rxn_sel(46_184)),
        ]),
    );
    let hexite = build(
        id(HEXITE_B),
        simple_reaction(46_174, HEXITE, "Hexite"),
        2,
        Some((id(FERROGEL_B), HEXITE)),
        reactions_draft(Vec::new()),
    );
    let ferrofluid = build(
        id(FERROFLUID_B),
        simple_reaction(46_172, FERROFLUID, "Ferrofluid"),
        2,
        Some((id(FERROGEL_B), FERROFLUID)),
        reactions_draft(Vec::new()),
    );
    let prometium = build(
        id(PROMETIUM_B),
        simple_reaction(46_184, PROMETIUM, "Prometium"),
        2,
        Some((id(FERROGEL_B), PROMETIUM)),
        reactions_draft(Vec::new()),
    );
    (
        root,
        vec![
            plasma_thruster,
            deflection,
            ferrogel_a,
            ferrogel_b,
            hexite,
            ferrofluid,
            prometium,
        ],
    )
}

pub(crate) fn ferrogel_key() -> CanonicalProducerKey {
    CanonicalProducerKey {
        output_type_id: FERROGEL,
        method: ProductionMethod::Reaction {
            reaction_formula_type_id: FERROGEL_FORMULA,
        },
    }
}

// ---- Muninn Ferrogel: divergent duplicate producer candidates ---------------

// ---- identical duplicate producer candidates --------------------------------

pub(crate) const X: i64 = 700;
pub(crate) const X_FORMULA: i64 = 7_000;
pub(crate) const Y: i64 = 800;
pub(crate) const Y_FORMULA: i64 = 8_000;

/// Root -> consumers A, B -> X (own copy each) -> Y (own copy each).
/// `tweak` adjusts X-under-B / Y-under-B to create divergences.
pub(crate) fn duplicate_fixture(tweak: impl FnOnce(&mut Draft, &mut Draft)) -> (Build, Vec<Build>) {
    let root = build(
        id(ROOT),
        mfg(
            1_000,
            ln(500, "Root", 1),
            vec![ln(501, "Consumer A", 1), ln(502, "Consumer B", 1)],
        ),
        1,
        None,
        Draft {
            resolutions: vec![res(501, mfg_sel(1_501)), res(502, mfg_sel(1_502))],
            ..Draft::default()
        },
    );
    let consumer = |n: u128, product: i64, bp: i64| {
        build(
            id(n),
            mfg(bp, ln(product, "Consumer", 1), vec![ln(X, "X", 10)]),
            1,
            Some((id(ROOT), product)),
            Draft {
                resolutions: vec![res(X, rxn_sel(X_FORMULA))],
                ..Draft::default()
            },
        )
    };
    let x_recipe = || {
        rxn(
            X_FORMULA,
            ln(X, "X", 100),
            vec![ln(Y, "Y", 50), ln(900, "Fuel", 5)],
        )
    };
    let y_recipe = || rxn(Y_FORMULA, ln(Y, "Y", 200), vec![ln(901, "Goo", 100)]);
    let x_draft = || reactions_draft(vec![res(Y, rxn_sel(Y_FORMULA))]);
    let y_draft = || reactions_draft(Vec::new());

    let mut x_b = x_draft();
    let mut y_b = y_draft();
    tweak(&mut x_b, &mut y_b);

    (
        root,
        vec![
            consumer(10, 501, 1_501),
            consumer(11, 502, 1_502),
            build(id(20), x_recipe(), 1, Some((id(10), X)), x_draft()),
            build(id(21), x_recipe(), 1, Some((id(11), X)), x_b),
            build(id(30), y_recipe(), 1, Some((id(20), Y)), y_draft()),
            build(id(31), y_recipe(), 1, Some((id(21), Y)), y_b),
        ],
    )
}

// ---- adapter diagnostics / errors -------------------------------------------

// ---- target shape: one producer, many demand edges ----------------------------

pub(crate) const CONSUMER_A: u128 = 10;
pub(crate) const CONSUMER_B: u128 = 11;
pub(crate) const PRODUCER_X: u128 = 20;

fn edge_to_x(consumer: u128, scope: FulfillmentScope) -> ProductionDependency {
    ProductionDependency {
        id: ProductionDependencyId::derived(id(consumer), FERROGEL),
        consumer: id(consumer),
        component_type_id: FERROGEL,
        component_type_name: "Ferrogel".into(),
        base_quantity_per_run: 1,
        fulfillment_scope: scope,
        sourcing: DependencySourcing::Produce {
            method: ferrogel_key().method,
        },
        producer: Some(id(PRODUCER_X)),
        legacy_consumer_overrides: None,
    }
}

fn producer(
    n: u128,
    output_type_id: i64,
    dependencies: Vec<ProductionDependencyId>,
) -> ProducerConfiguration {
    ProducerConfiguration {
        producer: id(n),
        is_root: n == ROOT,
        key: CanonicalProducerKey {
            output_type_id,
            method: ProductionMethod::Reaction {
                reaction_formula_type_id: output_type_id * 10,
            },
        },
        output_type_name: format!("type {output_type_id}"),
        output_per_run: 400,
        facility_profile_id: None,
        blueprint: None,
        requested_runs: (n == ROOT).then_some(1),
        persisted_runs: 1,
        dependencies,
    }
}

/// Consumer A --\
///               -> one Ferrogel producer X
/// Consumer B --/
fn single_producer_graph(
    scope_a: FulfillmentScope,
    scope_b: FulfillmentScope,
) -> RootPlanDependencyGraph {
    let a = edge_to_x(CONSUMER_A, scope_a);
    let b = edge_to_x(CONSUMER_B, scope_b);
    let mut x = producer(PRODUCER_X, FERROGEL, Vec::new());
    x.key = ferrogel_key();
    RootPlanDependencyGraph::from_parts(
        id(ROOT),
        vec![
            producer(ROOT, 1, Vec::new()),
            producer(CONSUMER_A, 2, vec![a.id.clone()]),
            producer(CONSUMER_B, 3, vec![b.id.clone()]),
            x,
        ],
        vec![a, b],
    )
}

fn requirements() -> Vec<DemandEdgeRequirement> {
    vec![
        DemandEdgeRequirement {
            dependency: ProductionDependencyId::derived(id(CONSUMER_A), FERROGEL),
            required_quantity: 201,
        },
        DemandEdgeRequirement {
            dependency: ProductionDependencyId::derived(id(CONSUMER_B), FERROGEL),
            required_quantity: 1_206,
        },
    ]
}

#[test]
fn one_producer_two_consumers_is_sized_once_from_aggregate_demand() {
    let graph = single_producer_graph(FulfillmentScope::Missing, FulfillmentScope::Missing);

    // Target shape: one producer, two incoming edges, no clone.
    assert_eq!(
        graph
            .producers()
            .iter()
            .filter(|p| p.key == ferrogel_key())
            .count(),
        1
    );
    let incoming: Vec<BuildId> = graph
        .incoming(id(PRODUCER_X))
        .iter()
        .map(|edge| edge.consumer)
        .collect();
    assert_eq!(incoming, vec![id(CONSUMER_A), id(CONSUMER_B)]);

    let mut inventory = PlanningInventory::seed([]);
    let demand =
        aggregate_producer_demand(&graph, id(PRODUCER_X), &requirements(), &mut inventory).unwrap();
    assert_eq!(demand.total_production_demand, 1_407);
    assert_eq!(
        demand
            .edges
            .iter()
            .map(|e| (e.consumer, e.production_demand))
            .collect::<Vec<_>>(),
        vec![(id(CONSUMER_A), 201), (id(CONSUMER_B), 1_206)],
        "consumer attribution preserved"
    );

    // ceil(1407 / 400) = 4, never 1 + 4 = 5.
    assert_eq!(
        demand.size(400),
        ProducerSizing {
            runs: 4,
            output_per_run: 400,
            produced_quantity: 1_600,
            surplus_quantity: 193,
        }
    );
}

#[test]
fn inventory_is_allocated_per_edge_before_production_demand_is_aggregated() {
    let graph = single_producer_graph(FulfillmentScope::Missing, FulfillmentScope::Missing);
    let mut inventory = PlanningInventory::seed([(FERROGEL, 200)]);
    let demand =
        aggregate_producer_demand(&graph, id(PRODUCER_X), &requirements(), &mut inventory).unwrap();

    // Allocation order is the caller's: A draws all 200 first.
    assert_eq!(
        demand
            .edges
            .iter()
            .map(|e| (
                e.required_quantity,
                e.allocated_inventory,
                e.production_demand
            ))
            .collect::<Vec<_>>(),
        vec![(201, 200, 1), (1_206, 0, 1_206)]
    );
    assert_eq!(demand.total_required, 1_407);
    assert_eq!(demand.total_allocated_inventory, 200);
    assert_eq!(demand.total_production_demand, 1_207);
    assert_eq!(
        inventory.take(FERROGEL, u64::MAX),
        0,
        "pool drawn down once"
    );
    assert_eq!(demand.size(400).runs, 4);
    assert_eq!(demand.size(400).surplus_quantity, 393);
}

#[test]
fn full_scope_edges_never_draw_inventory() {
    let graph = single_producer_graph(FulfillmentScope::Full, FulfillmentScope::Missing);
    let mut inventory = PlanningInventory::seed([(FERROGEL, 300)]);
    let demand =
        aggregate_producer_demand(&graph, id(PRODUCER_X), &requirements(), &mut inventory).unwrap();
    assert_eq!(
        demand
            .edges
            .iter()
            .map(|e| (
                e.fulfillment_scope,
                e.allocated_inventory,
                e.production_demand
            ))
            .collect::<Vec<_>>(),
        vec![
            (FulfillmentScope::Full, 0, 201),
            (FulfillmentScope::Missing, 300, 906),
        ]
    );
    assert_eq!(demand.total_production_demand, 1_107);
}

#[test]
fn fully_covered_demand_sizes_to_zero_runs() {
    let graph = single_producer_graph(FulfillmentScope::Missing, FulfillmentScope::Missing);
    let mut inventory = PlanningInventory::seed([(FERROGEL, 5_000)]);
    let demand =
        aggregate_producer_demand(&graph, id(PRODUCER_X), &requirements(), &mut inventory).unwrap();
    assert_eq!(demand.total_production_demand, 0);
    assert_eq!(
        demand.size(400),
        ProducerSizing {
            runs: 0,
            output_per_run: 400,
            produced_quantity: 0,
            surplus_quantity: 0,
        }
    );
}

#[test]
fn aggregation_rejects_edges_the_producer_does_not_satisfy() {
    let graph = single_producer_graph(FulfillmentScope::Missing, FulfillmentScope::Missing);
    let mut inventory = PlanningInventory::seed([]);

    let unknown = DemandEdgeRequirement {
        dependency: ProductionDependencyId::derived(id(77), FERROGEL),
        required_quantity: 1,
    };
    assert!(matches!(
        aggregate_producer_demand(&graph, id(PRODUCER_X), &[unknown], &mut inventory),
        Err(DemandAggregationError::UnknownDependency(_))
    ));
    assert!(matches!(
        aggregate_producer_demand(&graph, id(CONSUMER_A), &requirements(), &mut inventory),
        Err(DemandAggregationError::NotSatisfiedByProducer { .. })
    ));
    let twice = vec![requirements()[0].clone(), requirements()[0].clone()];
    assert!(matches!(
        aggregate_producer_demand(&graph, id(PRODUCER_X), &twice, &mut inventory),
        Err(DemandAggregationError::DuplicateRequirement(_))
    ));
}

#[test]
fn pooled_run_rule_matches_the_planner_bounds() {
    assert_eq!(pooled_production_runs(0, 400), 0);
    assert_eq!(pooled_production_runs(1, 400), 1);
    assert_eq!(pooled_production_runs(400, 400), 1);
    assert_eq!(pooled_production_runs(1_407, 400), 4);
    assert_eq!(pooled_production_runs(u64::MAX, 1), 1_000_000);
}

// ---- Persisted graph ---------------------------------------------------------

/// Persisted rows for a fixture tree, derived with the same rules as the
/// `iskworks_sync_production_dependencies` SQL function: one row per
/// captured material, the consumer's *requested* method, the slot child as
/// producer only when its recipe matches that method.
pub(crate) fn derive_persisted(root: &Build, descendants: &[Build]) -> RootPlanRecords {
    let mut dependencies = Vec::new();
    let mut next_id = 0x1000_u128;
    for build in std::iter::once(root).chain(descendants) {
        let draft = build.draft_planning.as_ref().map(|d| &d.input);
        let mut seen = BTreeSet::new();
        for line in build.recipe.materials() {
            if !seen.insert(line.type_id) {
                continue;
            }
            let resolution = draft.and_then(|input| {
                input
                    .component_resolutions
                    .iter()
                    .rev()
                    .find(|r| r.type_id == line.type_id)
            });
            let full = draft.is_some_and(|input| {
                input
                    .fulfillment_scopes
                    .iter()
                    .any(|s| s.type_id == line.type_id && s.scope == FulfillmentScope::Full)
            });
            let (sourcing, producer_build_id) = match resolution {
                None => (DependencySourcing::Buy, None),
                Some(resolution) => {
                    let method = ProductionMethod::from(resolution.recipe);
                    let producer = descendants
                        .iter()
                        .find(|child| producer_of(child.id) == Some((build.id, line.type_id)))
                        .filter(|child| {
                            ProductionMethod::from(recipe_selection_of(&child.recipe)) == method
                        })
                        .map(|child| child.id);
                    (DependencySourcing::Produce { method }, producer)
                }
            };
            next_id += 1;
            dependencies.push(PersistedProductionDependency {
                id: Uuid::from_u128(next_id),
                plan_root_build_id: root.id,
                consumer_build_id: build.id,
                component_type_id: line.type_id,
                sourcing,
                producer_build_id,
                fulfillment_scope: if full {
                    FulfillmentScope::Full
                } else {
                    FulfillmentScope::Missing
                },
                revision: 1,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            });
        }
    }
    RootPlanRecords {
        root: root.clone(),
        producers: descendants.to_vec(),
        dependencies,
        retired_producers: Vec::new(),
    }
}

#[test]
fn a_retained_producer_is_detached_kept_and_never_a_conflict() {
    let (mut root, descendants) = muninn_fixture();
    // Deflection Shield Emitter -> Buy: its subtree (incl. Ferrogel B) is
    // retained in storage with no active incoming edge.
    root.draft_planning
        .as_mut()
        .unwrap()
        .input
        .component_resolutions
        .retain(|r| r.type_id != DEFLECTION_SHIELD_EMITTER);
    let persisted =
        RootPlanDependencyGraph::from_persisted(&derive_persisted(&root, &descendants)).unwrap();

    assert_eq!(
        persisted.detached_producers(),
        &[
            id(DSE),
            id(FERROGEL_B),
            id(HEXITE_B),
            id(FERROFLUID_B),
            id(PROMETIUM_B)
        ]
    );
    assert!(
        persisted.producer(id(FERROGEL_B)).is_some(),
        "kept in the model"
    );
    assert!(
        persisted.incoming(id(DSE)).is_empty(),
        "zero incoming edges"
    );
}

#[test]
fn one_persisted_producer_can_serve_many_consumer_edges() {
    let (root, descendants) = muninn_fixture();
    let mut records = derive_persisted(&root, &descendants);
    // Target shape: PT's Ferrogel edge also references Ferrogel B; A is gone.
    for edge in &mut records.dependencies {
        if edge.consumer_build_id == id(PT) && edge.component_type_id == FERROGEL {
            edge.producer_build_id = Some(id(FERROGEL_B));
        }
    }
    records.producers.retain(|b| b.id != id(FERROGEL_A));
    let graph = RootPlanDependencyGraph::from_persisted(&records).unwrap();

    let consumers: Vec<BuildId> = graph
        .incoming(id(FERROGEL_B))
        .iter()
        .map(|e| e.consumer)
        .collect();
    assert_eq!(consumers.len(), 2);
    assert!(consumers.contains(&id(PT)) && consumers.contains(&id(DSE)));
    assert_eq!(
        graph
            .producers()
            .iter()
            .filter(|p| p.producer == id(FERROGEL_B))
            .count(),
        1,
        "visited once, never cloned"
    );
    assert!(graph.validate_acyclic().is_ok());
}

#[test]
fn persisted_drift_is_reported_as_diagnostics() {
    let (root, descendants) = muninn_fixture();
    let mut records = derive_persisted(&root, &descendants);
    // Missing: drop Ferrogel B's Hyperflurite row.
    records.dependencies.retain(|e| {
        !(e.consumer_build_id == id(FERROGEL_B) && e.component_type_id == HYPERFLURITE)
    });
    // Unmatched: a row for a component PT's recipe does not have.
    let mut extra = records.dependencies[0].clone();
    extra.id = Uuid::from_u128(0xdead);
    extra.consumer_build_id = id(PT);
    extra.component_type_id = 555;
    records.dependencies.push(extra);
    // Dangling: DSE's Ferrogel edge names a Build outside the plan.
    for edge in &mut records.dependencies {
        if edge.consumer_build_id == id(DSE) && edge.component_type_id == FERROGEL {
            edge.producer_build_id = Some(id(0xabc));
        }
    }
    let graph = RootPlanDependencyGraph::from_persisted(&records).unwrap();
    assert!(graph
        .diagnostics
        .contains(&AdapterDiagnostic::MissingPersistedEdge {
            consumer: id(FERROGEL_B),
            component_type_id: HYPERFLURITE,
        }));
    assert!(graph
        .diagnostics
        .contains(&AdapterDiagnostic::UnmatchedPersistedEdge {
            dependency: ProductionDependencyId::persisted(Uuid::from_u128(0xdead)),
            consumer: id(PT),
            component_type_id: 555,
        }));
    assert!(graph.diagnostics.iter().any(|d| matches!(
        d,
        AdapterDiagnostic::DanglingProducerReference { producer, .. } if *producer == id(0xabc)
    )));

    let mut duplicate = derive_persisted(&root, &descendants);
    let copy = duplicate.dependencies[0].clone();
    duplicate.dependencies.push(copy);
    assert!(matches!(
        RootPlanDependencyGraph::from_persisted(&duplicate),
        Err(PersistedGraphError::DuplicateEdge { .. })
    ));
}

#[test]
fn persisted_loading_is_deterministic_regardless_of_row_order() {
    let (root, descendants) = muninn_fixture();
    let records = derive_persisted(&root, &descendants);
    let mut shuffled = records.clone();
    shuffled.dependencies.reverse();
    shuffled.producers.reverse();
    assert_eq!(
        RootPlanDependencyGraph::from_persisted(&records).unwrap(),
        RootPlanDependencyGraph::from_persisted(&shuffled).unwrap()
    );
}

// ---- Cycle validation ------------------------------------------------------

/// A graph where each `(consumer, producer)` pair is one Produce edge.
fn edge_graph(pairs: &[(u128, u128)]) -> RootPlanDependencyGraph {
    let mut nodes: BTreeSet<u128> = BTreeSet::new();
    for (c, p) in pairs {
        nodes.insert(*c);
        nodes.insert(*p);
    }
    let mut dependencies = Vec::new();
    let mut outgoing: BTreeMap<u128, Vec<ProductionDependencyId>> = BTreeMap::new();
    for (index, (consumer, producer)) in pairs.iter().enumerate() {
        let component = 1_000 + i64::try_from(index).unwrap();
        let edge = ProductionDependency {
            id: ProductionDependencyId::derived(id(*consumer), component),
            consumer: id(*consumer),
            component_type_id: component,
            component_type_name: "c".into(),
            base_quantity_per_run: 1,
            fulfillment_scope: FulfillmentScope::Missing,
            sourcing: DependencySourcing::Produce {
                method: ProductionMethod::Reaction {
                    reaction_formula_type_id: component,
                },
            },
            producer: Some(id(*producer)),
            legacy_consumer_overrides: None,
        };
        outgoing.entry(*consumer).or_default().push(edge.id.clone());
        dependencies.push(edge);
    }
    let producers = nodes
        .iter()
        .map(|n| {
            let mut config = producer(*n, i64::try_from(*n).unwrap(), Vec::new());
            config.dependencies = outgoing.remove(n).unwrap_or_default();
            config
        })
        .collect();
    RootPlanDependencyGraph::from_parts(id(1), producers, dependencies)
}

#[test]
fn a_direct_cycle_is_rejected() {
    let graph = edge_graph(&[(1, 2)]);
    assert!(graph.would_create_cycle(id(2), id(2)), "self-production");
    assert!(
        graph.would_create_cycle(id(2), id(1)),
        "2 -> 1 while 1 -> 2"
    );
    assert_eq!(
        edge_graph(&[(1, 2), (2, 1)]).validate_acyclic(),
        Err(ProducerCycle(vec![id(1), id(2)]))
    );
}

#[test]
fn an_indirect_cycle_is_rejected() {
    let graph = edge_graph(&[(1, 2), (2, 3), (3, 4)]);
    assert!(graph.would_create_cycle(id(4), id(1)));
    assert!(graph.would_create_cycle(id(3), id(2)));
    assert!(
        !graph.would_create_cycle(id(1), id(4)),
        "a shortcut is not a cycle"
    );
    assert!(edge_graph(&[(1, 2), (2, 3), (3, 1)])
        .validate_acyclic()
        .is_err());
}

#[test]
fn a_diamond_and_many_consumers_are_allowed() {
    // 1 -> {2, 3}, 2 -> 4, 3 -> 4: one producer (4) with two consumers.
    let diamond = edge_graph(&[(1, 2), (1, 3), (2, 4), (3, 4)]);
    assert!(diamond.validate_acyclic().is_ok());
    assert_eq!(diamond.incoming(id(4)).len(), 2);
    assert!(!diamond.would_create_cycle(id(1), id(4)));
    // A third consumer of 4.
    assert!(!diamond.would_create_cycle(id(5), id(4)));
    assert!(
        diamond.would_create_cycle(id(4), id(2)),
        "4 would consume 2 which consumes 4"
    );
}
