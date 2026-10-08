//! Regression matrix for [`project_build_cost`]: pure-function tests
//! over hand-built allocation-aware quantity evidence. Every arithmetic path,
//! every completeness path, the surplus-conservation invariant, the whole-plan
//! reconciliation identity, and the bottom-up ordering invariant.

use std::collections::BTreeMap;

use rust_decimal::Decimal;

use super::*;
use crate::build_materials::{
    InventoryBasisEntry, MaterialActivity, MaterialBoundaryResolution, VerificationBoundaryInput,
    VerificationOperationInput,
};
use crate::industry::{BuildId, Money};
use crate::FulfillmentScope;

// ---------------------------------------------------------------------------
// builders
// ---------------------------------------------------------------------------

fn money(value: &str) -> Money {
    Money::parse(value).unwrap()
}
fn dec(value: &str) -> Decimal {
    value.parse().unwrap()
}

/// An operation with **no facility** -- installation is always incomplete, so
/// the operation is incomplete, but every material-side figure is exercised.
fn op(
    op_index: u32,
    parent: Option<u32>,
    node_runs: u64,
    output_per_run: u64,
) -> VerificationOperationInput {
    VerificationOperationInput {
        op_index,
        parent_op_index: parent,
        parent_traversal_index: None,
        incoming: Vec::new(),
        graph_node_id: if parent.is_none() {
            "root:00000000-0000-0000-0000-000000000000".to_string()
        } else {
            format!("build:00000000-0000-0000-0000-{op_index:012}")
        },
        build_id: BuildId::default(),
        revision: 1,
        tree_path: Vec::new(),
        activity: MaterialActivity::Manufacturing,
        product_type_id: 90_000 + i64::from(op_index),
        product_name: format!("Product {op_index}"),
        output_per_run,
        base_material_count: 1,
        blueprint_or_formula_type_id: 10_000 + i64::from(op_index),
        blueprint_or_formula_name: format!("Blueprint {op_index}"),
        node_runs,
        persisted_runs: node_runs,
        recipe_currency: crate::RecipeCurrency::Current,
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

/// Attach a facility with the given cost primitives (percentages as plain
/// numbers, e.g. `"1.5"` == 1.5 %). `system_cost_index` is a fraction
/// (`"0.05"` == 5 %).
fn with_facility(
    mut operation: VerificationOperationInput,
    system_cost_index: Option<&str>,
    job_cost_reduction_percent: &str,
    facility_tax_percent: &str,
    scc_surcharge_percent: &str,
    alliance_surcharge_percent: &str,
    fixed_supplemental_cost: &str,
) -> VerificationOperationInput {
    operation.facility_id = Some(uuid::Uuid::from_u128(u128::from(operation.op_index) + 1));
    operation.facility_name = Some("Test Facility".to_string());
    operation.facility_profile_revision = Some(1);
    operation.system_cost_index = system_cost_index.map(dec);
    operation.job_cost_reduction_percent = dec(job_cost_reduction_percent);
    operation.facility_tax_percent = dec(facility_tax_percent);
    operation.scc_surcharge_percent = dec(scc_surcharge_percent);
    operation.alliance_surcharge_percent = dec(alliance_surcharge_percent);
    operation.fixed_supplemental_cost = money(fixed_supplemental_cost);
    operation.installation_formula_version = "eve-manufacturing-facility-v1".to_string();
    operation
}

#[allow(clippy::too_many_arguments)]
fn boundary(
    traversal_index: u32,
    op_index: u32,
    parent_traversal_index: Option<u32>,
    type_id: i64,
    resolution: MaterialBoundaryResolution,
    scope: FulfillmentScope,
    base_quantity_per_run: u64,
    api_required: u64,
    api_planned_use: u64,
    api_shortage: u64,
) -> VerificationBoundaryInput {
    VerificationBoundaryInput {
        traversal_index,
        parent_traversal_index,
        op_index,
        build_id: BuildId::default(),
        graph_node_id: "root:00000000-0000-0000-0000-000000000000".to_string(),
        tree_path: Vec::new(),
        type_id,
        type_name: format!("Type {type_id}"),
        activity: MaterialActivity::Manufacturing,
        resolution,
        scope,
        node_runs: 1,
        base_quantity_per_run,
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
        fresh_unit_price: None,
        fresh_price_selection: crate::PricingSelectionKind::Default,
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

fn with_price(
    mut boundary: VerificationBoundaryInput,
    unit_price: &str,
) -> VerificationBoundaryInput {
    boundary.fresh_unit_price = Some(money(unit_price));
    boundary
}

fn basis(type_id: i64, unit_basis: Option<&str>) -> InventoryBasisEntry {
    InventoryBasisEntry {
        type_id,
        quantity: 0,
        unit_basis: unit_basis.map(dec),
        total_basis: Decimal::ZERO,
    }
}

fn run(
    operations: &[VerificationOperationInput],
    boundaries: &[VerificationBoundaryInput],
    inventory_basis: &[InventoryBasisEntry],
    adjusted_prices: &[(i64, &str)],
) -> BuildCostProjection {
    let adjusted: BTreeMap<i64, Decimal> = adjusted_prices
        .iter()
        .map(|(type_id, value)| (*type_id, dec(value)))
        .collect();
    project_build_cost(operations, boundaries, inventory_basis, &adjusted, None)
}

fn op_cost(projection: &BuildCostProjection, op_index: u32) -> &OperationCostProjection {
    projection
        .operations
        .iter()
        .find(|operation| operation.op_index == op_index)
        .unwrap_or_else(|| panic!("no operation {op_index}"))
}
fn b_cost(projection: &BuildCostProjection, traversal_index: u32) -> &BoundaryCostProjection {
    projection
        .boundaries
        .iter()
        .find(|boundary| boundary.traversal_index == traversal_index)
        .unwrap_or_else(|| panic!("no boundary {traversal_index}"))
}

// ---------------------------------------------------------------------------
// conservation asserts
// ---------------------------------------------------------------------------

/// Every conservation identity that must hold *exactly* at Money scale for a
/// complete projection.
fn assert_conservation(projection: &BuildCostProjection) {
    for boundary in &projection.boundaries {
        if !boundary.complete {
            continue;
        }
        match boundary.kind {
            BoundaryCostKind::Buy => {
                assert_eq!(
                    add(boundary.inventory_cost, boundary.fresh_cost),
                    boundary.requirement_cost,
                    "Buy boundary {} decomposition",
                    boundary.traversal_index
                );
            }
            BoundaryCostKind::Build | BoundaryCostKind::Reaction => {
                assert_eq!(
                    add(boundary.inventory_cost, boundary.child_consumed_cost),
                    boundary.requirement_cost,
                    "Build boundary {} decomposition",
                    boundary.traversal_index
                );
                // Child production conservation -- EXACT.
                if let (Some(total), Some(consumed), Some(surplus)) = (
                    boundary.child_total_production_cost,
                    boundary.child_consumed_cost,
                    boundary.child_surplus_retained_basis,
                ) {
                    assert_eq!(
                        consumed.0 + surplus.0,
                        total.0,
                        "child conservation at boundary {}",
                        boundary.traversal_index
                    );
                }
            }
            BoundaryCostKind::FullyCovered => {
                assert_eq!(
                    boundary.inventory_cost, boundary.requirement_cost,
                    "fully-covered boundary {}",
                    boundary.traversal_index
                );
            }
            BoundaryCostKind::Unresolved => {}
        }
    }

    for operation in &projection.operations {
        if !operation.complete {
            continue;
        }
        let sum_requirements: Decimal = projection
            .boundaries
            .iter()
            .filter(|boundary| boundary.op_index == operation.op_index)
            .map(|boundary| boundary.requirement_cost.unwrap().0)
            .sum();
        assert_eq!(
            Some(Money(sum_requirements)),
            operation.material_component_cost,
            "operation {} Σ requirement_cost == material_component_cost",
            operation.op_index
        );
        assert_eq!(
            operation.direct_inventory_cost.0
                + operation.direct_buy_cost.0
                + operation.consumed_child_cost.0,
            operation.material_component_cost.unwrap().0,
            "operation {} directs sum to material_component_cost",
            operation.op_index
        );
        assert_eq!(
            add(
                operation.material_component_cost,
                operation.own_installation.total
            ),
            operation.total_production_cost,
            "operation {} total = material + installation",
            operation.op_index
        );
    }

    // Root == root operation total; NOT the sum of all operation totals.
    let root_total = op_cost(projection, 0).total_production_cost;
    assert_eq!(
        projection.root.planning_total_production_cost, root_total,
        "root == root operation total"
    );
    if projection.operations.len() > 1 && projection.complete {
        let naive_sum: Decimal = projection
            .operations
            .iter()
            .map(|operation| operation.total_production_cost.unwrap().0)
            .sum();
        assert_ne!(
            Some(naive_sum),
            root_total.map(|money| money.0),
            "summing all operation totals double-counts descendants"
        );
    }
}

/// Whole-plan reconciliation -- EXACT for a complete projection:
/// `fresh_outlay + inventory_basis_consumed == root_total + surplus_retained_basis`.
fn assert_reconciliation(projection: &BuildCostProjection) {
    if !projection.complete {
        return;
    }
    let lhs =
        projection.root.total_fresh_outlay.0 + projection.root.total_inventory_basis_consumed.0;
    let rhs = projection.root.planning_total_production_cost.unwrap().0
        + projection.root.total_surplus_retained_basis.0;
    assert_eq!(
        lhs, rhs,
        "resources consumed == root cost + retained surplus asset"
    );
}

fn add(a: Option<Money>, b: Option<Money>) -> Option<Money> {
    match (a, b) {
        (Some(a), Some(b)) => Some(Money(a.0 + b.0)),
        _ => None,
    }
}

// Multi-BPC job split: the fixed per-job fee is charged once per job; the
// EIV-proportional components are unchanged (EIV is linear in runs).
#[test]
fn fixed_installation_fee_is_charged_per_job() {
    let mut operation = with_facility(op(0, None, 4, 1), Some("0.05"), "0", "0", "0", "0", "100");
    operation.job_count = 4;
    let boundaries = vec![with_price(
        boundary(
            0,
            0,
            None,
            34,
            MaterialBoundaryResolution::Buy,
            FulfillmentScope::Missing,
            100,
            400,
            0,
            400,
        ),
        "8",
    )];
    let projection = run(&[operation], &boundaries, &[basis(34, None)], &[(34, "5")]);

    let root = op_cost(&projection, 0);
    // EIV = 5 * 100 * 4 = 2000; system index = 100; fixed = 4 jobs x 100.
    assert_eq!(root.own_installation.eiv, Some(money("2000")));
    assert_eq!(root.own_installation.fixed_supplemental_cost, money("400"));
    assert_eq!(root.own_installation.total, Some(money("500")));
}

// ---------------------------------------------------------------------------
// Simple Buy, no inventory
// ---------------------------------------------------------------------------

#[test]
fn simple_buy_no_inventory() {
    let operations = vec![with_facility(
        op(0, None, 1, 1),
        Some("0.05"),
        "0",
        "1",
        "1.5",
        "0",
        "0",
    )];
    let boundaries = vec![with_price(
        boundary(
            0,
            0,
            None,
            34,
            MaterialBoundaryResolution::Buy,
            FulfillmentScope::Missing,
            100,
            100,
            0,
            100,
        ),
        "8",
    )];
    let projection = run(&operations, &boundaries, &[basis(34, None)], &[(34, "5")]);

    let boundary = b_cost(&projection, 0);
    assert_eq!(boundary.inventory_cost, Some(Money::zero()));
    assert_eq!(boundary.fresh_cost, Some(money("800"))); // 100 * 8
    assert_eq!(boundary.requirement_cost, Some(money("800")));
    assert!(boundary.complete);

    let root = op_cost(&projection, 0);
    assert_eq!(root.material_component_cost, Some(money("800")));
    // args: (sci 0.05, jcr 0, tax 1%, scc 1.5%, alliance 0, fixed 0).
    // EIV = 5 * 100 * 1 = 500; system_index = 500*0.05*1 = 25; tax = 500*1% = 5;
    // scc = 500*1.5% = 7.5; alliance 0; total = 25 + 5 + 7.5 = 37.5.
    assert_eq!(root.own_installation.eiv, Some(money("500")));
    assert_eq!(root.own_installation.system_index_cost, Some(money("25")));
    assert_eq!(root.own_installation.facility_tax, Some(money("5")));
    assert_eq!(root.own_installation.scc_surcharge, Some(money("7.5")));
    assert_eq!(root.own_installation.total, Some(money("37.5")));
    assert_eq!(root.total_production_cost, Some(money("837.5")));
    assert!(root.complete);
    assert!(projection.complete);
    assert_conservation(&projection);
    assert_reconciliation(&projection);
}

// ---------------------------------------------------------------------------
// Partial Buy inventory
// ---------------------------------------------------------------------------

#[test]
fn partial_buy_inventory() {
    let operations = vec![op(0, None, 1, 1)];
    let boundaries = vec![with_price(
        boundary(
            0,
            0,
            None,
            34,
            MaterialBoundaryResolution::Buy,
            FulfillmentScope::Missing,
            1_000,
            1_000,
            400,
            600,
        ),
        "8",
    )];
    let projection = run(&operations, &boundaries, &[basis(34, Some("5"))], &[]);

    let boundary = b_cost(&projection, 0);
    assert_eq!(boundary.inventory_cost, Some(money("2000"))); // 400 * 5
    assert_eq!(boundary.fresh_cost, Some(money("4800"))); // 600 * 8
    assert_eq!(boundary.requirement_cost, Some(money("6800")));
    assert_eq!(
        op_cost(&projection, 0).material_component_cost,
        Some(money("6800"))
    );
    assert_conservation(&projection);
}

// ---------------------------------------------------------------------------
// Full-scope Buy despite inventory
// ---------------------------------------------------------------------------

#[test]
fn full_scope_buy_ignores_inventory() {
    let operations = vec![op(0, None, 1, 1)];
    // Full scope => allocate() returns (0, required): planned_use 0, shortage = required.
    let boundaries = vec![with_price(
        boundary(
            0,
            0,
            None,
            34,
            MaterialBoundaryResolution::Buy,
            FulfillmentScope::Full,
            1_000,
            1_000,
            0,
            1_000,
        ),
        "8",
    )];
    let projection = run(&operations, &boundaries, &[basis(34, Some("5"))], &[]);

    let boundary = b_cost(&projection, 0);
    assert_eq!(boundary.inventory_cost, Some(Money::zero()));
    assert_eq!(boundary.fresh_cost, Some(money("8000")));
    assert_eq!(boundary.requirement_cost, Some(money("8000")));
    assert_conservation(&projection);
}

// ---------------------------------------------------------------------------
// Build child, output divides shortage exactly (no surplus)
// ---------------------------------------------------------------------------

#[test]
fn build_child_exact_output_no_surplus() {
    // root (op0) needs 500 of type 90_001 via Build; child (op1) produces
    // exactly 500 (5 runs * 100/run).
    let operations = vec![op(0, None, 1, 1), op(1, Some(0), 5, 100)];
    let boundaries = vec![
        // op0's Build boundary for 90_001, shortage 500 => child sized.
        boundary(
            0,
            0,
            None,
            90_001,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Missing,
            500,
            500,
            0,
            500,
        ),
        // op1's own Buy boundary: 500 units of type 34 @ 3.
        with_price(
            boundary(
                1,
                1,
                Some(0),
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                500,
                500,
                0,
                500,
            ),
            "3",
        ),
    ];
    let projection = run(
        &operations,
        &boundaries,
        &[basis(34, None), basis(90_001, None)],
        &[],
    );

    let child = op_cost(&projection, 1);
    assert_eq!(child.material_component_cost, Some(money("1500"))); // 500 * 3
                                                                    // no facility on child => installation incomplete => child.total None, so
                                                                    // the parent boundary is ChildCostIncomplete. Give the child a facility.
    assert!(!child.complete);

    // Re-run with facilities so totals resolve.
    let operations = vec![
        with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"),
        with_facility(op(1, Some(0), 5, 100), Some("0"), "0", "0", "0", "0", "0"),
    ];
    let projection = run(
        &operations,
        &boundaries,
        &[basis(34, None), basis(90_001, None)],
        &[(34, "3"), (90_001, "0")],
    );
    let child = op_cost(&projection, 1);
    assert_eq!(child.total_production_cost, Some(money("1500"))); // materials 1500 + install 0
    assert_eq!(child.produced_quantity, 500);

    let boundary = b_cost(&projection, 0);
    assert_eq!(boundary.child_op_index, Some(1));
    assert_eq!(boundary.child_produced_quantity, 500);
    assert_eq!(boundary.child_consumed_quantity, 500);
    assert_eq!(boundary.child_consumed_cost, Some(money("1500"))); // 1500 * 500/500
    assert_eq!(boundary.child_surplus_quantity, 0);
    assert_eq!(boundary.child_surplus_retained_basis, Some(Money::zero()));
    assert_eq!(boundary.requirement_cost, Some(money("1500")));

    let root = op_cost(&projection, 0);
    assert_eq!(root.total_production_cost, Some(money("1500")));
    assert!(projection.complete);
    assert_conservation(&projection);
    assert_reconciliation(&projection);
}

// ---------------------------------------------------------------------------
// Build child with unavoidable surplus
// ---------------------------------------------------------------------------

#[test]
fn build_child_with_surplus_proportional_not_whole_job() {
    // Parent needs 500; child output/run 1_000; child runs 1 => produced 1_000.
    // child materials 800_000, child installation 200_000 => child total 1_000_000.
    let operations = vec![
        with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"),
        // installation: EIV of the child = adjusted_price[34] * base_qpr * runs.
        // Set adjusted_price[34]=0 and add fixed_supplemental 200_000 so the
        // child installation total is exactly 200_000.
        with_facility(
            op(1, Some(0), 1, 1_000),
            Some("0"),
            "0",
            "0",
            "0",
            "0",
            "200000",
        ),
    ];
    let boundaries = vec![
        boundary(
            0,
            0,
            None,
            90_001,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Missing,
            500,
            500,
            0,
            500,
        ),
        with_price(
            boundary(
                1,
                1,
                Some(0),
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                800_000,
                800_000,
                0,
                800_000,
            ),
            "1",
        ),
    ];
    let projection = run(
        &operations,
        &boundaries,
        &[basis(34, None), basis(90_001, None)],
        &[(34, "0"), (90_001, "0")],
    );

    let child = op_cost(&projection, 1);
    assert_eq!(child.material_component_cost, Some(money("800000")));
    assert_eq!(child.own_installation.total, Some(money("200000")));
    assert_eq!(child.total_production_cost, Some(money("1000000")));
    assert_eq!(child.produced_quantity, 1_000);
    assert_eq!(child.unit_production_cost, Some(money("1000")));

    let boundary = b_cost(&projection, 0);
    assert_eq!(boundary.child_consumed_quantity, 500);
    assert_eq!(
        boundary.child_consumed_cost,
        Some(money("500000")),
        "500/1000 of 1_000_000"
    );
    assert_eq!(boundary.child_surplus_quantity, 500);
    assert_eq!(boundary.child_surplus_retained_basis, Some(money("500000")));
    assert_eq!(
        boundary.requirement_cost,
        Some(money("500000")),
        "NOT 1_000_000"
    );

    let root = op_cost(&projection, 0);
    assert_eq!(root.total_production_cost, Some(money("500000")));
    assert_eq!(
        projection.root.total_surplus_retained_basis,
        money("500000")
    );
    // Root does NOT include retained surplus basis.
    assert_eq!(
        projection.root.planning_total_production_cost,
        Some(money("500000"))
    );
    assert_conservation(&projection);
    assert_reconciliation(&projection);
}

// ---------------------------------------------------------------------------
// Core regression: physical surplus quantity must not depend on cost
// completeness. A child with NO facility (installation always incomplete,
// so `total_production_cost` is always `None`) still has a fully known
// `produced_quantity` -- `child_surplus_quantity` must reflect the real
// physical overproduction, never fall back to `0` merely because the cost
// side is unknown. `child_surplus_retained_basis`, by contrast, has no
// meaning without a known child total and must stay `None`.
// ---------------------------------------------------------------------------

#[test]
fn child_surplus_quantity_is_known_even_when_child_cost_is_incomplete() {
    // Child: no facility at all -> installation always incomplete ->
    // total_production_cost is always None, regardless of pricing. Runs 1,
    // output/run 1_000 -> produced 1_000. Parent consumes 600 (shortage) ->
    // physical surplus = 400.
    let operations = vec![
        op(0, None, 1, 1),
        op(1, Some(0), 1, 1_000), // no facility -- `op()`'s own default
    ];
    let boundaries = vec![
        boundary(
            0,
            0,
            None,
            90_001,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Missing,
            600,
            600,
            0,
            600,
        ),
        with_price(
            boundary(
                1,
                1,
                Some(0),
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                1,
                1,
                0,
                1,
            ),
            "1",
        ),
    ];
    let projection = run(&operations, &boundaries, &[basis(34, None)], &[(34, "0")]);

    let child = op_cost(&projection, 1);
    assert_eq!(
        child.total_production_cost, None,
        "no facility -> installation incomplete -> total unknown"
    );
    assert_eq!(child.produced_quantity, 1_000, "produced is always known");
    assert!(!child.complete);

    let boundary = b_cost(&projection, 0);
    assert_eq!(
        boundary.child_surplus_quantity, 400,
        "physical surplus must be known even though the child's cost is not"
    );
    assert_eq!(
        boundary.child_surplus_retained_basis, None,
        "the cost BASIS of that surplus is genuinely unknown -- never fabricated"
    );
    assert_eq!(boundary.child_consumed_cost, None);
    assert!(!boundary.complete);
    assert!(!projection.complete);
}

// ---------------------------------------------------------------------------
// Realistic surplus fixture: big output_per_run, non-trivial rounding
// ---------------------------------------------------------------------------

#[test]
fn realistic_surplus_rounding_and_conservation() {
    // parent shortage 1_283; child output/run 10_000; child runs 1.
    // child material 34: 10_000 units @ 7.31 => 73_100 ; + fixed install 12_345.6789.
    let operations = vec![
        with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"),
        with_facility(
            op(1, Some(0), 1, 10_000),
            Some("0"),
            "0",
            "0",
            "0",
            "0",
            "12345.6789",
        ),
    ];
    let boundaries = vec![
        boundary(
            0,
            0,
            None,
            90_001,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Missing,
            1_283,
            1_283,
            0,
            1_283,
        ),
        with_price(
            boundary(
                1,
                1,
                Some(0),
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                10_000,
                10_000,
                0,
                10_000,
            ),
            "7.31",
        ),
    ];
    let projection = run(
        &operations,
        &boundaries,
        &[basis(34, None), basis(90_001, None)],
        &[(34, "0"), (90_001, "0")],
    );

    let child_total = op_cost(&projection, 1).total_production_cost.unwrap();
    assert_eq!(child_total, money("85445.6789")); // 73_100 + 12_345.6789
    let boundary = b_cost(&projection, 0);
    // consumed = round(85445.6789 * 1283 / 10000, 4)
    let expected_consumed =
        Money((child_total.0 * Decimal::from(1_283) / Decimal::from(10_000)).round_dp(4));
    assert_eq!(boundary.child_consumed_cost, Some(expected_consumed));
    assert_eq!(
        boundary.child_surplus_retained_basis,
        Some(Money(child_total.0 - expected_consumed.0))
    );
    // EXACT conservation.
    assert_eq!(
        boundary.child_consumed_cost.unwrap().0 + boundary.child_surplus_retained_basis.unwrap().0,
        child_total.0
    );
    assert_eq!(boundary.child_surplus_quantity, 8_717);
    assert_conservation(&projection);
    assert_reconciliation(&projection);
}

// ---------------------------------------------------------------------------
// Partial intermediate inventory + child surplus
// ---------------------------------------------------------------------------

#[test]
fn partial_intermediate_inventory_plus_child_surplus() {
    // required 1_426 of 90_001 (Build); 143 from inventory @ 4; shortage 1_283.
    // child produces 10_000 (1 run * 10_000).
    let operations = vec![
        with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"),
        with_facility(
            op(1, Some(0), 1, 10_000),
            Some("0"),
            "0",
            "0",
            "0",
            "0",
            "0",
        ),
    ];
    let boundaries = vec![
        boundary(
            0,
            0,
            None,
            90_001,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Missing,
            1_426,
            1_426,
            143,
            1_283,
        ),
        with_price(
            boundary(
                1,
                1,
                Some(0),
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                10_000,
                10_000,
                0,
                10_000,
            ),
            "2",
        ),
    ];
    let projection = run(
        &operations,
        &boundaries,
        &[basis(34, None), basis(90_001, Some("4"))],
        &[(34, "0"), (90_001, "0")],
    );

    let child_total = op_cost(&projection, 1).total_production_cost.unwrap();
    assert_eq!(child_total, money("20000")); // 10_000 * 2
    let boundary = b_cost(&projection, 0);
    assert_eq!(boundary.inventory_quantity, 143);
    assert_eq!(boundary.inventory_cost, Some(money("572"))); // 143 * 4
    let consumed =
        Money((child_total.0 * Decimal::from(1_283) / Decimal::from(10_000)).round_dp(4));
    assert_eq!(boundary.child_consumed_cost, Some(consumed)); // 20000 * 1283/10000 = 2566
    assert_eq!(consumed, money("2566"));
    assert_eq!(boundary.requirement_cost, Some(money("3138"))); // 572 + 2566
    assert_eq!(
        boundary.child_surplus_retained_basis,
        Some(Money(child_total.0 - consumed.0))
    );
    assert_conservation(&projection);
    assert_reconciliation(&projection);
}

// ---------------------------------------------------------------------------
// Fully-covered Build intermediate (child pruned)
// ---------------------------------------------------------------------------

#[test]
fn fully_covered_build_intermediate_uses_inventory_basis_only() {
    // required 500, inventory 500, Build/Missing => planned_use 500, shortage 0,
    // no child operation emitted.
    let operations = vec![with_facility(
        op(0, None, 1, 1),
        Some("0"),
        "0",
        "0",
        "0",
        "0",
        "0",
    )];
    let boundaries = vec![boundary(
        0,
        0,
        None,
        90_001,
        MaterialBoundaryResolution::Build,
        FulfillmentScope::Missing,
        500,
        500,
        500,
        0,
    )];
    let projection = run(
        &operations,
        &boundaries,
        &[basis(90_001, Some("7"))],
        &[(90_001, "0")],
    );

    let boundary = b_cost(&projection, 0);
    assert_eq!(boundary.kind, BoundaryCostKind::FullyCovered);
    assert_eq!(boundary.inventory_cost, Some(money("3500"))); // 500 * 7
    assert_eq!(boundary.requirement_cost, Some(money("3500")));
    assert_eq!(boundary.child_op_index, None);
    assert_eq!(boundary.child_consumed_cost, None);
    assert_eq!(boundary.child_surplus_retained_basis, None);
    assert_conservation(&projection);
    assert_reconciliation(&projection);
}

// ---------------------------------------------------------------------------
// Reaction child
// ---------------------------------------------------------------------------

#[test]
fn reaction_child_flows_through_identically() {
    let mut op1 = with_facility(op(1, Some(0), 2, 100), Some("0"), "0", "0", "0", "0", "0");
    op1.activity = MaterialActivity::Reaction;
    let operations = vec![
        with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"),
        op1,
    ];
    let mut child_boundary = with_price(
        boundary(
            1,
            1,
            Some(0),
            34,
            MaterialBoundaryResolution::Buy,
            FulfillmentScope::Missing,
            200,
            200,
            0,
            200,
        ),
        "6",
    );
    child_boundary.activity = MaterialActivity::Reaction;
    let boundaries = vec![
        boundary(
            0,
            0,
            None,
            90_001,
            MaterialBoundaryResolution::Reaction,
            FulfillmentScope::Missing,
            200,
            200,
            0,
            200,
        ),
        child_boundary,
    ];
    let projection = run(
        &operations,
        &boundaries,
        &[basis(34, None), basis(90_001, None)],
        &[(34, "0"), (90_001, "0")],
    );

    let boundary = b_cost(&projection, 0);
    assert_eq!(boundary.kind, BoundaryCostKind::Reaction);
    assert_eq!(boundary.child_op_index, Some(1));
    // child produced 200; consumed 200; cost = 200*6 = 1200; unit 6.
    assert_eq!(boundary.child_consumed_cost, Some(money("1200")));
    assert_eq!(boundary.child_surplus_quantity, 0);
    assert_conservation(&projection);
}

// ---------------------------------------------------------------------------
// Nested child / grandchild, bottom-up propagation
// ---------------------------------------------------------------------------

#[test]
fn nested_grandchild_surplus_propagates_bottom_up() {
    // root(0) -> A(1) -> B(2). B overproduces; A overproduces.
    // B: 1 run * 1_000 => produced 1_000. B materials: 1_000 units type 34 @ 10 => 10_000.
    // A needs 300 of B's product (Build), shortage 300 => B consumed = 10_000*300/1000 = 3_000.
    //     A produced: A runs 1 * output 400 => 400.  A materials = its own 34 buy (100 @ 5 = 500)
    //       + B consumed 3_000 => 3_500.
    // root needs 250 of A's product (Build), shortage 250 => A consumed = 3_500*250/400 = 2_187.5.
    let operations = vec![
        with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"),
        with_facility(op(1, Some(0), 1, 400), Some("0"), "0", "0", "0", "0", "0"),
        with_facility(op(2, Some(1), 1, 1_000), Some("0"), "0", "0", "0", "0", "0"),
    ];
    let boundaries = vec![
        boundary(
            0,
            0,
            None,
            91_001,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Missing,
            250,
            250,
            0,
            250,
        ),
        with_price(
            boundary(
                1,
                1,
                Some(0),
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                100,
                100,
                0,
                100,
            ),
            "5",
        ),
        boundary(
            2,
            1,
            Some(0),
            92_001,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Missing,
            300,
            300,
            0,
            300,
        ),
        with_price(
            boundary(
                3,
                2,
                Some(2),
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                1_000,
                1_000,
                0,
                1_000,
            ),
            "10",
        ),
    ];
    let projection = run(
        &operations,
        &boundaries,
        &[basis(34, None), basis(91_001, None), basis(92_001, None)],
        &[(34, "0"), (91_001, "0"), (92_001, "0")],
    );
    assert!(projection.complete, "warnings: {:?}", projection.warnings);

    let b = op_cost(&projection, 2);
    assert_eq!(b.total_production_cost, Some(money("10000")));
    assert_eq!(b.produced_quantity, 1_000);

    // boundary 2 (A consuming B):
    let bnd_ab = b_cost(&projection, 2);
    assert_eq!(bnd_ab.child_consumed_cost, Some(money("3000"))); // 10000 * 300/1000
    assert_eq!(bnd_ab.child_surplus_retained_basis, Some(money("7000")));

    let a = op_cost(&projection, 1);
    assert_eq!(a.material_component_cost, Some(money("3500"))); // 500 + 3000
    assert_eq!(a.total_production_cost, Some(money("3500")));
    assert_eq!(a.produced_quantity, 400);

    // boundary 0 (root consuming A): consumed = 3500 * 250/400 = 2187.5
    let bnd_root = b_cost(&projection, 0);
    assert_eq!(bnd_root.child_consumed_cost, Some(money("2187.5")));
    assert_eq!(bnd_root.child_surplus_retained_basis, Some(money("1312.5"))); // 3500 - 2187.5

    let root = op_cost(&projection, 0);
    assert_eq!(root.total_production_cost, Some(money("2187.5")));
    // Root receives only consumed descendant basis, never retained surplus.
    assert_eq!(
        projection.root.planning_total_production_cost,
        Some(money("2187.5"))
    );
    // per-boundary child conservation holds at every level:
    assert_conservation(&projection);
    assert_reconciliation(&projection);
}

// ---------------------------------------------------------------------------
// Different facilities at parent/child; installation counted once
// ---------------------------------------------------------------------------

#[test]
fn child_installation_counted_once_and_only_in_child() {
    let operations = vec![
        // root facility: fixed 1_000 install.
        with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "1000"),
        // child facility: fixed 5_000 install (different facility).
        with_facility(
            op(1, Some(0), 1, 100),
            Some("0"),
            "0",
            "0",
            "0",
            "0",
            "5000",
        ),
    ];
    let boundaries = vec![
        boundary(
            0,
            0,
            None,
            90_001,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Missing,
            100,
            100,
            0,
            100,
        ),
        with_price(
            boundary(
                1,
                1,
                Some(0),
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                100,
                100,
                0,
                100,
            ),
            "1",
        ),
    ];
    let projection = run(
        &operations,
        &boundaries,
        &[basis(34, None), basis(90_001, None)],
        &[(34, "0"), (90_001, "0")],
    );

    let child = op_cost(&projection, 1);
    assert_eq!(child.own_installation.total, Some(money("5000")));
    assert_eq!(child.total_production_cost, Some(money("5100"))); // 100 materials + 5000 install
    assert_eq!(child.produced_quantity, 100);

    let root = op_cost(&projection, 0);
    // root's own installation is ONLY its own 1_000 -- never the child's 5_000.
    assert_eq!(root.own_installation.total, Some(money("1000")));
    // child consumed 100/100 of 5_100 = 5_100 into the parent's material cost.
    assert_eq!(
        b_cost(&projection, 0).child_consumed_cost,
        Some(money("5100"))
    );
    assert_eq!(root.material_component_cost, Some(money("5100")));
    assert_eq!(root.total_production_cost, Some(money("6100"))); // 5_100 + 1_000
                                                                 // Explanatory: total own installation across ops = 1_000 + 5_000 = 6_000.
    assert_eq!(projection.root.total_own_installation_paid, money("6000"));
    assert_conservation(&projection);
    assert_reconciliation(&projection);
}

// ---------------------------------------------------------------------------
// Persisted child runs differ from projected runs
// ---------------------------------------------------------------------------

#[test]
fn cost_follows_projected_node_runs_not_persisted() {
    // The cost projection only ever sees `node_runs` (projected). This test
    // proves changing the projected runs changes the child cost -- there is no
    // persisted-runs input to the pure function at all.
    let boundaries = vec![
        boundary(
            0,
            0,
            None,
            90_001,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Missing,
            500,
            500,
            0,
            500,
        ),
        with_price(
            boundary(
                1,
                1,
                Some(0),
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                1,
                1,
                0,
                1,
            ),
            "10",
        ),
    ];
    // Child projected at 5 runs (output/run 100 => produced 500); its Buy
    // boundary is 1/run scaled by node_runs is NOT modeled here -- api_shortage
    // on the boundary already reflects the projected quantity in the real
    // traversal. Here we just vary produced via node_runs.
    let projection_a = run(
        &[
            with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"),
            with_facility(op(1, Some(0), 5, 100), Some("0"), "0", "0", "0", "0", "0"),
        ],
        &boundaries,
        &[basis(34, None), basis(90_001, None)],
        &[(34, "0"), (90_001, "0")],
    );
    // produced 500, consumed 500 => full child cost consumed.
    assert_eq!(b_cost(&projection_a, 0).child_produced_quantity, 500);
    assert_eq!(b_cost(&projection_a, 0).child_surplus_quantity, 0);

    let projection_b = run(
        &[
            with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"),
            with_facility(op(1, Some(0), 8, 100), Some("0"), "0", "0", "0", "0", "0"),
        ],
        &boundaries,
        &[basis(34, None), basis(90_001, None)],
        &[(34, "0"), (90_001, "0")],
    );
    // produced 800, consumed 500 => surplus 300, proportional consumed cost.
    assert_eq!(b_cost(&projection_b, 0).child_produced_quantity, 800);
    assert_eq!(b_cost(&projection_b, 0).child_surplus_quantity, 300);
    assert_ne!(
        b_cost(&projection_a, 0).child_consumed_cost,
        b_cost(&projection_b, 0).child_consumed_cost
    );
}

// ---------------------------------------------------------------------------
// Shared inventory across sibling branches
// ---------------------------------------------------------------------------

#[test]
fn shared_inventory_across_siblings_counted_once() {
    // Two sibling Buy boundaries for type 34; global allocator gave 400 to the
    // first (traversal 1) and 300 to the second (traversal 2), 700 total, with
    // basis 5. Neither branch may re-value the same 700 units.
    let operations = vec![
        op(0, None, 1, 1),
        op(1, Some(0), 1, 1),
        op(2, Some(0), 1, 1),
    ];
    let boundaries = vec![
        boundary(
            0,
            0,
            None,
            90_001,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Missing,
            1,
            1,
            1,
            0,
        ), // covered, irrelevant
        with_price(
            boundary(
                1,
                1,
                Some(0),
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                1_000,
                1_000,
                400,
                600,
            ),
            "8",
        ),
        with_price(
            boundary(
                2,
                2,
                Some(0),
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                800,
                800,
                300,
                500,
            ),
            "8",
        ),
    ];
    let projection = run(
        &operations,
        &boundaries,
        &[basis(34, Some("5")), basis(90_001, Some("1"))],
        &[],
    );

    let inv1 = b_cost(&projection, 1).inventory_cost.unwrap();
    let inv2 = b_cost(&projection, 2).inventory_cost.unwrap();
    assert_eq!(inv1, money("2000")); // 400 * 5
    assert_eq!(inv2, money("1500")); // 300 * 5
                                     // Σ inventory cost for type 34 == (400 + 300) * 5 == 3_500, never 2 * 1_000 * 5.
    assert_eq!(inv1.0 + inv2.0, dec("3500"));
    // + boundary 0 (fully-covered 90_001): planned_use 1 * basis 1 = 1.
    assert_eq!(
        projection.root.total_inventory_basis_consumed.0,
        dec("3501")
    );
}

// ---------------------------------------------------------------------------
// Full-scope Build child (sized for full required)
// ---------------------------------------------------------------------------

#[test]
fn full_scope_build_child() {
    // Full scope => planned_use 0, shortage = required = 500. Child sized for
    // 500 but produces 600 (2 runs * 300) => surplus 100.
    let operations = vec![
        with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"),
        with_facility(op(1, Some(0), 2, 300), Some("0"), "0", "0", "0", "0", "0"),
    ];
    let boundaries = vec![
        boundary(
            0,
            0,
            None,
            90_001,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Full,
            500,
            500,
            0,
            500,
        ),
        with_price(
            boundary(
                1,
                1,
                Some(0),
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                600,
                600,
                0,
                600,
            ),
            "10",
        ),
    ];
    let projection = run(
        &operations,
        &boundaries,
        &[basis(34, None), basis(90_001, Some("999"))],
        &[(34, "0"), (90_001, "0")],
    );

    let boundary = b_cost(&projection, 0);
    assert_eq!(
        boundary.inventory_cost,
        Some(Money::zero()),
        "Full scope: no inventory"
    );
    assert_eq!(boundary.child_produced_quantity, 600);
    assert_eq!(boundary.child_consumed_quantity, 500);
    // child total = 6_000; consumed = 6_000 * 500/600 = 5_000; surplus basis 1_000.
    assert_eq!(boundary.child_consumed_cost, Some(money("5000")));
    assert_eq!(boundary.child_surplus_retained_basis, Some(money("1000")));
    assert_eq!(boundary.requirement_cost, Some(money("5000")));
    assert_conservation(&projection);
    assert_reconciliation(&projection);
}

// ---------------------------------------------------------------------------
// Unresolved Build
// ---------------------------------------------------------------------------

#[test]
fn unresolved_build_is_incomplete_never_fabricated() {
    let operations = vec![with_facility(
        op(0, None, 1, 1),
        Some("0"),
        "0",
        "0",
        "0",
        "0",
        "0",
    )];
    let boundaries = vec![boundary(
        0,
        0,
        None,
        90_001,
        MaterialBoundaryResolution::Unresolved,
        FulfillmentScope::Missing,
        100,
        100,
        0,
        100,
    )];
    let projection = run(
        &operations,
        &boundaries,
        &[basis(90_001, None)],
        &[(90_001, "0")],
    );

    let boundary = b_cost(&projection, 0);
    assert_eq!(boundary.kind, BoundaryCostKind::Unresolved);
    assert_eq!(boundary.requirement_cost, None);
    assert!(!boundary.complete);
    assert!(boundary
        .warnings
        .iter()
        .any(|warning| matches!(warning, CostWarning::UnresolvedBuild { .. })));
    assert_eq!(op_cost(&projection, 0).material_component_cost, None);
    assert!(!op_cost(&projection, 0).complete);
    assert!(!projection.complete);
    assert_eq!(projection.root.planning_total_production_cost, None);
}

// ---------------------------------------------------------------------------
// Missing-data / completeness matrix
// ---------------------------------------------------------------------------

#[test]
fn missing_fresh_price_is_incomplete() {
    let operations = vec![op(0, None, 1, 1)];
    let boundaries = vec![boundary(
        0,
        0,
        None,
        34,
        MaterialBoundaryResolution::Buy,
        FulfillmentScope::Missing,
        100,
        100,
        0,
        100,
    )]; // no with_price
    let projection = run(&operations, &boundaries, &[basis(34, None)], &[]);
    let boundary = b_cost(&projection, 0);
    assert_eq!(boundary.fresh_cost, None);
    assert_eq!(boundary.requirement_cost, None);
    assert!(boundary
        .warnings
        .iter()
        .any(|w| matches!(w, CostWarning::MissingFreshPrice { .. })));
}

#[test]
fn missing_inventory_basis_is_incomplete() {
    let operations = vec![op(0, None, 1, 1)];
    let boundaries = vec![with_price(
        boundary(
            0,
            0,
            None,
            34,
            MaterialBoundaryResolution::Buy,
            FulfillmentScope::Missing,
            100,
            100,
            40,
            60,
        ),
        "8",
    )];
    let projection = run(&operations, &boundaries, &[basis(34, None)], &[]); // basis None, planned_use 40 > 0
    let boundary = b_cost(&projection, 0);
    assert_eq!(boundary.inventory_cost, None);
    assert_eq!(boundary.requirement_cost, None);
    assert!(boundary
        .warnings
        .iter()
        .any(|w| matches!(w, CostWarning::MissingInventoryBasis { .. })));
}

#[test]
fn missing_adjusted_price_blocks_installation() {
    let operations = vec![with_facility(
        op(0, None, 1, 1),
        Some("0.05"),
        "0",
        "0",
        "0",
        "0",
        "0",
    )];
    let boundaries = vec![with_price(
        boundary(
            0,
            0,
            None,
            34,
            MaterialBoundaryResolution::Buy,
            FulfillmentScope::Missing,
            100,
            100,
            0,
            100,
        ),
        "8",
    )];
    let projection = run(&operations, &boundaries, &[basis(34, None)], &[]); // no adjusted price for 34
    let root = op_cost(&projection, 0);
    assert_eq!(root.own_installation.eiv, None);
    assert_eq!(root.own_installation.total, None);
    assert!(!root.own_installation.complete);
    assert_eq!(root.own_installation.eiv_missing_type_ids, vec![34]);
    assert!(root
        .warnings
        .iter()
        .any(|w| matches!(w, CostWarning::MissingAdjustedPrice { .. })));
    // material side still complete
    assert_eq!(root.material_component_cost, Some(money("800")));
    assert_eq!(root.total_production_cost, None);
}

#[test]
fn missing_system_cost_index_blocks_installation_no_zero_substitution() {
    let operations = vec![with_facility(
        op(0, None, 1, 1),
        None,
        "0",
        "0",
        "0",
        "0",
        "0",
    )];
    let boundaries = vec![with_price(
        boundary(
            0,
            0,
            None,
            34,
            MaterialBoundaryResolution::Buy,
            FulfillmentScope::Missing,
            100,
            100,
            0,
            100,
        ),
        "8",
    )];
    let projection = run(&operations, &boundaries, &[basis(34, None)], &[(34, "5")]);
    let root = op_cost(&projection, 0);
    assert_eq!(root.own_installation.eiv, Some(money("500")));
    assert_eq!(root.own_installation.system_index_cost, None);
    assert_eq!(root.own_installation.total, None);
    assert!(root
        .warnings
        .iter()
        .any(|w| matches!(w, CostWarning::MissingSystemCostIndex { .. })));
}

#[test]
fn no_facility_blocks_installation() {
    let operations = vec![op(0, None, 1, 1)];
    let boundaries = vec![with_price(
        boundary(
            0,
            0,
            None,
            34,
            MaterialBoundaryResolution::Buy,
            FulfillmentScope::Missing,
            100,
            100,
            0,
            100,
        ),
        "8",
    )];
    let projection = run(&operations, &boundaries, &[basis(34, None)], &[(34, "5")]);
    let root = op_cost(&projection, 0);
    assert_eq!(root.own_installation.total, None);
    assert!(!root.own_installation.complete);
    assert!(root
        .warnings
        .iter()
        .any(|w| matches!(w, CostWarning::NoFacilitySelected { .. })));
}

// ---------------------------------------------------------------------------
// Stale fresh price: complete but flagged
// ---------------------------------------------------------------------------

#[test]
fn stale_fresh_price_is_complete_but_flagged() {
    let operations = vec![with_facility(
        op(0, None, 1, 1),
        Some("0"),
        "0",
        "0",
        "0",
        "0",
        "0",
    )];
    let mut b = with_price(
        boundary(
            0,
            0,
            None,
            34,
            MaterialBoundaryResolution::Buy,
            FulfillmentScope::Missing,
            100,
            100,
            0,
            100,
        ),
        "8",
    );
    b.fresh_price_stale = true;
    let projection = run(&operations, &[b], &[basis(34, None)], &[(34, "0")]);
    let boundary = b_cost(&projection, 0);
    assert!(boundary.complete, "stale price is still usable");
    assert_eq!(boundary.fresh_cost, Some(money("800")));
    assert!(boundary
        .warnings
        .iter()
        .any(|w| matches!(w, CostWarning::StaleFreshPrice { .. })));
    assert!(projection.complete);
}

// ---------------------------------------------------------------------------
// Bottom-up ordering: parent op_index < child op_index for every link
// ---------------------------------------------------------------------------

#[test]
fn child_op_index_always_greater_than_parent() {
    let operations = vec![
        with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"),
        with_facility(op(1, Some(0), 1, 100), Some("0"), "0", "0", "0", "0", "0"),
        with_facility(op(2, Some(1), 1, 100), Some("0"), "0", "0", "0", "0", "0"),
    ];
    let boundaries = vec![
        boundary(
            0,
            0,
            None,
            91_001,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Missing,
            50,
            50,
            0,
            50,
        ),
        boundary(
            1,
            1,
            Some(0),
            92_001,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Missing,
            50,
            50,
            0,
            50,
        ),
        with_price(
            boundary(
                2,
                2,
                Some(1),
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                50,
                50,
                0,
                50,
            ),
            "1",
        ),
    ];
    let projection = run(
        &operations,
        &boundaries,
        &[basis(34, None), basis(91_001, None), basis(92_001, None)],
        &[(34, "0"), (91_001, "0"), (92_001, "0")],
    );
    for boundary in &projection.boundaries {
        if let Some(child) = boundary.child_op_index {
            assert!(
                child > boundary.op_index,
                "boundary {} (op {}) -> child op {} must be a descendant",
                boundary.traversal_index,
                boundary.op_index,
                child
            );
        }
    }
    assert!(projection.complete);
}

// ---------------------------------------------------------------------------
// Parity: an all-Buy build's material cost is the raw priced sum
// ---------------------------------------------------------------------------

#[test]
fn parity_all_buy_material_cost_is_priced_sum() {
    let operations = vec![with_facility(
        op(0, None, 2, 1),
        Some("0"),
        "0",
        "0",
        "0",
        "0",
        "0",
    )];
    let boundaries = vec![
        with_price(
            boundary(
                0,
                0,
                None,
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                100,
                200,
                0,
                200,
            ),
            "5",
        ),
        with_price(
            boundary(
                1,
                0,
                None,
                35,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                50,
                100,
                0,
                100,
            ),
            "11",
        ),
    ];
    let projection = run(
        &operations,
        &boundaries,
        &[basis(34, None), basis(35, None)],
        &[(34, "0"), (35, "0")],
    );
    // 200*5 + 100*11 = 1000 + 1100 = 2100
    assert_eq!(
        op_cost(&projection, 0).material_component_cost,
        Some(money("2100"))
    );
    assert_conservation(&projection);
}

// ---------------------------------------------------------------------------
// Empty projection is trivially complete
// ---------------------------------------------------------------------------

#[test]
fn empty_projection_is_trivially_complete() {
    let projection = run(&[], &[], &[], &[]);
    assert!(projection.complete);
    assert!(projection.operations.is_empty());
    assert!(projection.boundaries.is_empty());
    assert_eq!(projection.root.planning_total_production_cost, None);
    assert_eq!(projection.root.total_fresh_outlay, Money::zero());
}

// ---------------------------------------------------------------------------
// determinism
// ---------------------------------------------------------------------------

#[test]
fn projection_is_deterministic() {
    let operations = vec![
        with_facility(op(0, None, 1, 1), Some("0.02"), "0", "1", "0.5", "0", "10"),
        with_facility(
            op(1, Some(0), 1, 1_000),
            Some("0.02"),
            "0",
            "1",
            "0.5",
            "0",
            "10",
        ),
    ];
    let boundaries = vec![
        boundary(
            0,
            0,
            None,
            90_001,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Missing,
            333,
            333,
            0,
            333,
        ),
        with_price(
            boundary(
                1,
                1,
                Some(0),
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                1_000,
                1_000,
                0,
                1_000,
            ),
            "7.77",
        ),
    ];
    let a = run(
        &operations,
        &boundaries,
        &[basis(34, None), basis(90_001, None)],
        &[(34, "3.5"), (90_001, "0")],
    );
    let b = run(
        &operations,
        &boundaries,
        &[basis(34, None), basis(90_001, None)],
        &[(34, "3.5"), (90_001, "0")],
    );
    assert_eq!(a, b);
    assert_conservation(&a);
    assert_reconciliation(&a);
}

// ---------------------------------------------------------------------------
// One producer operation, many demand edges.
// ---------------------------------------------------------------------------

/// Root -> consumers C1 (op 1), C2 (op 2) -> ONE producer P (op 3) serving
/// both demand edges (`incoming`). P costs exactly 1.0000 and produces 3; each
/// consumer takes 1. Each consumer is charged round(1 * 1/3) = 0.3333; the
/// operation's one surplus (1 unit) retains the EXACT remainder 0.3334 on one
/// edge (never a separately rounded ratio 0.3333, which would leak 0.0001),
/// so consumed + retained == total at Money scale. P's cost is computed once.
#[test]
fn a_shared_producer_total_is_split_across_its_demand_edges_without_leakage() {
    use crate::build_materials::OperationIncomingDemand;
    let root = with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0");
    let mut c1 = with_facility(op(1, Some(0), 1, 1), Some("0"), "0", "0", "0", "0", "0");
    let mut c2 = with_facility(op(2, Some(0), 1, 1), Some("0"), "0", "0", "0", "0", "0");
    c1.parent_traversal_index = Some(0);
    c2.parent_traversal_index = Some(1);
    let mut producer = with_facility(op(3, Some(1), 1, 3), Some("0"), "0", "0", "0", "0", "1");
    producer.parent_traversal_index = Some(2);
    producer.incoming = vec![
        OperationIncomingDemand {
            traversal_index: 2,
            consumer_op_index: 1,
            dependency_id: "pd:c1-p".to_string(),
        },
        OperationIncomingDemand {
            traversal_index: 3,
            consumer_op_index: 2,
            dependency_id: "pd:c2-p".to_string(),
        },
    ];
    let p_type = 90_003;
    let boundaries = vec![
        boundary(
            0,
            0,
            None,
            90_001,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Missing,
            1,
            1,
            0,
            1,
        ),
        boundary(
            1,
            0,
            None,
            90_002,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Missing,
            1,
            1,
            0,
            1,
        ),
        boundary(
            2,
            1,
            Some(0),
            p_type,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Missing,
            1,
            1,
            0,
            1,
        ),
        boundary(
            3,
            2,
            Some(1),
            p_type,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Missing,
            1,
            1,
            0,
            1,
        ),
    ];
    let projection = run(
        &[root, c1, c2, producer],
        &boundaries,
        &[],
        &[(90_001, "0"), (90_002, "0"), (p_type, "0")],
    );

    let p = op_cost(&projection, 3);
    assert_eq!(p.total_production_cost, Some(money("1")), "costed once");
    assert_eq!(p.produced_quantity, 3);
    let first = b_cost(&projection, 2);
    let second = b_cost(&projection, 3);
    assert_eq!(first.child_op_index, Some(3));
    assert_eq!(
        second.child_op_index,
        Some(3),
        "both edges consume the SAME operation"
    );
    assert_eq!(first.child_consumed_cost, Some(money("0.3333")));
    assert_eq!(second.child_consumed_cost, Some(money("0.3333")));
    assert_eq!(first.child_surplus_quantity, 1, "3 - (1 + 1), once");
    assert_eq!(second.child_surplus_quantity, 0);
    assert_eq!(
        first.child_surplus_retained_basis,
        Some(money("0.3334")),
        "the exact remainder, not round(1 * 1/3)"
    );
    assert_eq!(second.child_surplus_retained_basis, Some(money("0")));
    let conserved = first.child_consumed_cost.unwrap().0
        + second.child_consumed_cost.unwrap().0
        + first.child_surplus_retained_basis.unwrap().0
        + second.child_surplus_retained_basis.unwrap().0;
    assert_eq!(conserved, p.total_production_cost.unwrap().0);
    assert_eq!(
        projection.root.total_surplus_retained_basis,
        money("0.3334")
    );
    assert_eq!(
        projection.root.planning_total_production_cost,
        Some(money("0.6666")),
        "the root pays only what its consumers consumed"
    );
    assert_eq!(
        projection.root.total_own_installation_paid,
        money("1"),
        "P's installation once"
    );
}

/// The web client reads `typeId` / `opIndex` off a cost warning: the fields
/// are camelCase on the wire, not only the variant tag.
#[test]
fn cost_warnings_serialize_camel_case_fields() {
    let warning = CostWarning::MissingFreshPrice {
        op_index: 3,
        traversal_index: 7,
        type_id: 34,
    };
    let json = serde_json::to_value(&warning).unwrap();
    assert_eq!(json["code"], "missingFreshPrice");
    assert_eq!(json["typeId"], 34);
    assert_eq!(json["opIndex"], 3);
    assert_eq!(json["traversalIndex"], 7);
}
