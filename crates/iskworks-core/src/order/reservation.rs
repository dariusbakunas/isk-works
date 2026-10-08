//! Epic inventory reservations: which frozen requirements get which stock,
//! and in what order. Pure; storage applies the result under the balance
//! lock.
//!
//! **Order.** Requirements are served earliest consumer first: ascending
//! consumer stage (`OperationDag::stages` of the requirement's
//! `operation_occurrence_key`: `0` is a leaf operation, the root is last),
//! then `operation_occurrence_key`, then requirement id. The same order is
//! used when reserving existing stock at creation, topping up, and
//! distributing recorded output, so the outcome never depends on input
//! order.

use std::collections::BTreeMap;

use super::*;

/// One allocation to insert, owned by a requirement.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PlannedReservation {
    pub requirement_id: OrderRequirementId,
    pub type_id: i64,
    pub quantity: u64,
}

/// A type the Epic means to reuse more of than is free.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReservationShortfall {
    pub type_id: i64,
    /// Σ frozen `reused_quantity` of this type.
    pub wanted: u64,
    /// Free stock (`physical - active reservations`) under the lock.
    pub free: u64,
}

/// One type whose frozen reuse differs from what the client was shown.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReuseChange {
    pub type_id: i64,
    pub expected: u64,
    pub now: u64,
}

/// `frozen` reuse compared with the reuse the client previewed.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct ReuseComparison {
    /// Less reuse than previewed: more to buy or build than the user agreed
    /// to, so creation is refused.
    pub decreased: Vec<ReuseChange>,
    /// More reuse than previewed (stock arrived): strictly less work, so
    /// creation proceeds and the caller reports it.
    pub increased: Vec<ReuseChange>,
}

/// Σ `reused_quantity` per type; types with no reuse are absent.
#[must_use]
pub fn reuse_by_type(requirements: &[NewOrderRequirement]) -> BTreeMap<i64, u64> {
    let mut reuse: BTreeMap<i64, u64> = BTreeMap::new();
    for requirement in requirements {
        if requirement.reused_quantity > 0 {
            let entry = reuse.entry(requirement.type_id).or_insert(0);
            *entry = entry.saturating_add(requirement.reused_quantity);
        }
    }
    reuse
}

/// Compare the freeze's per-type reuse with what the client previewed. A
/// type missing from either side counts as `0` there.
#[must_use]
pub fn compare_reuse(
    expected: &BTreeMap<i64, u64>,
    frozen: &BTreeMap<i64, u64>,
) -> ReuseComparison {
    let mut comparison = ReuseComparison::default();
    let type_ids: std::collections::BTreeSet<i64> =
        expected.keys().chain(frozen.keys()).copied().collect();
    for type_id in type_ids {
        let expected = expected.get(&type_id).copied().unwrap_or(0);
        let now = frozen.get(&type_id).copied().unwrap_or(0);
        let change = ReuseChange {
            type_id,
            expected,
            now,
        };
        match now.cmp(&expected) {
            std::cmp::Ordering::Less => comparison.decreased.push(change),
            std::cmp::Ordering::Greater => comparison.increased.push(change),
            std::cmp::Ordering::Equal => {}
        }
    }
    comparison
}

/// The deterministic serving order key for a requirement (see the module
/// doc). A requirement with no operation (pre-v3) sorts last.
fn serving_key<'a>(
    operation_occurrence_key: Option<&'a str>,
    requirement_id: OrderRequirementId,
    stages: &BTreeMap<String, u32>,
) -> (u32, &'a str, Uuid) {
    let stage = operation_occurrence_key
        .and_then(|key| stages.get(key).copied())
        .unwrap_or(u32::MAX);
    (
        stage,
        operation_occurrence_key.unwrap_or(""),
        requirement_id.0,
    )
}

/// Reserve exactly each requirement's frozen `reused_quantity` from free
/// stock, in serving order. All or nothing: if any type's total reuse
/// exceeds its free stock, every such type is returned as a shortfall and
/// nothing is reserved.
///
/// # Errors
///
/// The per-type shortfalls, ascending by `type_id`.
pub fn plan_epic_reservations(
    requirements: &[NewOrderRequirement],
    stages: &BTreeMap<String, u32>,
    free_by_type: &BTreeMap<i64, u64>,
) -> Result<Vec<PlannedReservation>, Vec<ReservationShortfall>> {
    let shortfalls: Vec<ReservationShortfall> = reuse_by_type(requirements)
        .into_iter()
        .filter_map(|(type_id, wanted)| {
            let free = free_by_type.get(&type_id).copied().unwrap_or(0);
            (wanted > free).then_some(ReservationShortfall {
                type_id,
                wanted,
                free,
            })
        })
        .collect();
    if !shortfalls.is_empty() {
        return Err(shortfalls);
    }

    let mut ordered: Vec<&NewOrderRequirement> = requirements
        .iter()
        .filter(|requirement| requirement.reused_quantity > 0)
        .collect();
    ordered.sort_by_key(|requirement| {
        serving_key(
            requirement.operation_occurrence_key.as_deref(),
            requirement.id,
            stages,
        )
    });
    Ok(ordered
        .into_iter()
        .map(|requirement| PlannedReservation {
            requirement_id: requirement.id,
            type_id: requirement.type_id,
            quantity: requirement.reused_quantity,
        })
        .collect())
}

/// How much more one requirement of an existing Epic wants reserved.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ReservationNeed<'a> {
    pub requirement_id: OrderRequirementId,
    pub type_id: i64,
    pub operation_occurrence_key: Option<&'a str>,
    pub quantity: u64,
}

/// A top-up's result: what was reserved, and what free stock couldn't
/// cover.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct CappedReservationPlan {
    pub reservations: Vec<PlannedReservation>,
    /// Per type: `wanted` = Σ needs, `free` = free stock before reserving.
    pub shortfalls: Vec<ReservationShortfall>,
}

/// Reserve what free stock allows, in serving order: each need gets
/// `min(need, free left of its type)`. Partial by design -- an explicit
/// top-up takes what it can and reports the rest.
#[must_use]
pub fn plan_capped_reservations(
    needs: &[ReservationNeed<'_>],
    stages: &BTreeMap<String, u32>,
    free_by_type: &BTreeMap<i64, u64>,
) -> CappedReservationPlan {
    let mut ordered: Vec<&ReservationNeed<'_>> =
        needs.iter().filter(|need| need.quantity > 0).collect();
    ordered.sort_by_key(|need| {
        serving_key(need.operation_occurrence_key, need.requirement_id, stages)
    });

    let mut left = free_by_type.clone();
    let mut wanted: BTreeMap<i64, u64> = BTreeMap::new();
    let mut reservations = Vec::new();
    for need in ordered {
        *wanted.entry(need.type_id).or_insert(0) += need.quantity;
        let free = left.entry(need.type_id).or_insert(0);
        let quantity = need.quantity.min(*free);
        *free -= quantity;
        if quantity > 0 {
            reservations.push(PlannedReservation {
                requirement_id: need.requirement_id,
                type_id: need.type_id,
                quantity,
            });
        }
    }
    let shortfalls = wanted
        .into_iter()
        .filter_map(|(type_id, wanted)| {
            let free = free_by_type.get(&type_id).copied().unwrap_or(0);
            (wanted > free).then_some(ReservationShortfall {
                type_id,
                wanted,
                free,
            })
        })
        .collect();
    CappedReservationPlan {
        reservations,
        shortfalls,
    }
}

/// Σ allocations owned by one requirement, by lifecycle (released rows
/// don't count).
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct RequirementReservationTotals {
    pub requirement_id: OrderRequirementId,
    /// Active: held for this requirement, not yet used.
    pub reserved: u64,
    /// Used by a recording.
    pub consumed: u64,
}

/// One requirement's live coverage in an Epic: what it holds, what it has
/// used, what it still needs, and how much of that free stock could cover
/// right now.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EpicCoverageLine {
    pub requirement_id: OrderRequirementId,
    pub reserved: u64,
    pub consumed: u64,
    /// `required - reserved - consumed`, floored at `0`.
    pub remaining_need: u64,
    /// Free stock of the type (`physical - every active reservation`).
    /// Shared by every line of the same type.
    pub free_available: u64,
    /// `min(remaining_need, free_available)`.
    pub free_coverable: u64,
}

#[must_use]
pub fn epic_coverage_line(
    requirement_id: OrderRequirementId,
    required: u64,
    totals: Option<RequirementReservationTotals>,
    free_available: u64,
) -> EpicCoverageLine {
    let (reserved, consumed) = totals.map_or((0, 0), |totals| (totals.reserved, totals.consumed));
    let remaining_need = required.saturating_sub(reserved.saturating_add(consumed));
    EpicCoverageLine {
        requirement_id,
        reserved,
        consumed,
        remaining_need,
        free_available,
        free_coverable: remaining_need.min(free_available),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requirement(type_id: i64, reused: u64, operation: &str) -> NewOrderRequirement {
        NewOrderRequirement {
            id: OrderRequirementId::new(),
            type_id,
            captured_name: format!("Type {type_id}"),
            kind: RequirementKind::Buy,
            source_build_id: None,
            required_quantity: reused.max(1),
            fulfillment_scope: FulfillmentScope::Missing,
            reused_quantity: reused,
            estimated_unit_cost: None,
            estimated_line_total: None,
            reused_line_total: None,
            operation_occurrence_key: Some(operation.to_string()),
            child_occurrence_key: None,
            inventory_unit_basis: None,
            child_produced_quantity: None,
            child_consumed_quantity: None,
            child_surplus_quantity: None,
            child_surplus_retained_basis: None,
            child_consumed_cost: None,
            dependency_id: None,
            price_evidence: None,
        }
    }

    fn stages(pairs: &[(&str, u32)]) -> BTreeMap<String, u32> {
        pairs
            .iter()
            .map(|(key, stage)| ((*key).to_string(), *stage))
            .collect()
    }

    #[test]
    fn reserves_exactly_the_frozen_reuse_when_it_fits() {
        let requirements = vec![
            requirement(34, 600, "root"),
            requirement(35, 0, "root"),
            requirement(36, 5, "root"),
        ];
        let planned = plan_epic_reservations(
            &requirements,
            &stages(&[("root", 0)]),
            &BTreeMap::from([(34, 600), (36, 10)]),
        )
        .unwrap();
        assert_eq!(planned.len(), 2, "zero-reuse rows reserve nothing");
        let quantities: BTreeMap<i64, u64> =
            planned.iter().map(|r| (r.type_id, r.quantity)).collect();
        assert_eq!(quantities, BTreeMap::from([(34, 600), (36, 5)]));
    }

    #[test]
    fn reports_every_short_type_and_reserves_nothing() {
        let requirements = vec![
            requirement(34, 600, "root"),
            requirement(35, 20, "root"),
            requirement(36, 5, "root"),
        ];
        let shortfalls = plan_epic_reservations(
            &requirements,
            &stages(&[("root", 0)]),
            &BTreeMap::from([(34, 400), (36, 5)]),
        )
        .unwrap_err();
        assert_eq!(
            shortfalls,
            vec![
                ReservationShortfall {
                    type_id: 34,
                    wanted: 600,
                    free: 400
                },
                ReservationShortfall {
                    type_id: 35,
                    wanted: 20,
                    free: 0
                },
            ]
        );
    }

    #[test]
    fn sums_one_type_across_requirements_before_checking_free_stock() {
        // Root and a child both reuse Tritanium: 300 + 300 against 500 free.
        let requirements = vec![requirement(34, 300, "root"), requirement(34, 300, "child")];
        let shortfalls = plan_epic_reservations(
            &requirements,
            &stages(&[("root", 1), ("child", 0)]),
            &BTreeMap::from([(34, 500)]),
        )
        .unwrap_err();
        assert_eq!(shortfalls[0].wanted, 600);
    }

    #[test]
    fn serves_earliest_consumer_first_then_key_then_id() {
        let root = requirement(34, 1, "root");
        let reaction_b = requirement(34, 1, "op:b");
        let reaction_a = requirement(34, 1, "op:a");
        let mut tie_low = requirement(34, 1, "op:a");
        let mut tie_high = requirement(34, 1, "op:a");
        let (low, high) = if tie_low.id.0 < tie_high.id.0 {
            (tie_low.id, tie_high.id)
        } else {
            (tie_high.id, tie_low.id)
        };
        tie_low.id = low;
        tie_high.id = high;
        let stage_map = stages(&[("root", 2), ("op:a", 0), ("op:b", 0)]);
        let free = BTreeMap::from([(34, 100)]);

        // Same answer whatever the input order.
        let forward = vec![
            root.clone(),
            reaction_b.clone(),
            tie_high.clone(),
            reaction_a.clone(),
            tie_low.clone(),
        ];
        let mut backward = forward.clone();
        backward.reverse();
        for input in [forward, backward] {
            let order: Vec<OrderRequirementId> = plan_epic_reservations(&input, &stage_map, &free)
                .unwrap()
                .into_iter()
                .map(|r| r.requirement_id)
                .collect();
            let mut op_a = vec![reaction_a.id, tie_low.id, tie_high.id];
            op_a.sort_by_key(|id| id.0);
            let mut expected = op_a;
            expected.push(reaction_b.id);
            expected.push(root.id);
            assert_eq!(order, expected);
        }
    }

    #[test]
    fn compare_reuse_splits_decreases_from_increases() {
        let expected = BTreeMap::from([(34, 600), (35, 20), (36, 5)]);
        let frozen = BTreeMap::from([(34, 400), (36, 9), (37, 1)]);
        let comparison = compare_reuse(&expected, &frozen);
        assert_eq!(
            comparison.decreased,
            vec![
                ReuseChange {
                    type_id: 34,
                    expected: 600,
                    now: 400
                },
                ReuseChange {
                    type_id: 35,
                    expected: 20,
                    now: 0
                },
            ]
        );
        assert_eq!(
            comparison.increased,
            vec![
                ReuseChange {
                    type_id: 36,
                    expected: 5,
                    now: 9
                },
                ReuseChange {
                    type_id: 37,
                    expected: 0,
                    now: 1
                },
            ]
        );
    }

    #[test]
    fn coverage_line_counts_reserved_and_consumed_against_the_need() {
        let id = OrderRequirementId::new();
        let line = epic_coverage_line(
            id,
            1_000,
            Some(RequirementReservationTotals {
                requirement_id: id,
                reserved: 300,
                consumed: 200,
            }),
            150,
        );
        assert_eq!(line.remaining_need, 500);
        assert_eq!(line.free_available, 150);
        assert_eq!(line.free_coverable, 150);

        let covered = epic_coverage_line(id, 100, None, 1_000);
        assert_eq!((covered.reserved, covered.consumed), (0, 0));
        assert_eq!(covered.remaining_need, 100);
        assert_eq!(covered.free_coverable, 100);

        // Over-held (e.g. recorded output beyond need) never underflows.
        let over = epic_coverage_line(
            id,
            100,
            Some(RequirementReservationTotals {
                requirement_id: id,
                reserved: 80,
                consumed: 50,
            }),
            10,
        );
        assert_eq!(over.remaining_need, 0);
        assert_eq!(over.free_coverable, 0);
    }

    #[test]
    fn capped_reservations_serve_the_earliest_consumer_and_report_the_rest() {
        let reaction = OrderRequirementId::new();
        let root = OrderRequirementId::new();
        let pyerite = OrderRequirementId::new();
        let needs = [
            ReservationNeed {
                requirement_id: root,
                type_id: 34,
                operation_occurrence_key: Some("root"),
                quantity: 600,
            },
            ReservationNeed {
                requirement_id: reaction,
                type_id: 34,
                operation_occurrence_key: Some("reaction"),
                quantity: 300,
            },
            ReservationNeed {
                requirement_id: pyerite,
                type_id: 35,
                operation_occurrence_key: Some("root"),
                quantity: 50,
            },
        ];
        let plan = plan_capped_reservations(
            &needs,
            &stages(&[("root", 1), ("reaction", 0)]),
            &BTreeMap::from([(34, 500), (35, 80)]),
        );

        // The reaction (stage 0) is served before the root (stage 1); the
        // root's own two rows tie on stage and key, so they order by id.
        assert_eq!(
            plan.reservations[0],
            PlannedReservation {
                requirement_id: reaction,
                type_id: 34,
                quantity: 300
            }
        );
        let mut rest = plan.reservations[1..].to_vec();
        rest.sort_by_key(|reservation| reservation.type_id);
        assert_eq!(
            rest,
            vec![
                PlannedReservation {
                    requirement_id: root,
                    type_id: 34,
                    quantity: 200
                },
                PlannedReservation {
                    requirement_id: pyerite,
                    type_id: 35,
                    quantity: 50
                },
            ]
        );
        assert_eq!(
            plan.shortfalls,
            vec![ReservationShortfall {
                type_id: 34,
                wanted: 900,
                free: 500
            }]
        );
    }

    #[test]
    fn capped_reservations_with_nothing_free_reserve_nothing() {
        let id = OrderRequirementId::new();
        let plan = plan_capped_reservations(
            &[ReservationNeed {
                requirement_id: id,
                type_id: 34,
                operation_occurrence_key: Some("root"),
                quantity: 10,
            }],
            &stages(&[("root", 0)]),
            &BTreeMap::new(),
        );
        assert!(plan.reservations.is_empty());
        assert_eq!(plan.shortfalls[0].free, 0);
    }

    #[test]
    fn compare_reuse_of_identical_maps_is_empty() {
        let reuse = BTreeMap::from([(34, 600)]);
        assert_eq!(compare_reuse(&reuse, &reuse), ReuseComparison::default());
    }
}
