use super::*;
use crate::build_materials::MaterialActivity;
use crate::RecipeCurrency;

fn money(value: i64) -> Money {
    Money(rust_decimal::Decimal::from(value))
}

fn operation(
    key: &str,
    product: (i64, &str),
    activity: MaterialActivity,
    produced: u64,
) -> PlanOperation {
    PlanOperation {
        id: PlanOperationId::new(),
        order_id: OrderId(Uuid::nil()),
        occurrence_key: key.to_string(),
        parent_occurrence_key: None,
        build_id: Some(BuildId::new()),
        activity,
        runs: produced,
        persisted_runs: produced,
        product_type_id: product.0,
        product_name: product.1.to_string(),
        output_per_run: 1,
        produced_quantity: produced,
        blueprint_or_formula_type_id: product.0 + 1,
        material_component_cost: Some(money(800)),
        own_installation_cost: Some(money(200)),
        total_production_cost: Some(money(1_000)),
        complete: true,
        consumed_quantity: None,
        surplus_quantity: Some(3),
        surplus_retained_basis: Some(money(6)),
        evidence: PlanOperationEvidence {
            effective_me: Some(10),
            effective_te: Some(20),
            job_count: 1,
            recipe_currency: RecipeCurrency::Current,
            installation: None,
            warnings: Vec::new(),
        },
        created_at: Utc::now(),
    }
}

#[allow(clippy::too_many_arguments)]
fn requirement(
    type_id: i64,
    name: &str,
    kind: RequirementKind,
    consumer: &str,
    producer: Option<&str>,
    required: u64,
    reused: u64,
    costs: Option<(i64, i64)>,
) -> OrderRequirement {
    OrderRequirement {
        id: OrderRequirementId::new(),
        order_id: OrderId(Uuid::nil()),
        type_id,
        captured_name: name.to_string(),
        kind,
        source_build_id: None,
        required_quantity: required,
        fulfillment_scope: crate::FulfillmentScope::Missing,
        reused_quantity: reused,
        fresh_quantity: required - reused,
        estimated_unit_cost: None,
        estimated_line_total: costs.map(|(total, _)| money(total)),
        reused_line_total: costs.map(|(_, reused)| money(reused)),
        operation_occurrence_key: Some(consumer.to_string()),
        child_occurrence_key: producer.map(str::to_string),
        inventory_unit_basis: None,
        child_produced_quantity: None,
        child_consumed_quantity: None,
        child_surplus_quantity: None,
        child_surplus_retained_basis: None,
        child_consumed_cost: None,
        dependency_id: Some(format!("dep:{consumer}:{type_id}")),
        price_evidence: None,
    }
}

/// Muninn (root) <- Fernite Carbide (reaction) <- Pyerite (buy);
/// Muninn also buys Tritanium.
fn muninn() -> (Vec<PlanOperation>, Vec<OrderRequirement>) {
    let operations = vec![
        operation(
            "root",
            (12_015, "Muninn"),
            MaterialActivity::Manufacturing,
            4,
        ),
        operation(
            "reaction",
            (16_673, "Fernite Carbide"),
            MaterialActivity::Reaction,
            503,
        ),
    ];
    let requirements = vec![
        requirement(
            16_673,
            "Fernite Carbide",
            RequirementKind::Build,
            "root",
            Some("reaction"),
            500,
            100,
            None,
        ),
        requirement(
            34,
            "Tritanium",
            RequirementKind::Buy,
            "root",
            None,
            1_000,
            600,
            Some((1_000, 600)),
        ),
        requirement(
            35,
            "Pyerite",
            RequirementKind::Buy,
            "reaction",
            None,
            200,
            0,
            None,
        ),
        requirement(
            36,
            "Mexallon",
            RequirementKind::Buy,
            "reaction",
            None,
            50,
            50,
            Some((100, 100)),
        ),
    ];
    (operations, requirements)
}

fn project(
    operations: &[PlanOperation],
    requirements: &[OrderRequirement],
) -> ExecutionPlanProjection {
    project_frozen_execution_plan(
        operations,
        requirements,
        &BTreeMap::from([(12_016, "Muninn Blueprint".to_string())]),
        &FrozenPlanInventory {
            free_by_type: BTreeMap::from([(34, 25), (16_673, 7)]),
            reserved_elsewhere_by_type: BTreeMap::from([(34, 300)]),
        },
        Utc::now(),
    )
    .unwrap()
}

#[test]
fn stages_follow_the_frozen_dag_with_the_root_last() {
    let (operations, requirements) = muninn();
    let plan = project(&operations, &requirements);

    assert_eq!(plan.root_node_id, "root");
    let stages: Vec<(u32, Vec<String>)> = plan
        .stages
        .iter()
        .map(|stage| (stage.index, stage.node_ids.clone()))
        .collect();
    assert_eq!(
        stages,
        vec![
            (0, vec!["reaction".to_string()]),
            (1, vec!["root".to_string()])
        ]
    );
    assert_eq!(plan.edges.len(), 1);
    assert_eq!(
        (plan.edges[0].from.as_str(), plan.edges[0].to.as_str()),
        ("reaction", "root")
    );
}

#[test]
fn a_producer_node_carries_its_consumers_frozen_demand() {
    let (operations, requirements) = muninn();
    let plan = project(&operations, &requirements);

    let reaction = plan
        .nodes
        .iter()
        .find(|node| node.id == "reaction")
        .unwrap();
    assert_eq!(reaction.required_quantity, 500);
    assert_eq!(reaction.planned_inventory_quantity, 100);
    assert_eq!(reaction.production_demand, 400);
    assert_eq!(reaction.projected_output, 503);
    assert_eq!(reaction.retained_surplus_quantity, 3);
    assert_eq!(reaction.available_quantity, 7);
    assert_eq!(reaction.consumers.len(), 1);
    assert_eq!(reaction.consumers[0].node_id, "root");
    assert_eq!(reaction.consumers[0].quantity, 400);

    let root = plan.nodes.iter().find(|node| node.id == "root").unwrap();
    assert_eq!(
        root.required_quantity, 0,
        "nothing consumes the final product"
    );
    let root_occurrence = plan
        .occurrences
        .iter()
        .find(|occurrence| occurrence.is_root)
        .unwrap();
    assert_eq!(
        root_occurrence.blueprint_or_formula_name,
        "Muninn Blueprint"
    );
    let fernite = root_occurrence
        .requirements
        .iter()
        .find(|requirement| requirement.type_id == 16_673)
        .unwrap();
    assert_eq!(fernite.resolution, MaterialBoundaryResolution::Build);
    assert_eq!(fernite.producer_node_id.as_deref(), Some("reaction"));
    assert_eq!(fernite.shortage_quantity, 400);
}

#[test]
fn buy_requirements_still_to_source_become_acquisitions() {
    let (operations, requirements) = muninn();
    let plan = project(&operations, &requirements);

    let types: Vec<i64> = plan.acquisitions.iter().map(|line| line.type_id).collect();
    assert_eq!(
        types,
        vec![34, 35],
        "fully reused Mexallon has nothing to source"
    );

    let tritanium = &plan.acquisitions[0];
    assert_eq!(tritanium.required_quantity, 1_000);
    assert_eq!(tritanium.planned_inventory_quantity, 600);
    assert_eq!(tritanium.shortage_quantity, 400);
    assert_eq!(tritanium.available_quantity, 25);
    assert_eq!(tritanium.reserved_quantity, 300);
    assert_eq!(tritanium.fresh_cost, Some(money(400)));
    assert_eq!(tritanium.fresh_unit_price, Some(money(1)));
    assert_eq!(tritanium.consumers[0].node_id, "root");

    let pyerite = &plan.acquisitions[1];
    assert_eq!(pyerite.shortage_quantity, 200);
    assert_eq!(pyerite.fresh_cost, None, "unpriced, never zero");
    assert_eq!(pyerite.consumers[0].node_id, "reaction");
}

fn order() -> Order {
    Order {
        id: OrderId(Uuid::nil()),
        workspace_id: crate::WorkspaceId::new(),
        owner_id: crate::OwnerId::new(),
        source_build_id: None,
        source_build_revision: 3,
        display_name: "Manufacture Muninn".to_string(),
        runs: 4,
        recipe_fingerprint: "fp".to_string(),
        price_snapshot_id: crate::PriceSnapshotId(Uuid::nil()),
        estimated_material_cost: Money::zero(),
        expected_revenue: None,
        estimated_margin: None,
        missing_price_count: 0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        started_at: None,
        completed_at: None,
        canceled_at: None,
        archived_at: None,
        planning_snapshot_version: 3,
    }
}

#[test]
fn overlay_sums_the_epics_holdings_per_acquisition_and_per_producer() {
    let (operations, requirements) = muninn();
    let tritanium = requirements.iter().find(|r| r.type_id == 34).unwrap();
    let fernite = requirements.iter().find(|r| r.type_id == 16_673).unwrap();
    let totals = std::collections::HashMap::from([
        (
            tritanium.id,
            RequirementReservationTotals {
                requirement_id: tritanium.id,
                reserved: 600,
                consumed: 0,
            },
        ),
        (
            fernite.id,
            RequirementReservationTotals {
                requirement_id: fernite.id,
                reserved: 100,
                consumed: 250,
            },
        ),
    ]);

    let overlay = epic_plan_overlay(&order(), &operations, &requirements, &totals, &[]);

    assert_eq!(overlay.source_build_revision, 3);
    assert_eq!(
        overlay.acquisitions[&34],
        EpicStockProgress {
            reserved: 600,
            consumed: 0,
            remaining_need: 400
        }
    );
    assert_eq!(overlay.acquisitions[&35].remaining_need, 200);
    let reaction = &overlay.nodes["reaction"];
    assert_eq!(
        reaction.output,
        EpicStockProgress {
            reserved: 100,
            consumed: 250,
            remaining_need: 150
        }
    );
    assert_eq!(reaction.ticket_id, None);
    assert_eq!(overlay.nodes["root"].output, EpicStockProgress::default());
}
