use std::collections::BTreeMap;

use chrono::Utc;
use rust_decimal::Decimal;
use uuid::Uuid;

use super::*;
use crate::build_materials::{
    AggregateMaterialLine, MaterialActivity, MaterialBoundaryResolution, NodeMaterialAllocation,
    OperationIncomingDemand, VerificationOperationInput,
};
use crate::{
    BoundaryCostKind, BoundaryCostProjection, BuildCostProjection, BuildId, FulfillmentScope,
    MarketPricingPolicy, Money, OperationCostProjection, OperationInstallationCost,
    PricingSelectionKind, RecipeCurrency, RootCostSummary,
};

const ROOT_TYPE: i64 = 100;
const RCF_TYPE: i64 = 200;
const ISOGEN_TYPE: i64 = 300;

fn build_id(value: u128) -> BuildId {
    BuildId(Uuid::from_u128(value))
}

fn operation(
    op_index: u32,
    build_id: BuildId,
    product_type_id: i64,
    name: &str,
) -> VerificationOperationInput {
    VerificationOperationInput {
        op_index,
        parent_op_index: None,
        parent_traversal_index: None,
        incoming: Vec::new(),
        graph_node_id: if op_index == 0 {
            format!("root:{}", build_id.0)
        } else {
            format!("build:{}", build_id.0)
        },
        build_id,
        revision: 1,
        tree_path: Vec::new(),
        activity: MaterialActivity::Manufacturing,
        product_type_id,
        product_name: name.into(),
        output_per_run: 1,
        base_material_count: 1,
        blueprint_or_formula_type_id: product_type_id + 1_000,
        blueprint_or_formula_name: format!("{name} Blueprint"),
        node_runs: 1,
        persisted_runs: 1,
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

#[allow(clippy::too_many_arguments)]
fn allocation(
    consumer: BuildId,
    graph_node_id: &str,
    type_id: i64,
    name: &str,
    required: u64,
    covered: u64,
    resolution: MaterialBoundaryResolution,
    dependency: &str,
    producer: Option<BuildId>,
) -> NodeMaterialAllocation {
    NodeMaterialAllocation {
        build_id: consumer,
        graph_node_id: graph_node_id.into(),
        tree_path: Vec::new(),
        type_id,
        type_name: name.into(),
        required_quantity: required,
        allocated_quantity: covered,
        shortage_quantity: required - covered,
        scope: FulfillmentScope::Missing,
        resolution,
        provisional: false,
        child_runs: 0,
        output_per_run: 0,
        produced_quantity: 0,
        surplus_quantity: 0,
        dependency_id: dependency.into(),
        producer_build_id: producer,
    }
}

fn metadata() -> BTreeMap<i64, WorksheetTypeMetadata> {
    BTreeMap::from([
        (ROOT_TYPE, WorksheetTypeMetadata::new("Ships", "Cruiser")),
        (
            RCF_TYPE,
            WorksheetTypeMetadata::new("Materials", "Construction Components"),
        ),
        (
            ISOGEN_TYPE,
            WorksheetTypeMetadata::new("Materials", "Mineral"),
        ),
    ])
}

fn empty_cost() -> BuildCostProjection {
    BuildCostProjection {
        operations: Vec::new(),
        boundaries: Vec::new(),
        root: RootCostSummary {
            root_op_index: 0,
            planning_total_production_cost: None,
            produced_quantity: 1,
            unit_production_cost: None,
            total_fresh_outlay: Money::zero(),
            total_inventory_basis_consumed: Money::zero(),
            total_own_installation_paid: Money::zero(),
            total_surplus_retained_basis: Money::zero(),
            adjusted_price_observed_at: None,
            complete: false,
        },
        complete: false,
        warnings: Vec::new(),
    }
}

fn installation() -> OperationInstallationCost {
    OperationInstallationCost {
        eiv: None,
        eiv_missing_type_ids: Vec::new(),
        system_cost_index: None,
        job_cost_reduction_percent: Decimal::ZERO,
        facility_tax_percent: Decimal::ZERO,
        scc_surcharge_percent: Decimal::ZERO,
        alliance_surcharge_percent: Decimal::ZERO,
        fixed_supplemental_cost: Money::zero(),
        unmodified_system_index_cost: None,
        system_index_cost: None,
        facility_tax: None,
        scc_surcharge: None,
        alliance_surcharge: None,
        total: None,
        complete: false,
        formula_version: String::new(),
        facility_profile_id: None,
        facility_profile_revision: None,
    }
}

fn operation_cost(
    op: &VerificationOperationInput,
    produced: u64,
    unit: Option<&str>,
    total: Option<&str>,
) -> OperationCostProjection {
    OperationCostProjection {
        op_index: op.op_index,
        parent_op_index: op.parent_op_index,
        graph_node_id: op.graph_node_id.clone(),
        build_id: op.build_id,
        activity: op.activity,
        product_type_id: op.product_type_id,
        node_runs: op.node_runs,
        output_per_run: op.output_per_run,
        produced_quantity: produced,
        direct_inventory_cost: Money::zero(),
        direct_buy_cost: Money::zero(),
        consumed_child_cost: Money::zero(),
        material_component_cost: total.map(|value| Money::parse(value).unwrap()),
        own_installation: installation(),
        total_production_cost: total.map(|value| Money::parse(value).unwrap()),
        unit_production_cost: unit.map(|value| Money::parse(value).unwrap()),
        surplus_retained_basis_created: Money::zero(),
        complete: total.is_some(),
        warnings: Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn boundary_cost(
    traversal_index: u32,
    op_index: u32,
    type_id: i64,
    resolution: MaterialBoundaryResolution,
    required: u64,
    inventory: u64,
    fresh_unit: Option<&str>,
    child_unit: Option<&str>,
    total: Option<&str>,
    surplus: u64,
    surplus_basis: Option<&str>,
) -> BoundaryCostProjection {
    let kind = match resolution {
        MaterialBoundaryResolution::Buy => BoundaryCostKind::Buy,
        MaterialBoundaryResolution::Build => BoundaryCostKind::Build,
        MaterialBoundaryResolution::Reaction => BoundaryCostKind::Reaction,
        MaterialBoundaryResolution::Unresolved => BoundaryCostKind::Unresolved,
    };
    BoundaryCostProjection {
        traversal_index,
        op_index,
        type_id,
        type_name: format!("Type {type_id}"),
        resolution,
        scope: FulfillmentScope::Missing,
        kind,
        required_quantity: required,
        inventory_quantity: inventory,
        inventory_unit_basis: None,
        inventory_cost: (inventory > 0).then(Money::zero),
        fresh_quantity: required - inventory,
        fresh_unit_price: fresh_unit.map(|value| Money::parse(value).unwrap()),
        fresh_price_selection: PricingSelectionKind::Default,
        fresh_pricing_policy: None,
        fresh_price_note: String::new(),
        fresh_price_stale: false,
        market_region_id: None,
        market_location_id: None,
        fresh_cost: None,
        child_op_index: None,
        child_consumed_quantity: required - inventory,
        child_produced_quantity: required - inventory + surplus,
        child_total_production_cost: None,
        child_unit_production_cost: child_unit.map(|value| Money::parse(value).unwrap()),
        child_consumed_cost: total.map(|value| Money::parse(value).unwrap()),
        child_surplus_quantity: surplus,
        child_surplus_retained_basis: surplus_basis.map(|value| Money::parse(value).unwrap()),
        requirement_cost: total.map(|value| Money::parse(value).unwrap()),
        complete: total.is_some(),
        warnings: Vec::new(),
    }
}

fn project_for(
    operations: &[VerificationOperationInput],
    allocations: &[NodeMaterialAllocation],
    focused_producer_id: Option<BuildId>,
) -> Result<BuildWorksheetProjection, BuildWorksheetProjectionError> {
    let cost = empty_cost();
    project_for_cost(operations, allocations, focused_producer_id, &cost)
}

fn project_for_cost(
    operations: &[VerificationOperationInput],
    allocations: &[NodeMaterialAllocation],
    focused_producer_id: Option<BuildId>,
    cost: &BuildCostProjection,
) -> Result<BuildWorksheetProjection, BuildWorksheetProjectionError> {
    project_scoped(operations, allocations, focused_producer_id, cost, true)
}

fn project_scoped(
    operations: &[VerificationOperationInput],
    allocations: &[NodeMaterialAllocation],
    focused_producer_id: Option<BuildId>,
    cost: &BuildCostProjection,
    include_downstream: bool,
) -> Result<BuildWorksheetProjection, BuildWorksheetProjectionError> {
    project_build_worksheet(BuildWorksheetProjectionInput {
        operations,
        boundaries: &[],
        allocations,
        aggregate_rows: &[] as &[AggregateMaterialLine],
        cost,
        focused_producer_id,
        include_downstream,
        metadata: &metadata(),
        generated_at: Utc::now(),
    })
}

fn project(
    operations: &[VerificationOperationInput],
    allocations: &[NodeMaterialAllocation],
) -> BuildWorksheetProjection {
    project_for(operations, allocations, None).unwrap()
}

#[test]
fn root_projects_complete_composition_and_keeps_fully_covered_rows() {
    let root = build_id(1);
    let operations = [operation(0, root, ROOT_TYPE, "Squall")];
    let allocations = [allocation(
        root,
        &operations[0].graph_node_id,
        ISOGEN_TYPE,
        "Isogen",
        8_910,
        8_910,
        MaterialBoundaryResolution::Buy,
        "dep:isogen",
        None,
    )];

    let worksheet = project(&operations, &allocations);
    let row = worksheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .find(|row| row.type_id == ISOGEN_TYPE)
        .unwrap();

    assert_eq!(row.required_quantity, Some(8_910));
    assert_eq!(row.covered_quantity, Some(8_910));
    assert_eq!(row.shortage_quantity, Some(0));
    assert_eq!(row.coverage_percentage.as_deref(), Some("100.00"));
    assert_eq!(worksheet.output.type_id, ROOT_TYPE);
}

#[test]
fn root_aggregates_shared_rcf_once_as_161() {
    let root = build_id(1);
    let auto = build_id(2);
    let life = build_id(3);
    let rcf = build_id(4);
    let operations = [
        operation(0, root, ROOT_TYPE, "Squall"),
        operation(1, rcf, RCF_TYPE, "Reinforced Carbon Fiber"),
    ];
    let allocations = [
        allocation(
            auto,
            &format!("build:{}", auto.0),
            RCF_TYPE,
            "Reinforced Carbon Fiber",
            107,
            0,
            MaterialBoundaryResolution::Reaction,
            "pd:auto-rcf",
            Some(rcf),
        ),
        allocation(
            life,
            &format!("build:{}", life.0),
            RCF_TYPE,
            "Reinforced Carbon Fiber",
            54,
            0,
            MaterialBoundaryResolution::Reaction,
            "pd:life-rcf",
            Some(rcf),
        ),
    ];

    let worksheet = project(&operations, &allocations);
    let rows: Vec<_> = worksheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .filter(|row| row.type_id == RCF_TYPE)
        .collect();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].required_quantity, Some(161));
    assert_eq!(rows[0].sourcing, WorksheetSourcing::Reaction);
    assert_eq!(rows[0].producer_build_id, Some(rcf));
}

#[test]
fn distinct_same_type_sourcing_identities_do_not_collide() {
    let root = build_id(1);
    let consumer = build_id(2);
    let producer = build_id(3);
    let operations = [
        operation(0, root, ROOT_TYPE, "Squall"),
        operation(1, producer, RCF_TYPE, "Reinforced Carbon Fiber"),
    ];
    let allocations = [
        allocation(
            root,
            &operations[0].graph_node_id,
            RCF_TYPE,
            "Reinforced Carbon Fiber",
            10,
            0,
            MaterialBoundaryResolution::Buy,
            "dep:buy-rcf",
            None,
        ),
        allocation(
            consumer,
            &format!("build:{}", consumer.0),
            RCF_TYPE,
            "Reinforced Carbon Fiber",
            20,
            0,
            MaterialBoundaryResolution::Reaction,
            "pd:reaction-rcf",
            Some(producer),
        ),
    ];

    let worksheet = project(&operations, &allocations);
    let rows: Vec<_> = worksheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .filter(|row| row.type_id == RCF_TYPE)
        .collect();

    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0].id, rows[1].id);
    assert_eq!(
        rows.iter()
            .map(|row| row.required_quantity.unwrap())
            .sum::<u64>(),
        30
    );
    assert!(rows
        .iter()
        .any(|row| row.sourcing == WorksheetSourcing::Buy));
    assert!(rows
        .iter()
        .any(|row| row.sourcing == WorksheetSourcing::Reaction));
}

fn shared_fixture() -> (
    Vec<VerificationOperationInput>,
    Vec<NodeMaterialAllocation>,
    BuildId,
    BuildId,
    BuildId,
) {
    let root = build_id(1);
    let auto = build_id(2);
    let life = build_id(3);
    let rcf = build_id(4);
    let mut operations = vec![
        operation(0, root, ROOT_TYPE, "Squall"),
        operation(1, auto, 101, "Auto-Integrity Preservation Seal"),
        operation(2, life, 102, "Life Support Backup Unit"),
        operation(3, rcf, RCF_TYPE, "Reinforced Carbon Fiber"),
    ];
    operations[1].parent_op_index = Some(0);
    operations[2].parent_op_index = Some(0);
    operations[3].parent_op_index = Some(1);
    operations[3].incoming = vec![
        OperationIncomingDemand {
            traversal_index: 0,
            consumer_op_index: 1,
            dependency_id: "pd:auto-rcf".into(),
        },
        OperationIncomingDemand {
            traversal_index: 1,
            consumer_op_index: 2,
            dependency_id: "pd:life-rcf".into(),
        },
    ];
    operations[3].node_runs = 17;
    operations[3].output_per_run = 10;

    let allocations = vec![
        allocation(
            auto,
            &operations[1].graph_node_id,
            RCF_TYPE,
            "Reinforced Carbon Fiber",
            107,
            7,
            MaterialBoundaryResolution::Reaction,
            "pd:auto-rcf",
            Some(rcf),
        ),
        allocation(
            life,
            &operations[2].graph_node_id,
            RCF_TYPE,
            "Reinforced Carbon Fiber",
            54,
            4,
            MaterialBoundaryResolution::Reaction,
            "pd:life-rcf",
            Some(rcf),
        ),
        allocation(
            rcf,
            &operations[3].graph_node_id,
            ISOGEN_TYPE,
            "Isogen",
            322,
            22,
            MaterialBoundaryResolution::Buy,
            "dep:rcf-isogen",
            None,
        ),
    ];
    (operations, allocations, auto, life, rcf)
}

#[test]
fn focus_consumer_keeps_local_rcf_107_and_54() {
    let (operations, allocations, auto, life, _) = shared_fixture();

    let auto_worksheet = project_for(&operations, &allocations, Some(auto)).unwrap();
    let auto_rcf = auto_worksheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .find(|row| row.type_id == RCF_TYPE)
        .unwrap();
    assert_eq!(auto_rcf.required_quantity, Some(107));
    assert_eq!(auto_rcf.covered_quantity, Some(7));
    assert!(auto_worksheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .all(|row| row.type_id != ISOGEN_TYPE));

    let life_worksheet = project_for(&operations, &allocations, Some(life)).unwrap();
    let life_rcf = life_worksheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .find(|row| row.type_id == RCF_TYPE)
        .unwrap();
    assert_eq!(life_rcf.required_quantity, Some(54));
    assert_eq!(life_rcf.covered_quantity, Some(4));
}

#[test]
fn focus_shared_rcf_keeps_real_161_sizing_and_direct_inputs() {
    let (operations, allocations, _, _, rcf) = shared_fixture();

    let worksheet = project_for(&operations, &allocations, Some(rcf)).unwrap();
    assert_eq!(worksheet.output.type_id, RCF_TYPE);
    let isogen = worksheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .find(|row| row.type_id == ISOGEN_TYPE)
        .unwrap();
    assert_eq!(isogen.required_quantity, Some(322));
    assert_eq!(worksheet.scope.focused_producer_id, Some(rcf));
}

#[test]
fn focus_reuses_root_allocation_without_renetting() {
    let (operations, allocations, auto, _, _) = shared_fixture();
    let worksheet = project_for(&operations, &allocations, Some(auto)).unwrap();
    let rcf = worksheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .find(|row| row.type_id == RCF_TYPE)
        .unwrap();

    assert_eq!(rcf.required_quantity, Some(107));
    assert_eq!(rcf.covered_quantity, Some(7));
    assert_eq!(rcf.shortage_quantity, Some(100));
}

#[test]
fn focus_unrelated_producer_is_rejected() {
    let (operations, allocations, _, _, _) = shared_fixture();
    let error = project_for(&operations, &allocations, Some(build_id(999))).unwrap_err();
    assert_eq!(error, BuildWorksheetProjectionError::UnknownFocus);
}

fn shared_cost(operations: &[VerificationOperationInput]) -> BuildCostProjection {
    let mut cost = empty_cost();
    cost.operations = vec![
        operation_cost(&operations[0], 1, Some("1000"), Some("1000")),
        operation_cost(&operations[3], 170, Some("40"), Some("6800")),
    ];
    cost.boundaries = vec![
        boundary_cost(
            0,
            1,
            RCF_TYPE,
            MaterialBoundaryResolution::Reaction,
            107,
            7,
            None,
            Some("40"),
            Some("500"),
            9,
            Some("360"),
        ),
        boundary_cost(
            1,
            2,
            RCF_TYPE,
            MaterialBoundaryResolution::Reaction,
            54,
            4,
            None,
            Some("40"),
            Some("250"),
            0,
            None,
        ),
        boundary_cost(
            2,
            3,
            ISOGEN_TYPE,
            MaterialBoundaryResolution::Buy,
            322,
            22,
            Some("5"),
            None,
            Some("1500"),
            0,
            None,
        ),
    ];
    cost.complete = true;
    cost.root.complete = true;
    cost
}

#[test]
fn produced_total_value_uses_consumed_cost_not_surplus_basis() {
    let (operations, allocations, _, _, _) = shared_fixture();
    let cost = shared_cost(&operations);
    let worksheet = project_for_cost(&operations, &allocations, None, &cost).unwrap();
    let rcf = worksheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .find(|row| row.type_id == RCF_TYPE)
        .unwrap();

    assert_eq!(rcf.total_value, Some(Money::parse("750").unwrap()));
    assert_eq!(rcf.retained_surplus_quantity, Some(9));
    assert_eq!(
        rcf.retained_surplus_basis,
        Some(Money::parse("360").unwrap())
    );
    assert_ne!(rcf.total_value, Some(Money::parse("1110").unwrap()));
}

#[test]
fn focused_total_value_uses_local_consumed_contribution() {
    let (operations, allocations, auto, life, _) = shared_fixture();
    let cost = shared_cost(&operations);

    let auto_sheet = project_for_cost(&operations, &allocations, Some(auto), &cost).unwrap();
    let auto_rcf = auto_sheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .find(|row| row.type_id == RCF_TYPE)
        .unwrap();
    assert_eq!(auto_rcf.total_value, Some(Money::parse("500").unwrap()));

    let life_sheet = project_for_cost(&operations, &allocations, Some(life), &cost).unwrap();
    let life_rcf = life_sheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .find(|row| row.type_id == RCF_TYPE)
        .unwrap();
    assert_eq!(life_rcf.total_value, Some(Money::parse("250").unwrap()));
}

#[test]
fn buy_and_reaction_values_are_present() {
    let (operations, allocations, _, _, rcf) = shared_fixture();
    let cost = shared_cost(&operations);

    let root_sheet = project_for_cost(&operations, &allocations, None, &cost).unwrap();
    let reaction = root_sheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .find(|row| row.type_id == RCF_TYPE)
        .unwrap();
    assert_eq!(reaction.total_value, Some(Money::parse("750").unwrap()));

    let rcf_sheet = project_for_cost(&operations, &allocations, Some(rcf), &cost).unwrap();
    let buy = rcf_sheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .find(|row| row.type_id == ISOGEN_TYPE)
        .unwrap();
    assert_eq!(buy.total_value, Some(Money::parse("1500").unwrap()));
    // BuildCostProjection's authoritative Buy display cost is blended over
    // the whole requirement (inventory basis + fresh acquisition), not the
    // fresh market quote alone: 1,500 / 322, rounded by Money.
    assert_eq!(buy.unit_cost, Some(Money::parse("4.6584").unwrap()));
}

#[test]
fn unit_cost_is_copied_not_recomputed() {
    let (operations, allocations, auto, _, _) = shared_fixture();
    let cost = shared_cost(&operations);
    let worksheet = project_for_cost(&operations, &allocations, Some(auto), &cost).unwrap();
    let rcf = worksheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .find(|row| row.type_id == RCF_TYPE)
        .unwrap();

    assert_eq!(rcf.unit_cost, Some(Money::parse("40").unwrap()));
    assert_eq!(rcf.total_value, Some(Money::parse("500").unwrap()));
}

#[test]
fn output_has_no_coverage_or_shortage() {
    let (operations, allocations, _, _, _) = shared_fixture();
    let cost = shared_cost(&operations);
    let worksheet = project_for_cost(&operations, &allocations, None, &cost).unwrap();

    assert_eq!(worksheet.output.quantity, Some(1));
    assert_eq!(
        worksheet.output.unit_value,
        Some(Money::parse("1000").unwrap())
    );
    assert_eq!(
        worksheet.output.total_value,
        Some(Money::parse("1000").unwrap())
    );
}

#[test]
fn missing_cost_is_incomplete_not_zero() {
    let (operations, allocations, _, _, _) = shared_fixture();
    let worksheet = project(&operations, &allocations);
    let rcf = worksheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .find(|row| row.type_id == RCF_TYPE)
        .unwrap();

    assert_eq!(rcf.total_value, None);
    assert_eq!(rcf.unit_cost, None);
    assert_eq!(rcf.evidence_state, WorksheetEvidenceState::Incomplete);
}

#[test]
fn pricing_classification_is_separate_from_provenance() {
    let root = build_id(1);
    let producer = build_id(2);
    let operations = [
        operation(0, root, ROOT_TYPE, "Squall"),
        operation(1, producer, RCF_TYPE, "Reinforced Carbon Fiber"),
    ];
    let allocations = [
        allocation(
            root,
            &operations[0].graph_node_id,
            RCF_TYPE,
            "Reinforced Carbon Fiber",
            10,
            0,
            MaterialBoundaryResolution::Reaction,
            "dep:rcf",
            Some(producer),
        ),
        allocation(
            producer,
            &operations[1].graph_node_id,
            ISOGEN_TYPE,
            "Isogen",
            20,
            0,
            MaterialBoundaryResolution::Buy,
            "dep:isogen",
            None,
        ),
    ];
    let mut cost = empty_cost();
    let mut produced = boundary_cost(
        0,
        0,
        RCF_TYPE,
        MaterialBoundaryResolution::Reaction,
        10,
        0,
        None,
        Some("4"),
        Some("40"),
        0,
        None,
    );
    produced.fresh_price_note = "market-depth-v1; observed timestamp".into();
    produced.fresh_price_selection = PricingSelectionKind::MarketPolicy;
    produced.fresh_pricing_policy = Some(MarketPricingPolicy::LowestSell);
    let mut buy = boundary_cost(
        1,
        1,
        ISOGEN_TYPE,
        MaterialBoundaryResolution::Buy,
        20,
        0,
        Some("5"),
        None,
        Some("100"),
        0,
        None,
    );
    buy.fresh_price_note = "market-depth-v1; 3 orders".into();
    buy.fresh_price_selection = PricingSelectionKind::MarketPolicy;
    buy.fresh_pricing_policy = Some(MarketPricingPolicy::HighestBuy);
    cost.boundaries = vec![produced, buy];

    let worksheet = project_for_cost(&operations, &allocations, None, &cost).unwrap();
    let rows: Vec<_> = worksheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .collect();
    let produced = rows.iter().find(|row| row.type_id == RCF_TYPE).unwrap();
    assert_eq!(
        produced.pricing.classification,
        WorksheetPricingClassification::Production
    );
    assert_eq!(produced.pricing.policy, None);
    assert_eq!(
        produced.pricing.source_note.as_deref(),
        Some("market-depth-v1; observed timestamp")
    );
    let buy = rows.iter().find(|row| row.type_id == ISOGEN_TYPE).unwrap();
    assert_eq!(
        buy.pricing.classification,
        WorksheetPricingClassification::MarketPolicy
    );
    assert_eq!(buy.pricing.policy, Some(MarketPricingPolicy::HighestBuy));
    assert_eq!(
        buy.pricing.source_note.as_deref(),
        Some("market-depth-v1; 3 orders")
    );
}

#[test]
fn pricing_classifies_default_manual_mixed_and_unresolved_rows() {
    let root = build_id(1);
    let producer = build_id(2);
    let operations = [
        operation(0, root, ROOT_TYPE, "Squall"),
        operation(1, producer, 999, "Second consumer"),
    ];
    let allocations = [
        allocation(
            root,
            &operations[0].graph_node_id,
            301,
            "Default",
            1,
            0,
            MaterialBoundaryResolution::Buy,
            "default",
            None,
        ),
        allocation(
            root,
            &operations[0].graph_node_id,
            302,
            "Manual",
            1,
            0,
            MaterialBoundaryResolution::Buy,
            "manual",
            None,
        ),
        allocation(
            root,
            &operations[0].graph_node_id,
            303,
            "Mixed",
            1,
            0,
            MaterialBoundaryResolution::Buy,
            "mixed-a",
            None,
        ),
        allocation(
            producer,
            &operations[1].graph_node_id,
            303,
            "Mixed",
            1,
            0,
            MaterialBoundaryResolution::Buy,
            "mixed-b",
            None,
        ),
        allocation(
            root,
            &operations[0].graph_node_id,
            304,
            "Unresolved",
            1,
            0,
            MaterialBoundaryResolution::Unresolved,
            "unresolved",
            None,
        ),
    ];
    let mut boundaries = Vec::new();
    for (index, (op_index, type_id, resolution, selection, policy)) in [
        (
            0,
            301,
            MaterialBoundaryResolution::Buy,
            PricingSelectionKind::Default,
            Some(MarketPricingPolicy::HighestBuy),
        ),
        (
            0,
            302,
            MaterialBoundaryResolution::Buy,
            PricingSelectionKind::Manual,
            None,
        ),
        (
            0,
            303,
            MaterialBoundaryResolution::Buy,
            PricingSelectionKind::Default,
            Some(MarketPricingPolicy::HighestBuy),
        ),
        (
            1,
            303,
            MaterialBoundaryResolution::Buy,
            PricingSelectionKind::MarketPolicy,
            Some(MarketPricingPolicy::LowestSell),
        ),
        (
            0,
            304,
            MaterialBoundaryResolution::Unresolved,
            PricingSelectionKind::Default,
            None,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let mut boundary = boundary_cost(
            index as u32,
            op_index,
            type_id,
            resolution,
            1,
            0,
            Some("1"),
            None,
            Some("1"),
            0,
            None,
        );
        boundary.fresh_price_selection = selection;
        boundary.fresh_pricing_policy = policy;
        boundaries.push(boundary);
    }
    let mut cost = empty_cost();
    cost.boundaries = boundaries;
    let worksheet = project_for_cost(&operations, &allocations, None, &cost).unwrap();
    let rows: BTreeMap<_, _> = worksheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .map(|row| (row.type_id, row))
        .collect();

    assert_eq!(
        rows[&301].pricing.classification,
        WorksheetPricingClassification::Default
    );
    assert_eq!(
        rows[&302].pricing.classification,
        WorksheetPricingClassification::Manual
    );
    assert_eq!(
        rows[&303].pricing.classification,
        WorksheetPricingClassification::Mixed
    );
    assert_eq!(rows[&303].pricing.policy, None);
    assert_eq!(
        rows[&304].pricing.classification,
        WorksheetPricingClassification::Unresolved
    );
}

#[test]
fn pruned_fully_covered_producer_remains_visible() {
    let root = build_id(1);
    let producer = build_id(2);
    let operations = [operation(0, root, ROOT_TYPE, "Squall")];
    let allocations = [allocation(
        root,
        &operations[0].graph_node_id,
        RCF_TYPE,
        "Reinforced Carbon Fiber",
        10,
        10,
        MaterialBoundaryResolution::Reaction,
        "pd:covered",
        Some(producer),
    )];
    let mut cost = empty_cost();
    let mut boundary = boundary_cost(
        0,
        0,
        RCF_TYPE,
        MaterialBoundaryResolution::Reaction,
        10,
        10,
        None,
        None,
        Some("100"),
        0,
        None,
    );
    boundary.kind = BoundaryCostKind::FullyCovered;
    cost.boundaries = vec![boundary];
    let worksheet = project_for_cost(&operations, &allocations, None, &cost).unwrap();
    let row = worksheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .find(|row| row.type_id == RCF_TYPE)
        .unwrap();

    assert_eq!(row.required_quantity, Some(10));
    assert_eq!(row.covered_quantity, Some(10));
    assert_eq!(row.shortage_quantity, Some(0));
    assert_eq!(row.total_value, Some(Money::parse("100").unwrap()));
}

#[test]
fn economics_are_explicitly_non_additive() {
    let (operations, allocations, _, _, _) = shared_fixture();
    let worksheet = project(&operations, &allocations);
    assert!(!worksheet.economics_are_additive);
}

fn type_ids(worksheet: &BuildWorksheetProjection) -> Vec<i64> {
    worksheet
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .map(|row| row.type_id)
        .collect()
}

#[test]
fn direct_only_root_excludes_downstream_allocations() {
    let (operations, mut allocations, _, _, _) = shared_fixture();
    let root = &operations[0];
    allocations.push(allocation(
        root.build_id,
        &root.graph_node_id,
        101,
        "Auto-Integrity Preservation Seal",
        3,
        0,
        MaterialBoundaryResolution::Reaction,
        "pd:root-auto",
        Some(operations[1].build_id),
    ));
    let cost = empty_cost();

    let direct = project_scoped(&operations, &allocations, None, &cost, false).unwrap();
    assert_eq!(type_ids(&direct), vec![101]);
    assert!(!direct.scope.include_downstream);

    let full = project_scoped(&operations, &allocations, None, &cost, true).unwrap();
    let mut ids = type_ids(&full);
    ids.sort();
    assert_eq!(ids, vec![101, RCF_TYPE, ISOGEN_TYPE]);
    assert!(full.scope.include_downstream);
}

#[test]
fn direct_only_focus_shows_only_the_focused_operations_inputs() {
    let (operations, allocations, auto, _, rcf) = shared_fixture();
    let cost = empty_cost();

    let focused_auto = project_scoped(&operations, &allocations, Some(auto), &cost, false).unwrap();
    let rcf_row = focused_auto
        .groups
        .iter()
        .flat_map(|group| &group.rows)
        .find(|row| row.type_id == RCF_TYPE)
        .unwrap();
    assert_eq!(type_ids(&focused_auto), vec![RCF_TYPE]);
    assert_eq!(rcf_row.required_quantity, Some(107));

    let focused_rcf = project_scoped(&operations, &allocations, Some(rcf), &cost, false).unwrap();
    assert_eq!(type_ids(&focused_rcf), vec![ISOGEN_TYPE]);
}
