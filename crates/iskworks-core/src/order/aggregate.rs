use super::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrderId(pub Uuid);

impl OrderId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for OrderId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrderRequirementId(pub Uuid);

impl OrderRequirementId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for OrderRequirementId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrderRequirementFulfillmentId(pub Uuid);

impl OrderRequirementFulfillmentId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for OrderRequirementFulfillmentId {
    fn default() -> Self {
        Self::new()
    }
}

/// "Execute this version of this Build" -- an immutable snapshot captured
/// once, at creation. Unlike `Plan`, there is no Draft/Committed distinction
/// and no regenerate action: if the source Build changes and the user wants
/// to execute the new configuration, they create another Order.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    pub id: OrderId,
    pub workspace_id: WorkspaceId,
    pub owner_id: OwnerId,
    /// The Build this Epic was frozen from. `None` after that live planning
    /// object is deleted; the rest of this snapshot remains authoritative.
    pub source_build_id: Option<BuildId>,
    /// The source Build's `revision` at capture time -- purely informational
    /// (a soft "Build changed since" badge), never used to detect staleness
    /// that blocks anything, since Orders never regenerate.
    pub source_build_revision: u64,
    pub display_name: String,
    pub runs: u64,
    pub recipe_fingerprint: String,
    pub price_snapshot_id: PriceSnapshotId,
    pub estimated_material_cost: Money,
    pub expected_revenue: Option<Money>,
    pub estimated_margin: Option<Money>,
    pub missing_price_count: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Final assembly begun -- set by an explicit user action, only once
    /// `derive_order_status` reports `Ready`. Final assembly is not itself
    /// modeled as a `Ticket`; these two timestamps are its whole lifecycle.
    pub started_at: Option<DateTime<Utc>>,
    /// Final assembly done, user-confirmed. Distinct from every requirement
    /// merely being satisfied -- only the user knows the job was actually
    /// installed in EVE.
    pub completed_at: Option<DateTime<Utc>>,
    pub canceled_at: Option<DateTime<Utc>>,
    /// Orthogonal to workflow state -- never read by `derive_order_status`
    /// or any fulfillment-derivation function. Archiving only affects
    /// Board/list visibility filtering.
    pub archived_at: Option<DateTime<Utc>>,
    /// `1` -- legacy, root-only freeze (`create_order`'s
    /// single-level `OrderRequirement` list, no `PlanOperation`
    /// rows). `2` -- whole-tree freeze (`create_order_plan`): every active
    /// Build/Reaction operation in the tree has a `PlanOperation` row, and
    /// every `OrderRequirement` carries an `operation_occurrence_key`
    /// placing it in that tree. Never backfilled for an existing Epic --
    /// read as "what shape is this Epic's frozen data in", not migrated.
    pub planning_snapshot_version: u8,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OrderStatus {
    Blocked,
    Ready,
    InProgress,
    Complete,
    Canceled,
}

/// Derived, never stored. Order has no `status` column -- see
/// `RequirementFulfillmentState`/`compute_order_rollup` for how the
/// `Ready` determination folds over requirement state.
#[must_use]
pub fn derive_order_status(
    order: &Order,
    requirement_states: &[RequirementFulfillmentState],
) -> OrderStatus {
    if order.canceled_at.is_some() {
        return OrderStatus::Canceled;
    }
    if order.completed_at.is_some() {
        return OrderStatus::Complete;
    }
    if order.started_at.is_some() {
        return OrderStatus::InProgress;
    }
    if requirement_states
        .iter()
        .all(RequirementFulfillmentState::is_satisfied)
    {
        OrderStatus::Ready
    } else {
        OrderStatus::Blocked
    }
}

/// How an `OrderRequirement` resolves execution -- the BUY/BUILD/RXN
/// dependency badges. Never "INV": inventory coverage
/// is a fulfillment *state* (`RequirementFulfillmentState::InventorySatisfied`),
/// not a sourcing kind -- any requirement, regardless of kind, can turn out
/// to be fully covered by stock at creation time.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RequirementKind {
    Buy,
    Build,
    React,
}

/// A frozen, single-level dependency of an Order's root Build -- the
/// execution-relevant state captured at Order-creation time
/// (`ComponentExpansionService::expand` on the root build only). Deliberately
/// carries no workflow status, `display_id`, or ticket reference of its
/// own -- those live on `Ticket`, linked via `OrderRequirementFulfillment`,
/// because a requirement can exist with zero tickets (inventory-covered)
/// or eventually point at a ticket shared with other Orders.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRequirement {
    pub id: OrderRequirementId,
    pub order_id: OrderId,
    pub type_id: i64,
    pub captured_name: String,
    pub kind: RequirementKind,
    /// The specific linked Build selected for this requirement at
    /// Order-creation time -- frozen alongside `kind`, `None` for `Buy`.
    /// Ticket creation reads this directly rather than re-resolving a
    /// `Build`/`React` requirement's source build from the Order's
    /// *current* build graph, which could have changed since: an Order
    /// freezes the selected sourcing identity.
    pub source_build_id: Option<BuildId>,
    pub required_quantity: u64,
    /// The scope this requirement was sourced under, frozen at Epic-creation
    /// time. `Missing` -- inventory coverage was netted, so `reused_quantity`
    /// may be non-zero and `fresh_quantity` is the shortage. `Full` -- the
    /// whole requirement is sourced fresh, `reused_quantity == 0`. Frozen so
    /// a later read never consults the (mutable) source Build's current
    /// `fulfillment_scopes`.
    pub fulfillment_scope: FulfillmentScope,
    /// How much of `required_quantity` this Epic intended to draw from
    /// existing inventory, measured at creation time. **Planning evidence,
    /// not a reservation** -- no `inventory_allocations` row backs it, and
    /// two Epics may both plan against the same stock. `0` for a `Full`-scoped
    /// requirement.
    pub reused_quantity: u64,
    /// `required_quantity - reused_quantity` -- the portion that still needs
    /// acquiring or building. This is the quantity every generated ticket is
    /// sized from; a requirement with `fresh_quantity == 0` needs no ticket
    /// (`RequirementFulfillmentState::InventorySatisfied`).
    pub fresh_quantity: u64,
    pub estimated_unit_cost: Option<Money>,
    /// The blended expected cost of the whole requirement:
    /// `reused_line_total` (inventory portion) + the fresh portion's own
    /// market/build cost.
    pub estimated_line_total: Option<Money>,
    /// The expected cost of just the `reused_quantity` portion, at the
    /// inventory weighted-average cost frozen at creation. `None` for a
    /// `Full`-scoped requirement, when `reused_quantity == 0`, or when the
    /// reused portion's own cost was unknown (in which case the whole row is
    /// unpriced). The single authoritative figure for the split -- the fresh
    /// portion's cost is `estimated_line_total - reused_line_total`.
    pub reused_line_total: Option<Money>,
    /// Which [`PlanOperation`] this requirement belongs to --
    /// `None` for a version-1 (root-only) Epic's requirement, which reads
    /// back as "the order's own root" exactly as before. `Some` for every
    /// requirement of a whole-tree-frozen (version-2) Epic, at any depth.
    pub operation_occurrence_key: Option<String>,
    /// For a `Build`/`React` row whose production is an
    /// active frozen operation, that operation's own `occurrence_key`.
    /// `None` for a `Buy` row, a fully-covered row (shortage `0`, the child
    /// subtree was pruned), or an unresolved-but-Build-intended row with no
    /// linked child at freeze time.
    pub child_occurrence_key: Option<String>,
    /// The weighted-average unit basis actually used for
    /// `reused_quantity` at freeze time.
    pub inventory_unit_basis: Option<Money>,
    /// Surplus-conserving child-production evidence -- see
    /// `crate::build_cost`'s module doc. `None` unless `child_occurrence_key`
    /// is `Some`.
    pub child_produced_quantity: Option<u64>,
    pub child_consumed_quantity: Option<u64>,
    pub child_surplus_quantity: Option<u64>,
    pub child_surplus_retained_basis: Option<Money>,
    /// This requirement's own frozen share
    /// of the child operation's `total_production_cost` -- exactly the
    /// `BoundaryCostProjection::child_consumed_cost` the live projection
    /// charged this consumer. Per producer operation,
    /// `total = sum(child_consumed_cost) + surplus_retained_basis` holds
    /// exactly. `None` for version-1/2 rows and non-produced rows.
    pub child_consumed_cost: Option<Money>,
    /// The frozen demand-edge identity
    /// (`pd:<production_dependencies.id>` for a persisted edge,
    /// `dep:<consumer>:<type>` for one not yet persisted). `None` for
    /// version-1/2 rows.
    pub dependency_id: Option<String>,
    /// Fresh-price selection/provenance, frozen as-is from
    /// `VerificationBoundaryInput` at freeze time. `None` for a version-1
    /// Epic's requirement, or a `Build`/`React` row (which has no single
    /// fresh price of its own).
    pub price_evidence: Option<PlanRequirementEvidence>,
}

/// Links an `OrderRequirement` to a `Ticket` that (fully or partially)
/// fulfills it. Many-to-many by construction: multiple fulfillments can
/// point at the same requirement (splitting demand across tickets over
/// time) and multiple fulfillments -- from different Orders' requirements
/// entirely -- can point at the same ticket. That second case is the whole
/// mechanism behind "Required by 3 Orders": nothing here forces 1:1, this
/// milestone just doesn't build the UI to create the N:1 case yet.
///
/// Never deleted, even once the ticket is Canceled -- fulfillment state is
/// always *derived* by filtering non-canceled links, not by removing rows,
/// so cancellation history stays intact.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRequirementFulfillment {
    pub id: OrderRequirementFulfillmentId,
    pub order_requirement_id: OrderRequirementId,
    pub ticket_id: TicketId,
    pub allocated_quantity: u64,
    pub linked_at: DateTime<Utc>,
}

/// One `OrderRequirement`'s derived fulfillment state -- never stored.
/// `Linked` deliberately isn't named `InProgress`: it covers a linked
/// ticket in *any* non-terminal state (`Todo` or `InProgress`), since from
/// the Order's point of view "someone created a ticket for this" is what
/// matters for the "needs action" vs. "in progress" rollup, not that
/// ticket's own finer status.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RequirementFulfillmentState {
    /// `fresh_quantity == 0` at creation time -- fully covered by stock,
    /// no ticket needed ("Inventory. No ticket required.").
    InventorySatisfied,
    /// Completed, non-canceled fulfillment(s) cover `fresh_quantity`.
    Satisfied,
    /// A non-canceled, non-Complete ticket is linked but doesn't yet
    /// (fully) cover `fresh_quantity`.
    Linked,
    /// No ticket, or every linked ticket is Canceled -- "Needs action" /
    /// "Create Acquisition Ticket".
    NeedsAction,
}

impl RequirementFulfillmentState {
    #[must_use]
    pub fn is_satisfied(&self) -> bool {
        matches!(self, Self::InventorySatisfied | Self::Satisfied)
    }
}

/// Derives one requirement's fulfillment state from its (already-fetched)
/// non-canceled fulfillment links. `fulfillments` is `(ticket status,
/// allocated_quantity)` for every fulfillment of this requirement whose
/// ticket is not `Canceled` -- callers filter canceled links out before
/// calling this (or pass them in and this function ignores tickets it
/// doesn't recognize as active; either way `Canceled` never contributes,
/// directly implementing "a canceled ticket never satisfies a
/// requirement").
#[must_use]
pub fn derive_requirement_state(
    requirement: &OrderRequirement,
    fulfillments: &[(TicketStatus, u64)],
) -> RequirementFulfillmentState {
    if requirement.fresh_quantity == 0 {
        return RequirementFulfillmentState::InventorySatisfied;
    }
    let completed_quantity: u64 = fulfillments
        .iter()
        .filter(|(status, _)| *status == TicketStatus::Complete)
        .map(|(_, quantity)| *quantity)
        .fold(0, u64::saturating_add);
    if completed_quantity >= requirement.fresh_quantity {
        return RequirementFulfillmentState::Satisfied;
    }
    let has_active_link = fulfillments
        .iter()
        .any(|(status, _)| matches!(status, TicketStatus::Todo | TicketStatus::InProgress));
    if has_active_link {
        RequirementFulfillmentState::Linked
    } else {
        RequirementFulfillmentState::NeedsAction
    }
}

/// The "1/6 satisfied, 2 need action, 2 in progress" progress bar.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRequirementRollup {
    pub satisfied: usize,
    pub needs_action: usize,
    pub in_progress: usize,
    pub total: usize,
}

#[must_use]
pub fn compute_order_rollup(states: &[RequirementFulfillmentState]) -> OrderRequirementRollup {
    let mut rollup = OrderRequirementRollup {
        total: states.len(),
        ..OrderRequirementRollup::default()
    };
    for state in states {
        match state {
            RequirementFulfillmentState::InventorySatisfied
            | RequirementFulfillmentState::Satisfied => {
                rollup.satisfied += 1;
            }
            RequirementFulfillmentState::Linked => rollup.in_progress += 1,
            RequirementFulfillmentState::NeedsAction => rollup.needs_action += 1,
        }
    }
    rollup
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requirement(fresh_quantity: u64) -> OrderRequirement {
        OrderRequirement {
            id: OrderRequirementId::new(),
            order_id: OrderId::new(),
            type_id: 34,
            captured_name: "Tritanium".into(),
            kind: RequirementKind::Buy,
            source_build_id: None,
            required_quantity: 100,
            fulfillment_scope: if fresh_quantity == 100 {
                FulfillmentScope::Full
            } else {
                FulfillmentScope::Missing
            },
            reused_quantity: 100 - fresh_quantity,
            fresh_quantity,
            estimated_unit_cost: None,
            estimated_line_total: None,
            reused_line_total: None,
            operation_occurrence_key: None,
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

    #[test]
    fn fully_reused_requirement_is_inventory_satisfied_regardless_of_links() {
        let requirement = requirement(0);
        let state = derive_requirement_state(&requirement, &[(TicketStatus::Canceled, 999)]);
        assert_eq!(state, RequirementFulfillmentState::InventorySatisfied);
    }

    #[test]
    fn no_links_is_needs_action() {
        let requirement = requirement(50);
        let state = derive_requirement_state(&requirement, &[]);
        assert_eq!(state, RequirementFulfillmentState::NeedsAction);
    }

    #[test]
    fn only_canceled_links_is_needs_action() {
        let requirement = requirement(50);
        let state = derive_requirement_state(&requirement, &[(TicketStatus::Canceled, 50)]);
        assert_eq!(state, RequirementFulfillmentState::NeedsAction);
    }

    #[test]
    fn a_todo_linked_ticket_still_counts_as_linked_not_needs_action() {
        let requirement = requirement(50);
        let state = derive_requirement_state(&requirement, &[(TicketStatus::Todo, 50)]);
        assert_eq!(state, RequirementFulfillmentState::Linked);
    }

    #[test]
    fn complete_link_covering_fresh_quantity_is_satisfied() {
        let requirement = requirement(50);
        let state = derive_requirement_state(&requirement, &[(TicketStatus::Complete, 50)]);
        assert_eq!(state, RequirementFulfillmentState::Satisfied);
    }

    #[test]
    fn complete_link_partially_covering_fresh_quantity_with_no_other_link_is_needs_action() {
        let requirement = requirement(50);
        let state = derive_requirement_state(&requirement, &[(TicketStatus::Complete, 30)]);
        assert_eq!(state, RequirementFulfillmentState::NeedsAction);
    }

    #[test]
    fn complete_link_partially_covering_fresh_quantity_with_another_active_link_is_linked() {
        let requirement = requirement(50);
        let state = derive_requirement_state(
            &requirement,
            &[(TicketStatus::Complete, 30), (TicketStatus::InProgress, 20)],
        );
        assert_eq!(state, RequirementFulfillmentState::Linked);
    }

    #[test]
    fn rollup_counts_each_bucket() {
        let states = vec![
            RequirementFulfillmentState::InventorySatisfied,
            RequirementFulfillmentState::Satisfied,
            RequirementFulfillmentState::Linked,
            RequirementFulfillmentState::Linked,
            RequirementFulfillmentState::NeedsAction,
            RequirementFulfillmentState::NeedsAction,
        ];
        let rollup = compute_order_rollup(&states);
        assert_eq!(
            rollup,
            OrderRequirementRollup {
                satisfied: 2,
                needs_action: 2,
                in_progress: 2,
                total: 6,
            }
        );
    }

    fn order() -> Order {
        Order {
            id: OrderId::new(),
            workspace_id: WorkspaceId::new(),
            owner_id: OwnerId::new(),
            source_build_id: Some(BuildId::new()),
            source_build_revision: 1,
            display_name: "Manufacture Ishtar".into(),
            runs: 1,
            recipe_fingerprint: "fp".into(),
            price_snapshot_id: PriceSnapshotId(Uuid::new_v4()),
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
            planning_snapshot_version: 1,
        }
    }

    #[test]
    fn blocked_when_any_requirement_is_unsatisfied() {
        let order = order();
        let states = [
            RequirementFulfillmentState::Satisfied,
            RequirementFulfillmentState::NeedsAction,
        ];
        assert_eq!(derive_order_status(&order, &states), OrderStatus::Blocked);
    }

    #[test]
    fn ready_when_every_requirement_is_satisfied() {
        let order = order();
        let states = [
            RequirementFulfillmentState::Satisfied,
            RequirementFulfillmentState::InventorySatisfied,
        ];
        assert_eq!(derive_order_status(&order, &states), OrderStatus::Ready);
    }

    #[test]
    fn ready_with_zero_requirements() {
        let order = order();
        assert_eq!(derive_order_status(&order, &[]), OrderStatus::Ready);
    }

    #[test]
    fn started_at_wins_over_requirement_state() {
        let mut order = order();
        order.started_at = Some(Utc::now());
        let states = [RequirementFulfillmentState::NeedsAction];
        assert_eq!(
            derive_order_status(&order, &states),
            OrderStatus::InProgress
        );
    }

    #[test]
    fn completed_at_wins_over_started_at() {
        let mut order = order();
        order.started_at = Some(Utc::now());
        order.completed_at = Some(Utc::now());
        assert_eq!(derive_order_status(&order, &[]), OrderStatus::Complete);
    }

    #[test]
    fn canceled_at_wins_over_everything() {
        let mut order = order();
        order.started_at = Some(Utc::now());
        order.completed_at = Some(Utc::now());
        order.canceled_at = Some(Utc::now());
        assert_eq!(derive_order_status(&order, &[]), OrderStatus::Canceled);
    }

    #[test]
    fn archived_at_never_affects_status() {
        let mut satisfied = order();
        satisfied.archived_at = Some(Utc::now());
        let states = [RequirementFulfillmentState::Satisfied];
        assert_eq!(derive_order_status(&satisfied, &states), OrderStatus::Ready);
    }
}
