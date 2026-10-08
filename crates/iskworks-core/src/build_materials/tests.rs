//! Unit tests for the pure whole-tree allocator **primitives** --
//! [`PlanningInventory`], [`allocate`], [`ProductionEvidence`],
//! [`MaterialsAccumulator`], and the slot-normalization helpers. All inputs
//! are synthetic; no repository, no SDE, no I/O.
//!
//! The *allocating traversal* that drives these (dynamic child sizing,
//! subtree pruning, Full-scope composition, the Sabre case) is async and
//! DB-backed -- it lives in `iskworks_core::industry::IndustryService::
//! project_build_materials` and is covered end to end by
//! `apps/iskworks-api/tests/industry_api.rs`.

use uuid::Uuid;

use super::*;
use crate::industry::{
    BuildId, BuildPlanId, BuildPlanRevision, PlannedMaterialLine, PriceSnapshot, PriceSnapshotId,
};
use crate::{FulfillmentScope, FulfillmentScopeOverride, Money};

// ---------------------------------------------------------------------------
// scaffolding
// ---------------------------------------------------------------------------

fn bid() -> BuildId {
    BuildId::new()
}

fn rec<'a>(build_id: BuildId, graph: &'a str, path: &'a [i64]) -> BoundaryRecord<'a> {
    BoundaryRecord {
        build_id,
        graph_node_id: graph,
        tree_path: path,
        type_id: 0,
        type_name: "",
        scope: FulfillmentScope::Missing,
        required: 0,
        allocated: 0,
        remaining: 0,
        dependency_id: None,
        producer_build_id: None,
    }
}

fn plan_line(type_id: i64, total_quantity: u64, is_build_resolved: bool) -> PlannedMaterialLine {
    PlannedMaterialLine {
        type_id,
        type_name: format!("T{type_id}"),
        quantity_per_run: total_quantity,
        total_quantity,
        unit_price: None,
        line_total: None,
        missing: false,
        contributions: Vec::new(),
        is_build_resolved,
        installation_cost: None,
        reused_quantity: None,
        missing_quantity: None,
        reused_line_total: None,
        planning_evidence: None,
    }
}

fn revision(lines: Vec<PlannedMaterialLine>) -> BuildPlanRevision {
    BuildPlanRevision {
        id: BuildPlanId(Uuid::nil()),
        revision: 1,
        runs: 1,
        recipe_fingerprint: "fp".into(),
        snapshot: PriceSnapshot {
            id: PriceSnapshotId(Uuid::nil()),
            price_source_id: None,
            source_name: String::new(),
            source_revision: 0,
            created_at: chrono::Utc::now(),
            items: Vec::new(),
        },
        pricing_complete: false,
        estimated_material_cost: Money::zero(),
        expected_revenue: None,
        estimated_margin: None,
        missing_price_count: 0,
        active: true,
        planned_at: chrono::Utc::now(),
        superseded_at: None,
        material_lines: lines,
        manufacturing_facility: None,
        reaction_facility: None,
        blueprint: None,
        effective_requirements: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// PlanningInventory
// ---------------------------------------------------------------------------

#[test]
fn planning_inventory_take_is_underflow_safe_and_bounded() {
    let mut pool = PlanningInventory::seed([(34, 1_000)]);
    assert_eq!(pool.available(34), 1_000);
    assert_eq!(pool.remaining(34), 1_000);

    assert_eq!(pool.take(34, 400), 400);
    assert_eq!(pool.remaining(34), 600);

    assert_eq!(pool.take(34, 10_000), 600); // only what remains, no underflow
    assert_eq!(pool.remaining(34), 0);
    assert_eq!(pool.take(34, 1), 0);

    assert_eq!(pool.take(35, 50), 0); // never seeded
    assert_eq!(pool.available(35), 0);

    assert_eq!(pool.available(34), 1_000); // immutable seed
}

#[test]
fn planning_inventory_seed_sums_duplicate_type_ids() {
    let pool = PlanningInventory::seed([(34, 1_000), (35, 5), (34, 250)]);
    assert_eq!(pool.available(34), 1_250);
    assert_eq!(pool.remaining(34), 1_250);
    assert_eq!(pool.available(35), 5);
}

// ---------------------------------------------------------------------------
// allocate -- the boundary primitive
// ---------------------------------------------------------------------------

#[test]
fn allocate_missing_scope_covers_full_partial_and_uncovered() {
    // Full cover.
    let mut pool = PlanningInventory::seed([(1, 100)]);
    assert_eq!(
        allocate(&mut pool, 1, 100, FulfillmentScope::Missing),
        (100, 0)
    );
    assert_eq!(pool.remaining(1), 0);

    // Partial cover -> the shortfall is `remaining`.
    let mut pool = PlanningInventory::seed([(1, 40)]);
    assert_eq!(
        allocate(&mut pool, 1, 100, FulfillmentScope::Missing),
        (40, 60)
    );

    // Uncovered.
    let mut pool = PlanningInventory::seed([]);
    assert_eq!(
        allocate(&mut pool, 1, 100, FulfillmentScope::Missing),
        (0, 100)
    );

    // Excess stock -> take is capped at `required`, the rest stays in the pool.
    let mut pool = PlanningInventory::seed([(1, 500)]);
    assert_eq!(
        allocate(&mut pool, 1, 100, FulfillmentScope::Missing),
        (100, 0)
    );
    assert_eq!(pool.remaining(1), 400);
}

#[test]
fn allocate_full_scope_never_draws_the_pool() {
    let mut pool = PlanningInventory::seed([(1, 1_000)]);
    assert_eq!(
        allocate(&mut pool, 1, 100, FulfillmentScope::Full),
        (0, 100)
    );
    assert_eq!(pool.remaining(1), 1_000);
}

#[test]
fn allocate_is_sequential_first_visitor_wins() {
    // The shared-intermediate guarantee: two boundaries, one pool.
    let mut pool = PlanningInventory::seed([(1, 1_000)]);
    assert_eq!(
        allocate(&mut pool, 1, 800, FulfillmentScope::Missing),
        (800, 0)
    );
    assert_eq!(
        allocate(&mut pool, 1, 800, FulfillmentScope::Missing),
        (200, 600)
    );
    // 1,000 consumed once, never 1,600.
    assert_eq!(pool.remaining(1), 0);
}

// ---------------------------------------------------------------------------
// ProductionEvidence -- discrete-run sizing / surplus
// ---------------------------------------------------------------------------

#[test]
fn production_evidence_sized_records_discrete_overproduction() {
    // remaining 55, output/run 10 -> 6 runs, 60 produced, surplus 5.
    let evidence = ProductionEvidence::sized(6, 10, 55);
    assert_eq!(evidence.child_runs, 6);
    assert_eq!(evidence.produced_quantity, 60);
    assert_eq!(evidence.surplus_quantity, 5);

    // Exact multiple -> no surplus.
    let evidence = ProductionEvidence::sized(4, 25, 100);
    assert_eq!(evidence.produced_quantity, 100);
    assert_eq!(evidence.surplus_quantity, 0);
}

#[test]
fn production_evidence_carries_output_per_run_for_sized_and_pruned() {
    // A sized boundary records the per-run yield used to size it...
    assert_eq!(ProductionEvidence::sized(6, 10, 55).output_per_run, 10);
    // ...and a pruned (fully covered) boundary still records the recipe's
    // per-run yield, with no runs / production / surplus.
    let pruned = ProductionEvidence::pruned(2_852);
    assert_eq!(pruned.output_per_run, 2_852);
    assert_eq!(pruned.child_runs, 0);
    assert_eq!(pruned.produced_quantity, 0);
    assert_eq!(pruned.surplus_quantity, 0);
    // A non-production leaf carries none.
    assert_eq!(ProductionEvidence::default().output_per_run, 0);
}

#[test]
fn node_allocation_surfaces_output_per_run_from_production_evidence() {
    let root = bid();
    let mut acc = MaterialsAccumulator::new();
    acc.record_intermediate(
        BoundaryRecord {
            type_id: 100,
            type_name: "Fernite Carbide",
            required: 100,
            allocated: 45,
            remaining: 55,
            ..rec(root, "root:r", &[])
        },
        MaterialBoundaryResolution::Reaction,
        ProductionEvidence::sized(6, 10, 55),
        None,
    );
    let pool = PlanningInventory::seed([(100, 45)]);
    let aggregate = acc.finish(&pool).unwrap();
    let allocation = aggregate.node_allocation(root, 100).unwrap();
    assert_eq!(allocation.output_per_run, 10);
    assert_eq!(allocation.child_runs, 6);
    assert_eq!(allocation.produced_quantity, 60);
    assert_eq!(allocation.surplus_quantity, 5);
}

// ---------------------------------------------------------------------------
// MaterialsAccumulator
// ---------------------------------------------------------------------------

fn buy(rec: BoundaryRecord<'_>) -> (BoundaryRecord<'_>, MaterialRowStrategy, bool) {
    (rec, MaterialRowStrategy::Buy, false)
}

#[test]
fn accumulator_emits_a_row_for_every_boundary_covered_or_short() {
    let build = bid();
    let mut acc = MaterialsAccumulator::new();

    // Type 1: fully covered Buy -> still a row.
    let (r, s, p) = buy(BoundaryRecord {
        type_id: 1,
        type_name: "Covered",
        required: 100,
        allocated: 100,
        remaining: 0,
        ..rec(build, "root:x", &[])
    });
    acc.record_leaf(r, s, p, None);
    // Type 2: short Buy -> a row.
    let (r, s, p) = buy(BoundaryRecord {
        type_id: 2,
        type_name: "Short",
        required: 100,
        allocated: 40,
        remaining: 60,
        ..rec(build, "root:x", &[])
    });
    acc.record_leaf(r, s, p, None);

    let pool = PlanningInventory::seed([(1, 100), (2, 40)]);
    let aggregate = acc.finish(&pool).unwrap();

    assert_eq!(aggregate.lines.len(), 2);
    let covered = aggregate.line(1).unwrap();
    assert!(covered.fully_covered);
    assert_eq!(covered.shortage_quantity, 0);
    assert_eq!(covered.strategy, MaterialRowStrategy::Buy);
    let short = aggregate.line(2).unwrap();
    assert!(!short.fully_covered);
    assert_eq!(short.shortage_quantity, 60);
    assert_eq!(aggregate.node_allocations.len(), 2);
    assert_eq!(aggregate.sources.len(), 2);
}

#[test]
fn accumulator_merges_a_type_across_two_leaves_and_sums() {
    let a = bid();
    let b = bid();
    let mut acc = MaterialsAccumulator::new();
    let (r, s, p) = buy(BoundaryRecord {
        type_id: 34,
        type_name: "Tritanium",
        required: 100,
        allocated: 100,
        remaining: 0,
        ..rec(a, "root:a", &[])
    });
    acc.record_leaf(r, s, p, None);
    let (r, s, p) = buy(BoundaryRecord {
        type_id: 34,
        type_name: "Tritanium",
        required: 500,
        allocated: 0,
        remaining: 500,
        ..rec(b, "build:b", &[90_001])
    });
    acc.record_leaf(r, s, p, None);
    let pool = PlanningInventory::seed([(34, 100)]);
    let aggregate = acc.finish(&pool).unwrap();

    let line = aggregate.line(34).unwrap();
    assert_eq!(line.required_quantity, 600);
    assert_eq!(line.allocated_quantity, 100);
    assert_eq!(line.shortage_quantity, 500);
    assert_eq!(line.strategy, MaterialRowStrategy::Buy);

    // Σ all node allocations for the type == the aggregate row.
    let node_alloc: u64 = aggregate
        .node_allocations
        .iter()
        .filter(|n| n.type_id == 34)
        .map(|n| n.allocated_quantity)
        .sum();
    assert_eq!(node_alloc, line.allocated_quantity);
}

#[test]
fn accumulator_mixed_strategy_when_one_type_is_buy_and_build_at_different_nodes() {
    let a = bid();
    let b = bid();
    let mut acc = MaterialsAccumulator::new();
    let (r, s, p) = buy(BoundaryRecord {
        type_id: 7,
        type_name: "Widget",
        required: 10,
        allocated: 0,
        remaining: 10,
        ..rec(a, "root:a", &[])
    });
    acc.record_leaf(r, s, p, None);
    acc.record_intermediate(
        BoundaryRecord {
            type_id: 7,
            type_name: "Widget",
            required: 4,
            allocated: 0,
            remaining: 4,
            ..rec(b, "build:b", &[900])
        },
        MaterialBoundaryResolution::Build,
        ProductionEvidence::sized(4, 1, 4),
        None,
    );
    let pool = PlanningInventory::seed([]);
    let aggregate = acc.finish(&pool).unwrap();
    assert_eq!(
        aggregate.line(7).unwrap().strategy,
        MaterialRowStrategy::Mixed
    );
}

#[test]
fn accumulator_build_intermediate_is_a_row_and_carries_production_evidence() {
    let root = bid();
    let mut acc = MaterialsAccumulator::new();

    // Fully-covered Build intermediate -> visible row, shortage 0, child pruned.
    acc.record_intermediate(
        BoundaryRecord {
            type_id: 2_852,
            type_name: "Composite Armor Plate",
            required: 2_852,
            allocated: 2_852,
            remaining: 0,
            ..rec(root, "root:r", &[])
        },
        MaterialBoundaryResolution::Build,
        ProductionEvidence::default(),
        None,
    );
    // Partially-covered Reaction intermediate -> visible row + child sizing.
    acc.record_intermediate(
        BoundaryRecord {
            type_id: 100,
            type_name: "Fernite Carbide",
            required: 100,
            allocated: 45,
            remaining: 55,
            ..rec(root, "root:r", &[])
        },
        MaterialBoundaryResolution::Reaction,
        ProductionEvidence::sized(6, 10, 55),
        None,
    );

    let pool = PlanningInventory::seed([(2_852, 5_000), (100, 45)]);
    let aggregate = acc.finish(&pool).unwrap();

    assert_eq!(aggregate.lines.len(), 2);
    assert!(aggregate.sources.is_empty(), "intermediates are not leaves");

    let plate = aggregate.line(2_852).unwrap();
    assert_eq!(plate.required_quantity, 2_852);
    assert_eq!(plate.allocated_quantity, 2_852);
    assert_eq!(plate.shortage_quantity, 0);
    assert!(plate.fully_covered);
    assert_eq!(plate.strategy, MaterialRowStrategy::Build);
    assert_eq!(plate.available_quantity, 5_000);

    let carbide = aggregate.line(100).unwrap();
    assert_eq!(carbide.shortage_quantity, 55);
    assert!(!carbide.fully_covered);
    assert_eq!(carbide.strategy, MaterialRowStrategy::Reaction);

    let partial = aggregate.node_allocation(root, 100).unwrap();
    assert_eq!(partial.child_runs, 6);
    assert_eq!(partial.produced_quantity, 60);
    assert_eq!(partial.surplus_quantity, 5);
    assert_eq!(partial.tree_path, Vec::<i64>::new());
    assert_eq!(partial.graph_node_id, "root:r");
}

#[test]
fn accumulator_provisional_boundary_emits_row_source_and_warning() {
    let root = bid();
    let mut acc = MaterialsAccumulator::new();
    acc.record_leaf(
        BoundaryRecord {
            type_id: 900,
            type_name: "Widget",
            required: 10,
            allocated: 4,
            remaining: 6,
            ..rec(root, "root:r", &[900])
        },
        MaterialRowStrategy::Build,
        true,
        None,
    );
    let pool = PlanningInventory::seed([(900, 4)]);
    let aggregate = acc.finish(&pool).unwrap();

    let line = aggregate.line(900).unwrap();
    assert_eq!(line.shortage_quantity, 6);
    assert!(line.provisional);
    assert_eq!(line.strategy, MaterialRowStrategy::Build);
    let source = &aggregate.sources[0];
    assert!(source.provisional);
    assert_eq!(source.tree_path, vec![900]);
    let node = aggregate.node_allocation(root, 900).unwrap();
    assert_eq!(node.resolution, MaterialBoundaryResolution::Unresolved);
    assert!(node.provisional);
    assert_eq!(aggregate.warnings.len(), 1);
}

#[test]
fn accumulator_fully_covered_provisional_is_a_visible_row_without_shortage() {
    let root = bid();
    let mut acc = MaterialsAccumulator::new();
    acc.record_leaf(
        BoundaryRecord {
            type_id: 900,
            type_name: "Widget",
            required: 10,
            allocated: 10,
            remaining: 0,
            ..rec(root, "root:r", &[900])
        },
        MaterialRowStrategy::Build,
        true,
        None,
    );
    let pool = PlanningInventory::seed([(900, 50)]);
    let aggregate = acc.finish(&pool).unwrap();

    let line = aggregate.line(900).unwrap();
    assert!(line.fully_covered);
    assert_eq!(line.shortage_quantity, 0);
    assert!(line.provisional, "still intended-but-unsourced");
    assert_eq!(aggregate.warnings.len(), 1);
}

#[test]
fn accumulator_row_provisional_only_when_every_contribution_is_provisional() {
    let a = bid();
    let b = bid();
    let mut acc = MaterialsAccumulator::new();
    // One provisional, one real Buy, same type -> row not provisional.
    acc.record_leaf(
        BoundaryRecord {
            type_id: 5,
            type_name: "T5",
            required: 3,
            allocated: 0,
            remaining: 3,
            ..rec(a, "root:a", &[5])
        },
        MaterialRowStrategy::Build,
        true,
        None,
    );
    let (r, s, p) = buy(BoundaryRecord {
        type_id: 5,
        type_name: "T5",
        required: 2,
        allocated: 0,
        remaining: 2,
        ..rec(b, "build:b", &[])
    });
    acc.record_leaf(r, s, p, None);
    let pool = PlanningInventory::seed([]);
    let aggregate = acc.finish(&pool).unwrap();
    assert!(!aggregate.line(5).unwrap().provisional);
}

#[test]
fn accumulator_note_missing_node_fails_the_whole_projection() {
    let missing = bid();
    let mut acc = MaterialsAccumulator::new();
    let (r, s, p) = buy(BoundaryRecord {
        type_id: 1,
        type_name: "T1",
        required: 10,
        allocated: 0,
        remaining: 10,
        ..rec(bid(), "root:r", &[])
    });
    acc.record_leaf(r, s, p, None);
    acc.note_missing_node(missing);

    let pool = PlanningInventory::seed([]);
    match acc.finish(&pool) {
        Err(BuildMaterialsError::NodeRevisionUnavailable { missing_nodes }) => {
            assert_eq!(missing_nodes, vec![missing]);
        }
        other => panic!("expected NodeRevisionUnavailable, got {other:?}"),
    }
}

#[test]
fn accumulator_node_allocations_are_sorted_and_map_is_addressable() {
    let a = bid();
    let b = bid();
    let mut acc = MaterialsAccumulator::new();
    for (build, type_id) in [(b, 2), (a, 5), (a, 1), (b, 1)] {
        let (r, s, p) = buy(BoundaryRecord {
            type_id,
            type_name: "T",
            required: 1,
            allocated: 0,
            remaining: 1,
            ..rec(build, "n", &[])
        });
        acc.record_leaf(r, s, p, None);
    }
    let pool = PlanningInventory::seed([]);
    let aggregate = acc.finish(&pool).unwrap();

    let keys: Vec<(Uuid, i64)> = aggregate
        .node_allocations
        .iter()
        .map(|n| (n.build_id.0, n.type_id))
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted);
    assert!(aggregate.node_allocation(a, 5).is_some());
    assert_eq!(aggregate.node_allocation_map().len(), 4);
}

// ---------------------------------------------------------------------------
// NodePlanMaterials::from_revision
// ---------------------------------------------------------------------------

#[test]
fn node_plan_materials_pairs_lines_with_that_nodes_scopes() {
    let revision = revision(vec![
        plan_line(34, 1_000, false),
        plan_line(35, 20, true),
        plan_line(36, 5, false),
    ]);
    let scopes = [FulfillmentScopeOverride {
        type_id: 34,
        scope: FulfillmentScope::Full,
    }];
    let materials = NodePlanMaterials::from_revision(&revision, &scopes);

    assert_eq!(materials.lines.len(), 3);
    assert_eq!(materials.lines[0].type_id, 34);
    assert_eq!(materials.lines[0].scope, FulfillmentScope::Full);
    assert_eq!(materials.lines[0].total_quantity, 1_000);
    assert_eq!(materials.lines[1].scope, FulfillmentScope::Missing); // default
    assert!(materials.lines[1].is_build_resolved);
}

// ---------------------------------------------------------------------------
// materials_slots_from_revision -- descendant slot classification
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// normalize_slots -- root slot classification (expansion-authoritative)
// ---------------------------------------------------------------------------
