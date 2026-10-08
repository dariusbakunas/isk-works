use super::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AcquisitionRunId(pub Uuid);

impl AcquisitionRunId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AcquisitionRunId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AcquisitionRunStatus {
    Ready,
    InProgress,
    Complete,
}

/// The only Execution Batch kind implemented so far -- a `CHECK` constraint
/// in the schema, deliberately not a rules engine. Widen this enum (and the
/// matching `CHECK`) if/when a second kind is actually needed.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AcquisitionRunKind {
    Acquisition,
}

/// An Execution Batch: groups compatible, already-Ready standalone
/// Acquisition tickets for one shopping trip, without merging or
/// destroying the source tickets. Every
/// member ticket shares this Run's `(market_region_id, market_location_id)`
/// scope, or -- for manually-priced tickets, where scope doesn't apply --
/// its `price_source_id`.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcquisitionRun {
    pub id: AcquisitionRunId,
    pub workspace_id: WorkspaceId,
    pub owner_id: OwnerId,
    pub display_id: String,
    pub name: String,
    pub kind: AcquisitionRunKind,
    pub status: AcquisitionRunStatus,
    pub market_region_id: Option<i64>,
    pub market_location_id: Option<i64>,
    pub price_source_id: Option<PriceSourceId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// One item's recorded-progress update within a
/// `record_order_acquisition_progress` call (edits are batched per "Save
/// Progress" click, distributed across that item's member tickets
/// server-side, not asked of the user). `acquired_quantity` is the new
/// cumulative total the user has actually acquired for this type,
/// deliberately *not* capped to the Run's total demand for it --
/// over-acquisition is a valid real-world outcome (buying 3,000 Tritanium
/// against a 2,985 requirement), tracked in full on `acquisition_run_items`
/// and only capped, per-ticket, at delivery/allocation time (see
/// `allocate_acquisition_delivery`).
#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcquisitionProgressUpdate {
    pub type_id: i64,
    pub acquired_quantity: u64,
}

/// The real, uncapped total recorded as acquired for one material type in
/// a Run -- may exceed the sum of member tickets' `fresh_quantity`
/// (over-acquisition). Backed by `acquisition_run_items`, the single
/// source of truth `record_order_acquisition_progress` upserts into; ticket-level
/// `acquired_quantity` stays a derived, per-ticket-capped view of this same
/// number.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcquisitionRunItem {
    pub type_id: i64,
    pub acquired_quantity: u64,
}

/// Deterministic allocation of one type's real acquired total (from
/// `AcquisitionRunItem::acquired_quantity`) across its member tickets'
/// outstanding demand at Run completion/delivery time: ascending
/// `display_id` (`members` must already be sorted that way by the
/// caller), fill each ticket's own `fresh_quantity` before moving to the
/// next, carrying the remainder forward ("fill-to-need, carry
/// remainder").
/// Returns each ticket's allocated share (never exceeding its own
/// `fresh_quantity`) alongside the leftover surplus that covers no
/// ticket's demand -- delivered to Inventory as ordinary unreserved stock.
/// Pure and stateless: the storage layer owns turning this into real
/// reservation/allocation rows and ticket completions.
///
/// Generic over the member id type so this shared algorithm can be reused
/// unchanged wherever an Acquisition Run needs it -- there is nothing
/// id-type-specific about it.
#[must_use]
pub fn allocate_acquisition_delivery<Id: Copy>(
    members: &[(Id, u64)],
    acquired_total: u64,
) -> (Vec<(Id, u64)>, u64) {
    let mut remaining = acquired_total;
    let mut allocations = Vec::with_capacity(members.len());
    for (task_id, fresh_quantity) in members {
        let allocated = remaining.min(*fresh_quantity);
        remaining -= allocated;
        allocations.push((*task_id, allocated));
    }
    (allocations, remaining)
}

/// Quantity-weighted average unit cost across a type's contributing
/// tickets, weighted by each ticket's own `fresh_quantity` -- the basis
/// for the single Inventory `Purchase` posting a Run's delivery makes for
/// that type (covering both allocated and surplus quantity alike, since
/// Inventory has no per-ticket cost-lot tracking).
///
/// Each contributor's own `estimated_unit_cost` is used when present;
/// `fallback` (the type's current inventory weighted average, resolved
/// once by the caller -- every contributor here shares the same
/// `type_id`, so one fallback value covers all of them) is used for any
/// contributor missing its own estimate, per the approved cost-resolution
/// hierarchy, so one ticket's missing estimate does not collapse the
/// entire blended result to unresolvable -- `None` only when a contributor
/// has *neither* its own estimate *nor* the fallback, or there are no
/// contributors at all.
pub fn weighted_unit_cost(
    costs: &[(Option<Money>, u64)],
    fallback: Option<Money>,
) -> Result<Option<Money>, IndustryError> {
    let mut total_quantity: u64 = 0;
    let mut total_cost = Money::zero();
    for (unit_cost, quantity) in costs {
        let Some(unit_cost) = unit_cost.or(fallback) else {
            return Ok(None);
        };
        if *quantity == 0 {
            continue;
        }
        let line_total = unit_cost.checked_mul_quantity(*quantity)?;
        total_cost = total_cost.checked_add(line_total)?;
        total_quantity = total_quantity
            .checked_add(*quantity)
            .ok_or(IndustryError::MoneyOverflow)?;
    }
    if total_quantity == 0 {
        return Ok(None);
    }
    Ok(Some(total_cost.checked_div_quantity(total_quantity)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_fulfillment_leaves_no_surplus() {
        let members = [(AcquisitionRunId::new(), 50)];
        let (allocations, surplus) = allocate_acquisition_delivery(&members, 50);
        assert_eq!(allocations, vec![(members[0].0, 50)]);
        assert_eq!(surplus, 0);
    }

    #[test]
    fn short_delivery_allocates_only_what_was_delivered() {
        let members = [(AcquisitionRunId::new(), 50)];
        let (allocations, surplus) = allocate_acquisition_delivery(&members, 30);
        assert_eq!(allocations, vec![(members[0].0, 30)]);
        assert_eq!(surplus, 0);
    }

    #[test]
    fn zero_delivery_allocates_nothing() {
        let members = [(AcquisitionRunId::new(), 50)];
        let (allocations, surplus) = allocate_acquisition_delivery(&members, 0);
        assert_eq!(allocations, vec![(members[0].0, 0)]);
        assert_eq!(surplus, 0);
    }

    #[test]
    fn multiple_members_fill_in_order_before_moving_to_the_next() {
        let first = AcquisitionRunId::new();
        let second = AcquisitionRunId::new();
        let third = AcquisitionRunId::new();
        let members = [(first, 50), (second, 30), (third, 20)];
        // Covers all of `first`, all of `second`, and only half of `third`.
        let (allocations, surplus) = allocate_acquisition_delivery(&members, 90);
        assert_eq!(allocations, vec![(first, 50), (second, 30), (third, 10)]);
        assert_eq!(surplus, 0);
    }

    #[test]
    fn surplus_beyond_total_demand_is_returned_not_discarded() {
        let first = AcquisitionRunId::new();
        let second = AcquisitionRunId::new();
        let members = [(first, 50), (second, 30)];
        let (allocations, surplus) = allocate_acquisition_delivery(&members, 100);
        assert_eq!(allocations, vec![(first, 50), (second, 30)]);
        assert_eq!(surplus, 20);
    }

    #[test]
    fn generic_over_a_different_id_type_behaves_identically() {
        #[derive(Debug, Clone, Copy, Eq, PartialEq)]
        struct OtherId(u32);

        let members = [(OtherId(1), 50), (OtherId(2), 30)];
        let (allocations, surplus) = allocate_acquisition_delivery(&members, 60);
        assert_eq!(allocations, vec![(OtherId(1), 50), (OtherId(2), 10)]);
        assert_eq!(surplus, 0);
    }

    #[test]
    fn weighted_unit_cost_blends_by_fresh_quantity() {
        let cost = weighted_unit_cost(
            &[
                (Some(Money::parse("4.0000").unwrap()), 1_400),
                (Some(Money::parse("5.0000").unwrap()), 12_800),
            ],
            None,
        )
        .unwrap();
        // (1400*4 + 12800*5) / 14200 = 4.9014...
        assert_eq!(cost, Some(Money::parse("4.9014").unwrap()));
    }

    #[test]
    fn weighted_unit_cost_uses_the_fallback_for_a_contributor_missing_its_own_estimate() {
        let cost = weighted_unit_cost(
            &[
                (Some(Money::parse("4.0000").unwrap()), 1_400),
                (None, 12_800),
            ],
            Some(Money::parse("5.0000").unwrap()),
        )
        .unwrap();
        // The second contributor falls back to the shared inventory
        // average (5.0000) instead of poisoning the whole blend.
        assert_eq!(cost, Some(Money::parse("4.9014").unwrap()));
    }

    #[test]
    fn weighted_unit_cost_is_unresolvable_if_a_contributor_has_neither_estimate_nor_fallback() {
        let cost = weighted_unit_cost(
            &[
                (Some(Money::parse("4.0000").unwrap()), 1_400),
                (None, 12_800),
            ],
            None,
        )
        .unwrap();
        assert_eq!(cost, None);
    }

    #[test]
    fn weighted_unit_cost_of_no_contributors_is_unknown() {
        assert_eq!(weighted_unit_cost(&[], None).unwrap(), None);
    }
}
