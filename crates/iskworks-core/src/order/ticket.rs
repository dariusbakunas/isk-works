use super::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TicketId(pub Uuid);

impl TicketId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TicketId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TicketPrerequisiteId(pub Uuid);

impl TicketPrerequisiteId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TicketPrerequisiteId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TicketPrerequisiteFulfillmentId(pub Uuid);

impl TicketPrerequisiteFulfillmentId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TicketPrerequisiteFulfillmentId {
    fn default() -> Self {
        Self::new()
    }
}

/// A standalone execution work item -- not tied 1:1 to any single Order,
/// linked to whichever `OrderRequirement`/`TicketPrerequisite` row(s) it
/// fulfills via a join table. This is what makes "Required by 3 Orders"
/// possible without a rewrite: a `Ticket` does not *belong* to the Order
/// that first needed it.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TicketKind {
    Acquisition,
    Manufacturing,
    Reaction,
    /// A standalone organizational work item -- no item, no quantity, no
    /// Build, no execution snapshot, no inventory recording contract.
    /// "Move blueprints to C-J6MT" is valid Generic work; it moves across
    /// Board lanes like any other ticket, and completing it has no side
    /// effect of any kind.
    Generic,
}

/// Purely organizational, purely user-controlled. Every transition between
/// these four is legal in both directions via `set_ticket_status` -- there
/// is no state machine and no automatic writer. Dependency/blocker state is
/// a *separate*, derived concept: `derive_ticket_blockers` reports which
/// prerequisites are still unmet, independently of this status. A ticket
/// may legitimately be `InProgress` with a non-empty blocker list, or
/// `Todo` with none -- the system reports dependencies, the user controls
/// workflow. (`Complete` and `Canceled` stay distinguishable outcomes,
/// since a canceled ticket must never satisfy a dependency.)
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TicketStatus {
    Todo,
    InProgress,
    Complete,
    Canceled,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticket {
    pub id: TicketId,
    pub workspace_id: WorkspaceId,
    pub owner_id: OwnerId,
    /// "ISK-####", assigned at creation, never reassigned or reused.
    pub display_id: String,
    /// Explicit organizational Epic membership -- `Some(order)` when this
    /// ticket was generated as work belonging to that Order (a requirement
    /// ticket, or the root Manufacturing/Reaction ticket), `None` for a
    /// standalone ticket. This is the **authoritative** answer to "which
    /// Epic contains this ticket?" -- unlike `source_build_id` (which
    /// Build this ticket executes) or `order_requirement_fulfillments`
    /// (which frozen requirement(s) this ticket contributes to/fulfills,
    /// possibly across *other* Orders via `link_ticket_to_requirement`),
    /// this field is never used to infer anything beyond organizational
    /// membership, and nothing else should be used to infer it.
    pub order_id: Option<OrderId>,
    pub kind: TicketKind,
    /// `None` only for `Generic` -- every other kind requires an item.
    pub type_id: Option<i64>,
    /// This ticket's human-readable title -- an item/product name for a
    /// generated ticket (frozen at creation, same as always), or a
    /// user-chosen free-form title for a manually created one (most
    /// naturally so for `Generic`, which has no item to name it after).
    /// Always required and non-empty; there is deliberately no separate
    /// "title" column.
    pub captured_name: String,
    /// `None` only for `Generic` -- every other kind requires an output
    /// quantity.
    pub quantity: Option<u64>,
    /// Which recipe this ticket executes -- `None` for Acquisition/Generic,
    /// `Some` for Manufacturing/Reaction (the linked build whose recipe
    /// this ticket's job assumes). Independent of `order_id`: a
    /// standalone ticket can still execute a Build, and an Epic-owned
    /// Acquisition ticket never has one.
    pub source_build_id: Option<BuildId>,
    /// Free-form work notes -- optional, always `""` rather than `NULL` so
    /// callers never need an extra `Option` check for what is, in
    /// practice, just an empty string.
    pub notes: String,
    /// The connected EVE character this ticket is assigned to, if any --
    /// purely organizational (who is doing this work), never read by any
    /// derivation/recording function in this module. `None` when
    /// unassigned, or when the assignee's connection has since been
    /// disconnected (soft, on `eve_connections` -- the FK is
    /// `ON DELETE SET NULL`, so this also survives a hard delete if one is
    /// ever added).
    pub assignee_character_id: Option<ConnectedCharacterId>,
    pub status: TicketStatus,
    pub estimated_unit_cost: Option<Money>,
    pub estimated_line_total: Option<Money>,
    pub actual_unit_cost: Option<Money>,
    pub actual_line_total: Option<Money>,
    /// The "acquisition location" this ticket was priced from, frozen at
    /// creation time from the creating Order's price snapshot's *material*
    /// lines (an Acquisition ticket only ever concerns acquisition, never
    /// output valuation) -- meaningful for any kind, but only an
    /// Acquisition ticket's value is checked for Acquisition Run batching
    /// compatibility (every member of one Run must share the same scope).
    /// `None` when the snapshot had no resolved market scope for its
    /// material lines (e.g. fully manual pricing -- see `price_source_id`
    /// below for that case's own provenance).
    pub market_region_id: Option<i64>,
    pub market_location_id: Option<i64>,
    /// Set only when this ticket was priced from a Manual Price List
    /// -- `market_region_id`/`market_location_id` don't
    /// apply there since a manual list has no market scope, so this is the
    /// batching-compatibility key for that case instead. `None` whenever
    /// `market_region_id` is `Some`.
    pub price_source_id: Option<PriceSourceId>,
    pub acquisition_run_id: Option<AcquisitionRunId>,
    pub acquired_quantity: Option<u64>,
    /// The blueprint/facility/duration/cost this ticket's own job assumes,
    /// frozen at *ticket* creation time (not Order-generation time -- there
    /// is no Order-wide generation moment to freeze at). `None` for
    /// Acquisition.
    pub execution_snapshot: Option<TaskExecutionSnapshot>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Orthogonal to `status` -- never read by any derivation function in
    /// this module, only by Board/list visibility filtering.
    pub archived_at: Option<DateTime<Utc>>,
    /// This ticket's own frozen operation identity -- `Some`
    /// for a ticket generated from a whole-tree [`PlanOperation`] (the root,
    /// or an active Build/Reaction descendant), `None` for a version-1,
    /// standalone, or Acquisition/Generic ticket.
    pub occurrence_key: Option<String>,
    /// The parent ticket this operation's production feeds --
    /// `None` for the root ticket, a standalone ticket, or a version-1 ticket.
    pub parent_ticket_id: Option<TicketId>,
    /// This ticket's own full job output at freeze time --
    /// `PlanOperation::produced_quantity` (never the parent-consumed
    /// portion `quantity` already carries for an Acquisition ticket).
    pub produced_quantity: Option<u64>,
    /// This ticket's own material / installation / total
    /// production cost, from `PlanOperation` -- additive
    /// (`total_production_cost == material_component_cost +
    /// own_installation_cost`), distinct from `estimated_unit_cost` /
    /// `estimated_line_total` (which keep meaning the fresh/consumed
    /// portion for every ticket kind, unchanged).
    pub material_component_cost: Option<Money>,
    pub own_installation_cost: Option<Money>,
    pub total_production_cost: Option<Money>,
    pub plan_evidence: Option<PlanOperationEvidence>,
}

/// A Manufacturing/Reaction ticket's own frozen, single-level material
/// need -- computed at ticket-creation time from the linked build's own
/// recipe (`ComponentExpansionService::expand`, same function
/// `OrderRequirement` uses, just rooted one level down). Only ever
/// populated for `TicketKind::Manufacturing`/`Reaction`; Acquisition and
/// Generic tickets have zero rows and therefore never have blockers. An
/// unmet prerequisite row makes the ticket appear in `derive_ticket_blockers`
/// -- it never changes the ticket's own workflow status.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketPrerequisite {
    pub id: TicketPrerequisiteId,
    pub ticket_id: TicketId,
    pub type_id: i64,
    pub captured_name: String,
    /// How this prerequisite resolves execution -- frozen at *this*
    /// ticket's creation time, same reasoning as `OrderRequirement::kind`/
    /// `source_build_id` one level up: a later recursive child-ticket
    /// action reads this directly instead of re-resolving it against
    /// whatever the linked Build's component graph looks like by then.
    pub kind: RequirementKind,
    /// The specific linked Build selected for this prerequisite, frozen
    /// alongside `kind`. `None` for `Buy`, `Some` for `Build`/`React`.
    pub source_build_id: Option<BuildId>,
    pub required_quantity: u64,
    /// The scope this prerequisite was sourced under, frozen at ticket-
    /// creation time -- same meaning as `OrderRequirement::fulfillment_scope`
    /// one level up. The **root** Manufacturing/Reaction ticket's
    /// prerequisites mirror the Epic's own requirements (built from the same
    /// `create_order` calculation), so they carry the Epic's frozen scope. A
    /// child ticket created later from a `Build`/`React` requirement gets
    /// `Full` for now (its own linked-Build coverage netting is a follow-up).
    pub fulfillment_scope: FulfillmentScope,
    /// How much of `required_quantity` was planned to come from existing
    /// inventory at ticket-creation time. **Planning evidence, not a
    /// reservation.** `0` for a `Full`-scoped prerequisite (and for every
    /// pre-Model-B row, which read back as `Full`).
    pub reused_quantity: u64,
    /// `required_quantity - reused_quantity` -- the outstanding portion. A
    /// prerequisite with `fresh_quantity == 0` is treated as satisfied
    /// without any fulfilling work (`is_prerequisite_satisfied`).
    pub fresh_quantity: u64,
    pub estimated_unit_cost: Option<Money>,
    pub estimated_line_total: Option<Money>,
    /// Expected cost of just the `reused_quantity` portion at the frozen
    /// inventory weighted-average cost -- see
    /// `OrderRequirement::reused_line_total`. `None` unless this is a
    /// `Missing`-scoped root prerequisite with a known reused cost.
    pub reused_line_total: Option<Money>,
    /// See `OrderRequirement::operation_occurrence_key` -- the frozen
    /// operation whose own requirement this prerequisite was copied from
    /// (whole-tree tickets only; `None` for a version-1 or
    /// standalone/manual ticket's prerequisite).
    pub operation_occurrence_key: Option<String>,
    pub child_occurrence_key: Option<String>,
    pub inventory_unit_basis: Option<Money>,
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
    pub price_evidence: Option<PlanRequirementEvidence>,
}

/// Links a `TicketPrerequisite` to another `Ticket` that supplies it --
/// the same shape/semantics as `OrderRequirementFulfillment`, one level
/// down. Kept as a separate table/type rather than a polymorphic one: the
/// two answer different reverse-lookup questions and
/// don't share a numeric pool that needs single-query aggregation the way
/// `InventoryAllocation` does.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketPrerequisiteFulfillment {
    pub id: TicketPrerequisiteFulfillmentId,
    pub ticket_prerequisite_id: TicketPrerequisiteId,
    pub fulfilling_ticket_id: TicketId,
    pub allocated_quantity: u64,
    pub linked_at: DateTime<Utc>,
}

/// Mirrors `RequirementFulfillmentState`'s satisfaction rule for one
/// `TicketPrerequisite`: `fresh_quantity == 0` (only possible for a
/// older row that was inventory-covered at creation), or non-canceled
/// `Complete` fulfillments cover `fresh_quantity`. `fulfillments` is
/// `(ticket status, allocated_quantity)` for this prerequisite's links.
/// A newly created prerequisite always has
/// `fresh_quantity == required_quantity > 0`, so it is satisfied only by
/// completed fulfilling work -- never merely because stock exists.
#[must_use]
pub fn is_prerequisite_satisfied(
    prerequisite: &TicketPrerequisite,
    fulfillments: &[(TicketStatus, u64)],
) -> bool {
    if prerequisite.fresh_quantity == 0 {
        return true;
    }
    let completed_quantity: u64 = fulfillments
        .iter()
        .filter(|(status, _)| *status == TicketStatus::Complete)
        .map(|(_, quantity)| *quantity)
        .fold(0, u64::saturating_add);
    completed_quantity >= prerequisite.fresh_quantity
}

/// One unsatisfied prerequisite behind a ticket, built from
/// `TicketPrerequisite`/`TicketPrerequisiteFulfillment`. A prerequisite can eventually have
/// more than one non-canceled fulfillment (splitting demand across
/// tickets over time); the fields below name a single *representative*
/// one for compact display, not the complete fulfillment relationship --
/// callers who need the full list already have `list_ticket_prerequisite_fulfillments`
/// for that.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketBlockerRef {
    pub prerequisite_id: TicketPrerequisiteId,
    /// Frozen on the prerequisite itself -- see `TicketPrerequisite::kind`.
    pub kind: RequirementKind,
    pub type_id: i64,
    pub captured_name: String,
    /// `fresh_quantity` minus whatever non-canceled `Complete` fulfillments
    /// already cover -- "how much is still actually missing", not the raw
    /// requirement.
    pub outstanding_quantity: u64,
    pub representative_fulfilling_ticket_id: Option<TicketId>,
    pub representative_fulfilling_ticket_display_id: Option<String>,
    pub representative_fulfilling_ticket_status: Option<TicketStatus>,
}

/// The **only** dependency/blocker read model: given a ticket's frozen
/// prerequisites and their fulfillment links, returns the prerequisites
/// still unmet. Pure -- it never writes a ticket status, and its result is
/// independent of the subject ticket's own workflow status (a `Complete`
/// ticket with an unmet prerequisite still reports that blocker). A
/// prerequisite is resolved iff `is_prerequisite_satisfied` -- i.e. it was
/// inventory-covered at creation (`fresh_quantity == 0`) or non-canceled
/// `Complete` fulfilling work covers it. Callers surface this as an
/// informational indicator; nothing gates workflow on it.
///
/// `fulfillments_by_prerequisite` holds, for each `TicketPrerequisiteId`,
/// every non-canceled fulfillment as `(fulfilling ticket id, its display
/// id, its status, allocated_quantity)`. Where several fulfillments exist
/// for one prerequisite, the representative preferred is the "most
/// progressed" one: `InProgress` over `Todo` over `Complete` over none.
#[must_use]
pub fn derive_ticket_blockers(
    prerequisites: &[TicketPrerequisite],
    fulfillments_by_prerequisite: &HashMap<
        TicketPrerequisiteId,
        Vec<(TicketId, String, TicketStatus, u64)>,
    >,
) -> Vec<TicketBlockerRef> {
    prerequisites
        .iter()
        .filter_map(|prerequisite| {
            let fulfillments = fulfillments_by_prerequisite
                .get(&prerequisite.id)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let satisfaction_input: Vec<(TicketStatus, u64)> = fulfillments
                .iter()
                .map(|(_, _, status, quantity)| (*status, *quantity))
                .collect();
            if is_prerequisite_satisfied(prerequisite, &satisfaction_input) {
                return None;
            }
            let completed_quantity: u64 = fulfillments
                .iter()
                .filter(|(_, _, status, _)| *status == TicketStatus::Complete)
                .map(|(_, _, _, quantity)| *quantity)
                .fold(0, u64::saturating_add);
            let outstanding_quantity = prerequisite
                .fresh_quantity
                .saturating_sub(completed_quantity);
            let representative = fulfillments
                .iter()
                .max_by_key(|(_, _, status, _)| match status {
                    TicketStatus::InProgress => 3,
                    TicketStatus::Todo => 2,
                    TicketStatus::Complete => 1,
                    TicketStatus::Canceled => 0,
                });
            Some(TicketBlockerRef {
                prerequisite_id: prerequisite.id,
                kind: prerequisite.kind,
                type_id: prerequisite.type_id,
                captured_name: prerequisite.captured_name.clone(),
                outstanding_quantity,
                representative_fulfilling_ticket_id: representative.map(|(id, _, _, _)| *id),
                representative_fulfilling_ticket_display_id: representative
                    .map(|(_, display_id, _, _)| display_id.clone()),
                representative_fulfilling_ticket_status: representative
                    .map(|(_, _, status, _)| *status),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prerequisite(fresh_quantity: u64) -> TicketPrerequisite {
        TicketPrerequisite {
            id: TicketPrerequisiteId::new(),
            ticket_id: TicketId::new(),
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
    fn is_prerequisite_satisfied_ignores_canceled_fulfillments() {
        let prerequisite = prerequisite(50);
        assert!(!is_prerequisite_satisfied(
            &prerequisite,
            &[(TicketStatus::Canceled, 50)]
        ));
    }

    #[test]
    fn is_prerequisite_satisfied_true_when_complete_fulfillment_covers_fresh_quantity() {
        let prerequisite = prerequisite(50);
        assert!(is_prerequisite_satisfied(
            &prerequisite,
            &[(TicketStatus::Complete, 50)]
        ));
    }

    #[test]
    fn is_prerequisite_satisfied_false_while_fulfilling_work_is_only_in_progress() {
        // Delivery/recording progress alone never resolves a blocker -- only
        // the fulfilling ticket reaching Complete does (recording-based resolution is a later concern).
        let prerequisite = prerequisite(50);
        assert!(!is_prerequisite_satisfied(
            &prerequisite,
            &[(TicketStatus::InProgress, 50)]
        ));
    }

    #[test]
    fn no_prerequisites_means_no_blockers() {
        assert_eq!(derive_ticket_blockers(&[], &HashMap::new()), vec![]);
    }

    #[test]
    fn unmet_prerequisite_with_no_fulfillment_is_a_blocker_with_no_representative() {
        let a = prerequisite(50);
        let blockers = derive_ticket_blockers(std::slice::from_ref(&a), &HashMap::new());
        assert_eq!(blockers.len(), 1);
        assert_eq!(blockers[0].prerequisite_id, a.id);
        assert_eq!(blockers[0].outstanding_quantity, 50);
        assert_eq!(blockers[0].representative_fulfilling_ticket_id, None);
        assert_eq!(blockers[0].representative_fulfilling_ticket_status, None);
    }

    #[test]
    fn unmet_prerequisite_with_in_progress_fulfillment_surfaces_it_as_representative() {
        let a = prerequisite(50);
        let fulfilling_ticket = TicketId::new();
        let mut fulfillments = HashMap::new();
        fulfillments.insert(
            a.id,
            vec![(
                fulfilling_ticket,
                "ISK-1000".to_string(),
                TicketStatus::InProgress,
                20,
            )],
        );
        let blockers = derive_ticket_blockers(&[a], &fulfillments);
        assert_eq!(blockers.len(), 1);
        assert_eq!(
            blockers[0].representative_fulfilling_ticket_id,
            Some(fulfilling_ticket)
        );
        assert_eq!(
            blockers[0].representative_fulfilling_ticket_display_id,
            Some("ISK-1000".to_string())
        );
        assert_eq!(
            blockers[0].representative_fulfilling_ticket_status,
            Some(TicketStatus::InProgress)
        );
        // 20 delivered but not yet Complete -- none of it counts against
        // outstanding_quantity, which only subtracts Complete coverage.
        assert_eq!(blockers[0].outstanding_quantity, 50);
    }

    #[test]
    fn fully_complete_covered_prerequisite_is_excluded_even_if_another_keeps_the_ticket_blocked() {
        let covered = prerequisite(50);
        let uncovered = prerequisite(30);
        let fulfilling_ticket = TicketId::new();
        let mut fulfillments = HashMap::new();
        fulfillments.insert(
            covered.id,
            vec![(
                fulfilling_ticket,
                "ISK-1001".to_string(),
                TicketStatus::Complete,
                50,
            )],
        );
        let blockers = derive_ticket_blockers(&[covered, uncovered.clone()], &fulfillments);
        assert_eq!(blockers.len(), 1);
        assert_eq!(blockers[0].prerequisite_id, uncovered.id);
    }

    #[test]
    fn partial_complete_coverage_leaves_the_remainder_as_outstanding_quantity() {
        let a = prerequisite(50);
        let fulfilling_ticket = TicketId::new();
        let mut fulfillments = HashMap::new();
        fulfillments.insert(
            a.id,
            vec![(
                fulfilling_ticket,
                "ISK-1002".to_string(),
                TicketStatus::Complete,
                30,
            )],
        );
        let blockers = derive_ticket_blockers(&[a], &fulfillments);
        assert_eq!(blockers.len(), 1);
        assert_eq!(blockers[0].outstanding_quantity, 20);
    }

    #[test]
    fn canceled_fulfillment_does_not_satisfy_and_is_never_the_representative() {
        let a = prerequisite(50);
        let canceled_ticket = TicketId::new();
        let todo_ticket = TicketId::new();
        let mut fulfillments = HashMap::new();
        fulfillments.insert(
            a.id,
            vec![
                (
                    canceled_ticket,
                    "ISK-1003".to_string(),
                    TicketStatus::Canceled,
                    50,
                ),
                (todo_ticket, "ISK-1004".to_string(), TicketStatus::Todo, 10),
            ],
        );
        let blockers = derive_ticket_blockers(&[a], &fulfillments);
        assert_eq!(blockers.len(), 1);
        assert_eq!(
            blockers[0].representative_fulfilling_ticket_id,
            Some(todo_ticket)
        );
    }

    #[test]
    fn inventory_satisfied_prerequisite_is_never_a_blocker() {
        let a = prerequisite(0);
        assert_eq!(derive_ticket_blockers(&[a], &HashMap::new()), vec![]);
    }
}
