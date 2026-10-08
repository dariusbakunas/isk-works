use super::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InventoryAllocationId(pub Uuid);

impl InventoryAllocationId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for InventoryAllocationId {
    fn default() -> Self {
        Self::new()
    }
}

/// A typed exclusive-or, not a stringly-typed polymorphic `owner_type`/
/// `owner_id` pair -- the storage layer's schema enforces the same
/// constraint with a `CHECK (num_nonnulls(...) = 1)` over two nullable
/// FKs, but the domain type doesn't need to represent the invalid "both"
/// or "neither" states at all.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AllocationOwner {
    OrderRequirement(OrderRequirementId),
    TicketPrerequisite(TicketPrerequisiteId),
}

/// Why an allocation exists. `Legacy` rows predate Epic reservations (the
/// retired Order/Ticket lifecycle claims, all released or consumed); every
/// row written since is owned by an Epic requirement and says how it was
/// reserved.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AllocationReason {
    Legacy,
    /// Reserved from existing stock when the Epic was created.
    EpicCreate,
    /// Reserved later through the Epic's explicit "Reserve inventory" action.
    EpicTopUp,
    /// A ticket's recorded output, reserved to the requirement it feeds.
    RecordedOutput,
    /// Moved or adjusted by hand.
    Manual,
}

impl AllocationReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::EpicCreate => "epic_create",
            Self::EpicTopUp => "epic_top_up",
            Self::RecordedOutput => "recorded_output",
            Self::Manual => "manual",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "legacy" => Some(Self::Legacy),
            "epic_create" => Some(Self::EpicCreate),
            "epic_top_up" => Some(Self::EpicTopUp),
            "recorded_output" => Some(Self::RecordedOutput),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

/// A standing claim against physical inventory, owned by an `OrderRequirement`
/// (or, for `Legacy` rows only, a `TicketPrerequisite`); only active rows
/// count toward reserved quantity, and every planning read counts free stock
/// (`physical - active`).
///
/// Two timestamps rather than a status enum -- `released_at`/
/// `consumed_at` -- so the terminal outcome (freed vs. actually used) is
/// self-evident from the row itself, the same pattern `Order`/`Ticket`
/// use for their own cancellation/completion/archival.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryAllocation {
    pub id: InventoryAllocationId,
    pub workspace_id: WorkspaceId,
    pub owner_id: OwnerId,
    pub type_id: i64,
    pub quantity: u64,
    pub owner: AllocationOwner,
    pub reason: AllocationReason,
    /// The recording whose output this row reserves; set exactly when
    /// `reason` is `RecordedOutput`.
    pub source_recording_id: Option<TicketInventoryRecordingId>,
    pub created_at: DateTime<Utc>,
    /// Set when the claim is abandoned (Epic canceled/archived, recorded
    /// output reverted, stock taken by another Epic) -- no ledger event
    /// posted, nothing was ever actually consumed.
    pub released_at: Option<DateTime<Utc>>,
    /// Set by the recording that used this stock, in the same transaction
    /// as its real `InventoryEventKind::Consumption` event.
    pub consumed_at: Option<DateTime<Utc>>,
    /// The recording that consumed this row; set together with
    /// `consumed_at` on every non-`Legacy` row.
    pub consumed_by_recording_id: Option<TicketInventoryRecordingId>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AllocationLifecycleState {
    Active,
    Released,
    Consumed,
}

#[must_use]
pub fn allocation_lifecycle_state(allocation: &InventoryAllocation) -> AllocationLifecycleState {
    if allocation.consumed_at.is_some() {
        AllocationLifecycleState::Consumed
    } else if allocation.released_at.is_some() {
        AllocationLifecycleState::Released
    } else {
        AllocationLifecycleState::Active
    }
}

#[must_use]
pub fn is_active(allocation: &InventoryAllocation) -> bool {
    allocation_lifecycle_state(allocation) == AllocationLifecycleState::Active
}

/// `available = physical_balance - sum(active allocations)`. The real
/// `FOR UPDATE` locking that makes this race-safe across concurrent
/// Order/Ticket creation is storage-layer work (not this pure function's
/// job). `allocations` should already be filtered
/// to the relevant `type_id`; this only filters by lifecycle state.
#[must_use]
pub fn available_quantity(balance: u64, allocations: &[InventoryAllocation]) -> u64 {
    let allocated = allocations
        .iter()
        .filter(|allocation| is_active(allocation))
        .fold(0_u64, |total, allocation| {
            total.saturating_add(allocation.quantity)
        });
    balance.saturating_sub(allocated)
}

/// The Inventory page's own "Available" figure -- deliberately **not**
/// `available_quantity` above, which clamps at zero because it answers a
/// different question ("how much can a new allocation still grant"). This
/// answers "does physical stock actually cover what's already claimed", so
/// a shortfall (`reserved > physical`, e.g. after a future reconciliation
/// adjustment shrinks the balance below what active allocations already
/// claim) must surface as a real negative number, not silently disappear.
#[must_use]
pub fn reporting_available_quantity(physical: u64, reserved: u64) -> i64 {
    physical as i64 - reserved as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allocation(quantity: u64) -> InventoryAllocation {
        InventoryAllocation {
            id: InventoryAllocationId::new(),
            workspace_id: WorkspaceId::new(),
            owner_id: OwnerId::new(),
            type_id: 34,
            quantity,
            owner: AllocationOwner::OrderRequirement(OrderRequirementId::new()),
            reason: AllocationReason::EpicCreate,
            source_recording_id: None,
            created_at: Utc::now(),
            released_at: None,
            consumed_at: None,
            consumed_by_recording_id: None,
        }
    }

    #[test]
    fn allocation_reason_round_trips_its_storage_string() {
        for reason in [
            AllocationReason::Legacy,
            AllocationReason::EpicCreate,
            AllocationReason::EpicTopUp,
            AllocationReason::RecordedOutput,
            AllocationReason::Manual,
        ] {
            assert_eq!(AllocationReason::parse(reason.as_str()), Some(reason));
        }
        assert_eq!(AllocationReason::parse("reserved"), None);
    }

    #[test]
    fn fresh_allocation_is_active() {
        let allocation = allocation(100);
        assert_eq!(
            allocation_lifecycle_state(&allocation),
            AllocationLifecycleState::Active
        );
        assert!(is_active(&allocation));
    }

    #[test]
    fn released_allocation_is_not_active() {
        let mut allocation = allocation(100);
        allocation.released_at = Some(Utc::now());
        assert_eq!(
            allocation_lifecycle_state(&allocation),
            AllocationLifecycleState::Released
        );
        assert!(!is_active(&allocation));
    }

    #[test]
    fn consumed_allocation_is_not_active() {
        let mut allocation = allocation(100);
        allocation.consumed_at = Some(Utc::now());
        assert_eq!(
            allocation_lifecycle_state(&allocation),
            AllocationLifecycleState::Consumed
        );
        assert!(!is_active(&allocation));
    }

    #[test]
    fn consumed_wins_over_released_if_somehow_both_are_set() {
        let mut allocation = allocation(100);
        allocation.released_at = Some(Utc::now());
        allocation.consumed_at = Some(Utc::now());
        assert_eq!(
            allocation_lifecycle_state(&allocation),
            AllocationLifecycleState::Consumed
        );
    }

    #[test]
    fn available_subtracts_only_active_allocations() {
        let mut released = allocation(30);
        released.released_at = Some(Utc::now());
        let mut consumed = allocation(20);
        consumed.consumed_at = Some(Utc::now());
        let active = allocation(40);
        let allocations = vec![released, consumed, active];
        assert_eq!(available_quantity(100, &allocations), 60);
    }

    #[test]
    fn available_never_goes_negative() {
        let allocations = vec![allocation(150)];
        assert_eq!(available_quantity(100, &allocations), 0);
    }

    #[test]
    fn two_orders_created_back_to_back_see_reduced_availability() {
        // First Order allocates 500 of an 840 Morphite balance.
        let first_order_allocation = allocation(500);
        let available_for_second_order =
            available_quantity(840, std::slice::from_ref(&first_order_allocation));
        assert_eq!(available_for_second_order, 340);

        // Second Order can only allocate up to what's left, not the full
        // physical balance -- this is the honest cross-Order behavior the
        // ledger exists for.
        let second_order_claim = available_for_second_order.min(840);
        assert_eq!(second_order_claim, 340);

        // Canceling the first Order releases its claim immediately.
        let mut released = first_order_allocation;
        released.released_at = Some(Utc::now());
        assert_eq!(available_quantity(840, &[released]), 840);
    }

    #[test]
    fn reporting_available_with_no_reservation_equals_physical() {
        assert_eq!(reporting_available_quantity(100, 0), 100);
    }

    #[test]
    fn reporting_available_when_reserved_equals_physical_is_zero() {
        assert_eq!(reporting_available_quantity(100, 100), 0);
    }

    #[test]
    fn reporting_available_when_reserved_exceeds_physical_is_negative_not_clamped() {
        assert_eq!(reporting_available_quantity(100, 150), -50);
    }
}
