//! Which stock a recording's inputs draw from, so recording never quietly
//! uses inventory another Epic is counting on.
//!
//! Per input type, in this order:
//! 1. the consuming ticket's own Epic reservations (for those requirements),
//! 2. free stock -- **measured before step 1**, under the balance lock,
//!    because own reservations were never part of free stock and consuming
//!    them must not make room for anything else,
//! 3. other Epics' reservations, only for Epics the caller explicitly
//!    named in `take_from` (the "Take N from EP-x?" confirmation).
//!
//! Anything still short is an [`AvailabilityShortage`] naming every holder.

use std::collections::{BTreeMap, BTreeSet};

use super::*;

/// One active allocation of the input type, as the planner sees it.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct HeldAllocation {
    pub id: InventoryAllocationId,
    pub order_id: OrderId,
    pub quantity: u64,
}

/// Use `used` of allocation `id`; a partial use leaves the remainder as a
/// new active row (see the spec's "split" rule).
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct AllocationUse {
    pub id: InventoryAllocationId,
    pub used: u64,
    pub remainder: u64,
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct ConsumptionPlan {
    /// The ticket's own reservations, consumed by this recording.
    pub own: Vec<AllocationUse>,
    /// Drawn from free stock.
    pub from_free: u64,
    /// Taken from other Epics (released from them, then consumed as free).
    pub taken: Vec<AllocationUse>,
}

/// An Epic holding stock of the short type.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReservationHolder {
    pub order_id: OrderId,
    pub quantity: u64,
}

/// Recording needs more of a type than its own reservations, free stock and
/// the Epics it may take from can cover.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailabilityShortage {
    pub type_id: i64,
    pub needed: u64,
    pub own: u64,
    pub free: u64,
    /// Every other Epic holding this type, largest first.
    pub holders: Vec<ReservationHolder>,
}

/// Everything the planner needs about one input type.
#[derive(Debug, Clone, Copy)]
pub struct InputAvailability<'a> {
    pub type_id: i64,
    /// Σ of the recording's input lines of this type.
    pub quantity: u64,
    /// The consuming ticket's own active allocations, in serving order.
    pub own: &'a [HeldAllocation],
    /// `physical - Σ every active allocation`, floored at `0`, measured
    /// under the balance lock before anything is consumed.
    pub free_before: u64,
    /// Other Epics' active allocations of this type, oldest first.
    pub others: &'a [HeldAllocation],
}

fn draw(allocations: &[HeldAllocation], mut wanted: u64) -> (Vec<AllocationUse>, u64) {
    let mut uses = Vec::new();
    let mut total = 0;
    for allocation in allocations {
        if wanted == 0 {
            break;
        }
        let used = allocation.quantity.min(wanted);
        if used == 0 {
            continue;
        }
        uses.push(AllocationUse {
            id: allocation.id,
            used,
            remainder: allocation.quantity - used,
        });
        wanted -= used;
        total += used;
    }
    (uses, total)
}

/// Plan one input type's draw.
///
/// # Errors
///
/// [`AvailabilityShortage`] when own + free + permitted takes fall short.
pub fn plan_input_consumption(
    input: InputAvailability<'_>,
    take_from: &BTreeSet<OrderId>,
) -> Result<ConsumptionPlan, AvailabilityShortage> {
    let (own, own_used) = draw(input.own, input.quantity);
    let remainder = input.quantity - own_used;
    let from_free = remainder.min(input.free_before);
    let still_short = remainder - from_free;

    let permitted: Vec<HeldAllocation> = input
        .others
        .iter()
        .filter(|allocation| take_from.contains(&allocation.order_id))
        .copied()
        .collect();
    let (taken, taken_total) = draw(&permitted, still_short);
    if taken_total < still_short {
        let mut by_order: BTreeMap<OrderId, u64> = BTreeMap::new();
        for allocation in input.others {
            *by_order.entry(allocation.order_id).or_insert(0) += allocation.quantity;
        }
        let mut holders: Vec<ReservationHolder> = by_order
            .into_iter()
            .map(|(order_id, quantity)| ReservationHolder { order_id, quantity })
            .collect();
        holders.sort_by(|a, b| {
            b.quantity
                .cmp(&a.quantity)
                .then(a.order_id.0.cmp(&b.order_id.0))
        });
        return Err(AvailabilityShortage {
            type_id: input.type_id,
            needed: input.quantity,
            own: input.own.iter().map(|allocation| allocation.quantity).sum(),
            free: input.free_before,
            holders,
        });
    }
    Ok(ConsumptionPlan {
        own,
        from_free,
        taken,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(order: OrderId, quantity: u64) -> HeldAllocation {
        HeldAllocation {
            id: InventoryAllocationId::new(),
            order_id: order,
            quantity,
        }
    }

    fn input<'a>(
        quantity: u64,
        own: &'a [HeldAllocation],
        free_before: u64,
        others: &'a [HeldAllocation],
    ) -> InputAvailability<'a> {
        InputAvailability {
            type_id: 34,
            quantity,
            own,
            free_before,
            others,
        }
    }

    /// The spec's worked example: physical 1,000, own 600, another Epic
    /// 400, the line needs 1,000. Free before consuming is 0, so the other
    /// Epic's 400 must not be used silently.
    #[test]
    fn never_counts_own_consumption_as_free_stock() {
        let epic = OrderId::new();
        let other = OrderId::new();
        let own = [held(epic, 600)];
        let others = [held(other, 400)];

        let shortage =
            plan_input_consumption(input(1_000, &own, 0, &others), &BTreeSet::new()).unwrap_err();

        assert_eq!(shortage.needed, 1_000);
        assert_eq!(shortage.own, 600);
        assert_eq!(shortage.free, 0);
        assert_eq!(
            shortage.holders,
            vec![ReservationHolder {
                order_id: other,
                quantity: 400
            }]
        );
    }

    #[test]
    fn own_reservations_cover_the_line_and_split_the_last_one() {
        let epic = OrderId::new();
        let own = [held(epic, 300), held(epic, 500)];
        let plan = plan_input_consumption(input(600, &own, 1_000, &[]), &BTreeSet::new()).unwrap();

        assert_eq!(
            plan.own,
            vec![
                AllocationUse {
                    id: own[0].id,
                    used: 300,
                    remainder: 0
                },
                AllocationUse {
                    id: own[1].id,
                    used: 300,
                    remainder: 200
                },
            ]
        );
        assert_eq!(plan.from_free, 0, "free stock is untouched");
        assert!(plan.taken.is_empty());
    }

    #[test]
    fn falls_back_to_free_stock_after_own() {
        let epic = OrderId::new();
        let own = [held(epic, 600)];
        let plan = plan_input_consumption(input(900, &own, 500, &[]), &BTreeSet::new()).unwrap();
        assert_eq!(plan.own[0].used, 600);
        assert_eq!(plan.from_free, 300);
    }

    #[test]
    fn a_ticket_outside_any_epic_draws_free_stock_only() {
        let other = OrderId::new();
        let others = [held(other, 400)];
        assert_eq!(
            plan_input_consumption(input(100, &[], 100, &others), &BTreeSet::new())
                .unwrap()
                .from_free,
            100
        );
        assert!(plan_input_consumption(input(101, &[], 100, &others), &BTreeSet::new()).is_err());
    }

    #[test]
    fn takes_only_what_is_short_from_a_named_epic() {
        let epic = OrderId::new();
        let other = OrderId::new();
        let own = [held(epic, 600)];
        let others = [held(other, 400)];

        let plan =
            plan_input_consumption(input(1_000, &own, 100, &others), &BTreeSet::from([other]))
                .unwrap();

        assert_eq!(plan.own[0].used, 600);
        assert_eq!(plan.from_free, 100);
        assert_eq!(
            plan.taken,
            vec![AllocationUse {
                id: others[0].id,
                used: 300,
                remainder: 100
            }]
        );
    }

    #[test]
    fn taking_from_a_named_epic_that_holds_too_little_is_still_short() {
        let epic = OrderId::new();
        let named = OrderId::new();
        let unnamed = OrderId::new();
        let own = [held(epic, 600)];
        let others = [held(named, 100), held(unnamed, 300)];

        let shortage =
            plan_input_consumption(input(1_000, &own, 0, &others), &BTreeSet::from([named]))
                .unwrap_err();

        assert_eq!(
            shortage.holders,
            vec![
                ReservationHolder {
                    order_id: unnamed,
                    quantity: 300
                },
                ReservationHolder {
                    order_id: named,
                    quantity: 100
                },
            ]
        );
    }
}
