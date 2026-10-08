use chrono::Utc;

use super::*;
use crate::production_dependency::tests::{
    build, derive_persisted, duplicate_fixture, id, ln, mfg, mfg_sel, reactions_draft, res, rxn,
    rxn_sel, Draft, X, X_FORMULA, Y, Y_FORMULA,
};
use crate::production_dependency::{PersistedProductionDependency, RootPlanDependencyGraph};
use crate::Build;

// ---- fixtures --------------------------------------------------------------

fn x_key() -> CanonicalProducerKey {
    CanonicalProducerKey {
        output_type_id: X,
        method: ProductionMethod::Reaction {
            reaction_formula_type_id: X_FORMULA,
        },
    }
}

/// What the repository persists for a committed canonical write (new
/// producers get one Buy edge per recipe material).
fn apply_writes(
    records: &RootPlanRecords,
    consumer: BuildId,
    writes: &[PlannedEdgeWrite],
    created: &[(CanonicalProducerKey, Build)],
) -> RootPlanRecords {
    let mut applied = records.clone();
    for (_, build) in created {
        applied.producers.push(build.clone());
        for (index, line) in build.recipe.materials().iter().enumerate() {
            applied.dependencies.push(PersistedProductionDependency {
                id: Uuid::from_u128(
                    0x9000_0000 + build.id.0.as_u128() % 0x1000 * 0x100 + index as u128,
                ),
                plan_root_build_id: applied.root.id,
                consumer_build_id: build.id,
                component_type_id: line.type_id,
                sourcing: DependencySourcing::Buy,
                producer_build_id: None,
                fulfillment_scope: FulfillmentScope::Missing,
                revision: 1,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            });
        }
    }
    for write in writes {
        let edge = applied
            .dependencies
            .iter_mut()
            .find(|edge| edge.id == write.dependency_id)
            .unwrap();
        assert_eq!(edge.consumer_build_id, consumer);
        edge.sourcing = write.sourcing;
        edge.fulfillment_scope = write.fulfillment_scope;
        edge.producer_build_id = match write.producer {
            PlannedProducer::None => None,
            PlannedProducer::Existing { producer } => Some(producer),
            PlannedProducer::Create { key } => Some(
                created
                    .iter()
                    .find(|(created_key, _)| *created_key == key)
                    .map(|(_, build)| build.id)
                    .expect("a created producer for the key"),
            ),
        };
    }
    applied
}

fn graph_of(records: &RootPlanRecords) -> RootPlanDependencyGraph {
    RootPlanDependencyGraph::from_persisted(records).unwrap()
}

fn intent(component_type_id: i64, produce: Option<ProductionMethod>) -> CanonicalEdgeIntent {
    CanonicalEdgeIntent {
        component_type_id,
        produce,
        fulfillment_scope: FulfillmentScope::Missing,
    }
}

fn x_by_reaction() -> Option<ProductionMethod> {
    Some(ProductionMethod::Reaction {
        reaction_formula_type_id: X_FORMULA,
    })
}

/// `duplicate_fixture` as a canonical plan: consumers A (10) and B (11) both
/// use the X producer 20; the duplicate X producer 21, and the Y producer 31
/// only it reached, are retired (the duplicate plan reconciled to X -> 20).
fn canonical_duplicate_plan() -> RootPlanRecords {
    let (root, builds) = duplicate_fixture(|_, _| {});
    let mut records = derive_persisted(&root, &builds);
    for edge in &mut records.dependencies {
        if edge.consumer_build_id == id(11) && edge.component_type_id == X {
            assert_eq!(edge.producer_build_id, Some(id(21)));
            edge.producer_build_id = Some(id(20));
        }
    }
    records.retired_producers = vec![id(21), id(31)];
    records
}

/// Root -> A -> X -> Y, a tree with no duplicates.
fn clean_fixture() -> (Build, Vec<Build>) {
    let root = build(
        id(1),
        mfg(1_000, ln(500, "Root", 1), vec![ln(501, "Consumer A", 1)]),
        2,
        None,
        Draft {
            resolutions: vec![res(501, mfg_sel(1_501))],
            ..Draft::default()
        },
    );
    (
        root,
        vec![
            build(
                id(10),
                mfg(1_501, ln(501, "Consumer A", 1), vec![ln(X, "X", 10)]),
                1,
                Some((id(1), 501)),
                Draft {
                    resolutions: vec![res(X, rxn_sel(X_FORMULA))],
                    ..Draft::default()
                },
            ),
            build(
                id(20),
                rxn(X_FORMULA, ln(X, "X", 100), vec![ln(Y, "Y", 50)]),
                1,
                Some((id(10), X)),
                reactions_draft(vec![res(Y, rxn_sel(Y_FORMULA))]),
            ),
            build(
                id(30),
                rxn(Y_FORMULA, ln(Y, "Y", 200), vec![ln(901, "Goo", 100)]),
                1,
                Some((id(20), Y)),
                reactions_draft(Vec::new()),
            ),
        ],
    )
}

// ---- authority ---------------------------------------------------------------

// ---- canonical writes -------------------------------------------------------

#[test]
fn produce_reuses_the_canonical_producer_and_buy_changes_only_that_edge() {
    let canonical = canonical_duplicate_plan();
    let graph = graph_of(&canonical);

    // Build -> Buy from consumer A: producer X(20) stays for consumer B.
    let writes =
        plan_canonical_edge_writes(&graph, &canonical, id(10), &[intent(X, None)]).unwrap();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].sourcing, DependencySourcing::Buy);
    assert_eq!(writes[0].producer, PlannedProducer::None);
    assert_eq!(writes[0].previous_producer, Some(id(20)));
    assert!(writes[0].previous_producer_still_referenced);
    let after_a = apply_writes(&canonical, id(10), &writes, &[]);

    // Last consumer -> Buy: the producer is kept, now detached.
    let graph = graph_of(&after_a);
    let writes = plan_canonical_edge_writes(&graph, &after_a, id(11), &[intent(X, None)]).unwrap();
    assert!(!writes[0].previous_producer_still_referenced);
    let both_buy = apply_writes(&after_a, id(11), &writes, &[]);
    let graph = graph_of(&both_buy);
    assert!(graph.detached_producers().contains(&id(20)));
    assert!(graph.producer(id(20)).is_some(), "never deleted");

    // Re-enable Build: the detached canonical producer is reused (the
    // retired duplicate 21 is never a candidate).
    let writes =
        plan_canonical_edge_writes(&graph, &both_buy, id(10), &[intent(X, x_by_reaction())])
            .unwrap();
    assert_eq!(
        writes[0].producer,
        PlannedProducer::Existing { producer: id(20) }
    );
    let a_again = apply_writes(&both_buy, id(10), &writes, &[]);

    // Second consumer Buy -> Build references the SAME producer.
    let graph = graph_of(&a_again);
    let writes =
        plan_canonical_edge_writes(&graph, &a_again, id(11), &[intent(X, x_by_reaction())])
            .unwrap();
    assert_eq!(
        writes[0].producer,
        PlannedProducer::Existing { producer: id(20) }
    );
    let shared = apply_writes(&a_again, id(11), &writes, &[]);
    assert_eq!(graph_of(&shared).incoming(id(20)).len(), 2);
}

#[test]
fn produce_with_no_producer_of_that_identity_creates_exactly_one() {
    let canonical = canonical_duplicate_plan();
    let graph = graph_of(&canonical);
    // X(20)'s Fuel (900) is bought; produce it by a (new) reaction.
    let fuel = ProductionMethod::Reaction {
        reaction_formula_type_id: 9_000,
    };
    let writes =
        plan_canonical_edge_writes(&graph, &canonical, id(20), &[intent(900, Some(fuel))]).unwrap();
    let key = CanonicalProducerKey {
        output_type_id: 900,
        method: fuel,
    };
    assert_eq!(writes[0].producer, PlannedProducer::Create { key });
    assert_eq!(writes[0].previous_producer, None);
}

#[test]
fn a_method_change_never_mutates_the_old_producer() {
    let canonical = canonical_duplicate_plan();
    let graph = graph_of(&canonical);
    let manufacturing = ProductionMethod::Manufacturing {
        blueprint_type_id: 7_777,
    };
    let writes = plan_canonical_edge_writes(
        &graph,
        &canonical,
        id(10),
        &[intent(X, Some(manufacturing))],
    )
    .unwrap();
    assert_eq!(writes.len(), 1, "only this demand edge changes");
    assert_eq!(
        writes[0].producer,
        PlannedProducer::Create {
            key: CanonicalProducerKey {
                output_type_id: X,
                method: manufacturing,
            },
        }
    );
    assert_eq!(writes[0].previous_producer, Some(id(20)));
    assert!(
        writes[0].previous_producer_still_referenced,
        "consumer B still produces X by reaction"
    );
    // Reverting to the reaction reuses the untouched reaction producer.
    let manufacturer = build(
        BuildId(Uuid::from_u128(0x77)),
        mfg(7_777, ln(X, "X", 1), vec![ln(34, "Tritanium", 1)]),
        1,
        None,
        Draft::default(),
    );
    let after = apply_writes(
        &canonical,
        id(10),
        &writes,
        &[(
            CanonicalProducerKey {
                output_type_id: X,
                method: manufacturing,
            },
            manufacturer,
        )],
    );
    let graph = graph_of(&after);
    let back =
        plan_canonical_edge_writes(&graph, &after, id(10), &[intent(X, x_by_reaction())]).unwrap();
    assert_eq!(
        back[0].producer,
        PlannedProducer::Existing { producer: id(20) }
    );
}

#[test]
fn several_detached_candidates_and_no_decision_are_ambiguous_not_guessed() {
    // Unreconciled duplicates (no retirements): make both consumers buy X,
    // leaving two detached reaction producers of X.
    let mut records = derive_persisted(
        &duplicate_fixture(|_, _| {}).0,
        &duplicate_fixture(|_, _| {}).1,
    );
    for consumer in [id(10), id(11)] {
        let edge = records
            .dependencies
            .iter_mut()
            .find(|edge| edge.consumer_build_id == consumer && edge.component_type_id == X)
            .unwrap();
        edge.sourcing = DependencySourcing::Buy;
        edge.producer_build_id = None;
    }
    let graph = graph_of(&records);
    let error = plan_canonical_edge_writes(&graph, &records, id(10), &[intent(X, x_by_reaction())])
        .unwrap_err();
    assert!(matches!(
        error,
        CanonicalWriteError::AmbiguousProducer { candidates, .. } if candidates.len() == 2
    ));
}

// ---- cycles --------------------------------------------------------------------

/// Root -> A (501) -> X -> Y, where Y's recipe also takes "Consumer A" (501)
/// and A's recipe takes its own product: the cycle probes.
fn cycle_fixture() -> RootPlanRecords {
    let (root, mut descendants) = clean_fixture();
    for build in &mut descendants {
        if build.id == id(30) {
            build.recipe = rxn(
                Y_FORMULA,
                ln(Y, "Y", 200),
                vec![ln(901, "Goo", 100), ln(501, "Consumer A", 1)],
            );
        }
        if build.id == id(10) {
            build.recipe = mfg(
                1_501,
                ln(501, "Consumer A", 1),
                vec![ln(X, "X", 10), ln(501, "Consumer A", 1)],
            );
        }
    }
    derive_persisted(&root, &descendants)
}

#[test]
fn a_direct_production_cycle_is_rejected() {
    let records = cycle_fixture();
    let graph = graph_of(&records);
    let error = plan_canonical_edge_writes(
        &graph,
        &records,
        id(10),
        &[intent(501, Some(mfg_sel(1_501).into()))],
    )
    .unwrap_err();
    assert_eq!(
        error,
        CanonicalWriteError::WouldCreateCycle {
            consumer: id(10),
            producer: id(10),
        }
    );
}

#[test]
fn an_indirect_production_cycle_is_rejected() {
    let records = cycle_fixture();
    let graph = graph_of(&records);
    // Y producing "Consumer A" by reusing A would close A -> X -> Y -> A.
    let error = plan_canonical_edge_writes(
        &graph,
        &records,
        id(30),
        &[intent(501, Some(mfg_sel(1_501).into()))],
    )
    .unwrap_err();
    assert_eq!(
        error,
        CanonicalWriteError::WouldCreateCycle {
            consumer: id(30),
            producer: id(10),
        }
    );
}

#[test]
fn a_diamond_and_many_consumers_of_one_producer_are_a_valid_topology() {
    let canonical = canonical_duplicate_plan();
    let graph = graph_of(&canonical);
    let topology = CanonicalPlanTopology::new(&graph, canonical.root.id, None).unwrap();
    // root -> {A, B} -> X -> Y: a diamond through X.
    assert_eq!(topology.incoming_edge_count[&id(20).0], 2);
    assert_eq!(topology.incoming_edge_count[&id(30).0], 1);
    assert_eq!(topology.reachable.len(), 5, "root, A, B, X, Y");
    assert!(graph.validate_acyclic().is_ok());
}

#[test]
fn the_planner_topology_rejects_duplicates_and_retired_references() {
    // Two active producers of X (a plan that was never reconciled).
    let records = derive_persisted(
        &duplicate_fixture(|_, _| {}).0,
        &duplicate_fixture(|_, _| {}).1,
    );
    let graph = graph_of(&records);
    assert!(matches!(
        CanonicalPlanTopology::new(&graph, records.root.id, None),
        Err(CanonicalGraphError::DuplicateActiveProducer { key, .. }) if key == x_key()
    ));

    // A demand edge referencing a retired producer is corruption.
    let mut canonical = canonical_duplicate_plan();
    canonical.retired_producers.push(id(20));
    canonical.retired_producers.sort_by_key(|build| build.0);
    let graph = graph_of(&canonical);
    assert!(matches!(
        CanonicalPlanTopology::new(&graph, canonical.root.id, None),
        Err(CanonicalGraphError::RetiredProducerReferenced { producer, .. }) if producer == id(20)
    ));
}

#[test]
fn a_retired_consumer_cannot_be_edited() {
    let canonical = canonical_duplicate_plan();
    let graph = graph_of(&canonical);
    assert_eq!(
        plan_canonical_edge_writes(&graph, &canonical, id(21), &[intent(X, None)]),
        Err(CanonicalWriteError::RetiredConsumer { build: id(21) })
    );
}
