use super::*;

/// Outcome of `OrderRepository::create_ticket_for_order_requirement`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RequirementTicketCreation {
    /// The ticket was created and linked to the requirement.
    Created(Box<Ticket>),
    /// An active ticket already holds the requirement; nothing was created.
    AlreadyLinked(TicketId),
}

/// One frozen material need to persist under an Order.
///
/// The `fulfillment_scope` / `reused_quantity` / `reused_line_total` triple
/// is the caller's authoritative Model-B snapshot of intended inventory
/// reuse, computed once at Epic creation from live `ProductionRepository`
/// coverage. Persisting it is **inventory-neutral**: it writes no
/// `inventory_allocations` / `inventory_events` row and changes no balance.
/// `fresh_quantity` is derived (`required_quantity - reused_quantity`).
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NewOrderRequirement {
    pub id: OrderRequirementId,
    pub type_id: i64,
    pub captured_name: String,
    pub kind: RequirementKind,
    /// See `OrderRequirement::source_build_id` -- `None` for `Buy`, `Some`
    /// for `Build`/`React`.
    pub source_build_id: Option<BuildId>,
    pub required_quantity: u64,
    /// The frozen scope (`Missing` nets inventory, `Full` ignores it).
    pub fulfillment_scope: FulfillmentScope,
    /// Frozen intended inventory reuse -- `min(required_quantity,
    /// available_to_this_build)` for a `Missing` row, `0` for a `Full` row.
    /// Storage derives `fresh_quantity = required_quantity - reused_quantity`
    /// and the `reused + fresh = required` CHECK enforces consistency.
    pub reused_quantity: u64,
    pub estimated_unit_cost: Option<Money>,
    pub estimated_line_total: Option<Money>,
    /// Frozen expected cost of the `reused_quantity` portion -- see
    /// `OrderRequirement::reused_line_total`.
    pub reused_line_total: Option<Money>,
    /// See `OrderRequirement::operation_occurrence_key`.
    pub operation_occurrence_key: Option<String>,
    /// See `OrderRequirement::child_occurrence_key`.
    pub child_occurrence_key: Option<String>,
    /// See `OrderRequirement::inventory_unit_basis`.
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

/// The `Order` row itself has no allocation-dependent fields, so -- unlike
/// its requirements -- it can be fully pre-built by the caller and just
/// persisted as-is. `order.started_at`/`completed_at`/`canceled_at` are expected
/// to be `None`; lifecycle transitions are separate repository calls, not
/// modeled through re-creation.
///
/// `price_snapshot` is persisted (a fresh `price_snapshots` row + its
/// `price_snapshot_items`) in the same transaction as `order` itself --
/// `order.price_snapshot_id` must equal `price_snapshot.id`. Reuses the
/// existing `PriceSnapshot`/`PriceSnapshotLine` shape (the same one
/// `BuildPlanRevision.snapshot` already produces via
/// `IndustryService::calculate_build_snapshot_with_coverage`), rather than inventing an
/// Order-specific pricing shape.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NewOrder {
    pub order: Order,
    pub price_snapshot: crate::PriceSnapshot,
    pub requirements: Vec<NewOrderRequirement>,
}

/// Everything `create_order_plan` persists atomically. One
/// [`NewPlanTicket`] per active production operation the whole-tree freeze
/// found (the root, and every Build/Reaction descendant not pruned as
/// fully-covered) -- ordered so a ticket's `parent_occurrence_key` (via its
/// `NewTicket::occurrence_key`) always names an entry earlier in the list,
/// letting the repository resolve `parent_ticket_id` from the just-inserted
/// parent's real `TicketId` without a second pass.
#[derive(Debug, Clone, PartialEq)]
pub struct NewOrderPlan {
    pub order: Order,
    pub price_snapshot: crate::PriceSnapshot,
    pub operations: Vec<NewPlanOperation>,
    /// Every frozen requirement at any tree depth -- `NewOrderRequirement`
    /// unchanged in shape, `operation_occurrence_key` places each row.
    pub requirements: Vec<NewOrderRequirement>,
    pub tickets: Vec<NewPlanTicket>,
    /// `Some` reserves every requirement's frozen `reused_quantity` from
    /// free stock in the same transaction (all or nothing,
    /// [`OrderError::ReservationShortfall`] otherwise); `None` persists
    /// the plan inventory-neutral.
    pub reservation: Option<NewPlanReservation>,
}

/// How to order a new plan's reservations: the plan's operation stages
/// (`OperationDag::stages`), so requirements are served earliest consumer
/// first (see `order::plan_epic_reservations`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewPlanReservation {
    pub stages: std::collections::BTreeMap<String, u32>,
}

/// One generated ticket within a whole-tree plan, paired with the
/// `occurrence_key` of the operation it was generated from (so the
/// repository can resolve `NewTicket::parent_ticket_id` by occurrence
/// rather than requiring the caller to pre-know not-yet-assigned
/// `TicketId`s).
#[derive(Debug, Clone, PartialEq)]
pub struct NewPlanTicket {
    pub ticket: NewTicket,
    /// This ticket's own operation occurrence (mirrors
    /// `NewTicket::occurrence_key`, kept alongside for clarity at the call
    /// site -- the repository reads `ticket.occurrence_key` itself).
    pub parent_occurrence_key: Option<String>,
}

/// [`OrderRepository::create_order_plan`]'s result -- everything the caller
/// (the `create_order` route) needs to build its response, already
/// resolved (real `TicketId`s, `display_id`s, timestamps).
#[derive(Debug, Clone, PartialEq)]
pub struct OrderPlanResult {
    pub order: Order,
    pub requirements: Vec<OrderRequirement>,
    pub operations: Vec<PlanOperation>,
    pub tickets: Vec<Ticket>,
    /// The allocations written for [`NewOrderPlan::reservation`], in
    /// serving order; empty when the plan reserved nothing.
    pub reservations: Vec<PlannedReservation>,
}

/// One frozen material need for a Manufacturing/Reaction ticket.
///
/// The **root** Epic ticket's prerequisites mirror the Epic's own
/// requirements (same `create_order` calculation), so they carry the Epic's
/// Model-B scope/reuse snapshot. A prerequisite frozen by a later child
/// ticket passes `fulfillment_scope: Full` / `reused_quantity: 0` -- netting
/// a child ticket's own linked-Build coverage is a deliberate follow-up.
/// Persisting any of this is inventory-neutral, exactly as before.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NewTicketPrerequisite {
    pub id: TicketPrerequisiteId,
    pub type_id: i64,
    pub captured_name: String,
    /// See `TicketPrerequisite::kind`/`source_build_id`.
    pub kind: RequirementKind,
    pub source_build_id: Option<BuildId>,
    pub required_quantity: u64,
    /// The frozen scope. `Full` for a child ticket's prerequisites (see the
    /// type doc); the Epic's own scope for the root ticket's prerequisites.
    pub fulfillment_scope: FulfillmentScope,
    /// Frozen intended inventory reuse. Storage derives
    /// `fresh_quantity = required_quantity - reused_quantity`.
    pub reused_quantity: u64,
    pub estimated_unit_cost: Option<Money>,
    pub estimated_line_total: Option<Money>,
    /// Frozen expected cost of the `reused_quantity` portion.
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

/// Excludes `status`/`display_id`/timestamps -- the repository assigns
/// `display_id` from the shared `ticket_display_id_seq`, stamps
/// `created_at`/`updated_at`, and always starts `status` at
/// `TicketStatus::Todo`. Unresolved prerequisites do not change the
/// starting status; they only surface in the derived `derive_ticket_blockers`
/// list.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NewTicket {
    pub id: TicketId,
    pub workspace_id: WorkspaceId,
    pub owner_id: OwnerId,
    /// Explicit organizational Epic membership -- `Some` for a ticket
    /// generated as work belonging to an Order (a requirement ticket, or
    /// the root Manufacturing/Reaction ticket), `None` for a standalone
    /// ticket. This is the authoritative answer to "which Order/Epic
    /// contains this ticket?" -- see `Ticket::order_id`.
    pub order_id: Option<OrderId>,
    pub kind: TicketKind,
    /// `None` only for `TicketKind::Generic`.
    pub type_id: Option<i64>,
    /// See `Ticket::captured_name` -- this ticket's title.
    pub captured_name: String,
    /// `None` only for `TicketKind::Generic`.
    pub quantity: Option<u64>,
    pub source_build_id: Option<BuildId>,
    pub estimated_unit_cost: Option<Money>,
    pub estimated_line_total: Option<Money>,
    /// See `Ticket::market_region_id`/`market_location_id`.
    pub market_region_id: Option<i64>,
    pub market_location_id: Option<i64>,
    /// See `Ticket::price_source_id`.
    pub price_source_id: Option<PriceSourceId>,
    /// See `Ticket::notes`.
    pub notes: String,
    /// See `Ticket::assignee_character_id`.
    pub assignee_character_id: Option<ConnectedCharacterId>,
    pub execution_snapshot: Option<TaskExecutionSnapshot>,
    pub prerequisites: Vec<NewTicketPrerequisite>,
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

/// A purely organizational, metadata-only Ticket update -- title, notes,
/// Epic membership, and assignee. Every field is independently
/// three-valued: `None` means "leave unchanged", `Some(None)` means "clear
/// it" (only meaningful for `order_id`/`assignee_character_id`, the two
/// nullable relationships), `Some(Some(x))` means "set it to `x`".
/// Deliberately never touches `status`, `recording`, `execution_snapshot`,
/// `source_build_id`, requirement fulfillments, or AcquisitionRun
/// membership -- moving a Ticket between Epics or reassigning it is a bare
/// metadata write, nothing else.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct TicketMetadataUpdate {
    pub captured_name: Option<String>,
    pub notes: Option<String>,
    pub order_id: Option<Option<OrderId>>,
    pub assignee_character_id: Option<Option<ConnectedCharacterId>>,
}

/// Already-validated input to `record_ticket_acquisition` -- the route
/// parses `unit_cost`, checks `quantity > 0`, trims the notes, and
/// defaults `effective_at`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RecordAcquisitionInput {
    /// Client-generated; `UNIQUE (ticket_id, idempotency_key)` in storage
    /// makes a replayed request a no-op that returns the prior recording.
    pub idempotency_key: Uuid,
    /// This recording's own amount (`> 0`), never cumulative and never
    /// capped at the ticket's demand.
    pub quantity: u64,
    /// `Some` -> the `Purchase` posts `Known`; `None` -> the cost falls
    /// through the same hierarchy `complete_ticket` uses (ticket estimate,
    /// then current inventory average, else `CostRequired`).
    pub unit_cost: Option<Money>,
    pub location_note: String,
    pub note: String,
    /// The `Purchase` event's `effective_at`.
    pub effective_at: DateTime<Utc>,
}

/// Result of `record_ticket_acquisition`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RecordAcquisitionOutcome {
    pub recording: TicketInventoryRecording,
    /// `true` when this call posted a new `Purchase`; `false` when
    /// `(ticket_id, idempotency_key)` was already recorded and the prior
    /// recording is returned unchanged with nothing posted (the idempotent
    /// replay path -- the route answers `200` rather than `201`).
    pub created: bool,
    pub summary: TicketRecordingSummary,
}

/// One actual material consumed by a `record_ticket_production` call. Only
/// `type_id` is caller-supplied; the type name is resolved server-side
/// from the ticket's frozen prerequisites, and the cost basis from the
/// live inventory ledger.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RecordProductionInputLine {
    pub type_id: i64,
    pub quantity: u64,
}

/// Already-validated input to `record_ticket_production`. The route checks
/// `runs_completed > 0`, every `inputs[].quantity > 0`, parses
/// `installation_cost` (`>= 0`, `0` allowed), trims notes, defaults
/// `effective_at`. `output.quantity == 0` is allowed (a scrapped job).
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RecordProductionInput {
    pub idempotency_key: Uuid,
    pub runs_completed: u64,
    /// Must equal the ticket's product `type_id`.
    pub output_type_id: i64,
    pub output_quantity: u64,
    /// Every `type_id` must be one of the ticket's frozen prerequisites
    /// (its material list); quantities are actuals and are *not* checked
    /// against the plan.
    pub inputs: Vec<RecordProductionInputLine>,
    /// Explicit actual installation cost for this recording -- never
    /// derived server-side from the plan.
    pub installation_cost: Money,
    pub location_note: String,
    pub note: String,
    /// The posted events' `effective_at`.
    pub effective_at: DateTime<Utc>,
    /// Epics the user agreed to take reserved stock from, per type, when
    /// own reservations and free stock fall short (the "Take N from
    /// EP-x?" confirmation). Empty: never take.
    pub take_from: Vec<TakeFrom>,
}

/// Permission to take `type_id` from another Epic's reservations.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct TakeFrom {
    pub order_id: OrderId,
    pub type_id: i64,
}

/// Result of `record_ticket_production`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RecordProductionOutcome {
    pub recording: TicketInventoryRecording,
    /// `false` on an idempotent replay (nothing posted); route answers
    /// `200` rather than `201`.
    pub created: bool,
    pub summary: TicketRecordingSummary,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RevertTicketInventoryRecordingOutcome {
    pub recording: TicketInventoryRecording,
    pub summary: TicketRecordingSummary,
}

#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    #[error("order was not found")]
    OrderNotFound,
    #[error("order requirement was not found")]
    OrderRequirementNotFound,
    #[error("ticket was not found")]
    TicketNotFound,
    /// A ticket create/update named an Epic, character, or price source
    /// that doesn't belong to this workspace (or doesn't exist).
    #[error("the Epic, character, or price source this ticket references was not found")]
    TicketReferenceNotFound,
    #[error("ticket prerequisite was not found")]
    TicketPrerequisiteNotFound,
    #[error("allocated quantity must be a positive whole number")]
    InvalidQuantity,
    #[error("a ticket title is required and cannot be blank")]
    InvalidTicketTitle,
    #[error("the selected Build's recipe does not match the requested ticket kind")]
    TicketKindDoesNotMatchBuildRecipe,
    #[error("ticket's type does not match the requirement/prerequisite it would fulfill")]
    TicketTypeMismatch,
    #[error("order is already started, completed, or canceled")]
    OrderNotStartable,
    #[error("order must be in progress, and not already completed or canceled, to complete")]
    OrderNotCompletable,
    #[error("ticket is not ready to start")]
    TicketNotStartable,
    #[error("ticket is not in progress")]
    TicketNotCompletable,
    #[error("order is already completed or canceled")]
    OrderNotCancelable,
    #[error("order is already archived")]
    OrderNotArchivable,
    #[error("order is not archived")]
    OrderNotRestorable,
    #[error("ticket is already complete or canceled")]
    TicketNotCancelable,
    #[error("ticket is already archived")]
    TicketNotArchivable,
    #[error("ticket is not archived")]
    TicketNotRestorable,
    #[error("quantity arithmetic exceeds the supported range")]
    ArithmeticOverflow,
    /// Reserving a new Epic's frozen reuse needs more of these types than
    /// is free (another Epic reserved it since the preview, or stock was
    /// removed).
    #[error("not enough free inventory to reserve this Epic's planned reuse")]
    ReservationShortfall(Vec<ReservationShortfall>),
    /// Reserving inventory for a completed, canceled or archived Epic.
    #[error("a completed, canceled or archived Epic can't reserve inventory")]
    OrderNotReservable,
    /// Recording needs stock other Epics have reserved (beyond its own
    /// reservations and free stock), and the caller didn't allow taking it.
    #[error("not enough free inventory: other Epics have reserved what this recording needs")]
    InsufficientAvailable(Vec<AvailabilityShortage>),
    /// Only version-3 Epics freeze a whole-tree plan the Plan view can show.
    #[error("this Epic was created before whole-tree plans and has no frozen plan to show")]
    FrozenPlanUnavailable,
    #[error("acquisition run was not found")]
    AcquisitionRunNotFound,
    #[error("an acquisition run needs at least one ticket")]
    AcquisitionRunEmpty,
    #[error("ticket is not a Ready, unbatched Acquisition ticket")]
    TicketNotBatchable,
    #[error("every ticket in an acquisition run must share the same price source")]
    AcquisitionRunCrossesIncompatibleLocation,
    #[error("acquisition run is not ready to start")]
    AcquisitionRunNotReady,
    #[error("acquisition run is not in progress")]
    AcquisitionRunNotInProgress,
    /// No actual price, ticket/run estimate, or existing inventory average
    /// was available to price a Purchase posting -- the cost-resolution
    /// hierarchy's last resort, since a price can never be silently
    /// assumed as zero or unknown.
    #[error("a cost is required to record this purchase: no actual price, estimate, or existing inventory average is available")]
    CostRequired,
    /// `record_ticket_acquisition` was called on a Manufacturing/Reaction
    /// ticket. Only a standalone Acquisition ticket records an acquisition.
    #[error("only an acquisition ticket can record an acquisition")]
    RecordingRequiresAcquisitionTicket,
    /// `record_ticket_acquisition` was called on a ticket batched into an
    /// AcquisitionRun -- the Run's own record/complete flow already posts
    /// that acquisition, and allowing both paths would double-count.
    #[error("this ticket is batched into an acquisition run -- record and complete it from the run instead")]
    RecordingNotAllowedForBatchedTicket,
    /// `record_ticket_production` was called on an Acquisition ticket.
    #[error("only a manufacturing or reaction ticket can record production")]
    RecordingRequiresProductionTicket,
    /// The recorded output `type_id` is not the ticket's product.
    #[error("the recorded output type does not match the ticket's product")]
    RecordingOutputTypeMismatch,
    /// A recorded input `type_id` is not one of the ticket's frozen
    /// prerequisites (its material list).
    #[error("a recorded input is not one of the ticket's materials")]
    RecordingInputNotAPrerequisite,
    #[error("ticket inventory recording was not found")]
    RecordingNotFound,
    #[error("this ticket inventory recording was already reversed")]
    RecordingAlreadyReversed,
    #[error("the ticket inventory recording's ledger evidence is incomplete or inconsistent")]
    RecordingEvidenceInvalid,
    #[error("the ticket inventory recording cannot be reversed against current inventory")]
    RecordingReversalInvalid,
    /// A recorded consumption exceeds the material's on-hand inventory.
    /// The whole recording is rolled back -- negative inventory is not
    /// allowed.
    #[error("not enough of a material is in stock to record this consumption")]
    InsufficientInventory,
    #[error("persistence failed: {0}")]
    Persistence(String),
    /// The canonical planner's operation
    /// graph being frozen is structurally inconsistent (a demand edge names
    /// a missing consumer or boundary, a boundary is claimed by two
    /// producers, the operation DAG has a cycle or more than one root, a
    /// producer yields less than its consumers need, ...). Corruption, never
    /// incompleteness: Create Epic refuses rather than freezing a plan a
    /// later reader could not reproduce.
    #[error("the production graph is corrupt: {detail}")]
    CorruptProductionGraph { detail: String },
    /// A frozen producer operation's cost
    /// shares do not conserve its total
    /// (`total != sum(consumed shares) + retained surplus basis`).
    #[error("frozen production cost does not conserve for {occurrence_key}")]
    FrozenCostNotConserved { occurrence_key: String },
    /// A version-3 requirement's frozen
    /// producer ticket no longer exists or was canceled. The operation was
    /// frozen once for every consumer it serves, so re-planning it live for
    /// one consumer would duplicate it; the caller must recreate the Epic.
    #[error("the frozen producer ticket for {occurrence_key} is unavailable -- recreate the Epic to re-plan it")]
    FrozenProducerTicketUnavailable { occurrence_key: String },
}

/// Pure persistence for the Order/Ticket domain (`crate::order`) --
/// creation, reads, and the full start/complete/cancel/archive lifecycle.
/// **No Order lifecycle operation mutates inventory**: `create_order`
/// reserves nothing, `start_order`/`complete_order`/`cancel_order`/
/// `archive_order`/`restore_order` are organizational timestamp writes
/// only. Ticket creation is likewise inventory-neutral. The only
/// execution/accounting paths are the explicit Ticket recordings
/// (`record_ticket_acquisition` / `record_ticket_production`).
///
/// `create_order` persists the caller's frozen Model-B inventory-reuse
/// snapshot (`NewOrderRequirement::fulfillment_scope` / `reused_quantity` /
/// `reused_line_total`) as-is. That is still not a reservation: it reads
/// nothing from and writes nothing to `inventory_allocations` /
/// `inventory_events` / `inventory_balances`. A non-zero frozen
/// `reused_quantity` is planning evidence -- two Epics may freeze the same
/// stock, by design, until a future reconciliation/commitment feature.
///
/// `start_order`/`complete_order`/`start_ticket`/`complete_ticket` only
/// enforce *stored*-state preconditions (the relevant timestamps/status
/// column) -- they do **not** re-derive `OrderStatus`/requirement
/// fulfillment. A caller must check `derive_order_status(...) ==
/// OrderStatus::Ready` itself before calling `start_order` (mirrored for
/// `Ticket::status == Ready` before `start_ticket`); this trait has no
/// way to compute that without re-fetching and re-deriving everything the
/// caller likely already has in hand, and duplicating the derivation here
/// would risk it drifting from `order::derive_order_status` itself.
#[async_trait::async_trait]
pub trait OrderRepository: Send + Sync {
    /// Fails with `TicketReferenceNotFound` unless every given id belongs to
    /// `workspace_id`. Ticket foreign keys are single-column, so this is
    /// what keeps a ticket from referencing another workspace's rows.
    async fn verify_ticket_references(
        &self,
        workspace_id: WorkspaceId,
        order_id: Option<OrderId>,
        assignee_character_id: Option<ConnectedCharacterId>,
        price_source_id: Option<PriceSourceId>,
    ) -> Result<(), OrderError>;

    async fn create_order(
        &self,
        new_order: NewOrder,
    ) -> Result<(Order, Vec<OrderRequirement>), OrderError>;
    /// The whole-tree counterpart of `create_order` +
    /// `create_ticket` combined -- one atomic transaction persisting the
    /// Order, its price snapshot, every frozen `PlanOperation`, every
    /// frozen `OrderRequirement` at any tree depth, and every generated
    /// Ticket (the root, and one per active Build/Reaction descendant)
    /// with its own frozen `TicketPrerequisite`s. Either the whole plan
    /// lands, or none of it does -- unlike the separate `create_order` +
    /// `create_ticket` sequence (two transactions), there is no
    /// window where an Order can exist without its root ticket.
    ///
    /// Never posts an `inventory_events` row or changes a balance. With
    /// [`NewOrderPlan::reservation`] it locks the reused types' balances
    /// (sorted `type_id` order), measures free stock under that lock and
    /// writes one `epic_create` allocation per reusing requirement, or
    /// fails with [`OrderError::ReservationShortfall`] and persists
    /// nothing.
    async fn create_order_plan(
        &self,
        new_plan: NewOrderPlan,
    ) -> Result<OrderPlanResult, OrderError>;
    async fn get_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Order, OrderError>;
    async fn list_orders(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
    ) -> Result<Vec<Order>, OrderError>;
    async fn list_order_requirements(
        &self,
        order_id: OrderId,
    ) -> Result<Vec<OrderRequirement>, OrderError>;

    /// Every frozen [`PlanOperation`] for a whole-tree (version-2)
    /// Epic, at any depth -- empty for a version-1 Epic. Used by
    /// verification/tests and the Board/Epic UI to read the frozen operation
    /// tree back.
    async fn list_order_plan_operations(
        &self,
        order_id: OrderId,
    ) -> Result<Vec<PlanOperation>, OrderError>;

    /// Resolves the `PriceSource` backing an already-captured price
    /// snapshot -- used to freeze `Ticket::price_source_id` at ticket
    /// creation time from the creating Order's own snapshot. `None` if the
    /// snapshot itself has no resolvable source (e.g. a fully manual
    /// pricing policy with no `PriceSource` selected).
    async fn get_price_source_for_snapshot(
        &self,
        price_snapshot_id: PriceSnapshotId,
    ) -> Result<Option<PriceSourceId>, OrderError>;

    /// Resolves the market scope frozen on an already-captured price
    /// snapshot's *material* lines -- used to freeze
    /// `Ticket::market_region_id`/`market_location_id` at ticket creation
    /// time (an Acquisition ticket only ever concerns material acquisition,
    /// never output valuation). Every material line in one
    /// snapshot shares the same scope (`BuildPricingConfiguration::material_scope`
    /// is one value for the whole build), so this reads any one of them.
    /// `None` if the snapshot has no material line with a resolved scope at
    /// all (e.g. fully manual pricing).
    async fn get_material_scope_for_snapshot(
        &self,
        price_snapshot_id: PriceSnapshotId,
    ) -> Result<Option<crate::MarketScope>, OrderError>;

    async fn create_ticket(
        &self,
        new_ticket: NewTicket,
    ) -> Result<(Ticket, Vec<TicketPrerequisite>), OrderError>;
    async fn get_ticket(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
    ) -> Result<Ticket, OrderError>;
    async fn list_tickets(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
    ) -> Result<Vec<Ticket>, OrderError>;

    /// Every ticket whose explicit `order_id` is this Order -- the
    /// authoritative answer to "which tickets does this Epic contain?"
    /// since it reads `tickets.order_id` directly, never resolving
    /// membership by joining through `order_requirement_fulfillments` (a
    /// requirement fulfillment answers a different question -- see
    /// `OrderRequirementFulfillment`'s own doc). Scoped by `workspace_id`
    /// for the same defense-in-depth every other lookup here has, even
    /// though `order_id` alone is already unambiguous.
    async fn list_tickets_for_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Vec<Ticket>, OrderError>;
    /// Per requirement of `order_id`: Σ its active (`reserved`) and
    /// consumed allocations. A requirement with no allocation reports
    /// zeros; every requirement of the order is present.
    /// The Epic's explicit "Reserve inventory" (top-up): for each frozen
    /// requirement, reserve up to `reused_quantity - held` (held = active
    /// + consumed) from free stock, in serving order, under the balance
    /// lock. Partial by design; what free stock can't cover comes back as
    /// shortfalls. Version-3, open (not completed, canceled or archived)
    /// Epics only.
    async fn reserve_order_inventory(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<CappedReservationPlan, OrderError>;
    async fn requirement_reservation_totals(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Vec<RequirementReservationTotals>, OrderError>;
    async fn list_ticket_prerequisites(
        &self,
        ticket_id: TicketId,
    ) -> Result<Vec<TicketPrerequisite>, OrderError>;

    /// Idempotent: if a non-canceled fulfillment already links this
    /// requirement to this exact ticket, returns that row instead of
    /// inserting a duplicate.
    async fn link_order_requirement_to_ticket(
        &self,
        order_requirement_id: OrderRequirementId,
        ticket_id: TicketId,
        allocated_quantity: u64,
    ) -> Result<OrderRequirementFulfillment, OrderError>;
    /// Mints `new_ticket` for an Order requirement and links it
    /// (`allocated_quantity`) in **one transaction**: a failed link leaves
    /// no ticket behind. At most one active (non-canceled) ticket can be
    /// minted per requirement -- enforced by the database, so when another
    /// request already holds the requirement (including one racing this
    /// call), nothing is created and `AlreadyLinked` names that ticket.
    async fn create_ticket_for_order_requirement(
        &self,
        order_requirement_id: OrderRequirementId,
        new_ticket: NewTicket,
        allocated_quantity: u64,
    ) -> Result<RequirementTicketCreation, OrderError>;
    async fn list_order_requirement_fulfillments(
        &self,
        order_requirement_id: OrderRequirementId,
    ) -> Result<Vec<OrderRequirementFulfillment>, OrderError>;

    /// Same idempotency rule as `link_order_requirement_to_ticket`.
    async fn link_ticket_prerequisite_to_ticket(
        &self,
        ticket_prerequisite_id: TicketPrerequisiteId,
        fulfilling_ticket_id: TicketId,
        allocated_quantity: u64,
    ) -> Result<TicketPrerequisiteFulfillment, OrderError>;
    async fn list_ticket_prerequisite_fulfillments(
        &self,
        ticket_prerequisite_id: TicketPrerequisiteId,
    ) -> Result<Vec<TicketPrerequisiteFulfillment>, OrderError>;

    /// `available = physical_balance - active allocations`, read-only, no
    /// lock. Neither `create_order` nor `create_ticket` allocates anything,
    /// so for any newly created row this equals the physical balance; a
    /// non-zero active-allocation term can only come from old allocation
    /// rows (released en masse by migration `202609040001`, so in practice
    /// zero).
    async fn available_quantity(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        type_id: i64,
    ) -> Result<u64, OrderError>;

    /// Organizational only: stamps `started_at`. Requires `started_at IS
    /// NULL AND canceled_at IS NULL AND completed_at IS NULL`. Reserves,
    /// consumes, and posts nothing. See the trait's own doc for why the
    /// *readiness* precondition (every requirement satisfied) is the
    /// caller's job, not this method's.
    async fn start_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Order, OrderError>;

    /// Organizational only: stamps `completed_at`. Requires `started_at IS
    /// NOT NULL AND completed_at IS NULL AND canceled_at IS NULL`.
    ///
    /// **Posts no inventory** -- no `Consumption`, no `ProductionOutput`,
    /// no balance/cost-basis change, no ticket mutation, no
    /// recording-completeness check. Releases whatever reservations the
    /// Order still holds, in the same transaction. Final production
    /// output is posted by the root Manufacturing ticket's explicit
    /// `record_ticket_production`. Workflow status and recording state are
    /// independent (Complete Order + unrecorded root ticket, or open Order +
    /// recorded root ticket, are both valid).
    async fn complete_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Order, OrderError>;

    /// `Ready -> InProgress`. Requires `status = 'ready'`.
    async fn start_ticket(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
    ) -> Result<Ticket, OrderError>;

    /// Bare workflow-status write for the Board.
    /// Sets `tickets.status` and `updated_at` and **nothing else**: no
    /// inventory event, no `inventory_allocations` lock/release/consume, no
    /// dependent-ticket cascade, no Order/AcquisitionRun/Build mutation. Any
    /// status may move to any other status -- including *out of*
    /// `Complete`/`Canceled` -- so a dragged card is always reversible.
    /// Dependency/blocker state is a separate, purely-derived concept
    /// (`derive_ticket_blockers`) and is never written here.
    ///
    /// Deliberately **not** a state machine: unlike `start_ticket`/
    /// `complete_ticket`/`cancel_ticket` -- which remain as separate,
    /// explicitly-invoked convenience commands -- this method has no
    /// precondition on the current status and no side effect. Its only
    /// failure is `TicketNotFound` (unknown id, or a
    /// ticket in another workspace). A no-op same-status write succeeds and
    /// returns the current row.
    async fn set_ticket_status(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        status: TicketStatus,
    ) -> Result<Ticket, OrderError>;

    /// Applies a purely organizational metadata patch -- see
    /// `TicketMetadataUpdate`. No status/recording/execution/inventory
    /// side effect of any kind; the only failure is `TicketNotFound`.
    async fn update_ticket_metadata(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        update: TicketMetadataUpdate,
    ) -> Result<Ticket, OrderError>;

    /// `InProgress -> Complete`. Requires `status = 'in_progress'`.
    /// Workflow-only: sets `status`/`updated_at` and **nothing else** --
    /// same no-side-effect contract as `set_ticket_status`, just with this
    /// one precondition on the current status kept as a "boring alias" for
    /// callers relying on it as a lifecycle step. No inventory posting, no
    /// prerequisite/allocation mutation, no dependent-ticket cascade, no
    /// actual-cost inference -- completion does not imply
    /// "this happened," only "I'm marking this done." The only accounting
    /// path is explicit recording (`record_ticket_acquisition`/
    /// `record_ticket_production`), which is fully independent of this call
    /// in both directions: recording before or after completion behaves
    /// identically, and completing never requires a recording to exist
    /// first.
    async fn complete_ticket(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
    ) -> Result<Ticket, OrderError>;

    /// Explicit "I acquired this" recording against a standalone
    /// Acquisition ticket -- the first of the explicit inventory-recording
    /// actions.
    /// In **one transaction**: inserts one immutable
    /// `ticket_inventory_recordings` row, posts exactly one
    /// `InventoryEventKind::Purchase` for `input.quantity` (cost via the
    /// same hierarchy `complete_ticket`'s acquisition path uses:
    /// `input.unit_cost` -> `Known`; else the ticket's `estimated_unit_cost`
    /// -> `Estimated`; else the type's current inventory weighted average
    /// -> `Estimated`; else `CostRequired`), and links the event to the
    /// recording via `inventory_events.ticket_inventory_recording_id`.
    ///
    /// Touches **nothing organizational**: not `status`, not `archived_at`,
    /// not prerequisites, not dependent tickets, no dependent-ticket
    /// cascade, no Build/Order/allocation. Rejects a
    /// Manufacturing/Reaction ticket (`RecordingRequiresAcquisitionTicket`)
    /// and one batched into an AcquisitionRun
    /// (`RecordingNotAllowedForBatchedTicket`). Idempotent on
    /// `(ticket_id, idempotency_key)`: a replay posts nothing and returns
    /// the prior recording with `created = false`. Surplus past the
    /// ticket's demand is allowed and never capped.
    async fn record_ticket_acquisition(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        input: RecordAcquisitionInput,
    ) -> Result<RecordAcquisitionOutcome, OrderError>;

    /// Explicit "I produced this" recording against a Manufacturing or
    /// Reaction ticket (an explicit inventory-recording action).
    /// In **one transaction**: inserts one immutable
    /// `ticket_inventory_recordings` row (`kind = 'production'`), posts one
    /// `InventoryEventKind::Consumption` per `input` at the material's
    /// current inventory weighted average, then -- when `output_quantity >
    /// 0` -- one `InventoryEventKind::ProductionOutput` for the produced
    /// units whose `total_cost_delta` is the **production batch basis**
    /// (`Σ abs(consumption.total_cost_delta) + installation_cost`, `Known`
    /// quality: both parts are exact), and links every event to the
    /// recording via `inventory_events.ticket_inventory_recording_id`.
    /// All produced units, including unavoidable surplus, receive the same
    /// per-unit share of that basis.
    ///
    /// Validates: ticket kind is Manufacturing/Reaction
    /// (`RecordingRequiresProductionTicket`); `output_type_id` equals the
    /// ticket's product (`RecordingOutputTypeMismatch`); every input
    /// `type_id` is one of the ticket's frozen prerequisites
    /// (`RecordingInputNotAPrerequisite`) -- quantities are actuals and are
    /// **not** checked against the plan; each consumption fits on-hand
    /// stock (`InsufficientInventory`, whole recording rolled back).
    ///
    /// Touches **nothing organizational**: not `status`, `archived_at`,
    /// prerequisites, dependent tickets, or any Build/Order/allocation.
    /// Idempotent on `(ticket_id, idempotency_key)`.
    async fn record_ticket_production(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        input: RecordProductionInput,
    ) -> Result<RecordProductionOutcome, OrderError>;

    /// Reverts one immutable ticket recording by appending exact
    /// compensating events for its linked ledger evidence in one transaction.
    async fn revert_ticket_inventory_recording(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        recording_id: TicketInventoryRecordingId,
    ) -> Result<RevertTicketInventoryRecordingOutcome, OrderError>;

    /// Every `ticket_inventory_recordings` row for `ticket_id`, oldest
    /// first -- the read side of the derived `TicketRecordingSummary`.
    async fn list_ticket_inventory_recordings(
        &self,
        ticket_id: TicketId,
    ) -> Result<Vec<TicketInventoryRecording>, OrderError>;

    /// Organizational only: stamps `canceled_at`. Valid any time before
    /// `completed_at` (requires `completed_at IS NULL AND canceled_at IS
    /// NULL`) -- a purely stored-state precondition. Releases the Order's
    /// active reservations in the same transaction (no ledger event) and
    /// never touches linked tickets -- a shared ticket may still be needed
    /// by other work, and Order status never controls Ticket status.
    async fn cancel_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Order, OrderError>;

    /// Orthogonal to workflow state -- valid from any status, requires
    /// only `archived_at IS NULL`. Releases the Order's active
    /// reservations in the same transaction; `restore_order` does not
    /// re-reserve.
    async fn archive_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Order, OrderError>;

    /// Requires `archived_at IS NOT NULL`.
    async fn restore_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Order, OrderError>;

    /// Permanently delete an Epic/Order and its frozen plan. Member tickets
    /// survive with `order_id = NULL`; their recordings and inventory history
    /// are independent domain facts. Unlike `cancel_order` / `archive_order`
    /// this removes the row. `OrderNotFound` if it does not exist in the
    /// workspace.
    async fn delete_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<(), OrderError>;

    /// Valid from `Todo | InProgress` (requires `status IN ('todo',
    /// 'in_progress')`). Workflow-only, like `set_ticket_status`: sets
    /// `status = 'canceled'`/`updated_at` and **nothing else**. Touches no
    /// inventory (a Ticket owns no reservations, so there is nothing to
    /// release), no recording, no prerequisite/allocation row, and no
    /// *other* ticket's status. "Canceled" means "I am no longer
    /// tracking this work as active," not "undo accounting/history" --
    /// explicit accounting reversal, if ever added, is a separate,
    /// deliberately invoked operation.
    async fn cancel_ticket(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
    ) -> Result<Ticket, OrderError>;

    /// Orthogonal to `status` -- valid from any status, requires only
    /// `archived_at IS NULL`.
    async fn archive_ticket(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
    ) -> Result<Ticket, OrderError>;

    /// Requires `archived_at IS NOT NULL`.
    async fn restore_ticket(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
    ) -> Result<Ticket, OrderError>;

    /// Permanently delete a ticket -- including an acquisition ("shopping
    /// trip") ticket -- and its workflow-owned rows: prerequisite/requirement
    /// fulfillments, `ticket_prerequisites` (cascade), and an
    /// `acquisition_runs` row it solely owns (its items cascade). Historical
    /// inventory recordings retain the original Ticket UUID and survive.
    /// Its `order_id` link, if any, just goes away. Unlike
    /// `cancel_ticket` / `archive_ticket` this removes the row.
    /// `TicketNotFound` if it does not exist in the workspace.
    async fn delete_ticket(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
    ) -> Result<(), OrderError>;

    /// Groups `ticket_ids` into one shopping trip, using the
    /// `AcquisitionRun`/`AcquisitionRunItem` domain types and
    /// `acquisition_runs`/`acquisition_run_items` tables. Requires every named ticket to belong to
    /// `workspace_id`/`owner_id`, be `TicketKind::Acquisition`,
    /// `TicketStatus::Todo`, not already batched, and share one
    /// compatibility key -- `(market_region_id, market_location_id)` when
    /// the ticket has a resolved market scope, else `price_source_id` for
    /// a manually-priced ticket -- else
    /// `TicketNotBatchable`/`AcquisitionRunCrossesIncompatibleLocation`. A
    /// ticket with neither a scope nor a `price_source_id` can never be
    /// batched. A Run only ever references tickets from this table.
    async fn create_order_acquisition_run(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        name: Option<String>,
        ticket_ids: Vec<TicketId>,
    ) -> Result<AcquisitionRun, OrderError>;

    async fn list_order_acquisition_runs(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
    ) -> Result<Vec<AcquisitionRun>, OrderError>;

    async fn get_order_acquisition_run(
        &self,
        workspace_id: WorkspaceId,
        run_id: AcquisitionRunId,
    ) -> Result<AcquisitionRun, OrderError>;

    async fn list_order_acquisition_run_tickets(
        &self,
        workspace_id: WorkspaceId,
        run_id: AcquisitionRunId,
    ) -> Result<Vec<Ticket>, OrderError>;

    async fn list_order_acquisition_run_items(
        &self,
        workspace_id: WorkspaceId,
        run_id: AcquisitionRunId,
    ) -> Result<Vec<AcquisitionRunItem>, OrderError>;

    /// `Ready -> InProgress` for the Run itself, and nothing else. Member
    /// Tickets' workflow `status` is user-controlled and is never touched
    /// here -- the Run and the Ticket lane are independent axes. There is no
    /// preview/confirm step: `Order` status is fully *derived* from
    /// requirement/ticket state (no stored status, no lock transition), so
    /// there is nothing to warn about -- this starts directly. Never mutates any Order.
    async fn start_order_acquisition_run(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        run_id: AcquisitionRunId,
    ) -> Result<AcquisitionRun, OrderError>;

    /// Upserts each item's real, uncapped acquired total into
    /// `acquisition_run_items` (an update, never a delta -- repeated edits
    /// are never double-counted) and distributes it across member tickets'
    /// `acquired_quantity` (capped per ticket at its own `fresh_quantity`,
    /// capped in total at the type's summed demand -- the *recorded* total
    /// itself is never capped, over-acquisition is a valid, honestly
    /// tracked outcome). Requires the run `InProgress`. Posts **no**
    /// inventory event and satisfies **no** downstream prerequisite --
    /// recording progress is informational execution state only; the
    /// material may still be in transit. That boundary only moves at
    /// `complete_order_acquisition_run`.
    async fn record_order_acquisition_progress(
        &self,
        workspace_id: WorkspaceId,
        run_id: AcquisitionRunId,
        items: Vec<AcquisitionProgressUpdate>,
    ) -> Result<AcquisitionRun, OrderError>;

    /// Requires the run `InProgress`. An explicit recording/accounting
    /// action only. For each distinct `type_id` among members: posts
    /// exactly one `InventoryEventKind::Purchase` event for the *full*
    /// recorded acquired total (surplus included -- it becomes ordinary
    /// unreserved stock simply by not being allocated to any ticket below),
    /// using `allocate_acquisition_delivery` to split that total across
    /// member tickets' `fresh_quantity` in display-id order and stamp each
    /// member ticket's own `acquired_quantity` with its delivered share.
    /// Then sets the Run's own `status = 'complete'`.
    ///
    /// Does **not** touch any member or dependent Ticket's *workflow*
    /// `status`: that axis is user-controlled and independent of delivery.
    /// It does not complete a "fully delivered" member, does not run any
    /// dependent-ticket cascade, and does not infer status from
    /// `acquired_quantity`. "Run Complete" means this shopping trip is
    /// done, not that every ticket's demand was met or that any ticket's
    /// lane moved. Inserts no reservation-equivalent row: the resulting
    /// stock becomes ordinary `inventory_balances` that later Order/ticket
    /// creation's `available_quantity` calculation already accounts for.
    async fn complete_order_acquisition_run(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        run_id: AcquisitionRunId,
    ) -> Result<AcquisitionRun, OrderError>;
}
