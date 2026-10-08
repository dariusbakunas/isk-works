use super::*;

/// Resolves `PlannedMaterialLine`s into an `OrderRequirement`/
/// `TicketPrerequisite`'s frozen sourcing decision -- `(Buy, None)` unless
/// the row is build-resolved (with single-level dependency scope, a
/// Build-resolved row is satisfied by a single Manufacturing/
/// Reaction ticket for the linked build's own job, never expanded into
/// further rows at this level). `Build` vs `React` is decided by the
/// *linked* build's own recipe kind -- `is_build_resolved` alone doesn't
/// distinguish them. The resolved `Build`'s id is returned alongside its
/// kind and frozen onto the caller's row (`OrderRequirement.source_build_id`
/// / `TicketPrerequisite.source_build_id`) -- callers must not re-resolve
/// this later against the live component graph, which the requirement/
/// prerequisite owner's Build may have changed since.
///
/// One resolver serves every line of one consumer Build: the root plan is
/// loaded lazily, at most once, on the first build-resolved line (a plan
/// with no build-resolved rows never touches the repository).
pub(super) struct RequirementKindResolver<'a> {
    state: &'a AppState,
    workspace_id: iskworks_core::WorkspaceId,
    root_build_id: BuildId,
    plan: Option<Option<iskworks_core::production_dependency::RootPlanRecords>>,
}

impl<'a> RequirementKindResolver<'a> {
    pub(super) fn new(
        state: &'a AppState,
        workspace_id: iskworks_core::WorkspaceId,
        root_build_id: BuildId,
    ) -> Self {
        Self {
            state,
            workspace_id,
            root_build_id,
            plan: None,
        }
    }

    async fn plan(
        &mut self,
    ) -> Result<Option<&iskworks_core::production_dependency::RootPlanRecords>, ApiError> {
        if self.plan.is_none() {
            let repository = self.state.industry_repository()?;
            let records = match repository
                .plan_root_of(self.workspace_id, self.root_build_id)
                .await?
            {
                Some(root) => Some(repository.load_root_plan(self.workspace_id, root).await?),
                None => None,
            };
            self.plan = Some(records);
        }
        Ok(self.plan.as_ref().and_then(Option::as_ref))
    }

    pub(super) async fn kind_for_line(
        &mut self,
        is_build_resolved: bool,
        type_id: i64,
    ) -> Result<(RequirementKind, Option<BuildId>), ApiError> {
        if !is_build_resolved {
            return Ok((RequirementKind::Buy, None));
        }
        let root_build_id = self.root_build_id;
        // The producer is whatever the consumer's demand edge for this
        // component points at.
        let linked = self.plan().await?.and_then(|records| {
            records
                .dependencies
                .iter()
                .find(|edge| {
                    edge.consumer_build_id == root_build_id && edge.component_type_id == type_id
                })
                .and_then(|edge| edge.producer_build_id)
                .and_then(|producer| records.producers.iter().find(|build| build.id == producer))
        });
        Ok(match linked {
            Some(build) => {
                let kind = match build.recipe {
                    BuildRecipe::Manufacturing(_) => RequirementKind::Build,
                    BuildRecipe::Reaction(_) => RequirementKind::React,
                };
                (kind, Some(build.id))
            }
            // Shouldn't happen (a build-resolved row implies a linked build
            // exists), but default to Build rather than fail the whole
            // Order creation over a display-kind lookup miss. No build id to
            // freeze in this case -- a later ticket-creation attempt for this
            // row will surface an honest error instead of silently guessing one.
            None => (RequirementKind::Build, None),
        })
    }
}

/// The frozen production plan a Manufacturing/Reaction ticket carries: its
/// `TicketPrerequisite`-shaped material list *and* the `TaskExecutionSnapshot`
/// persisted on the ticket, both derived from a single **coverage-aware**
/// snapshot pass (`BuildPreviewCoordinator::calculate_epic_snapshot`) over
/// the *linked* build.
///
/// The pass runs against a clone of the linked build with `runs` overridden
/// to `intended_runs` -- the run count that actually produces this ticket's
/// output demand -- so every runs-dependent figure (durations, installation
/// cost, material value, prerequisite quantities) is frozen for *this
/// ticket*, not for the linked Build's live `runs`, which may differ and may
/// change later. Each material row's own `Buy`/`Build`/`React` sourcing is
/// resolved and frozen via `RequirementKindResolver` rooted at the linked
/// build, exactly the treatment `create_order` gives an `OrderRequirement`
/// one level up.
///
/// **Inventory-aware snapshotting applies recursively here.** Every prerequisite is netted against
/// the linked Build's *own* live `ProductionRepository::coverage`, honoring
/// that Build's own persisted `fulfillment_scopes` -- so a child production
/// ticket's frozen prerequisites represent remaining work after planned
/// inventory reuse, exactly like the root Epic's requirements. This reads
/// inventory only: no `inventory_allocations`, `inventory_events`, or balance
/// change (`calculate_epic_snapshot`'s own contract).
///
/// Known limitation (reported, not fixed here): coverage is resolved
/// per-Build with no aggregate deduction, so a raw material consumed *both*
/// by the root recipe and by this linked Build can be frozen as `reused` in
/// both places within one Epic -- optimistic planning evidence, never a
/// reservation. Execution stays correct: `record_ticket_production` draws
/// from the live ledger and rejects a consumption that exceeds on-hand
/// stock. A within-Epic aggregate allocator is a separate design.
pub(super) async fn linked_build_plan_for_ticket(
    state: &AppState,
    workspace_id: iskworks_core::WorkspaceId,
    linked_build: &iskworks_core::Build,
    intended_runs: u64,
) -> Result<(Vec<NewTicketPrerequisite>, TaskExecutionSnapshot), ApiError> {
    let mut plan_build = linked_build.clone();
    plan_build.runs = intended_runs;
    let snapshot = state
        .build_materials_coordinator()?
        .ticket_snapshot(workspace_id, &plan_build)
        .await?;
    let mut prerequisites = Vec::with_capacity(snapshot.material_lines.len());
    let mut kinds = RequirementKindResolver::new(state, workspace_id, linked_build.id);
    for line in &snapshot.material_lines {
        let (kind, source_build_id) = kinds
            .kind_for_line(line.is_build_resolved, line.type_id)
            .await?;
        let (fulfillment_scope, reused_quantity, reused_line_total) = frozen_reuse(line);
        prerequisites.push(NewTicketPrerequisite {
            id: TicketPrerequisiteId::new(),
            type_id: line.type_id,
            captured_name: line.type_name.clone(),
            kind,
            source_build_id,
            required_quantity: line.total_quantity,
            fulfillment_scope,
            reused_quantity,
            estimated_unit_cost: line.unit_price,
            estimated_line_total: line.line_total,
            reused_line_total,
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
        });
    }
    Ok((prerequisites, snapshot.to_task_execution_snapshot()))
}

/// Idempotent: if a non-canceled ticket already covers this requirement's
/// `fresh_quantity` (or is at least linked and not canceled), returns that
/// ticket instead of creating a duplicate -- mirroring
/// `OrderRepository::link_order_requirement_to_ticket`'s own idempotency
/// one level up, since a bulk-create retry or a double-click shouldn't
/// spawn a second ticket for the same requirement.
pub(super) async fn create_ticket_for_requirement(
    state: &AppState,
    workspace_id: iskworks_core::WorkspaceId,
    owner_id: iskworks_core::OwnerId,
    order: &Order,
    requirement: &OrderRequirement,
    market_scope: Option<MarketScope>,
    price_source_id: Option<iskworks_core::PriceSourceId>,
) -> Result<Ticket, ApiError> {
    let repository = state.order_repository()?;
    let existing_links = repository
        .list_order_requirement_fulfillments(requirement.id)
        .await?;
    for link in &existing_links {
        let ticket = repository.get_ticket(workspace_id, link.ticket_id).await?;
        if ticket.status != TicketStatus::Canceled {
            return Ok(ticket);
        }
    }

    // A whole-tree-frozen (version-2) requirement's Build/
    // React production ticket, if active, was already created eagerly by
    // `create_order_plan` -- linked by `occurrence_key`, never by
    // `order_requirement_fulfillments` (that join table only ever links a
    // *Buy* ticket to the requirement it fulfills). Without this check,
    // this route would silently mint a SECOND ticket for the same
    // operation, re-netting against *live* inventory instead of the
    // frozen plan -- exactly the double-counted-reuse bug the whole-tree
    // freeze exists to prevent. Reuse the existing ticket instead of
    // creating a duplicate.
    if let Some(child_occurrence_key) = &requirement.child_occurrence_key {
        let existing = repository
            .list_tickets_for_order(workspace_id, requirement.order_id)
            .await?
            .into_iter()
            .find(|ticket| {
                ticket.occurrence_key.as_deref() == Some(child_occurrence_key.as_str())
                    && ticket.status != TicketStatus::Canceled
            });
        if let Some(ticket) = existing {
            return Ok(ticket);
        }
    }

    if requirement.fresh_quantity == 0 {
        // Fully inventory-covered (`InventorySatisfied`) -- no ticket is
        // needed at all, and `tickets.quantity` must be positive besides.
        // Callers that create tickets in bulk skip these requirements
        // instead of surfacing this as an error.
        return Err(OrderError::InvalidQuantity.into());
    }

    // A version-3 Build/React requirement's
    // producer was frozen exactly once, as one operation serving every
    // requirement that names it. Its ticket is that operation's; minting a
    // per-requirement ticket from a live re-plan (below) would duplicate
    // the operation and re-net against live inventory. Refuse instead.
    if order.planning_snapshot_version >= 3 && requirement.kind != RequirementKind::Buy {
        return Err(OrderError::FrozenProducerTicketUnavailable {
            occurrence_key: requirement
                .child_occurrence_key
                .clone()
                .unwrap_or_else(|| format!("requirement:{}", requirement.id.0)),
        }
        .into());
    }
    let quantity = requirement.fresh_quantity;
    // The ticket represents only the fresh (still-to-source) portion, so its
    // estimate is that portion's own cost -- the requirement's blended
    // `estimated_line_total` minus the frozen inventory `reused_line_total`,
    // never the blend (which also priced stock this ticket won't buy).
    // Degenerates to the full estimate for a `Full`-scoped requirement or one
    // with nothing reused (`reused_line_total` is then `None`).
    let fresh_line_total = match (
        requirement.estimated_line_total,
        requirement.reused_line_total,
    ) {
        (Some(total), Some(reused)) => Some(total.checked_sub(reused)?),
        (total, _) => total,
    };
    let fresh_unit_cost = fresh_line_total
        .map(|total| total.checked_div_quantity(quantity))
        .transpose()?;
    // Reads the sourcing decision frozen on the requirement at Order-creation
    // time -- never re-resolves it against the Order's *current* build graph,
    // which may have changed since (e.g. the component was relinked to a
    // different build). See `RequirementKindResolver`'s own doc.
    let (kind, source_build_id, prerequisites, execution_snapshot) = match requirement.kind {
        RequirementKind::Buy => (TicketKind::Acquisition, None, Vec::new(), None),
        RequirementKind::Build | RequirementKind::React => {
            let source_build_id = requirement
                .source_build_id
                .ok_or(OrderError::OrderRequirementNotFound)?;
            let linked = state
                .industry_repository()?
                .get_build(workspace_id, source_build_id)
                .await?;
            let kind = match linked.recipe {
                BuildRecipe::Manufacturing(_) => TicketKind::Manufacturing,
                BuildRecipe::Reaction(_) => TicketKind::Reaction,
            };
            // Freeze the plan for *this ticket's* output demand rather than the
            // linked Build's live `runs`: the ticket must yield `quantity` units
            // of `requirement.type_id`, i.e. `intended_runs` runs of a recipe
            // producing `quantity_per_run` each. Clamped to the planner's own
            // 1..=1_000_000 run bound so an outsized order can't make snapshot
            // calculation fail outright (the degenerate cap is a rare,
            // acceptable understatement of a snapshot that has no UI yet).
            let per_run = linked.recipe.primary_product().quantity_per_run.max(1);
            let intended_runs = quantity.div_ceil(per_run).clamp(1, 1_000_000);
            let (prerequisites, snapshot) =
                linked_build_plan_for_ticket(state, workspace_id, &linked, intended_runs).await?;
            (kind, Some(linked.id), prerequisites, Some(snapshot))
        }
    };

    let new_ticket = NewTicket {
        id: TicketId::new(),
        workspace_id,
        owner_id,
        // Explicit Epic membership -- this ticket is generated work
        // belonging to the Order whose requirement it fulfills.
        order_id: Some(requirement.order_id),
        kind,
        type_id: Some(requirement.type_id),
        captured_name: requirement.captured_name.clone(),
        quantity: Some(quantity),
        source_build_id,
        estimated_unit_cost: fresh_unit_cost,
        estimated_line_total: fresh_line_total,
        market_region_id: market_scope.map(|scope| scope.region_id),
        market_location_id: market_scope.and_then(|scope| scope.location_id),
        price_source_id,
        notes: String::new(),
        assignee_character_id: None,
        execution_snapshot,
        prerequisites,
        occurrence_key: None,
        parent_ticket_id: None,
        produced_quantity: None,
        material_component_cost: None,
        own_installation_cost: None,
        total_production_cost: None,
        plan_evidence: None,
    };

    // Create + link in one transaction, guarded by the database's
    // one-active-ticket-per-requirement claim: a request that lost a race
    // to a concurrent create (double-click, bulk + single) gets the
    // winner's ticket back -- the same answer the idempotency check above
    // gives once the winner has committed.
    match repository
        .create_ticket_for_order_requirement(requirement.id, new_ticket, quantity)
        .await?
    {
        RequirementTicketCreation::Created(ticket) => Ok(*ticket),
        RequirementTicketCreation::AlreadyLinked(ticket_id) => {
            Ok(repository.get_ticket(workspace_id, ticket_id).await?)
        }
    }
}

pub(super) async fn create_ticket_for_requirement_route(
    State(state): State<AppState>,
    Path((order_id, requirement_id)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Ticket>), ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let order_id = OrderId(order_id);
    let requirement_id = OrderRequirementId(requirement_id);
    let repository = state.order_repository()?;
    // Workspace-ownership check -- `list_order_requirements` below is only
    // scoped by `order_id`, not `workspace_id`, so this is what stops a
    // caller from reaching another workspace's requirement by id.
    let order = repository.get_order(workspace_id, order_id).await?;
    let market_scope = repository
        .get_material_scope_for_snapshot(order.price_snapshot_id)
        .await?;
    let price_source_id = repository
        .get_price_source_for_snapshot(order.price_snapshot_id)
        .await?;
    let requirement = repository
        .list_order_requirements(order_id)
        .await?
        .into_iter()
        .find(|requirement| requirement.id == requirement_id)
        .ok_or(OrderError::OrderRequirementNotFound)?;

    let ticket = create_ticket_for_requirement(
        &state,
        workspace_id,
        owner_id,
        &order,
        &requirement,
        market_scope,
        price_source_id,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(ticket)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BulkCreateTicketsRequest {
    requirement_ids: Vec<uuid::Uuid>,
}

pub(super) async fn bulk_create_tickets(
    State(state): State<AppState>,
    Path(order_id): Path<uuid::Uuid>,
    Json(request): Json<BulkCreateTicketsRequest>,
) -> Result<Json<Vec<Ticket>>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let order_id = OrderId(order_id);
    let repository = state.order_repository()?;
    // Workspace-ownership check -- see the same note in
    // `create_ticket_for_requirement_route`.
    let order = repository.get_order(workspace_id, order_id).await?;
    let market_scope = repository
        .get_material_scope_for_snapshot(order.price_snapshot_id)
        .await?;
    let price_source_id = repository
        .get_price_source_for_snapshot(order.price_snapshot_id)
        .await?;
    let all_requirements = repository.list_order_requirements(order_id).await?;

    let requested_ids: std::collections::HashSet<_> = request
        .requirement_ids
        .into_iter()
        .map(OrderRequirementId)
        .collect();
    let mut tickets = Vec::with_capacity(requested_ids.len());
    for requirement in all_requirements
        .iter()
        .filter(|requirement| requested_ids.contains(&requirement.id))
    {
        // An `InventorySatisfied` requirement (fully covered by frozen
        // inventory reuse) has nothing left to do: skip it
        // silently rather than let `create_ticket_for_requirement`'s
        // positive-quantity guard fail the whole bulk request. Direct
        // single-ticket creation on such a requirement still returns
        // `InvalidQuantity`.
        if requirement.fresh_quantity == 0 {
            continue;
        }
        let ticket = create_ticket_for_requirement(
            &state,
            workspace_id,
            owner_id,
            &order,
            requirement,
            market_scope,
            price_source_id,
        )
        .await?;
        tickets.push(ticket);
    }
    Ok(Json(tickets))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LinkTicketRequest {
    ticket_id: uuid::Uuid,
}

/// Attaches an *existing* ticket instead of creating a new one -- the
/// mechanism behind e.g. "Isogen x1,400 -- Executing via ACQ-0042".
/// Compatibility is `type_id` equality only (the
/// repository's own `TicketTypeMismatch` guard); a ticket originally
/// created to satisfy a `Build`-kind requirement can still be linked to
/// satisfy a `Buy`-kind one for the same material, deliberately -- how a
/// worksheet row was originally resolved shouldn't block reusing
/// already-existing work for the same underlying material.
pub(super) async fn link_ticket_to_requirement(
    State(state): State<AppState>,
    Path((order_id, requirement_id)): Path<(uuid::Uuid, uuid::Uuid)>,
    Json(request): Json<LinkTicketRequest>,
) -> Result<Json<Ticket>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let order_id = OrderId(order_id);
    let requirement_id = OrderRequirementId(requirement_id);
    let ticket_id = TicketId(request.ticket_id);
    let repository = state.order_repository()?;
    // Workspace-ownership check -- `list_order_requirements` below is only
    // scoped by `order_id`, not `workspace_id`, so this is what stops a
    // caller from reaching another workspace's requirement by id.
    repository.get_order(workspace_id, order_id).await?;

    let requirement = repository
        .list_order_requirements(order_id)
        .await?
        .into_iter()
        .find(|requirement| requirement.id == requirement_id)
        .ok_or(OrderError::OrderRequirementNotFound)?;
    if requirement.fresh_quantity == 0 {
        return Err(OrderError::InvalidQuantity.into());
    }
    let ticket = repository.get_ticket(workspace_id, ticket_id).await?;
    // A Generic ticket has no quantity at all -- it can never fulfill a
    // requirement (which always needs a real item/quantity), same
    // conceptual rejection as a type_id mismatch.
    let ticket_quantity = ticket.quantity.ok_or(OrderError::TicketTypeMismatch)?;
    let allocated_quantity = ticket_quantity.min(requirement.fresh_quantity);

    repository
        .link_order_requirement_to_ticket(requirement_id, ticket_id, allocated_quantity)
        .await?;
    Ok(Json(ticket))
}
