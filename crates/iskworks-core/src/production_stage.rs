//! The one authoritative production-stage
//! algorithm.
//!
//! A stage is **structural production order only** -- never readiness.
//! Over a production-operation DAG whose edges run producer -> consumer:
//!
//! ```text
//! stage(op) = 0                                  op consumes no produced operation
//! stage(op) = 1 + max(stage(p) for p producing op)   otherwise
//! ```
//!
//! i.e. the longest producer chain beneath an operation. For every edge
//! `P -> C` this guarantees `stage(P) < stage(C)`, whatever depths `P`'s
//! other consumers sit at (fan-out never duplicates `P`).
//!
//! Computed over *operations* (Stages rows / frozen Epic operations), never
//! over tree occurrences: a pooled sibling
//! occurrence owns no walked children of its own, so an occurrence-level
//! stage would place its consumer in the same stage as the pooled producer
//! (e.g. Reinforced Carbon Fiber sharing a stage with Life Support Backup
//! Unit). Used by `crate::execution_plan::project_execution_plan`
//! (live Stages/Plan) and `crate::order::derive_operation_dag` (frozen
//! Epics).

use std::collections::{BTreeMap, BTreeSet};

/// The operation relation has a cycle -- no production order exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StageCycle;

/// Longest-producer-chain stage of every node; see the module doc.
/// Deterministic (Kahn's algorithm in ascending node order), `O(V + E)`
/// up to the ordered-set overhead. Edges naming a node outside `nodes` are
/// ignored; a self edge is a cycle.
///
/// # Errors
///
/// [`StageCycle`] when the edges contain a cycle.
pub fn production_stages<K: Ord + Clone>(
    nodes: impl IntoIterator<Item = K>,
    producer_consumer_edges: impl IntoIterator<Item = (K, K)>,
) -> Result<BTreeMap<K, u32>, StageCycle> {
    let nodes: BTreeSet<K> = nodes.into_iter().collect();
    let mut producers_of: BTreeMap<K, BTreeSet<K>> = BTreeMap::new();
    let mut consumers_of: BTreeMap<K, BTreeSet<K>> = BTreeMap::new();
    for (producer, consumer) in producer_consumer_edges {
        if !nodes.contains(&producer) || !nodes.contains(&consumer) {
            continue;
        }
        if producer == consumer {
            return Err(StageCycle);
        }
        producers_of
            .entry(consumer.clone())
            .or_default()
            .insert(producer.clone());
        consumers_of.entry(producer).or_default().insert(consumer);
    }

    let mut pending: BTreeMap<K, usize> = nodes
        .iter()
        .map(|node| {
            (
                node.clone(),
                producers_of.get(node).map_or(0, BTreeSet::len),
            )
        })
        .collect();
    let mut ready: BTreeSet<K> = pending
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(node, _)| node.clone())
        .collect();
    let mut stages: BTreeMap<K, u32> = BTreeMap::new();
    while let Some(node) = ready.pop_first() {
        let stage = producers_of
            .get(&node)
            .into_iter()
            .flatten()
            .filter_map(|producer| stages.get(producer))
            .map(|stage| stage + 1)
            .max()
            .unwrap_or(0);
        for consumer in consumers_of.get(&node).into_iter().flatten() {
            if let Some(count) = pending.get_mut(consumer) {
                *count -= 1;
                if *count == 0 {
                    ready.insert(consumer.clone());
                }
            }
        }
        stages.insert(node, stage);
    }
    if stages.len() == nodes.len() {
        Ok(stages)
    } else {
        Err(StageCycle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stages(edges: &[(&'static str, &'static str)]) -> BTreeMap<&'static str, u32> {
        let nodes: BTreeSet<&str> = edges.iter().flat_map(|(p, c)| [*p, *c]).collect();
        let result = production_stages(nodes, edges.iter().copied()).unwrap();
        for (producer, consumer) in edges {
            assert!(
                result[producer] < result[consumer],
                "stage({producer}) < stage({consumer}) for edge {producer} -> {consumer}: {result:?}"
            );
        }
        result
    }

    #[test]
    fn a_simple_chain_orders_every_producer_first() {
        let s = stages(&[("Y", "X"), ("X", "A"), ("A", "Root")]);
        assert_eq!((s["Y"], s["X"], s["A"], s["Root"]), (0, 1, 2, 3));
    }

    #[test]
    fn fan_out_places_the_producer_before_every_consumer_at_any_depth() {
        // X -> A -> Root and X -> B -> C -> Root: X once, before A AND B.
        let s = stages(&[
            ("X", "A"),
            ("A", "Root"),
            ("X", "B"),
            ("B", "C"),
            ("C", "Root"),
        ]);
        assert_eq!(s["X"], 0);
        assert_eq!((s["A"], s["B"], s["C"], s["Root"]), (1, 1, 2, 3));
    }

    #[test]
    fn a_diamond_takes_the_longest_producer_chain() {
        let s = stages(&[("D", "B"), ("D", "C"), ("B", "A"), ("C", "A"), ("E", "C")]);
        assert_eq!((s["D"], s["E"], s["B"], s["C"], s["A"]), (0, 0, 1, 1, 2));
    }

    #[test]
    fn nested_shared_producers_stay_strictly_ordered() {
        // Y shared by X and Root; X shared by A and B.
        let s = stages(&[
            ("Y", "X"),
            ("Y", "Root"),
            ("X", "A"),
            ("X", "B"),
            ("A", "Root"),
            ("B", "Root"),
        ]);
        assert_eq!((s["Y"], s["X"], s["A"], s["B"], s["Root"]), (0, 1, 2, 2, 3));
    }

    #[test]
    fn squall_reinforced_carbon_fiber_precedes_both_of_its_consumers() {
        let s = stages(&[
            ("Carbon Fiber", "Reinforced Carbon Fiber"),
            ("Oxy-Organic Solvents", "Reinforced Carbon Fiber"),
            ("Thermosetting Polymer", "Reinforced Carbon Fiber"),
            ("Reinforced Carbon Fiber", "Life Support Backup Unit"),
            (
                "Reinforced Carbon Fiber",
                "Auto-Integrity Preservation Seal",
            ),
            ("Life Support Backup Unit", "Squall"),
            ("Auto-Integrity Preservation Seal", "Squall"),
        ]);
        assert_eq!(s["Reinforced Carbon Fiber"], 1);
        assert_eq!(s["Life Support Backup Unit"], 2);
        assert_eq!(s["Auto-Integrity Preservation Seal"], 2);
        assert_eq!(s["Squall"], 3);
    }

    #[test]
    fn isolated_nodes_are_stage_zero_and_unknown_edges_are_ignored() {
        let s = production_stages(["A", "B"], [("A", "ghost")]).unwrap();
        assert_eq!((s["A"], s["B"]), (0, 0));
    }

    #[test]
    fn cycles_and_self_edges_are_refused() {
        assert_eq!(
            production_stages(["A", "B"], [("A", "B"), ("B", "A")]),
            Err(StageCycle)
        );
        assert_eq!(production_stages(["A"], [("A", "A")]), Err(StageCycle));
    }
}
