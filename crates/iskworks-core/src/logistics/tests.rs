use std::collections::BTreeMap;

use rust_decimal::Decimal;
use uuid::Uuid;

use super::*;
use crate::build_materials::MaterialActivity;
use crate::{Money, RecipeCurrency};

const ALPHA: u128 = 1;
const BETA: u128 = 2;

fn op(
    graph_node_id: &str,
    product: &str,
    facility: Option<(u128, &str)>,
) -> VerificationOperationInput {
    let build_uuid =
        Uuid::from_u128(u128::from(graph_node_id.bytes().map(u32::from).sum::<u32>()) + 0xB000);
    VerificationOperationInput {
        op_index: 0,
        parent_op_index: None,
        parent_traversal_index: None,
        incoming: Vec::new(),
        graph_node_id: graph_node_id.to_string(),
        build_id: BuildId(build_uuid),
        revision: 1,
        tree_path: Vec::new(),
        activity: MaterialActivity::Manufacturing,
        product_type_id: 1,
        product_name: product.to_string(),
        output_per_run: 1,
        base_material_count: 1,
        blueprint_or_formula_type_id: 1,
        blueprint_or_formula_name: String::new(),
        node_runs: 1,
        persisted_runs: 1,
        recipe_currency: RecipeCurrency::Current,
        me: Some(0),
        te: Some(0),
        blueprint_selection: None,
        facility_id: facility.map(|(id, _)| Uuid::from_u128(id)),
        facility_name: facility.map(|(_, name)| name.to_string()),
        structure_type: None,
        solar_system: facility.map(|(_, name)| format!("{name} System")),
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

#[allow(clippy::too_many_arguments)]
fn alloc(
    consumer: &VerificationOperationInput,
    type_id: i64,
    name: &str,
    resolution: MaterialBoundaryResolution,
    scope: FulfillmentScope,
    required: u64,
    allocated: u64,
    producer: Option<&VerificationOperationInput>,
) -> NodeMaterialAllocation {
    NodeMaterialAllocation {
        build_id: consumer.build_id,
        graph_node_id: consumer.graph_node_id.clone(),
        tree_path: Vec::new(),
        type_id,
        type_name: name.to_string(),
        required_quantity: required,
        allocated_quantity: allocated,
        shortage_quantity: required - allocated,
        scope,
        resolution,
        provisional: resolution == MaterialBoundaryResolution::Unresolved,
        child_runs: 0,
        output_per_run: 0,
        produced_quantity: 0,
        surplus_quantity: 0,
        dependency_id: format!("pd:{}:{type_id}", consumer.graph_node_id),
        producer_build_id: producer.map(|p| p.build_id),
    }
}

fn buy(
    consumer: &VerificationOperationInput,
    type_id: i64,
    name: &str,
    required: u64,
    allocated: u64,
) -> NodeMaterialAllocation {
    alloc(
        consumer,
        type_id,
        name,
        MaterialBoundaryResolution::Buy,
        FulfillmentScope::Missing,
        required,
        allocated,
        None,
    )
}

fn volumes() -> BTreeMap<i64, Option<Decimal>> {
    BTreeMap::from([
        (34, Some(Decimal::new(1, 2))),   // Tritanium 0.01 m3
        (35, Some(Decimal::new(1, 2))),   // Pyerite 0.01 m3
        (200, Some(Decimal::new(15, 1))), // X 1.5 m3
        (300, None),                      // unknown volume
    ])
}

fn destination<'a>(plan: &'a LogisticsPlan, name: &str) -> &'a LogisticsDestination {
    plan.destinations
        .iter()
        .find(|d| d.facility_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("destination {name}"))
}

fn line(destination: &LogisticsDestination, type_id: i64) -> &LogisticsLine {
    destination
        .lines
        .iter()
        .find(|l| l.type_id == type_id)
        .unwrap_or_else(|| panic!("line {type_id}"))
}

#[test]
fn the_same_item_at_the_same_destination_aggregates() {
    let root = op("root:r", "Root", Some((ALPHA, "Alpha")));
    let a = op("build:a", "A", Some((ALPHA, "Alpha")));
    let plan = project_logistics(
        &[root.clone(), a.clone()],
        &[
            buy(&root, 34, "Tritanium", 10_000, 0),
            buy(&a, 34, "Tritanium", 5_000, 0),
        ],
        &volumes(),
    );
    assert_eq!(plan.destinations.len(), 1);
    let alpha = destination(&plan, "Alpha");
    assert_eq!(alpha.lines.len(), 1);
    let trit = line(alpha, 34);
    assert_eq!(trit.quantity, 15_000);
    assert_eq!(trit.acquire_quantity, 15_000);
    assert_eq!(trit.consumers.len(), 2);
    assert_eq!(trit.total_volume_m3, Some(Decimal::new(15_000, 2)));
    assert_eq!(alpha.solar_system.as_deref(), Some("Alpha System"));
}

#[test]
fn the_same_item_at_different_destinations_stays_separate() {
    let root = op("root:r", "Root", Some((ALPHA, "Alpha")));
    let b = op("build:b", "B", Some((BETA, "Beta")));
    let plan = project_logistics(
        &[root.clone(), b.clone()],
        &[
            buy(&root, 34, "Tritanium", 10_000, 0),
            buy(&b, 34, "Tritanium", 5_000, 0),
        ],
        &volumes(),
    );
    assert_eq!(plan.destinations.len(), 2);
    assert_eq!(line(destination(&plan, "Alpha"), 34).quantity, 10_000);
    assert_eq!(line(destination(&plan, "Beta"), 34).quantity, 5_000);
    assert_eq!(plan.total_volume_m3, Decimal::new(15_000, 2));
}

#[test]
fn planned_inventory_use_reduces_the_shortage_and_is_never_renetted() {
    let root = op("root:r", "Root", Some((ALPHA, "Alpha")));
    let plan = project_logistics(
        std::slice::from_ref(&root),
        &[buy(&root, 34, "Tritanium", 10_000, 4_000)],
        &volumes(),
    );
    let trit = line(destination(&plan, "Alpha"), 34);
    assert_eq!(trit.quantity, 10_000);
    assert_eq!(trit.planned_inventory_quantity, 4_000);
    assert_eq!(trit.shortage_quantity, 6_000);
    assert_eq!(trit.acquire_quantity, 6_000);
    // Volume covers everything needed at the destination.
    assert_eq!(trit.total_volume_m3, Some(Decimal::new(10_000, 2)));
}

#[test]
fn full_and_missing_scopes_are_carried_per_consumer() {
    let root = op("root:r", "Root", Some((ALPHA, "Alpha")));
    let a = op("build:a", "A", Some((ALPHA, "Alpha")));
    let full = alloc(
        &root,
        35,
        "Pyerite",
        MaterialBoundaryResolution::Buy,
        FulfillmentScope::Full,
        100,
        0,
        None,
    );
    let missing = buy(&a, 35, "Pyerite", 50, 50);
    let plan = project_logistics(&[root.clone(), a.clone()], &[full, missing], &volumes());
    let pye = line(destination(&plan, "Alpha"), 35);
    assert_eq!(pye.quantity, 150);
    assert_eq!(pye.planned_inventory_quantity, 50);
    assert_eq!(pye.shortage_quantity, 100);
    let scopes: Vec<FulfillmentScope> = pye.consumers.iter().map(|c| c.fulfillment_scope).collect();
    assert!(scopes.contains(&FulfillmentScope::Full));
    assert!(scopes.contains(&FulfillmentScope::Missing));
}

#[test]
fn a_produced_component_is_delivered_to_its_consumer_from_its_producer() {
    // X is produced at Beta and consumed by the root at Alpha: the input
    // belongs to Alpha (the consumer), with Beta as source evidence; X's own
    // Buy input belongs to Beta.
    let root = op("root:r", "Root", Some((ALPHA, "Alpha")));
    let x = op("build:x", "X", Some((BETA, "Beta")));
    let plan = project_logistics(
        &[root.clone(), x.clone()],
        &[
            alloc(
                &root,
                200,
                "X",
                MaterialBoundaryResolution::Build,
                FulfillmentScope::Missing,
                40,
                10,
                Some(&x),
            ),
            buy(&x, 34, "Tritanium", 300, 0),
        ],
        &volumes(),
    );
    let alpha = destination(&plan, "Alpha");
    let x_line = line(alpha, 200);
    assert_eq!(x_line.produced_quantity, 30);
    assert_eq!(x_line.acquire_quantity, 0);
    assert_eq!(x_line.producers.len(), 1);
    assert_eq!(x_line.producers[0].facility_name.as_deref(), Some("Beta"));
    assert_eq!(x_line.producers[0].quantity, 30);
    assert_eq!(x_line.total_volume_m3, Some(Decimal::new(600, 1)));
    assert!(alpha.lines.iter().all(|l| l.type_id != 34));
    assert_eq!(line(destination(&plan, "Beta"), 34).acquire_quantity, 300);
}

#[test]
fn nested_producers_each_receive_their_own_inputs() {
    let root = op("root:r", "Root", Some((ALPHA, "Alpha")));
    let x = op("build:x", "X", Some((BETA, "Beta")));
    let y = op("build:y", "Y", Some((BETA, "Beta")));
    let plan = project_logistics(
        &[root.clone(), x.clone(), y.clone()],
        &[
            alloc(
                &root,
                200,
                "X",
                MaterialBoundaryResolution::Build,
                FulfillmentScope::Missing,
                10,
                0,
                Some(&x),
            ),
            alloc(
                &x,
                300,
                "Y",
                MaterialBoundaryResolution::Reaction,
                FulfillmentScope::Missing,
                20,
                0,
                Some(&y),
            ),
            buy(&y, 34, "Tritanium", 1_000, 0),
        ],
        &volumes(),
    );
    let beta = destination(&plan, "Beta");
    assert_eq!(
        line(beta, 300).produced_quantity,
        20,
        "Y moves within Beta (Y -> X)"
    );
    assert_eq!(line(beta, 34).acquire_quantity, 1_000);
    assert_eq!(line(destination(&plan, "Alpha"), 200).produced_quantity, 10);
    assert_eq!(
        beta.operation_ids,
        vec!["build:x".to_string(), "build:y".to_string()]
    );
}

#[test]
fn a_fully_covered_requirement_still_needs_moving_with_zero_shortage() {
    let root = op("root:r", "Root", Some((ALPHA, "Alpha")));
    let plan = project_logistics(
        std::slice::from_ref(&root),
        &[buy(&root, 34, "Tritanium", 500, 500)],
        &volumes(),
    );
    let trit = line(destination(&plan, "Alpha"), 34);
    assert_eq!(trit.shortage_quantity, 0);
    assert_eq!(trit.acquire_quantity, 0);
    assert_eq!(trit.planned_inventory_quantity, 500);
}

#[test]
fn unknown_volume_marks_the_destination_incomplete_and_unassigned_sorts_last() {
    let root = op("root:r", "Root", None);
    let a = op("build:a", "A", Some((ALPHA, "Alpha")));
    let plan = project_logistics(
        &[root.clone(), a.clone()],
        &[
            alloc(
                &root,
                300,
                "Mystery",
                MaterialBoundaryResolution::Unresolved,
                FulfillmentScope::Missing,
                7,
                0,
                None,
            ),
            buy(&a, 34, "Tritanium", 100, 0),
        ],
        &volumes(),
    );
    assert_eq!(plan.destinations.len(), 2);
    assert_eq!(plan.destinations[0].facility_name.as_deref(), Some("Alpha"));
    let unassigned = &plan.destinations[1];
    assert_eq!(unassigned.key, "unassigned");
    let mystery = line(unassigned, 300);
    assert_eq!(mystery.unresolved_quantity, 7);
    assert_eq!(mystery.unit_volume_m3, None);
    assert_eq!(mystery.total_volume_m3, None);
    assert!(!unassigned.volume_complete);
    assert!(!plan.volume_complete);
    assert_eq!(plan.total_volume_m3, Decimal::ONE, "100 x 0.01 known");
}

#[test]
fn ordering_is_deterministic_regardless_of_input_order() {
    let root = op("root:r", "Root", Some((BETA, "Beta")));
    let a = op("build:a", "A", Some((ALPHA, "Alpha")));
    let allocations = vec![
        buy(&root, 35, "Pyerite", 1, 0),
        buy(&root, 34, "Tritanium", 1, 0),
        buy(&a, 34, "Tritanium", 1, 0),
    ];
    let mut reversed = allocations.clone();
    reversed.reverse();
    let forward = project_logistics(&[root.clone(), a.clone()], &allocations, &volumes());
    let backward = project_logistics(&[a, root], &reversed, &volumes());
    assert_eq!(forward, backward);
    assert_eq!(
        forward.destinations[0].facility_name.as_deref(),
        Some("Alpha")
    );
    let beta_names: Vec<&str> = forward.destinations[1]
        .lines
        .iter()
        .map(|l| l.type_name.as_str())
        .collect();
    assert_eq!(beta_names, vec!["Pyerite", "Tritanium"]);
}
