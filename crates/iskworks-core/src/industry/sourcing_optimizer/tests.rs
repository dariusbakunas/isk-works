//! Pure tests for the sourcing optimizer: candidate construction, the
//! bottom-up decision over a hand-built cost projection, the chosen plan,
//! and the proposal.

use std::collections::BTreeSet;

use rust_decimal::Decimal;

use super::*;
use crate::build_cost::tests::{basis, boundary, money, op, run, with_facility, with_price};
use crate::build_materials::{MaterialBoundaryResolution, VerificationOperationInput};
use crate::production_dependency::tests::{build, derive_persisted, id, ln, mfg, rxn, Draft};
use crate::{BlueprintKind, BlueprintSelection};

const ROOT_OUT: i64 = 500;
const X: i64 = 700;
const X_FORMULA: i64 = 7_000;
const Y: i64 = 800;
const Y_FORMULA: i64 = 8_000;
const Z: i64 = 900;
const W_BLUEPRINT: i64 = 9_100;

fn options() -> SourcingOptions {
    SourcingOptions {
        min_saving_percent: Decimal::ZERO,
        include_without_blueprint: false,
        exclude: BTreeSet::new(),
    }
}

/// Root needs 100 X per run; everything bought.
fn root_build() -> Build {
    build(
        id(1),
        mfg(1_000, ln(ROOT_OUT, "Root", 1), vec![ln(X, "X", 100)]),
        1,
        None,
        Draft::default(),
    )
}

fn x_producer() -> Build {
    build(
        id(20),
        rxn(X_FORMULA, ln(X, "X", 100), vec![ln(Y, "Y", 100)]),
        1,
        None,
        Draft::default(),
    )
}

fn y_producer() -> Build {
    build(
        id(30),
        rxn(Y_FORMULA, ln(Y, "Y", 100), vec![ln(Z, "Z", 100)]),
        1,
        None,
        Draft::default(),
    )
}

fn types(pending: &[PendingComponent]) -> Vec<i64> {
    pending.iter().map(|component| component.type_id).collect()
}

fn edge(
    records: &RootPlanRecords,
    consumer: BuildId,
    type_id: i64,
) -> &PersistedProductionDependency {
    records
        .dependencies
        .iter()
        .find(|edge| edge.consumer_build_id == consumer && edge.component_type_id == type_id)
        .unwrap_or_else(|| panic!("no edge {type_id} on {consumer:?}"))
}

fn resolutions(records: &RootPlanRecords, build_id: BuildId) -> Vec<i64> {
    std::iter::once(&records.root)
        .chain(&records.producers)
        .find(|build| build.id == build_id)
        .and_then(|build| build.draft_planning.as_ref())
        .map(|draft| {
            draft
                .input
                .component_resolutions
                .iter()
                .map(|resolution| resolution.type_id)
                .collect()
        })
        .unwrap_or_default()
}

/// Root -> X (reaction) -> Y (reaction) -> Z (raw), all produced in the
/// candidate; X and Y producers are virtual.
fn chain_candidate(options: &SourcingOptions) -> CandidatePlan {
    let mut candidate = CandidatePlan::new(derive_persisted(&root_build(), &[]), options);
    let pending = candidate.pending().unwrap();
    candidate.attach(&pending[0], CandidateProducer::New(Box::new(x_producer())));
    let pending = candidate.pending().unwrap();
    candidate.attach(&pending[0], CandidateProducer::New(Box::new(y_producer())));
    let pending = candidate.pending().unwrap();
    candidate.attach(&pending[0], CandidateProducer::Skip(None));
    candidate
}

// ---- candidate ---------------------------------------------------------------

#[test]
fn candidate_switches_a_bought_reaction_chain_to_produce() {
    let mut candidate = CandidatePlan::new(derive_persisted(&root_build(), &[]), &options());

    let pending = candidate.pending().unwrap();
    assert_eq!(types(&pending), vec![X]);
    assert_eq!(pending[0].consumer, id(1));
    assert_eq!(pending[0].type_name, "X");
    candidate.attach(&pending[0], CandidateProducer::New(Box::new(x_producer())));

    let pending = candidate.pending().unwrap();
    assert_eq!(types(&pending), vec![Y]);
    assert_eq!(pending[0].consumer, id(20));
    candidate.attach(&pending[0], CandidateProducer::New(Box::new(y_producer())));

    let pending = candidate.pending().unwrap();
    assert_eq!(types(&pending), vec![Z]);
    candidate.attach(&pending[0], CandidateProducer::Skip(None));
    assert!(candidate.pending().unwrap().is_empty());

    let records = &candidate.records;
    assert_eq!(edge(records, id(1), X).producer_build_id, Some(id(20)));
    assert_eq!(edge(records, id(20), Y).producer_build_id, Some(id(30)));
    assert!(matches!(
        edge(records, id(30), Z).sourcing,
        DependencySourcing::Buy
    ));
    assert_eq!(resolutions(records, id(1)), vec![X]);
    assert_eq!(resolutions(records, id(20)), vec![Y]);
    assert_eq!(
        candidate.virtual_producers,
        [id(20), id(30)].into_iter().collect()
    );
    // The persisted plan is untouched.
    assert!(matches!(
        edge(&candidate.current, id(1), X).sourcing,
        DependencySourcing::Buy
    ));
    assert!(candidate.warnings.is_empty());
}

#[test]
fn candidate_reuses_a_detached_producer() {
    // Root buys X, but the plan kept an X producer from an earlier switch.
    let current = derive_persisted(&root_build(), &[x_producer()]);
    let mut candidate = CandidatePlan::new(current, &options());

    assert_eq!(candidate.existing_producer(X).unwrap(), Some(id(20)));
    let pending = candidate.pending().unwrap();
    assert_eq!(
        types(&pending),
        vec![X],
        "a detached producer's edges are not pending"
    );
    candidate.attach(&pending[0], CandidateProducer::Existing(id(20)));

    assert_eq!(
        edge(&candidate.records, id(1), X).producer_build_id,
        Some(id(20))
    );
    assert!(candidate.virtual_producers.is_empty());
    assert_eq!(candidate.records.producers.len(), 1);
    // Now attached, its own bought input is pending.
    assert_eq!(types(&candidate.pending().unwrap()), vec![Y]);
}

#[test]
fn excluded_and_ineligible_types_stay_bought() {
    let mut excluded = options();
    excluded.exclude.insert(X);
    let candidate = CandidatePlan::new(derive_persisted(&root_build(), &[]), &excluded);
    assert!(candidate.pending().unwrap().is_empty());

    let mut candidate = CandidatePlan::new(derive_persisted(&root_build(), &[]), &options());
    let pending = candidate.pending().unwrap();
    candidate.attach(
        &pending[0],
        CandidateProducer::Skip(Some(SourcingWarningKind::NoBlueprint)),
    );
    assert!(candidate.pending().unwrap().is_empty());
    assert!(matches!(
        edge(&candidate.records, id(1), X).sourcing,
        DependencySourcing::Buy
    ));
    assert_eq!(
        candidate.warnings,
        vec![SourcingWarning {
            type_id: X,
            type_name: "X".to_string(),
            kind: SourcingWarningKind::NoBlueprint,
        }]
    );
}

#[test]
fn manufacturing_needs_a_synced_blueprint_unless_lifted() {
    let unowned = build(
        id(40),
        mfg(W_BLUEPRINT, ln(X, "X", 1), vec![ln(Z, "Z", 1)]),
        1,
        None,
        Draft::default(),
    );
    let owned = build(
        id(41),
        mfg(W_BLUEPRINT, ln(X, "X", 1), vec![ln(Z, "Z", 1)]),
        1,
        None,
        Draft {
            blueprint: Some(BlueprintSelection::ObservedAsset {
                observation_id: uuid::Uuid::from_u128(7),
                kind: BlueprintKind::Original,
                material_efficiency: 10,
                time_efficiency: 20,
                licensed_runs: None,
            }),
            ..Draft::default()
        },
    );
    assert!(!producer_eligible(&unowned, &options()));
    assert!(producer_eligible(&owned, &options()));
    assert!(
        producer_eligible(&x_producer(), &options()),
        "reactions always"
    );
    let lifted = SourcingOptions {
        include_without_blueprint: true,
        ..options()
    };
    assert!(producer_eligible(&unowned, &lifted));
}

// ---- decision ----------------------------------------------------------------

fn priced_op(
    op_index: u32,
    parent: Option<u32>,
    build_id: BuildId,
    product: i64,
    output_per_run: u64,
) -> VerificationOperationInput {
    let mut operation = with_facility(
        op(op_index, parent, 1, output_per_run),
        Some("0"),
        "0",
        "0",
        "0",
        "0",
        "0",
    );
    operation.build_id = build_id;
    operation.product_type_id = product;
    operation
}

/// The candidate chain's cost: 100 X <- 100 Y <- 100 Z bought at
/// `z_price`; X and Y have market prices `x_market` / `y_market`.
/// Installation is free, so a produce unit cost is its input unit cost.
fn chain_cost(
    z_price: &str,
    y_market: Option<&str>,
    x_market: Option<&str>,
) -> BuildCostProjection {
    let operations = vec![
        priced_op(0, None, id(1), ROOT_OUT, 1),
        priced_op(1, Some(0), id(20), X, 100),
        priced_op(2, Some(1), id(30), Y, 100),
    ];
    let mut x = boundary(
        0,
        0,
        None,
        X,
        MaterialBoundaryResolution::Reaction,
        FulfillmentScope::Missing,
        100,
        100,
        0,
        100,
    );
    x.type_name = "X".to_string();
    x.market_unit_price = x_market.map(money);
    let mut y = boundary(
        1,
        1,
        Some(0),
        Y,
        MaterialBoundaryResolution::Reaction,
        FulfillmentScope::Missing,
        100,
        100,
        0,
        100,
    );
    y.type_name = "Y".to_string();
    y.market_unit_price = y_market.map(money);
    let z = with_price(
        boundary(
            2,
            2,
            Some(1),
            Z,
            MaterialBoundaryResolution::Buy,
            FulfillmentScope::Missing,
            100,
            100,
            0,
            100,
        ),
        z_price,
    );
    run(
        &operations,
        &[x, y, z],
        &[basis(X, None), basis(Y, None), basis(Z, None)],
        &[(X, "0"), (Y, "0"), (Z, "0")],
    )
}

fn decision(decided: &SourcingDecisions, type_id: i64) -> SourcingDecision {
    decided.decisions[&type_id]
}

#[test]
fn cheap_to_buy_intermediate_is_bought_and_its_inputs_drop_out() {
    // Y costs 1 to react but 3 to buy -> Produce. X costs 1 to react
    // (100 Y at 1) but only 0.5 to buy -> Buy, so Y is no longer needed.
    let candidate = chain_candidate(&options());
    let cost = chain_cost("1", Some("3"), Some("0.5"));
    assert!(cost.complete, "{:?}", cost.operations);
    let decided = decide_sourcing(&candidate, &cost, &options());

    assert_eq!(decision(&decided, Y), SourcingDecision::Produce);
    assert_eq!(decision(&decided, X), SourcingDecision::Buy);
    let x = decided.evaluations.iter().find(|e| e.type_id == X).unwrap();
    assert_eq!(x.depth, 1);
    assert_eq!(x.produce_unit, Some(money("1")));
    assert_eq!(x.buy_unit, Some(money("0.5")));
    assert_eq!(x.current, Some(CurrentSourcing::Buy));

    let chosen = chosen_records(&candidate, &decided.decisions);
    assert!(matches!(
        edge(&chosen, id(1), X).sourcing,
        DependencySourcing::Buy
    ));
    assert!(
        chosen.producers.is_empty(),
        "virtual producers nothing reaches are dropped"
    );
    assert!(resolutions(&chosen, id(1)).is_empty());
}

#[test]
fn produce_cost_uses_the_cheaper_side_of_each_input() {
    // Z is expensive (10): reacting Y would cost 10 but buying it costs 3
    // -> Buy Y. X then costs 3 to react against 5 to buy -> Produce X.
    let candidate = chain_candidate(&options());
    let decided = decide_sourcing(
        &candidate,
        &chain_cost("10", Some("3"), Some("5")),
        &options(),
    );

    assert_eq!(decision(&decided, Y), SourcingDecision::Buy);
    assert_eq!(decision(&decided, X), SourcingDecision::Produce);
    let x = decided.evaluations.iter().find(|e| e.type_id == X).unwrap();
    assert_eq!(x.produce_unit, Some(money("3")));

    let chosen = chosen_records(&candidate, &decided.decisions);
    assert_eq!(edge(&chosen, id(1), X).producer_build_id, Some(id(20)));
    assert!(matches!(
        edge(&chosen, id(20), Y).sourcing,
        DependencySourcing::Buy
    ));
    assert_eq!(
        chosen
            .producers
            .iter()
            .map(|build| build.id)
            .collect::<Vec<_>>(),
        vec![id(20)]
    );
    assert_eq!(resolutions(&chosen, id(1)), vec![X]);
    assert!(resolutions(&chosen, id(20)).is_empty());
}

#[test]
fn a_saving_below_the_threshold_keeps_the_current_sourcing() {
    // X: react at 1 vs buy at 1.02 -- a 2 % saving.
    let candidate = chain_candidate(&options());
    let cost = chain_cost("1", Some("3"), Some("1.02"));

    let strict = SourcingOptions {
        min_saving_percent: Decimal::from(5),
        ..options()
    };
    assert_eq!(
        decision(&decide_sourcing(&candidate, &cost, &strict), X),
        SourcingDecision::Buy
    );
    assert_eq!(
        decision(&decide_sourcing(&candidate, &cost, &options()), X),
        SourcingDecision::Produce
    );
}

#[test]
fn no_buy_price_keeps_the_current_sourcing_with_a_warning() {
    let candidate = chain_candidate(&options());
    let decided = decide_sourcing(&candidate, &chain_cost("1", None, Some("5")), &options());

    assert_eq!(decision(&decided, Y), SourcingDecision::Keep);
    // Y is kept as it is now (bought under a new producer), so X's produce
    // side has an unpriced input and X is kept too.
    assert_eq!(decision(&decided, X), SourcingDecision::Keep);
    assert_eq!(
        decided.warnings,
        vec![
            SourcingWarning {
                type_id: Y,
                type_name: "Y".to_string(),
                kind: SourcingWarningKind::NoBuyPrice,
            },
            SourcingWarning {
                type_id: X,
                type_name: "X".to_string(),
                kind: SourcingWarningKind::ProduceCostIncomplete,
            },
        ]
    );
    let chosen = chosen_records(&candidate, &decided.decisions);
    assert_eq!(chosen.dependencies, candidate.current.dependencies);
    assert!(chosen.producers.is_empty());
}

#[test]
fn excluded_type_is_kept_and_reoptimized_around() {
    let mut excluded = options();
    excluded.exclude.insert(Y);
    let candidate = chain_candidate(&options());
    let decided = decide_sourcing(
        &candidate,
        &chain_cost("1", Some("3"), Some("5")),
        &excluded,
    );
    assert_eq!(decision(&decided, Y), SourcingDecision::Keep);
    // Y is bought "now" (no current consumer), so X is costed at Y's buy
    // price of 3 -- still cheaper than buying X at 5.
    assert_eq!(decision(&decided, X), SourcingDecision::Produce);
}

// ---- proposal ----------------------------------------------------------------

#[test]
fn proposal_lists_switches_with_savings_and_margins() {
    let candidate = chain_candidate(&options());
    let decided = decide_sourcing(
        &candidate,
        &chain_cost("1", Some("3"), Some("5")),
        &options(),
    );
    let chosen = chosen_records(&candidate, &decided.decisions);

    let proposal = build_proposal(
        &candidate,
        &decided,
        &chosen,
        Some(money("500")),
        Some(money("100")),
        Some(money("1000")),
    );
    assert_eq!(proposal.current_margin, Some(money("500")));
    assert_eq!(proposal.proposed_margin, Some(money("900")));
    assert_eq!(proposal.delta, Some(money("400")));
    assert_eq!(proposal.fingerprint, plan_fingerprint(&candidate.current));
    let summary: Vec<(i64, Option<CurrentSourcing>, ProposedSourcing, u32)> = proposal
        .changes
        .iter()
        .map(|change| (change.type_id, change.from, change.to.clone(), change.depth))
        .collect();
    assert_eq!(
        summary,
        vec![
            (
                X,
                Some(CurrentSourcing::Buy),
                ProposedSourcing::Produce {
                    recipe: RecipeSelection::Reaction {
                        reaction_formula_type_id: X_FORMULA
                    }
                },
                1
            ),
            (
                Y,
                None,
                ProposedSourcing::Produce {
                    recipe: RecipeSelection::Reaction {
                        reaction_formula_type_id: Y_FORMULA
                    }
                },
                2
            ),
        ]
    );
    assert_eq!(proposal.changes[0].saving, Some(money("400")));
}

#[test]
fn proposal_is_empty_when_the_chosen_plan_is_not_cheaper() {
    let candidate = chain_candidate(&options());
    let decided = decide_sourcing(
        &candidate,
        &chain_cost("1", Some("3"), Some("5")),
        &options(),
    );
    let chosen = chosen_records(&candidate, &decided.decisions);
    let proposal = build_proposal(
        &candidate,
        &decided,
        &chosen,
        Some(money("100")),
        Some(money("100")),
        Some(money("1000")),
    );
    assert!(proposal.changes.is_empty());
    assert_eq!(proposal.delta, Some(Money::zero()));
    assert_eq!(proposal.proposed_margin, proposal.current_margin);
}

#[test]
fn fingerprint_changes_with_any_build_revision() {
    let records = derive_persisted(&root_build(), &[x_producer()]);
    let before = plan_fingerprint(&records);
    let mut changed = records.clone();
    changed.producers[0].revision += 1;
    assert_ne!(before, plan_fingerprint(&changed));
    assert_eq!(before, plan_fingerprint(&records));
}
