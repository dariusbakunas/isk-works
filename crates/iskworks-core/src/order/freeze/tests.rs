use std::collections::BTreeMap;

use super::*;
use crate::build_cost::project_build_cost;
use crate::build_materials::MaterialActivity;
use crate::{FulfillmentScope, PricingSelectionKind, RecipeCurrency};

fn money(value: &str) -> Money {
    Money::parse(value).unwrap()
}

fn root_op(node_runs: u64, output_per_run: u64) -> VerificationOperationInput {
    VerificationOperationInput {
        op_index: 0,
        parent_op_index: None,
        parent_traversal_index: None,
        incoming: Vec::new(),
        graph_node_id: "root:00000000-0000-0000-0000-000000000001".to_string(),
        build_id: BuildId::default(),
        revision: 1,
        tree_path: Vec::new(),
        activity: MaterialActivity::Manufacturing,
        product_type_id: 100,
        product_name: "Root Product".to_string(),
        output_per_run,
        base_material_count: 1,
        blueprint_or_formula_type_id: 1000,
        blueprint_or_formula_name: "Root Blueprint".to_string(),
        node_runs,
        persisted_runs: node_runs,
        recipe_currency: RecipeCurrency::Current,
        me: Some(0),
        te: Some(0),
        blueprint_selection: None,
        facility_id: None,
        facility_name: None,
        structure_type: None,
        solar_system: None,
        structure_material_reduction_percent: Decimal::ZERO,
        structure_time_reduction_percent: Decimal::ZERO,
        effective_material_factor: Decimal::ONE,
        facility_profile_revision: None,
        system_cost_index: None,
        job_cost_reduction_percent: Decimal::ZERO,
        facility_tax_percent: Decimal::ZERO,
        scc_surcharge_percent: Decimal::ZERO,
        alliance_surcharge_percent: Decimal::ZERO,
        fixed_supplemental_cost: Money::zero(),
        job_count: 1,
        installation_formula_version: String::new(),
    }
}

fn child_op(
    op_index: u32,
    parent_op_index: u32,
    build_id: BuildId,
    product_type_id: i64,
    node_runs: u64,
    output_per_run: u64,
) -> VerificationOperationInput {
    let mut op = root_op(node_runs, output_per_run);
    op.op_index = op_index;
    op.parent_op_index = Some(parent_op_index);
    op.graph_node_id = format!("build:{:032x}", build_id.0.as_u128());
    op.build_id = build_id;
    op.product_type_id = product_type_id;
    op.product_name = format!("Product {product_type_id}");
    op
}

#[allow(clippy::too_many_arguments)]
fn boundary(
    traversal_index: u32,
    op_index: u32,
    type_id: i64,
    resolution: MaterialBoundaryResolution,
    scope: FulfillmentScope,
    api_required: u64,
    api_planned_use: u64,
    api_shortage: u64,
) -> VerificationBoundaryInput {
    VerificationBoundaryInput {
        traversal_index,
        parent_traversal_index: None,
        op_index,
        build_id: BuildId::default(),
        graph_node_id: "root:00000000-0000-0000-0000-000000000001".to_string(),
        tree_path: Vec::new(),
        type_id,
        type_name: format!("Type {type_id}"),
        activity: MaterialActivity::Manufacturing,
        resolution,
        scope,
        node_runs: 1,
        base_quantity_per_run: api_required,
        blueprint_me: 0,
        facility_material_factor: Decimal::ONE,
        output_per_run: 0,
        starting_inventory: api_planned_use,
        api_required,
        api_planned_use,
        api_shortage,
        api_child_runs: 0,
        api_produced: 0,
        api_surplus: 0,
        fresh_unit_price: (resolution == MaterialBoundaryResolution::Buy && api_shortage > 0)
            .then(|| money("5")),
        fresh_price_selection: PricingSelectionKind::Default,
        fresh_pricing_policy: None,
        fresh_price_note: String::new(),
        fresh_price_stale: false,
        market_region_id: Some(10_000_002),
        market_location_id: Some(60_003_760),
        intended_recipe: None,
        dependency_id: String::new(),
        producer_build_id: None,
    }
}

fn run(
    operations: &[VerificationOperationInput],
    boundaries: &[VerificationBoundaryInput],
    inventory_basis: &[InventoryBasisEntry],
) -> BuildCostProjection {
    project_build_cost(
        operations,
        boundaries,
        inventory_basis,
        &BTreeMap::new(),
        None,
    )
}

#[test]
fn root_only_buy_plan_freezes_one_operation_and_two_requirements() {
    let operations = vec![root_op(1, 10)];
    let boundaries = vec![
        boundary(
            0,
            0,
            34,
            MaterialBoundaryResolution::Buy,
            FulfillmentScope::Missing,
            100,
            40,
            60,
        ),
        boundary(
            1,
            0,
            35,
            MaterialBoundaryResolution::Buy,
            FulfillmentScope::Full,
            20,
            0,
            20,
        ),
    ];
    let inventory_basis = vec![InventoryBasisEntry {
        type_id: 34,
        quantity: 40,
        reserved_quantity: 0,
        unit_basis: Some(Decimal::from(2)),
        total_basis: Decimal::from(80),
    }];
    let cost = run(&operations, &boundaries, &inventory_basis);

    let frozen = freeze_order_plan(&operations, &boundaries, &cost, &inventory_basis).unwrap();

    assert_eq!(frozen.operations.len(), 1);
    let root = &frozen.operations[0];
    assert_eq!(
        root.occurrence_key,
        "root:00000000-0000-0000-0000-000000000001"
    );
    assert_eq!(root.parent_occurrence_key, None);
    assert_eq!(root.runs, 1);
    assert_eq!(root.produced_quantity, 10);

    assert_eq!(frozen.requirements.len(), 2);
    let tritanium = frozen
        .requirements
        .iter()
        .find(|r| r.type_id == 34)
        .expect("tritanium requirement");
    assert_eq!(tritanium.kind, RequirementKind::Buy);
    assert_eq!(tritanium.source_build_id, None);
    assert_eq!(tritanium.child_occurrence_key, None);
    assert_eq!(
        tritanium.operation_occurrence_key.as_deref(),
        Some("root:00000000-0000-0000-0000-000000000001")
    );
    assert_eq!(tritanium.reused_quantity, 40);
    assert_eq!(tritanium.required_quantity, 100);
    assert_eq!(
        tritanium.inventory_unit_basis,
        Some(Money(Decimal::from(2)))
    );
    assert!(tritanium.price_evidence.is_some());

    let full_scope = frozen
        .requirements
        .iter()
        .find(|r| r.type_id == 35)
        .unwrap();
    assert_eq!(full_scope.fulfillment_scope, FulfillmentScope::Full);
    assert_eq!(full_scope.reused_quantity, 0);
}

/// Multi-BPC job split: the operation's job count is frozen as evidence.
#[test]
fn operation_job_count_is_frozen_as_evidence() {
    let mut root = root_op(4, 1);
    root.job_count = 4;
    let operations = vec![root];
    let cost = run(&operations, &[], &[]);

    let frozen = freeze_order_plan(&operations, &[], &cost, &[]).unwrap();

    assert_eq!(frozen.operations[0].evidence.job_count, 4);
}

#[test]
fn fully_covered_build_boundary_gets_no_child_operation_or_linkage() {
    // `Build` resolution, but no matching child `VerificationOperationInput`
    // -- the allocator pruned the subtree (shortage 0).
    let operations = vec![root_op(1, 1)];
    let boundaries = vec![boundary(
        0,
        0,
        200,
        MaterialBoundaryResolution::Build,
        FulfillmentScope::Missing,
        15,
        15,
        0,
    )];
    let cost = run(&operations, &boundaries, &[]);

    let frozen = freeze_order_plan(&operations, &boundaries, &cost, &[]).unwrap();

    assert_eq!(frozen.operations.len(), 1);
    assert_eq!(frozen.requirements.len(), 1);
    let requirement = &frozen.requirements[0];
    assert_eq!(requirement.kind, RequirementKind::Build);
    // Known limitation (see this module's own doc): no walked child
    // operation means no resolvable linked-build identity here.
    assert_eq!(requirement.source_build_id, None);
    assert_eq!(requirement.child_occurrence_key, None);
    assert_eq!(requirement.child_produced_quantity, None);
}

#[test]
fn unresolved_boundary_maps_kind_from_intended_recipe() {
    let operations = vec![root_op(1, 1)];
    let mut unresolved = boundary(
        0,
        0,
        300,
        MaterialBoundaryResolution::Unresolved,
        FulfillmentScope::Missing,
        10,
        0,
        10,
    );
    unresolved.intended_recipe = Some(crate::RecipeSelection::Reaction {
        reaction_formula_type_id: 999,
    });
    let boundaries = vec![unresolved];
    let cost = run(&operations, &boundaries, &[]);

    let frozen = freeze_order_plan(&operations, &boundaries, &cost, &[]).unwrap();

    let requirement = &frozen.requirements[0];
    assert_eq!(requirement.kind, RequirementKind::React);
    assert_eq!(requirement.source_build_id, None);
    assert!(requirement.price_evidence.is_some());
}

#[test]
fn operation_evidence_warnings_are_non_empty_when_cost_is_incomplete() {
    // Root has no facility, so its installation (and therefore the whole
    // operation) is incomplete -- `CostWarning::NoFacilitySelected` must
    // survive into the frozen evidence as a human-readable string.
    let operations = vec![root_op(1, 1)];
    let boundaries = vec![boundary(
        0,
        0,
        34,
        MaterialBoundaryResolution::Buy,
        FulfillmentScope::Full,
        10,
        0,
        10,
    )];
    let cost = run(&operations, &boundaries, &[]);

    let frozen = freeze_order_plan(&operations, &boundaries, &cost, &[]).unwrap();

    let root = &frozen.operations[0];
    assert!(!root.complete);
    assert!(root.evidence.installation.is_none());
    assert!(!root.evidence.warnings.is_empty());
}

// -----------------------------------------------------------------
// One operation, many frozen requirements.
// -----------------------------------------------------------------

use crate::build_materials::OperationIncomingDemand;

const X: i64 = 300;
const Y: i64 = 400;

/// Facility with a zero system cost index and `fixed` ISK of fixed
/// supplemental cost -- a cost-complete installation.
fn priced(mut op: VerificationOperationInput, fixed: &str) -> VerificationOperationInput {
    op.facility_id = Some(uuid::Uuid::from_u128(u128::from(op.op_index) + 1));
    op.facility_name = Some("Test Facility".to_string());
    op.facility_profile_revision = Some(1);
    op.system_cost_index = Some(Decimal::ZERO);
    op.fixed_supplemental_cost = money(fixed);
    op.installation_formula_version = "eve-manufacturing-facility-v1".to_string();
    op
}

fn root() -> VerificationOperationInput {
    priced(root_op(1, 1), "0")
}

/// A canonical producer serving `incoming` = (traversal, consumer op,
/// dependency id) demand edges.
fn producer(
    op_index: u32,
    product_type_id: i64,
    node_runs: u64,
    output_per_run: u64,
    fixed: &str,
    incoming: &[(u32, u32, &str)],
) -> VerificationOperationInput {
    let build_id = BuildId(uuid::Uuid::from_u128(u128::from(op_index) + 0xB000));
    let mut op = priced(
        child_op(
            op_index,
            incoming[0].1,
            build_id,
            product_type_id,
            node_runs,
            output_per_run,
        ),
        fixed,
    );
    op.parent_traversal_index = Some(incoming[0].0);
    op.incoming = incoming
        .iter()
        .map(
            |&(traversal_index, consumer_op_index, dependency_id)| OperationIncomingDemand {
                traversal_index,
                consumer_op_index,
                dependency_id: dependency_id.to_string(),
            },
        )
        .collect();
    op
}

/// A Build-resolved demand edge (`required`, `planned` inventory use,
/// the rest short) on consumer `op_index`.
fn demand(
    traversal_index: u32,
    op_index: u32,
    type_id: i64,
    scope: FulfillmentScope,
    required: u64,
    planned: u64,
    dependency_id: &str,
) -> VerificationBoundaryInput {
    let mut b = boundary(
        traversal_index,
        op_index,
        type_id,
        MaterialBoundaryResolution::Build,
        scope,
        required,
        planned,
        required - planned,
    );
    b.dependency_id = dependency_id.to_string();
    b
}

fn buy(
    traversal_index: u32,
    op_index: u32,
    type_id: i64,
    quantity: u64,
) -> VerificationBoundaryInput {
    boundary(
        traversal_index,
        op_index,
        type_id,
        MaterialBoundaryResolution::Buy,
        FulfillmentScope::Full,
        quantity,
        0,
        quantity,
    )
}

fn freeze_canonical_plan(
    operations: &[VerificationOperationInput],
    boundaries: &[VerificationBoundaryInput],
    inventory_basis: &[InventoryBasisEntry],
) -> Result<FrozenPlan, OrderError> {
    // Every type any operation consumes gets a zero adjusted price, so
    // every installation (EIV) is complete.
    let adjusted: BTreeMap<i64, Decimal> = boundaries
        .iter()
        .map(|b| (b.type_id, Decimal::ZERO))
        .collect();
    let cost = project_build_cost(operations, boundaries, inventory_basis, &adjusted, None);
    freeze_order_plan(operations, boundaries, &cost, inventory_basis)
}

fn op_by_type(frozen: &FrozenPlan, type_id: i64) -> Vec<&NewPlanOperation> {
    frozen
        .operations
        .iter()
        .filter(|op| op.product_type_id == type_id)
        .collect()
}

fn served_by<'a>(frozen: &'a FrozenPlan, op: &NewPlanOperation) -> Vec<&'a NewOrderRequirement> {
    frozen
        .requirements
        .iter()
        .filter(|r| r.child_occurrence_key.as_deref() == Some(op.occurrence_key.as_str()))
        .collect()
}

fn assert_conserved(frozen: &FrozenPlan, op: &NewPlanOperation) {
    let shares: Decimal = served_by(frozen, op)
        .iter()
        .map(|r| r.child_consumed_cost.expect("share").0)
        .sum();
    assert_eq!(
        shares + op.surplus_retained_basis.expect("retained").0,
        op.total_production_cost.expect("total").0,
        "{} conserves",
        op.occurrence_key
    );
}

fn dag(frozen: &FrozenPlan) -> OperationDag {
    derive_operation_dag(
        frozen
            .operations
            .iter()
            .map(|op| op.occurrence_key.as_str()),
        frozen.requirements.iter().filter_map(|r| {
            Some(FrozenDemandEdge {
                consumer: r.operation_occurrence_key.as_deref()?,
                producer: r.child_occurrence_key.as_deref()?,
                dependency_id: r.dependency_id.as_deref(),
            })
        }),
    )
    .unwrap()
}

/// Root -> A (201), B (202); A needs 20 X, B needs 30 X; X yields
/// 10/run from 1 Buy unit (34 @ 5) per unit of output.
fn two_consumer_plan(
    a_scope: FulfillmentScope,
    a_planned: u64,
    b_scope: FulfillmentScope,
    b_planned: u64,
) -> (
    Vec<VerificationOperationInput>,
    Vec<VerificationBoundaryInput>,
) {
    let shortage = (20 - a_planned) + (30 - b_planned);
    let runs = shortage.div_ceil(10);
    let operations = vec![
        root(),
        producer(1, 201, 1, 1, "0", &[(0, 0, "pd:root-a")]),
        producer(2, 202, 1, 1, "0", &[(1, 0, "pd:root-b")]),
        producer(3, X, runs, 10, "0", &[(2, 1, "pd:a-x"), (3, 2, "pd:b-x")]),
    ];
    let boundaries = vec![
        demand(0, 0, 201, FulfillmentScope::Missing, 1, 0, "pd:root-a"),
        demand(1, 0, 202, FulfillmentScope::Missing, 1, 0, "pd:root-b"),
        demand(2, 1, X, a_scope, 20, a_planned, "pd:a-x"),
        demand(3, 2, X, b_scope, 30, b_planned, "pd:b-x"),
        buy(4, 3, 34, runs * 10),
    ];
    (operations, boundaries)
}

fn x_basis(quantity: u64) -> Vec<InventoryBasisEntry> {
    vec![InventoryBasisEntry {
        type_id: X,
        quantity,
        reserved_quantity: 0,
        unit_basis: Some(Decimal::from(3)),
        total_basis: Decimal::from(3 * quantity),
    }]
}

#[test]
fn canonical_shared_operation_freezes_once_and_serves_two_requirements() {
    let (operations, boundaries) =
        two_consumer_plan(FulfillmentScope::Missing, 0, FulfillmentScope::Missing, 0);
    let frozen = freeze_canonical_plan(&operations, &boundaries, &[]).unwrap();

    assert_eq!(frozen.operations.len(), 4, "root, A, B, X -- X once");
    assert!(frozen.operations[0]
        .occurrence_key
        .starts_with(ROOT_OCCURRENCE_PREFIX));
    let x = op_by_type(&frozen, X);
    assert_eq!(
        x.len(),
        1,
        "one ProductionOperation, never one per consumer"
    );
    let x = x[0];
    assert_eq!(x.runs, 5);
    assert_eq!(x.produced_quantity, 50);
    assert_eq!(x.consumed_quantity, Some(50));
    assert_eq!(x.surplus_quantity, Some(0));
    assert_eq!(x.total_production_cost, Some(money("250")));
    assert_eq!(x.surplus_retained_basis, Some(money("0")));
    assert_eq!(
        x.parent_occurrence_key, None,
        "fan-in: no arbitrary consumer"
    );

    let a = op_by_type(&frozen, 201)[0];
    assert_eq!(
        a.parent_occurrence_key.as_deref(),
        Some(frozen.operations[0].occurrence_key.as_str()),
        "one consumer: a genuine parent"
    );
    assert_eq!(a.consumed_quantity, Some(1));

    let served = served_by(&frozen, x);
    assert_eq!(served.len(), 2, "two frozen requirements, one operation");
    let from_a = served
        .iter()
        .find(|r| r.dependency_id.as_deref() == Some("pd:a-x"))
        .unwrap();
    let from_b = served
        .iter()
        .find(|r| r.dependency_id.as_deref() == Some("pd:b-x"))
        .unwrap();
    assert_eq!(
        from_a.operation_occurrence_key.as_deref(),
        Some(a.occurrence_key.as_str())
    );
    assert_eq!(from_a.required_quantity, 20);
    assert_eq!(from_a.child_consumed_quantity, Some(20));
    assert_eq!(from_a.child_consumed_cost, Some(money("100")));
    assert_eq!(from_b.child_consumed_quantity, Some(30));
    assert_eq!(from_b.child_consumed_cost, Some(money("150")));
    assert_eq!(from_a.source_build_id, Some(x.build_id));
    assert_eq!(from_b.source_build_id, Some(x.build_id));
    assert_conserved(&frozen, x);

    let dag = dag(&frozen);
    assert_eq!(dag.root_occurrence_key, frozen.operations[0].occurrence_key);
    assert_eq!(dag.stages[&x.occurrence_key], 0);
    assert_eq!(dag.stages[&a.occurrence_key], 1);
    assert_eq!(dag.stages[&frozen.operations[0].occurrence_key], 2);
    assert_eq!(dag.order.first(), Some(&x.occurrence_key));
    assert_eq!(dag.dependencies.len(), 4, "X->A, X->B, A->root, B->root");
}

#[test]
fn canonical_inventory_edge_nets_before_the_shared_operation_is_sized() {
    // 15 X on hand: A's edge (traversal first) reuses 15, needs 5 fresh;
    // B needs 30 -- X plans 35 -> 4 runs -> 40, surplus 5.
    let (operations, boundaries) =
        two_consumer_plan(FulfillmentScope::Missing, 15, FulfillmentScope::Missing, 0);
    let frozen = freeze_canonical_plan(&operations, &boundaries, &x_basis(15)).unwrap();

    let x = op_by_type(&frozen, X)[0];
    assert_eq!(x.runs, 4);
    assert_eq!(x.produced_quantity, 40);
    assert_eq!(x.consumed_quantity, Some(35));
    assert_eq!(x.surplus_quantity, Some(5), "the operation's one surplus");
    assert_eq!(x.total_production_cost, Some(money("200")));
    assert_eq!(x.surplus_retained_basis, Some(money("25")));

    let served = served_by(&frozen, x);
    let from_a = served
        .iter()
        .find(|r| r.dependency_id.as_deref() == Some("pd:a-x"))
        .unwrap();
    assert_eq!(from_a.reused_quantity, 15);
    assert_eq!(from_a.inventory_unit_basis, Some(money("3")));
    assert_eq!(from_a.reused_line_total, Some(money("45")));
    assert_eq!(from_a.child_consumed_quantity, Some(5));
    assert_eq!(from_a.child_consumed_cost, Some(money("25")));
    assert_eq!(
        from_a.estimated_line_total,
        Some(money("70")),
        "45 inventory + 25 share"
    );
    let from_b = served
        .iter()
        .find(|r| r.dependency_id.as_deref() == Some("pd:b-x"))
        .unwrap();
    assert_eq!(from_b.reused_quantity, 0);
    assert_eq!(from_b.child_consumed_cost, Some(money("150")));
    assert_conserved(&frozen, x);
}

#[test]
fn canonical_mixed_full_and_missing_consumers_share_one_operation() {
    // A is Full-scoped (ignores the 15 on hand), B is Missing (reuses them).
    let (operations, boundaries) =
        two_consumer_plan(FulfillmentScope::Full, 0, FulfillmentScope::Missing, 15);
    let frozen = freeze_canonical_plan(&operations, &boundaries, &x_basis(15)).unwrap();

    let x = op_by_type(&frozen, X);
    assert_eq!(x.len(), 1);
    let x = x[0];
    assert_eq!(x.consumed_quantity, Some(35));
    let served = served_by(&frozen, x);
    let from_a = served
        .iter()
        .find(|r| r.dependency_id.as_deref() == Some("pd:a-x"))
        .unwrap();
    let from_b = served
        .iter()
        .find(|r| r.dependency_id.as_deref() == Some("pd:b-x"))
        .unwrap();
    assert_eq!(from_a.fulfillment_scope, FulfillmentScope::Full);
    assert_eq!(from_a.reused_quantity, 0);
    assert_eq!(from_a.reused_line_total, None);
    assert_eq!(from_a.child_consumed_quantity, Some(20));
    assert_eq!(from_b.fulfillment_scope, FulfillmentScope::Missing);
    assert_eq!(from_b.reused_quantity, 15);
    assert_eq!(from_b.child_consumed_quantity, Some(15));
    assert_conserved(&frozen, x);
}

#[test]
fn canonical_nested_shared_producers_freeze_one_operation_each() {
    // X is shared by A and B; Y is shared by X and the root itself.
    let operations = vec![
        root(),
        producer(1, 201, 1, 1, "0", &[(0, 0, "pd:root-a")]),
        producer(2, 202, 1, 1, "0", &[(1, 0, "pd:root-b")]),
        producer(3, X, 1, 10, "10", &[(3, 1, "pd:a-x"), (4, 2, "pd:b-x")]),
        producer(4, Y, 1, 7, "7", &[(2, 0, "pd:root-y"), (5, 3, "pd:x-y")]),
    ];
    let boundaries = vec![
        demand(0, 0, 201, FulfillmentScope::Missing, 1, 0, "pd:root-a"),
        demand(1, 0, 202, FulfillmentScope::Missing, 1, 0, "pd:root-b"),
        demand(2, 0, Y, FulfillmentScope::Missing, 2, 0, "pd:root-y"),
        demand(3, 1, X, FulfillmentScope::Missing, 4, 0, "pd:a-x"),
        demand(4, 2, X, FulfillmentScope::Missing, 6, 0, "pd:b-x"),
        demand(5, 3, Y, FulfillmentScope::Missing, 5, 0, "pd:x-y"),
    ];
    let frozen = freeze_canonical_plan(&operations, &boundaries, &[]).unwrap();

    assert_eq!(frozen.operations.len(), 5);
    let x = op_by_type(&frozen, X)[0];
    let y = op_by_type(&frozen, Y)[0];
    assert_eq!(served_by(&frozen, x).len(), 2);
    assert_eq!(served_by(&frozen, y).len(), 2);
    assert_eq!(x.parent_occurrence_key, None);
    assert_eq!(y.parent_occurrence_key, None);
    assert_eq!(y.consumed_quantity, Some(7));
    assert_eq!(y.surplus_quantity, Some(0));
    // X's total includes its consumed share of Y: 7 * 5/7 = 5, + 10 own.
    assert_eq!(x.total_production_cost, Some(money("15")));
    assert_conserved(&frozen, x);
    assert_conserved(&frozen, y);

    let dag = dag(&frozen);
    assert_eq!(dag.stages[&y.occurrence_key], 0);
    assert_eq!(dag.stages[&x.occurrence_key], 1);
    assert_eq!(dag.stages[&frozen.operations[0].occurrence_key], 3);
    let root_key = &frozen.operations[0].occurrence_key;
    assert_eq!(dag.order.last(), Some(root_key));
}

#[test]
fn canonical_three_consumers_conserve_the_rounding_remainder() {
    // Cost regression: P costs exactly 1.0000 and yields 3; each of three
    // consumers takes 1 -> 0.3333 each, the one operation retains 0.0001.
    let operations = vec![
        root(),
        producer(1, 201, 1, 1, "0", &[(0, 0, "pd:root-c1")]),
        producer(2, 202, 1, 1, "0", &[(1, 0, "pd:root-c2")]),
        producer(3, 203, 1, 1, "0", &[(2, 0, "pd:root-c3")]),
        producer(
            4,
            X,
            1,
            3,
            "1",
            &[(3, 1, "pd:c1"), (4, 2, "pd:c2"), (5, 3, "pd:c3")],
        ),
    ];
    let boundaries = vec![
        demand(0, 0, 201, FulfillmentScope::Missing, 1, 0, "pd:root-c1"),
        demand(1, 0, 202, FulfillmentScope::Missing, 1, 0, "pd:root-c2"),
        demand(2, 0, 203, FulfillmentScope::Missing, 1, 0, "pd:root-c3"),
        demand(3, 1, X, FulfillmentScope::Missing, 1, 0, "pd:c1"),
        demand(4, 2, X, FulfillmentScope::Missing, 1, 0, "pd:c2"),
        demand(5, 3, X, FulfillmentScope::Missing, 1, 0, "pd:c3"),
    ];
    let frozen = freeze_canonical_plan(&operations, &boundaries, &[]).unwrap();

    let x = op_by_type(&frozen, X);
    assert_eq!(x.len(), 1);
    let x = x[0];
    let served = served_by(&frozen, x);
    assert_eq!(served.len(), 3);
    for requirement in &served {
        assert_eq!(requirement.child_consumed_cost, Some(money("0.3333")));
    }
    assert_eq!(x.total_production_cost, Some(money("1")));
    assert_eq!(x.surplus_quantity, Some(0));
    assert_eq!(x.surplus_retained_basis, Some(money("0.0001")));
    assert_conserved(&frozen, x);
}

#[test]
fn canonical_fully_covered_edge_keeps_its_producer_identity() {
    let producer_build = BuildId::new();
    let mut covered = demand(0, 0, 201, FulfillmentScope::Missing, 5, 5, "pd:root-a");
    covered.producer_build_id = Some(producer_build);
    let frozen = freeze_canonical_plan(&[root()], &[covered], &[]).unwrap();

    assert_eq!(frozen.operations.len(), 1);
    let requirement = &frozen.requirements[0];
    assert_eq!(requirement.child_occurrence_key, None);
    assert_eq!(requirement.source_build_id, Some(producer_build));
    assert_eq!(requirement.dependency_id.as_deref(), Some("pd:root-a"));
}

fn assert_corrupt(result: Result<FrozenPlan, OrderError>) {
    match result {
        Err(OrderError::CorruptProductionGraph { .. }) => {}
        other => panic!("expected CorruptProductionGraph, got {other:?}"),
    }
}

#[test]
fn canonical_corrupt_graphs_are_refused_with_a_typed_error() {
    let (operations, boundaries) =
        two_consumer_plan(FulfillmentScope::Missing, 0, FulfillmentScope::Missing, 0);

    // A demand edge naming an unknown consumer operation.
    let mut unknown_consumer = operations.clone();
    unknown_consumer[3].incoming[1].consumer_op_index = 99;
    assert_corrupt(freeze_canonical_plan(&unknown_consumer, &boundaries, &[]));

    // Two producers claiming the same requirement.
    let mut double_claim = operations.clone();
    double_claim[2].incoming[0].traversal_index = 0;
    double_claim[2].product_type_id = 201;
    assert_corrupt(freeze_canonical_plan(&double_claim, &boundaries, &[]));

    // A non-root operation that serves nothing (two roots).
    let mut orphan = operations.clone();
    orphan[3].incoming.clear();
    assert_corrupt(freeze_canonical_plan(&orphan, &boundaries, &[]));

    // A production shortage no producer serves.
    let mut unserved = operations.clone();
    unserved[3].incoming.truncate(1);
    assert_corrupt(freeze_canonical_plan(&unserved, &boundaries, &[]));

    // A producer yielding less than its consumers need.
    let mut short = operations;
    short[3].node_runs = 4;
    assert_corrupt(freeze_canonical_plan(&short, &boundaries, &[]));
}

#[test]
fn derive_operation_dag_refuses_cycles_and_unknown_operations() {
    let cyclic = derive_operation_dag(
        ["root:r", "build:a", "build:b"],
        [
            FrozenDemandEdge {
                consumer: "root:r",
                producer: "build:a",
                dependency_id: None,
            },
            FrozenDemandEdge {
                consumer: "build:a",
                producer: "build:b",
                dependency_id: None,
            },
            FrozenDemandEdge {
                consumer: "build:b",
                producer: "build:a",
                dependency_id: None,
            },
        ],
    );
    assert!(matches!(
        cyclic,
        Err(OrderError::CorruptProductionGraph { .. })
    ));

    let unknown = derive_operation_dag(
        ["root:r"],
        [FrozenDemandEdge {
            consumer: "root:r",
            producer: "build:ghost",
            dependency_id: None,
        }],
    );
    assert!(matches!(
        unknown,
        Err(OrderError::CorruptProductionGraph { .. })
    ));
}

#[test]
fn canonical_freeze_refuses_a_non_conserving_cost_projection() {
    let (operations, boundaries) =
        two_consumer_plan(FulfillmentScope::Missing, 0, FulfillmentScope::Missing, 0);
    let adjusted: BTreeMap<i64, Decimal> = boundaries
        .iter()
        .map(|b| (b.type_id, Decimal::ZERO))
        .collect();
    let mut cost = project_build_cost(&operations, &boundaries, &[], &adjusted, None);
    let edge = cost
        .boundaries
        .iter_mut()
        .find(|b| b.traversal_index == 3)
        .unwrap();
    edge.child_consumed_cost = Some(money("149.9999"));

    let result = freeze_order_plan(&operations, &boundaries, &cost, &[]);
    assert!(matches!(
        result,
        Err(OrderError::FrozenCostNotConserved { .. })
    ));
}
