//! Execution Plan regression matrix: pure-function tests over
//! hand-built allocation-aware evidence, in the same style as
//! `crate::build_cost::tests` and `crate::build_materials::tests` (no
//! `DATABASE_URL`, no tree walk -- `VerificationOperationInput`/
//! `VerificationBoundaryInput` fixtures are constructed directly via a small
//! recursive `Spec`/`Req` builder, then run through the *real*
//! `project_build_cost` before `project_execution_plan`, so every fixture is
//! exercised by both the authoritative cost engine and this module).

use std::collections::BTreeMap;

use chrono::Utc;
use rust_decimal::Decimal;
use uuid::Uuid;

use super::*;
use crate::build_cost::project_build_cost;
use crate::industry::RecipeCurrency;
use crate::FulfillmentScope;

// ---------------------------------------------------------------------------
// fixture DSL
// ---------------------------------------------------------------------------

fn money(value: &str) -> Money {
    Money::parse(value).unwrap()
}
fn dec(value: &str) -> Decimal {
    value.parse().unwrap()
}

/// A logical production node -- one [`VerificationOperationInput`] plus its
/// own requirements, recursively.
#[derive(Clone)]
struct Spec {
    product_type_id: i64,
    product_name: &'static str,
    activity: MaterialActivity,
    blueprint_or_formula_type_id: i64,
    node_runs: u64,
    output_per_run: u64,
    facility_id: Option<Uuid>,
    me: Option<u8>,
    te: Option<u8>,
    requirements: Vec<Req>,
}

#[derive(Clone)]
enum Req {
    Buy {
        type_id: i64,
        name: &'static str,
        required: u64,
        planned_use: u64,
        fresh_unit_price: Option<Money>,
        scope: FulfillmentScope,
    },
    Unresolved {
        type_id: i64,
        name: &'static str,
        required: u64,
        planned_use: u64,
        recipe: RecipeSelection,
    },
    Produced {
        type_id: i64,
        name: &'static str,
        required: u64,
        planned_use: u64,
        scope: FulfillmentScope,
        child: Box<Spec>,
    },
}

fn base_spec(product_type_id: i64, name: &'static str) -> Spec {
    Spec {
        product_type_id,
        product_name: name,
        activity: MaterialActivity::Manufacturing,
        blueprint_or_formula_type_id: product_type_id + 100_000,
        node_runs: 1,
        output_per_run: 1,
        facility_id: None,
        me: None,
        te: None,
        requirements: Vec::new(),
    }
}

fn buy(type_id: i64, name: &'static str, required: u64) -> Req {
    Req::Buy {
        type_id,
        name,
        required,
        planned_use: 0,
        fresh_unit_price: None,
        scope: FulfillmentScope::Missing,
    }
}

fn buy_priced(type_id: i64, name: &'static str, required: u64, price: &str) -> Req {
    Req::Buy {
        type_id,
        name,
        required,
        planned_use: 0,
        fresh_unit_price: Some(money(price)),
        scope: FulfillmentScope::Missing,
    }
}

fn buy_partial(type_id: i64, name: &'static str, required: u64, planned_use: u64) -> Req {
    Req::Buy {
        type_id,
        name,
        required,
        planned_use,
        fresh_unit_price: None,
        scope: FulfillmentScope::Missing,
    }
}

fn produced(type_id: i64, name: &'static str, required: u64, child: Spec) -> Req {
    Req::Produced {
        type_id,
        name,
        required,
        planned_use: 0,
        scope: FulfillmentScope::Missing,
        child: Box::new(child),
    }
}

fn produced_partial(
    type_id: i64,
    name: &'static str,
    required: u64,
    planned_use: u64,
    child: Spec,
) -> Req {
    Req::Produced {
        type_id,
        name,
        required,
        planned_use,
        scope: FulfillmentScope::Missing,
        child: Box::new(child),
    }
}

fn produced_full_scope(type_id: i64, name: &'static str, required: u64, child: Spec) -> Req {
    Req::Produced {
        type_id,
        name,
        required,
        planned_use: 0,
        scope: FulfillmentScope::Full,
        child: Box::new(child),
    }
}

fn unresolved(type_id: i64, name: &'static str, required: u64, recipe: RecipeSelection) -> Req {
    Req::Unresolved {
        type_id,
        name,
        required,
        planned_use: 0,
        recipe,
    }
}

fn gid(op_index: u32) -> String {
    if op_index == 0 {
        "root:00000000-0000-0000-0000-000000000000".to_string()
    } else {
        format!("build:00000000-0000-0000-0000-{op_index:012}")
    }
}

#[allow(clippy::too_many_arguments)]
fn make_boundary(
    traversal_index: u32,
    op_index: u32,
    parent_traversal_index: Option<u32>,
    type_id: i64,
    type_name: &str,
    activity: MaterialActivity,
    resolution: MaterialBoundaryResolution,
    scope: FulfillmentScope,
    required: u64,
    planned_use: u64,
    output_per_run: u64,
) -> VerificationBoundaryInput {
    let shortage = required - planned_use;
    VerificationBoundaryInput {
        traversal_index,
        parent_traversal_index,
        op_index,
        build_id: BuildId(Uuid::from_u128(u128::from(op_index) + 1)),
        graph_node_id: gid(op_index),
        tree_path: Vec::new(),
        type_id,
        type_name: type_name.to_string(),
        activity,
        resolution,
        scope,
        node_runs: 1,
        base_quantity_per_run: 0,
        blueprint_me: 0,
        facility_material_factor: Decimal::ONE,
        output_per_run,
        starting_inventory: planned_use,
        api_required: required,
        api_planned_use: planned_use,
        api_shortage: shortage,
        api_child_runs: 0,
        api_produced: 0,
        api_surplus: 0,
        fresh_unit_price: None,
        fresh_price_selection: crate::PricingSelectionKind::Default,
        fresh_pricing_policy: None,
        fresh_price_note: String::new(),
        fresh_price_stale: false,
        market_region_id: None,
        market_location_id: None,
        intended_recipe: None,
        dependency_id: String::new(),
        producer_build_id: None,
    }
}

fn make_op(op_index: u32, parent_op_index: Option<u32>, spec: &Spec) -> VerificationOperationInput {
    VerificationOperationInput {
        op_index,
        parent_op_index,
        parent_traversal_index: None,
        incoming: Vec::new(),
        graph_node_id: gid(op_index),
        build_id: BuildId(Uuid::from_u128(u128::from(op_index) + 1)),
        revision: 1,
        tree_path: Vec::new(),
        activity: spec.activity,
        product_type_id: spec.product_type_id,
        product_name: spec.product_name.to_string(),
        output_per_run: spec.output_per_run,
        base_material_count: u32::try_from(spec.requirements.len()).unwrap_or(u32::MAX),
        blueprint_or_formula_type_id: spec.blueprint_or_formula_type_id,
        blueprint_or_formula_name: format!("Recipe {}", spec.blueprint_or_formula_type_id),
        node_runs: spec.node_runs,
        persisted_runs: spec.node_runs,
        recipe_currency: RecipeCurrency::Current,
        me: spec.me,
        te: spec.te,
        blueprint_selection: None,
        facility_id: spec.facility_id,
        facility_name: spec.facility_id.map(|_| "Test Facility".to_string()),
        structure_type: None,
        solar_system: None,
        structure_material_reduction_percent: Decimal::ZERO,
        structure_time_reduction_percent: Decimal::ZERO,
        effective_material_factor: Decimal::ONE,
        facility_profile_revision: spec.facility_id.map(|_| 1),
        system_cost_index: spec.facility_id.map(|_| Decimal::ZERO),
        job_cost_reduction_percent: Decimal::ZERO,
        facility_tax_percent: Decimal::ZERO,
        scc_surcharge_percent: Decimal::ZERO,
        alliance_surcharge_percent: Decimal::ZERO,
        fixed_supplemental_cost: Money::zero(),
        job_count: 1,
        installation_formula_version: if spec.facility_id.is_some() {
            "test-v1".to_string()
        } else {
            String::new()
        },
    }
}

fn build_tree(
    spec: &Spec,
) -> (
    Vec<VerificationOperationInput>,
    Vec<VerificationBoundaryInput>,
) {
    let mut ops = Vec::new();
    let mut boundaries = Vec::new();
    let mut next_op = 0u32;
    let mut next_traversal = 0u32;
    walk(
        spec,
        None,
        None,
        &mut next_op,
        &mut next_traversal,
        &mut ops,
        &mut boundaries,
    );
    (ops, boundaries)
}

fn walk(
    spec: &Spec,
    parent_op_index: Option<u32>,
    parent_traversal_index: Option<u32>,
    next_op: &mut u32,
    next_traversal: &mut u32,
    ops: &mut Vec<VerificationOperationInput>,
    boundaries: &mut Vec<VerificationBoundaryInput>,
) {
    let this_op_index = *next_op;
    *next_op += 1;
    ops.push(make_op(this_op_index, parent_op_index, spec));

    for req in &spec.requirements {
        match req {
            Req::Buy {
                type_id,
                name,
                required,
                planned_use,
                fresh_unit_price,
                scope,
            } => {
                let t = *next_traversal;
                *next_traversal += 1;
                let mut boundary = make_boundary(
                    t,
                    this_op_index,
                    parent_traversal_index,
                    *type_id,
                    name,
                    spec.activity,
                    MaterialBoundaryResolution::Buy,
                    *scope,
                    *required,
                    *planned_use,
                    0,
                );
                boundary.fresh_unit_price = *fresh_unit_price;
                boundaries.push(boundary);
            }
            Req::Unresolved {
                type_id,
                name,
                required,
                planned_use,
                recipe,
            } => {
                let t = *next_traversal;
                *next_traversal += 1;
                let mut boundary = make_boundary(
                    t,
                    this_op_index,
                    parent_traversal_index,
                    *type_id,
                    name,
                    spec.activity,
                    MaterialBoundaryResolution::Unresolved,
                    FulfillmentScope::Missing,
                    *required,
                    *planned_use,
                    0,
                );
                boundary.intended_recipe = Some(*recipe);
                boundaries.push(boundary);
            }
            Req::Produced {
                type_id,
                name,
                required,
                planned_use,
                scope,
                child,
            } => {
                let t = *next_traversal;
                *next_traversal += 1;
                let resolution = match child.activity {
                    MaterialActivity::Manufacturing => MaterialBoundaryResolution::Build,
                    MaterialActivity::Reaction => MaterialBoundaryResolution::Reaction,
                };
                let boundary = make_boundary(
                    t,
                    this_op_index,
                    parent_traversal_index,
                    *type_id,
                    name,
                    spec.activity,
                    resolution,
                    *scope,
                    *required,
                    *planned_use,
                    child.output_per_run,
                );
                let shortage = boundary.api_shortage;
                boundaries.push(boundary);
                if shortage > 0 {
                    walk(
                        child,
                        Some(this_op_index),
                        Some(t),
                        next_op,
                        next_traversal,
                        ops,
                        boundaries,
                    );
                }
            }
        }
    }
}

/// `project_execution_plan` also takes the canonical
/// `NodeMaterialAllocation`/`AggregateMaterialLine` rollups
/// (`BuildMaterialsSummary::node_allocations`/`::rows` in production, from
/// the *same* planning call -- never recomputed). Test fixtures build
/// `VerificationOperationInput`/`VerificationBoundaryInput` directly rather
/// than through the real `MaterialsAccumulator`, so these two are derived
/// here by a small, honest 1:1/grouped mapping over the same boundary
/// fixtures -- mirroring (not reimplementing) the real accumulator's own
/// per-boundary-becomes-one-allocation, per-type-fold-of-allocations shape.
/// Never used to test the accumulator itself, only to prove
/// `project_execution_plan`'s own consumption of these inputs.
fn node_allocations_from_boundaries(
    boundaries: &[VerificationBoundaryInput],
) -> Vec<crate::build_materials::NodeMaterialAllocation> {
    boundaries
        .iter()
        .map(|b| crate::build_materials::NodeMaterialAllocation {
            build_id: b.build_id,
            graph_node_id: b.graph_node_id.clone(),
            tree_path: b.tree_path.clone(),
            type_id: b.type_id,
            type_name: b.type_name.clone(),
            required_quantity: b.api_required,
            allocated_quantity: b.api_planned_use,
            shortage_quantity: b.api_shortage,
            scope: b.scope,
            resolution: b.resolution,
            provisional: b.resolution == MaterialBoundaryResolution::Unresolved,
            child_runs: 0,
            output_per_run: b.output_per_run,
            produced_quantity: 0,
            surplus_quantity: 0,
            dependency_id: String::new(),
            producer_build_id: None,
        })
        .collect()
}

fn material_lines_from_node_allocations(
    allocations: &[crate::build_materials::NodeMaterialAllocation],
) -> Vec<AggregateMaterialLine> {
    struct Totals {
        type_name: String,
        required: u64,
        allocated: u64,
        shortage: u64,
        strategy: Option<MaterialRowStrategy>,
        all_provisional: Option<bool>,
    }
    let mut by_type: BTreeMap<i64, Totals> = BTreeMap::new();
    for allocation in allocations {
        let row_strategy = match allocation.resolution {
            MaterialBoundaryResolution::Buy => MaterialRowStrategy::Buy,
            MaterialBoundaryResolution::Build => MaterialRowStrategy::Build,
            MaterialBoundaryResolution::Reaction => MaterialRowStrategy::Reaction,
            // Real accumulator derives this from `intended_recipe`; not
            // exercised by these fixtures' Unresolved rows.
            MaterialBoundaryResolution::Unresolved => MaterialRowStrategy::Build,
        };
        let entry = by_type.entry(allocation.type_id).or_insert_with(|| Totals {
            type_name: allocation.type_name.clone(),
            required: 0,
            allocated: 0,
            shortage: 0,
            strategy: None,
            all_provisional: None,
        });
        entry.required = entry.required.saturating_add(allocation.required_quantity);
        entry.allocated = entry
            .allocated
            .saturating_add(allocation.allocated_quantity);
        entry.shortage = entry.shortage.saturating_add(allocation.shortage_quantity);
        entry.strategy = Some(match entry.strategy {
            None => row_strategy,
            Some(current) if current == row_strategy => current,
            Some(_) => MaterialRowStrategy::Mixed,
        });
        entry.all_provisional =
            Some(entry.all_provisional.unwrap_or(true) && allocation.provisional);
    }
    by_type
        .into_iter()
        .map(|(type_id, totals)| AggregateMaterialLine {
            type_id,
            type_name: totals.type_name,
            required_quantity: totals.required,
            // Test-fixture approximation: these fixtures don't model a
            // separate "seeded but unused" inventory pool, so the whole-tree
            // starting inventory is exactly what was allocated. A dedicated
            // test overrides `material_lines` directly where this
            // distinction matters (see
            // `acquisitions_expose_available_quantity_from_the_whole_tree_rollup`).
            available_quantity: totals.allocated,
            reserved_quantity: 0,
            allocated_quantity: totals.allocated,
            shortage_quantity: totals.shortage,
            fully_covered: totals.shortage == 0,
            strategy: totals.strategy.unwrap_or(MaterialRowStrategy::Buy),
            provisional: totals.all_provisional.unwrap_or(false),
        })
        .collect()
}

fn run_full(
    spec: &Spec,
    inventory_basis: &[(i64, &str)],
    adjusted_prices: &[(i64, &str)],
) -> ExecutionPlanProjection {
    let (operations, boundaries) = build_tree(spec);
    let basis: Vec<crate::build_materials::InventoryBasisEntry> = inventory_basis
        .iter()
        .map(
            |(type_id, unit_basis)| crate::build_materials::InventoryBasisEntry {
                type_id: *type_id,
                quantity: 0,
                reserved_quantity: 0,
                unit_basis: Some(dec(unit_basis)),
                total_basis: Decimal::ZERO,
            },
        )
        .collect();
    let adjusted: BTreeMap<i64, Decimal> = adjusted_prices
        .iter()
        .map(|(type_id, value)| (*type_id, dec(value)))
        .collect();
    let cost = project_build_cost(&operations, &boundaries, &basis, &adjusted, None);
    let node_allocations = node_allocations_from_boundaries(&boundaries);
    let material_lines = material_lines_from_node_allocations(&node_allocations);
    project_execution_plan(
        &operations,
        &boundaries,
        &cost,
        &node_allocations,
        &material_lines,
        Utc::now(),
    )
}

fn run(spec: &Spec) -> ExecutionPlanProjection {
    run_full(spec, &[], &[])
}

#[test]
fn occurrence_requirements_copy_direct_allocation_evidence() {
    let mut reaction = base_spec(57_457, "Reinforced Carbon Fiber");
    reaction.activity = MaterialActivity::Reaction;
    reaction.requirements = vec![buy(46_135, "Carbon Fiber", 200)];
    let mut root = base_spec(3_575, "Auto-Integrity Preservation Seal");
    root.node_runs = 12;
    root.requirements = vec![
        buy_partial(23_192, "Supertense Plastics", 43, 43),
        produced(57_457, "Reinforced Carbon Fiber", 107, reaction),
        unresolved(
            99_999,
            "Unknown Component",
            5,
            RecipeSelection::Manufacturing {
                blueprint_type_id: 199_999,
            },
        ),
    ];
    let (operations, boundaries) = build_tree(&root);
    let cost = project_build_cost(&operations, &boundaries, &[], &BTreeMap::new(), None);
    let mut allocations = node_allocations_from_boundaries(&boundaries);
    let rcf = allocations
        .iter_mut()
        .find(|row| row.type_id == 57_457)
        .unwrap();
    rcf.producer_build_id = Some(operations[1].build_id);
    let material_lines = material_lines_from_node_allocations(&allocations);
    let projection = project_execution_plan(
        &operations,
        &boundaries,
        &cost,
        &allocations,
        &material_lines,
        Utc::now(),
    );

    let requirements = &occ(&projection, &gid(0)).requirements;
    let plastics = requirements
        .iter()
        .find(|row| row.type_id == 23_192)
        .unwrap();
    assert_eq!(plastics.planned_inventory_quantity, 43);
    assert_eq!(plastics.shortage_quantity, 0);
    let rcf = requirements
        .iter()
        .find(|row| row.type_id == 57_457)
        .unwrap();
    assert_eq!(rcf.required_quantity, 107);
    assert_eq!(rcf.resolution, MaterialBoundaryResolution::Reaction);
    assert_eq!(rcf.producer_node_id.as_deref(), Some(gid(1).as_str()));
    let unresolved = requirements
        .iter()
        .find(|row| row.type_id == 99_999)
        .unwrap();
    assert_eq!(
        unresolved.resolution,
        MaterialBoundaryResolution::Unresolved
    );
    assert_eq!(unresolved.producer_node_id, None);

    let json = serde_json::to_value(&projection).unwrap();
    let root = json["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|occurrence| occurrence["id"] == gid(0))
        .unwrap();
    let serialized_rcf = root["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|requirement| requirement["typeId"] == 57_457)
        .unwrap();
    assert_eq!(serialized_rcf["requiredQuantity"], 107);
    assert_eq!(serialized_rcf["producerNodeId"], gid(1));
    assert!(serialized_rcf.get("required_quantity").is_none());
}

fn occ<'a>(projection: &'a ExecutionPlanProjection, id: &str) -> &'a ExecutionOccurrence {
    projection
        .occurrences
        .iter()
        .find(|occurrence| occurrence.id == id)
        .unwrap_or_else(|| panic!("no occurrence {id}"))
}

fn node<'a>(projection: &'a ExecutionPlanProjection, id: &str) -> &'a ExecutionNode {
    projection
        .nodes
        .iter()
        .find(|node| node.id == id)
        .unwrap_or_else(|| panic!("no node {id}"))
}

// ---------------------------------------------------------------------------
// generic invariants (reused across scenarios)
// ---------------------------------------------------------------------------

/// Every grouped node's aggregate fields are exact sums of its
/// member occurrences' own authoritative fields -- never a recomputation.
fn assert_quantity_conservation(projection: &ExecutionPlanProjection) {
    for n in &projection.nodes {
        let members: Vec<&ExecutionOccurrence> = n
            .occurrence_ids
            .iter()
            .map(|id| occ(projection, id))
            .collect();
        assert_eq!(
            n.required_quantity,
            members.iter().map(|m| m.required_quantity).sum::<u64>(),
            "node {} required_quantity",
            n.id
        );
        assert_eq!(
            n.planned_inventory_quantity,
            members
                .iter()
                .map(|m| m.planned_inventory_quantity)
                .sum::<u64>(),
            "node {} planned_inventory_quantity",
            n.id
        );
        assert_eq!(
            n.production_demand,
            members.iter().map(|m| m.production_demand).sum::<u64>(),
            "node {} production_demand",
            n.id
        );
        assert_eq!(
            n.projected_output,
            members.iter().map(|m| m.projected_output).sum::<u64>(),
            "node {} projected_output",
            n.id
        );
        assert_eq!(
            n.projected_runs,
            members.iter().map(|m| m.projected_runs).sum::<u64>(),
            "node {} projected_runs",
            n.id
        );
        assert_eq!(
            n.retained_surplus_quantity,
            members
                .iter()
                .map(|m| m.retained_surplus_quantity)
                .sum::<u64>(),
            "node {} retained_surplus_quantity",
            n.id
        );
    }
}

// ---------------------------------------------------------------------------
// A -- root-only Buy-material Build
// ---------------------------------------------------------------------------

#[test]
fn a_root_only_buy_material_build() {
    let spec = Spec {
        requirements: vec![buy(34, "Tritanium", 100)],
        ..base_spec(500, "Muninn")
    };
    let p = run(&spec);

    assert_eq!(p.occurrences.len(), 1);
    assert!(p.occurrences[0].is_root);
    assert_eq!(p.nodes.len(), 1);
    assert_eq!(p.edges.len(), 0);
    assert_eq!(p.stages.len(), 1);
    assert_eq!(p.stages[0].index, 0);
    assert_eq!(p.root_node_id, p.nodes[0].id);
    assert_eq!(p.acquisitions.len(), 1);
    assert_eq!(p.acquisitions[0].type_id, 34);
    assert_eq!(p.acquisitions[0].required_quantity, 100);
    assert_eq!(p.acquisitions[0].shortage_quantity, 100);
    assert!(p.unresolved.is_empty());
}

// ---------------------------------------------------------------------------
// B -- one child Build
// ---------------------------------------------------------------------------

#[test]
fn b_one_child_build_has_two_stages_and_one_edge() {
    let child = Spec {
        requirements: vec![buy(1, "Filler", 1)],
        ..base_spec(900, "Comp")
    };
    let root = Spec {
        requirements: vec![produced(900, "Comp", 10, child)],
        ..base_spec(500, "Root")
    };
    let p = run(&root);

    assert_eq!(p.occurrences.len(), 2);
    assert_eq!(p.nodes.len(), 2);
    assert_eq!(p.stages.len(), 2);
    assert_eq!(p.stages[0].index, 0);
    assert_eq!(p.stages[1].index, 1);
    assert_eq!(p.edges.len(), 1);

    let child_occ = occ(&p, &gid(1));
    let root_occ = occ(&p, &gid(0));
    assert_eq!(child_occ.stage, 0);
    assert_eq!(root_occ.stage, 1);
    assert_eq!(child_occ.required_quantity, 10);
    assert_eq!(child_occ.production_demand, 10);

    let child_node = node(&p, &gid(1));
    assert_eq!(child_node.consumers.len(), 1);
    assert_eq!(child_node.consumers[0].node_id, gid(0));
    assert_eq!(child_node.consumers[0].occurrence_id, gid(0));
    assert_eq!(child_node.consumers[0].quantity, 10);

    assert_eq!(p.edges[0].from, gid(1));
    assert_eq!(p.edges[0].to, gid(0));
    assert_quantity_conservation(&p);
}

// Stages polish pass: `projectedRuns` must be the actual authoritative
// `ceil(demand / output_per_run)` projection, never a 1:1 mirror of
// `requiredQuantity` -- proven with an `output_per_run > 1` fixture where
// the two numbers are deliberately far apart.
#[test]
fn projected_runs_reflects_output_per_run_not_a_1_to_1_mapping_with_required() {
    // `node_runs` here is set to exactly what the real allocator
    // (`industry/service.rs`'s `remaining.div_ceil(output_per_run)`) would
    // have computed for a demand of 2_500 at output_per_run 100 -- this
    // module's pure projector never recomputes run sizing itself, it only
    // carries `VerificationOperationInput::node_runs` through verbatim to
    // `ExecutionOccurrence`/`ExecutionNode::projected_runs`. This is the
    // exact plumbing the UI's Runs column depends on.
    let child = Spec {
        node_runs: 2_500_u64.div_ceil(100),
        output_per_run: 100,
        requirements: vec![buy(1, "Filler", 1)],
        ..base_spec(900, "Comp")
    };
    let root = Spec {
        requirements: vec![produced(900, "Comp", 2_500, child)],
        ..base_spec(500, "Root")
    };
    let p = run(&root);

    let child_occ = occ(&p, &gid(1));
    assert_eq!(child_occ.required_quantity, 2_500);
    assert_eq!(child_occ.projected_runs, 25);
    assert_ne!(
        child_occ.required_quantity, child_occ.projected_runs,
        "required != runs whenever output_per_run > 1 -- the Runs column is not a disguised \
         copy of Required, it is ceil(demand / output_per_run)"
    );
}

// ---------------------------------------------------------------------------
// C -- three-level chain
// ---------------------------------------------------------------------------

#[test]
fn c_three_level_chain_stage_numbers_increase_toward_root() {
    let leaf = Spec {
        requirements: vec![buy(34, "Trit", 1_000)],
        ..base_spec(92_001, "B")
    };
    let mid = Spec {
        requirements: vec![buy(34, "Trit", 100), produced(92_001, "B", 300, leaf)],
        ..base_spec(91_001, "A")
    };
    let root = Spec {
        requirements: vec![produced(91_001, "A", 250, mid)],
        ..base_spec(500, "Root")
    };
    let p = run(&root);

    assert_eq!(occ(&p, &gid(2)).stage, 0); // B (leaf)
    assert_eq!(occ(&p, &gid(1)).stage, 1); // A
    assert_eq!(occ(&p, &gid(0)).stage, 2); // Root
    assert_eq!(p.stages.len(), 3);
    assert_eq!(p.edges.len(), 2);
    assert_quantity_conservation(&p);
}

// ---------------------------------------------------------------------------
// D -- branching tree: A(root) -> B -> D, A -> C
// ---------------------------------------------------------------------------

fn branching_tree() -> Spec {
    let d = Spec {
        requirements: vec![buy(34, "Trit", 5)],
        ..base_spec(700, "D")
    };
    let b = Spec {
        requirements: vec![buy(35, "Pyer", 5), produced(700, "D", 20, d)],
        ..base_spec(800, "B")
    };
    let c = Spec {
        requirements: vec![buy(36, "Mex", 5)],
        ..base_spec(600, "C")
    };
    Spec {
        requirements: vec![produced(800, "B", 10, b), produced(600, "C", 10, c)],
        ..base_spec(500, "A")
    }
}

#[test]
fn d_branching_tree_has_correct_stage_numbers_and_parallel_siblings() {
    let p = run(&branching_tree());
    // DFS order: A=0, B=1, D=2, C=3.
    assert_eq!(occ(&p, &gid(2)).stage, 0); // D (leaf)
    assert_eq!(occ(&p, &gid(3)).stage, 0); // C (leaf, unrelated branch)
    assert_eq!(occ(&p, &gid(1)).stage, 1); // B (child D)
    assert_eq!(occ(&p, &gid(0)).stage, 2); // A (root)

    let stage0 = &p.stages.iter().find(|s| s.index == 0).unwrap().node_ids;
    assert!(stage0.contains(&gid(2)));
    assert!(stage0.contains(&gid(3)));
    assert_quantity_conservation(&p);
}

// ---------------------------------------------------------------------------
// E / K / T -- shared compatible intermediate, two consumers (the
// Neo-Mercurite regression)
// ---------------------------------------------------------------------------

/// root -> ConsumerA(Nanotransistors) -> NeoMercurite(runs=4, output/run=100,
/// demand=300, surplus=100); root -> ConsumerB(Plasmonic Metamaterials) ->
/// NeoMercurite(runs=9, output/run=100, demand=700, surplus=200). Both
/// NeoMercurite occurrences share the same Reaction/formula/facility
/// compatibility key (activity=Reaction => ME/TE both `None` on both sides)
/// and are mutually unrelated (independent branches) -- they MUST group.
fn neo_mercurite_scenario() -> Spec {
    let facility = Uuid::from_u128(999);
    let neo_a = Spec {
        node_runs: 4,
        facility_id: Some(facility),
        requirements: vec![buy_priced(1, "Filler", 1, "0")],
        ..base_spec(700, "Neo Mercurite")
    }
    .with_reaction(7_000, 100);
    let neo_b = Spec {
        node_runs: 9,
        facility_id: Some(facility),
        requirements: vec![buy_priced(1, "Filler", 1, "0")],
        ..base_spec(700, "Neo Mercurite")
    }
    .with_reaction(7_000, 100);
    let consumer_a = Spec {
        requirements: vec![produced(700, "Neo Mercurite", 300, neo_a)],
        ..base_spec(810, "Nanotransistors")
    };
    let consumer_b = Spec {
        requirements: vec![produced(700, "Neo Mercurite", 700, neo_b)],
        ..base_spec(820, "Plasmonic Metamaterials")
    };
    Spec {
        requirements: vec![
            produced(810, "Nanotransistors", 1, consumer_a),
            produced(820, "Plasmonic Metamaterials", 1, consumer_b),
        ],
        ..base_spec(500, "Muninn")
    }
}

impl Spec {
    fn with_reaction(mut self, formula_type_id: i64, output_per_run: u64) -> Spec {
        self.activity = MaterialActivity::Reaction;
        self.blueprint_or_formula_type_id = formula_type_id;
        self.output_per_run = output_per_run;
        self
    }
}

// ---------------------------------------------------------------------------
// G/H/I -- incompatible occurrences never group
// ---------------------------------------------------------------------------

fn two_consumer_tree(intermediate_a: Spec, intermediate_b: Spec) -> Spec {
    let consumer_a = Spec {
        requirements: vec![produced(
            intermediate_a.product_type_id,
            "Shared",
            300,
            intermediate_a,
        )],
        ..base_spec(810, "ConsumerA")
    };
    let consumer_b = Spec {
        requirements: vec![produced(
            intermediate_b.product_type_id,
            "Shared",
            300,
            intermediate_b,
        )],
        ..base_spec(820, "ConsumerB")
    };
    Spec {
        requirements: vec![
            produced(810, "ConsumerA", 1, consumer_a),
            produced(820, "ConsumerB", 1, consumer_b),
        ],
        ..base_spec(500, "Root")
    }
}

#[test]
fn g_same_type_different_facility_never_groups() {
    let facility_a = Uuid::from_u128(1);
    let facility_b = Uuid::from_u128(2);
    let a = Spec {
        node_runs: 3,
        facility_id: Some(facility_a),
        me: Some(10),
        te: Some(10),
        blueprint_or_formula_type_id: 7_000,
        output_per_run: 100,
        ..base_spec(700, "Shared")
    };
    let b = Spec {
        facility_id: Some(facility_b),
        ..a.clone()
    };
    let p = run(&two_consumer_tree(a, b));
    assert_ne!(occ(&p, &gid(2)).node_id, occ(&p, &gid(4)).node_id);
    assert_eq!(
        p.nodes.iter().filter(|n| n.output_type_id == 700).count(),
        2
    );
}

#[test]
fn h_same_type_different_me_te_never_groups() {
    let facility = Uuid::from_u128(1);
    let a = Spec {
        node_runs: 3,
        facility_id: Some(facility),
        me: Some(10),
        te: Some(10),
        blueprint_or_formula_type_id: 7_000,
        output_per_run: 100,
        ..base_spec(700, "Shared")
    };
    let b = Spec {
        me: Some(0),
        ..a.clone()
    };
    let p = run(&two_consumer_tree(a, b));
    assert_ne!(occ(&p, &gid(2)).node_id, occ(&p, &gid(4)).node_id);
}

#[test]
fn i_same_type_different_blueprint_never_groups() {
    let facility = Uuid::from_u128(1);
    let a = Spec {
        node_runs: 3,
        facility_id: Some(facility),
        me: Some(10),
        te: Some(10),
        blueprint_or_formula_type_id: 7_000,
        output_per_run: 100,
        ..base_spec(700, "Shared")
    };
    let b = Spec {
        blueprint_or_formula_type_id: 7_001,
        ..a.clone()
    };
    let p = run(&two_consumer_tree(a, b));
    assert_ne!(occ(&p, &gid(2)).node_id, occ(&p, &gid(4)).node_id);
}

// ---------------------------------------------------------------------------
// J -- synthetic ancestor/descendant collision never groups, even though
// current real SDE data could never produce it
// ---------------------------------------------------------------------------

#[test]
fn ancestor_descendant_collision_never_groups_even_with_a_matching_key() {
    let facility = Uuid::from_u128(1);
    // B has the SAME compatibility key as its own ancestor A -- impossible
    // under real SDE recipes (an item cannot require itself), but the
    // grouping guard must not depend on that assumption.
    let b = Spec {
        node_runs: 1,
        facility_id: Some(facility),
        blueprint_or_formula_type_id: 7_000,
        output_per_run: 50,
        activity: MaterialActivity::Reaction,
        requirements: vec![buy(1, "Filler", 1)],
        ..base_spec(700, "Weird")
    };
    let a = Spec {
        node_runs: 1,
        facility_id: Some(facility),
        blueprint_or_formula_type_id: 7_000,
        output_per_run: 50,
        activity: MaterialActivity::Reaction,
        requirements: vec![produced(700, "Weird", 40, b)],
        ..base_spec(700, "Weird")
    };
    let root = Spec {
        requirements: vec![produced(700, "Weird", 1, a)],
        ..base_spec(500, "Root")
    };
    let p = run(&root);

    // DFS order: root=0, A=1, B=2.
    assert_ne!(
        occ(&p, &gid(1)).node_id,
        occ(&p, &gid(2)).node_id,
        "an occurrence must never group with its own ancestor/descendant"
    );
    assert_eq!(
        p.nodes.iter().filter(|n| n.output_type_id == 700).count(),
        2
    );
}

// ---------------------------------------------------------------------------
// L -- differing PlanningInventory allocations preserved
// ---------------------------------------------------------------------------

#[test]
fn l_differing_inventory_allocations_are_preserved_not_redistributed() {
    let facility = Uuid::from_u128(7);
    let make_intermediate = |runs: u64| Spec {
        node_runs: runs,
        facility_id: Some(facility),
        blueprint_or_formula_type_id: 7_000,
        output_per_run: 100,
        activity: MaterialActivity::Reaction,
        requirements: vec![buy(1, "Filler", 1)],
        ..base_spec(700, "Shared")
    };
    let consumer_a = Spec {
        requirements: vec![produced_partial(
            700,
            "Shared",
            500,
            200,
            make_intermediate(3),
        )],
        ..base_spec(810, "ConsumerA")
    };
    let consumer_b = Spec {
        requirements: vec![produced(700, "Shared", 500, make_intermediate(5))],
        ..base_spec(820, "ConsumerB")
    };
    let root = Spec {
        requirements: vec![
            produced(810, "ConsumerA", 1, consumer_a),
            produced(820, "ConsumerB", 1, consumer_b),
        ],
        ..base_spec(500, "Root")
    };
    let p = run(&root);

    assert_eq!(occ(&p, &gid(2)).planned_inventory_quantity, 200);
    assert_eq!(occ(&p, &gid(4)).planned_inventory_quantity, 0);
    let group_id = occ(&p, &gid(2)).node_id.clone();
    assert_eq!(node(&p, &group_id).planned_inventory_quantity, 200);
    assert_quantity_conservation(&p);
}

// ---------------------------------------------------------------------------
// M -- fully inventory-covered Build child is absent (never fabricated)
// ---------------------------------------------------------------------------

#[test]
fn m_fully_inventory_covered_child_produces_no_node() {
    let child = Spec {
        requirements: vec![buy(1, "Filler", 1)],
        ..base_spec(900, "Comp")
    };
    let root = Spec {
        // required == planned_use -> shortage 0 -> pruned, per the allocator's
        // own Missing-fulfillment-scope semantics.
        requirements: vec![produced_partial(900, "Comp", 100, 100, child)],
        ..base_spec(500, "Root")
    };
    let p = run(&root);

    assert_eq!(
        p.occurrences.len(),
        1,
        "only the root -- Comp was never walked"
    );
    assert!(p.nodes.iter().all(|n| n.output_type_id != 900));
    assert!(p.acquisitions.is_empty());
}

// ---------------------------------------------------------------------------
// N -- Full-scope production remains present despite available inventory
// ---------------------------------------------------------------------------

#[test]
fn n_full_scope_child_is_included_never_pruned() {
    let child = Spec {
        requirements: vec![buy(1, "Filler", 1)],
        ..base_spec(900, "Comp")
    };
    let root = Spec {
        requirements: vec![produced_full_scope(900, "Comp", 100, child)],
        ..base_spec(500, "Root")
    };
    let p = run(&root);

    assert_eq!(p.occurrences.len(), 2);
    assert_eq!(occ(&p, &gid(1)).required_quantity, 100);
    assert_eq!(occ(&p, &gid(1)).production_demand, 100);
}

// ---------------------------------------------------------------------------
// O -- Reaction -> Reaction -> Manufacturing chain
// ---------------------------------------------------------------------------

#[test]
fn o_reaction_reaction_manufacturing_chain_preserves_activity() {
    let neo_mercurite = Spec {
        activity: MaterialActivity::Reaction,
        blueprint_or_formula_type_id: 7_000,
        output_per_run: 100,
        node_runs: 3,
        requirements: vec![buy(1, "Filler", 1)],
        ..base_spec(700, "Neo Mercurite")
    };
    let nanotransistors = Spec {
        activity: MaterialActivity::Reaction,
        blueprint_or_formula_type_id: 8_000,
        output_per_run: 10,
        node_runs: 2,
        requirements: vec![produced(700, "Neo Mercurite", 300, neo_mercurite)],
        ..base_spec(810, "Nanotransistors")
    };
    let component = Spec {
        activity: MaterialActivity::Manufacturing,
        blueprint_or_formula_type_id: 9_000,
        requirements: vec![produced(810, "Nanotransistors", 15, nanotransistors)],
        ..base_spec(950, "Component")
    };
    let ship = Spec {
        requirements: vec![produced(950, "Component", 1, component)],
        ..base_spec(500, "Ship")
    };
    let p = run(&ship);

    assert_eq!(occ(&p, &gid(3)).activity, MaterialActivity::Reaction); // Neo Mercurite
    assert_eq!(occ(&p, &gid(2)).activity, MaterialActivity::Reaction); // Nanotransistors
    assert_eq!(occ(&p, &gid(1)).activity, MaterialActivity::Manufacturing); // Component
    assert_eq!(occ(&p, &gid(0)).activity, MaterialActivity::Manufacturing); // Ship (root)
    assert_eq!(occ(&p, &gid(3)).stage, 0);
    assert_eq!(occ(&p, &gid(2)).stage, 1);
    assert_eq!(occ(&p, &gid(1)).stage, 2);
    assert_eq!(occ(&p, &gid(0)).stage, 3);
}

// ---------------------------------------------------------------------------
// P/Q -- determinism: same input twice -> structurally identical output
// ---------------------------------------------------------------------------

#[test]
fn deterministic_stage_grouping_and_ordering_across_reprojection() {
    let now = Utc::now();

    let (operations, boundaries) = build_tree(&branching_tree());
    let cost = project_build_cost(&operations, &boundaries, &[], &BTreeMap::new(), None);
    let node_allocations = node_allocations_from_boundaries(&boundaries);
    let material_lines = material_lines_from_node_allocations(&node_allocations);
    let first = project_execution_plan(
        &operations,
        &boundaries,
        &cost,
        &node_allocations,
        &material_lines,
        now,
    );
    let second = project_execution_plan(
        &operations,
        &boundaries,
        &cost,
        &node_allocations,
        &material_lines,
        now,
    );
    assert_eq!(first, second);

    let (operations, boundaries) = build_tree(&neo_mercurite_scenario());
    let cost = project_build_cost(&operations, &boundaries, &[], &BTreeMap::new(), None);
    let node_allocations = node_allocations_from_boundaries(&boundaries);
    let material_lines = material_lines_from_node_allocations(&node_allocations);
    let first = project_execution_plan(
        &operations,
        &boundaries,
        &cost,
        &node_allocations,
        &material_lines,
        now,
    );
    let second = project_execution_plan(
        &operations,
        &boundaries,
        &cost,
        &node_allocations,
        &material_lines,
        now,
    );
    assert_eq!(first, second);
}

// ---------------------------------------------------------------------------
// R -- quantity conservation across a richer tree
// ---------------------------------------------------------------------------

#[test]
fn r_quantity_conservation_holds_across_branching_and_grouped_trees() {
    assert_quantity_conservation(&run(&branching_tree()));
    assert_quantity_conservation(&run(&neo_mercurite_scenario()));
}

// ---------------------------------------------------------------------------
// S -- cost conservation
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// U -- an incomplete member propagates group incompleteness
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Core regression (grouped): a known physical surplus on an
// incomplete-cost occurrence must survive both the boundary fix AND
// grouping -- never erased, never blended with the complete occurrence's
// own known cost.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// V -- unresolved boundary: truthful placeholder, no fabricated descendant
// ---------------------------------------------------------------------------

#[test]
fn v_unresolved_boundary_is_a_truthful_placeholder() {
    let root = Spec {
        requirements: vec![unresolved(
            900,
            "Comp",
            50,
            RecipeSelection::Manufacturing {
                blueprint_type_id: 2_000,
            },
        )],
        ..base_spec(500, "Root")
    };
    let p = run(&root);

    assert_eq!(p.occurrences.len(), 1, "no fabricated descendant");
    assert!(p.nodes.iter().all(|n| n.output_type_id != 900));
    assert_eq!(p.unresolved.len(), 1);
    let u = &p.unresolved[0];
    assert_eq!(u.type_id, 900);
    assert_eq!(u.required_quantity, 50);
    assert_eq!(u.net_required_quantity, 50);
    assert_eq!(u.owning_occurrence_id, gid(0));
    assert_eq!(
        u.intended_recipe,
        Some(RecipeSelection::Manufacturing {
            blueprint_type_id: 2_000
        })
    );
}

// ---------------------------------------------------------------------------
// W -- root always occupies the maximum stage, derived from topology
// ---------------------------------------------------------------------------

#[test]
fn w_root_occupies_the_maximum_stage() {
    let p = run(&branching_tree());
    let root_stage = occ(&p, &gid(0)).stage;
    let max_stage = p.stages.last().unwrap().index;
    assert_eq!(root_stage, max_stage);
    assert_eq!(p.root_node_id, occ(&p, &gid(0)).node_id);
}

// ---------------------------------------------------------------------------
// X -- non-linear run aggregation counterexample
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// acquisitions: Buy-only, shortage-only rollup
// ---------------------------------------------------------------------------

#[test]
fn acquisitions_include_only_buy_shortage_and_exclude_fully_covered() {
    let child = Spec {
        // Fully covered leaf of its own -- shortage 0, so it must not leak
        // into the acquisitions rollup either.
        requirements: vec![buy_partial(1, "Filler", 1, 1)],
        ..base_spec(900, "Comp")
    };
    let root = Spec {
        requirements: vec![
            buy(34, "Tritanium", 100),          // shortage 100 -> included
            buy_partial(35, "Pyerite", 50, 50), // shortage 0 -> excluded
            produced(900, "Comp", 10, child),   // Build resolution -> never an acquisition
        ],
        ..base_spec(500, "Root")
    };
    let p = run(&root);

    assert_eq!(p.acquisitions.len(), 1);
    assert_eq!(p.acquisitions[0].type_id, 34);
    assert_eq!(p.acquisitions[0].shortage_quantity, 100);
}

// ---------------------------------------------------------------------------
// A type sourced as Buy in one branch and Build/Reaction in
// another (`Mixed` in the whole-tree rollup) must expose ONLY its Buy-side
// shortage as acquisition work -- never the Build/Reaction production
// shortage misclassified as something to purchase.
// ---------------------------------------------------------------------------

#[test]
fn acquisitions_never_conflate_mixed_source_shortage_with_buy_shortage() {
    let child = Spec {
        requirements: vec![buy(1, "Filler", 1)],
        ..base_spec(999, "Widget")
    };
    let consumer_a = Spec {
        requirements: vec![buy(999, "Widget", 50)],
        ..base_spec(810, "ConsumerA")
    };
    let consumer_b = Spec {
        requirements: vec![produced(999, "Widget", 200, child)],
        ..base_spec(820, "ConsumerB")
    };
    let root = Spec {
        requirements: vec![
            produced(810, "ConsumerA", 1, consumer_a),
            produced(820, "ConsumerB", 1, consumer_b),
        ],
        ..base_spec(500, "Root")
    };
    let p = run(&root);

    let widget = p
        .acquisitions
        .iter()
        .find(|a| a.type_id == 999)
        .expect("Widget acquisition line");
    assert_eq!(
        widget.shortage_quantity, 50,
        "must reflect ONLY the Buy-side shortage, never the 200-unit \
         Build-side production shortage from ConsumerB"
    );
    assert_eq!(widget.required_quantity, 50);
    assert_eq!(
        widget.source_strategy,
        MaterialRowStrategy::Mixed,
        "informational only -- the type is ALSO produced elsewhere in the tree"
    );
}

// ---------------------------------------------------------------------------
// Stages polish pass: acquisition consumer provenance ("what is this
// acquired material used by?").
// ---------------------------------------------------------------------------

#[test]
fn acquisitions_expose_a_single_consumer_occurrence() {
    let root = Spec {
        requirements: vec![buy(34, "Tritanium", 500)],
        ..base_spec(500, "Root")
    };
    let p = run(&root);

    let line = p.acquisitions.iter().find(|a| a.type_id == 34).unwrap();
    assert_eq!(line.consumers.len(), 1);
    let consumer = &line.consumers[0];
    assert_eq!(consumer.occurrence_id, gid(0));
    assert_eq!(consumer.node_id, occ(&p, &gid(0)).node_id);
    assert_eq!(consumer.quantity, 500);
}

#[test]
fn acquisitions_expose_multiple_consumers_with_authoritative_reconciling_quantities() {
    let ferrogel = Spec {
        requirements: vec![buy(999, "Widget", 8_000)],
        ..base_spec(810, "Ferrogel")
    };
    let phenolic = Spec {
        requirements: vec![buy(999, "Widget", 6_000)],
        ..base_spec(820, "Phenolic Composites")
    };
    let root = Spec {
        requirements: vec![
            produced(810, "Ferrogel", 1, ferrogel),
            produced(820, "Phenolic Composites", 1, phenolic),
        ],
        ..base_spec(500, "Root")
    };
    let p = run(&root);

    // DFS order: root=0, Ferrogel=1, Phenolic Composites=2.
    let line = p.acquisitions.iter().find(|a| a.type_id == 999).unwrap();
    assert_eq!(line.shortage_quantity, 14_000);
    assert_eq!(line.consumers.len(), 2);

    let total: u64 = line.consumers.iter().map(|c| c.quantity).sum();
    assert_eq!(
        total, line.shortage_quantity,
        "consumer quantities must reconcile exactly with the line's own shortage total, \
         with no proportional split invented"
    );

    let ferrogel_consumer = line
        .consumers
        .iter()
        .find(|c| c.occurrence_id == gid(1))
        .expect("Ferrogel consumer entry");
    assert_eq!(ferrogel_consumer.quantity, 8_000);
    assert_eq!(ferrogel_consumer.node_id, occ(&p, &gid(1)).node_id);

    let phenolic_consumer = line
        .consumers
        .iter()
        .find(|c| c.occurrence_id == gid(2))
        .expect("Phenolic Composites consumer entry");
    assert_eq!(phenolic_consumer.quantity, 6_000);
    assert_eq!(phenolic_consumer.node_id, occ(&p, &gid(2)).node_id);
}

#[test]
fn acquisitions_never_include_a_consumer_whose_own_need_is_fully_covered() {
    // ConsumerA has a genuine shortage; ConsumerB's own need for the SAME
    // type is fully covered by inventory -- it must not appear as a "Used
    // By" reason to acquire more.
    let consumer_a = Spec {
        requirements: vec![buy(999, "Widget", 1_000)],
        ..base_spec(810, "ConsumerA")
    };
    let consumer_b = Spec {
        requirements: vec![buy_partial(999, "Widget", 500, 500)],
        ..base_spec(820, "ConsumerB")
    };
    let root = Spec {
        requirements: vec![
            produced(810, "ConsumerA", 1, consumer_a),
            produced(820, "ConsumerB", 1, consumer_b),
        ],
        ..base_spec(500, "Root")
    };
    let p = run(&root);

    let line = p.acquisitions.iter().find(|a| a.type_id == 999).unwrap();
    assert_eq!(line.shortage_quantity, 1_000);
    assert_eq!(
        line.consumers.len(),
        1,
        "ConsumerB's fully-covered need is not a reason to buy"
    );
    assert_eq!(line.consumers[0].occurrence_id, gid(1));
    assert_eq!(line.consumers[0].quantity, 1_000);
}

// ---------------------------------------------------------------------------
// `available_quantity` is read from the whole-tree rollup, never recomputed
// -- proved with a value that could not come from anywhere else.
// ---------------------------------------------------------------------------

#[test]
fn acquisitions_expose_available_quantity_from_the_whole_tree_rollup() {
    let root = Spec {
        requirements: vec![buy(34, "Tritanium", 100)],
        ..base_spec(500, "Root")
    };
    let (operations, boundaries) = build_tree(&root);
    let cost = project_build_cost(&operations, &boundaries, &[], &BTreeMap::new(), None);
    let node_allocations = node_allocations_from_boundaries(&boundaries);
    // A value that could not come from anywhere but `material_lines` --
    // proves the field is read through, never recomputed from the
    // allocations themselves.
    let material_lines = vec![AggregateMaterialLine {
        type_id: 34,
        type_name: "Tritanium".to_string(),
        required_quantity: 100,
        available_quantity: 9_999,
        reserved_quantity: 4_321,
        allocated_quantity: 0,
        shortage_quantity: 100,
        fully_covered: false,
        strategy: MaterialRowStrategy::Buy,
        provisional: false,
    }];
    let p = project_execution_plan(
        &operations,
        &boundaries,
        &cost,
        &node_allocations,
        &material_lines,
        Utc::now(),
    );

    let line = p.acquisitions.iter().find(|a| a.type_id == 34).unwrap();
    assert_eq!(line.available_quantity, 9_999);
    assert_eq!(line.reserved_quantity, 4_321);
    assert_eq!(line.source_strategy, MaterialRowStrategy::Buy);
}

// ---------------------------------------------------------------------------
// empty input
// ---------------------------------------------------------------------------

#[test]
fn empty_operations_yields_a_trivially_complete_empty_projection() {
    let cost = project_build_cost(&[], &[], &[], &BTreeMap::new(), None);
    let p = project_execution_plan(&[], &[], &cost, &[], &[], Utc::now());
    assert!(p.nodes.is_empty());
    assert!(p.occurrences.is_empty());
    assert!(p.stages.is_empty());
    assert!(p.complete);
}

// ---------------------------------------------------------------------------
// Per-edge and cost evidence for the Plan inspector.
// ---------------------------------------------------------------------------

/// Root buys Tritanium directly (priced) and builds a Hull from Tritanium
/// (unpriced there): the acquisition line reports each edge's own fresh
/// cost, and the total stays `None` because one edge is unpriced.
#[test]
fn acquisitions_carry_fresh_cost_per_edge_and_never_a_partial_total() {
    let mut hull = base_spec(200, "Hull");
    hull.requirements = vec![buy(34, "Tritanium", 50)];
    let mut root = base_spec(100, "Ship");
    root.requirements = vec![
        buy_priced(34, "Tritanium", 100, "5"),
        produced(200, "Hull", 1, hull),
    ];
    let projection = run(&root);

    let trit = projection
        .acquisitions
        .iter()
        .find(|line| line.type_id == 34)
        .unwrap();
    assert_eq!(trit.consumers.len(), 2);
    let root_edge = trit
        .consumers
        .iter()
        .find(|consumer| consumer.occurrence_id == gid(0))
        .unwrap();
    assert_eq!(root_edge.fresh_cost, Some(money("500")));
    assert_eq!(root_edge.fresh_unit_price, Some(money("5")));
    assert_eq!(root_edge.required_quantity, 100);
    assert_eq!(root_edge.fulfillment_scope, FulfillmentScope::Missing);
    let hull_edge = trit
        .consumers
        .iter()
        .find(|consumer| consumer.occurrence_id != gid(0))
        .unwrap();
    assert_eq!(hull_edge.fresh_cost, None, "unpriced -- never fabricated");
    assert_eq!(trit.fresh_cost, None, "no partial sum");
    assert_eq!(
        trit.fresh_unit_price,
        Some(money("5")),
        "the one known price"
    );

    let mut priced_only = base_spec(100, "Ship");
    priced_only.requirements = vec![buy_priced(34, "Tritanium", 100, "5")];
    let projection = run(&priced_only);
    assert_eq!(projection.acquisitions[0].fresh_cost, Some(money("500")));
}

/// A produced component's consumer edge carries the consuming Build, the
/// edge's scope, required and planned inventory use; the operation carries
/// its unit production cost and the type's whole-tree available stock.
#[test]
fn production_consumers_carry_edge_evidence_and_operations_their_unit_cost() {
    let mut hull = base_spec(200, "Hull");
    hull.output_per_run = 10;
    hull.node_runs = 1;
    hull.requirements = vec![buy_priced(34, "Tritanium", 20, "5")];
    let mut root = base_spec(100, "Ship");
    root.requirements = vec![produced_partial(200, "Hull", 12, 4, hull)];
    let projection = run(&root);

    let hull_node = projection
        .nodes
        .iter()
        .find(|node| node.output_type_id == 200)
        .unwrap();
    assert_eq!(hull_node.consumers.len(), 1);
    let edge = &hull_node.consumers[0];
    assert_eq!(edge.required_quantity, 12);
    assert_eq!(edge.planned_inventory_quantity, 4);
    assert_eq!(edge.quantity, 8, "production demand");
    assert_eq!(edge.fulfillment_scope, FulfillmentScope::Missing);
    assert_eq!(edge.build_id, occ(&projection, &gid(0)).build_id);
    assert_eq!(hull_node.available_quantity, 4, "whole-tree stock of Hull");
    // Hull costs 100 (20 x 5, no facility -> installation incomplete, so the
    // unit cost is only known when the total is).
    assert_eq!(
        hull_node.unit_production_cost,
        occ(&projection, &hull_node.occurrence_ids[0]).unit_production_cost
    );
}
