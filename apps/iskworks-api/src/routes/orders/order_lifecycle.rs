use super::*;

/// Whole-tree Epic freeze. `command` is the *same*
/// `PreviewBuildPlanCommand` overlay body Materials/Graph/Worksheet/
/// cost-projection already take (facility selections, fulfillment scopes,
/// manual prices, pricing policy, ...) -- the frontend already maintains
/// this live overlay for those views, and Create Epic freezes the
/// *current* live plan rather than the persisted Build's last-saved
/// configuration.
///
/// `order_requirements` (every depth) and `order_plan_operations` (root +
/// every active Build/Reaction descendant) come from **one** allocation-aware
/// walk (`OrderPlanCoordinator::freeze`, reusing
/// `materials_with_planning_cost_for_overlay` as-is) -- never a second
/// planning engine, never independent per-child re-derivation.
///
/// The Order's own financial summary (`price_snapshot`,
/// `estimated_material_cost`, `expected_revenue`, `estimated_margin`,
/// `missing_price_count`) and the root ticket's `execution_snapshot`
/// come from `frozen.revision` -- the *same* `OrderPlanCoordinator::freeze`
/// call's sell-side-enriched `BuildPlanRevision`, built from the identical
/// `BuildCostProjection` the whole-tree freeze itself used (see
/// `order_plan.rs`'s own doc). No second, independent planning calculation
/// (e.g. `calculate_epic_snapshot`) runs on this path. The one extra read is `frozen.revision`'s own root-level
/// preview pass (`IndustryService::preview_plan`), needed because
/// `expected_revenue` is sell-side evidence (an output sale price) the
/// materials/cost projection never resolves.
///
/// Only the root operation gets a ticket here (the final product, with
/// the frozen execution snapshot). Every other active operation (each
/// descendant the allocator did not prune as fully-covered) is frozen but
/// has no ticket until one is created on demand
/// (`POST /api/orders/:id/operations/:occurrence_key/ticket`), sized to its
/// own `produced_quantity` and parented to its owning operation's ticket.
/// Acquisition tickets for `Buy` requirements are likewise created on
/// demand through the per-requirement / bulk routes below, which own the
/// Acquisition batching-key semantics.
///
/// Whole-tree shared `PlanningInventory`: guaranteed
/// by construction -- `freeze_order_plan` reduces the *one* allocation-aware
/// walk's own output, and that walk already draws every boundary from one
/// shared pool (`PlanningInventory`), so a physical stock split
/// across root + child can never be double-counted here.
pub(super) async fn create_order(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(request): Json<CreateOrderRequest>,
) -> Result<Response, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let build_id = BuildId(build_id);
    let build = state
        .industry_repository()?
        .get_build(workspace_id, build_id)
        .await?;

    let frozen = state
        .order_plan_coordinator()?
        .freeze(workspace_id, owner_id, build_id, request.command)
        .await?;

    // Reservation drift: the client confirmed the reuse it previewed. Less
    // reuse now means more to buy or build than the user agreed to, so
    // refuse with the fresh preview; more reuse is strictly less work, so
    // proceed and report it.
    let reuse_increased = match &request.reservation {
        Some(reservation) => {
            let expected =
                reservation
                    .expected_reuse
                    .iter()
                    .fold(BTreeMap::new(), |mut reuse, line| {
                        *reuse.entry(line.type_id).or_insert(0_u64) += line.quantity;
                        reuse
                    });
            let comparison = compare_reuse(&expected, &reuse_by_type(&frozen.requirements));
            if !comparison.decreased.is_empty() {
                return Ok(reservation_drift_response(
                    EpicReusePreview::of(&frozen.requirements),
                    comparison.decreased,
                    Vec::new(),
                ));
            }
            comparison.increased
        }
        None => Vec::new(),
    };
    // Every Epic reserves what its frozen plan reuses: an Epic that froze
    // reuse without holding it would let the next plan count the same
    // stock (the double-planning reservations exist to prevent).
    let preview = EpicReusePreview::of(&frozen.requirements);
    let root_operation = frozen
        .operations
        .first()
        .ok_or_else(|| {
            ApiError::Industry(iskworks_core::IndustryError::Validation(
                "build plan produced no root operation".to_string(),
            ))
        })?
        .clone();

    // `frozen.revision` is the one sell-side-enriched
    // `BuildPlanRevision` `OrderPlanCoordinator::freeze` already built from
    // the *same* `BuildCostProjection` the whole-tree freeze itself used
    // (see `order_plan.rs`'s own doc) -- no second, independent
    // `calculate_epic_snapshot` call. Its `estimated_material_cost` /
    // `missing_price_count` / `pricing_complete` / `estimated_margin` /
    // facility installation cost are all root-`PlanOperation`-derived;
    // `expected_revenue` (and `snapshot`/`blueprint`/`facility` identity)
    // come from that one root-level preview's own live-overlay-driven
    // resolution, never the persisted Build's last-saved configuration.
    let root_execution_snapshot = frozen.revision.to_task_execution_snapshot();
    let root_kind = match build.recipe {
        BuildRecipe::Manufacturing(_) => TicketKind::Manufacturing,
        BuildRecipe::Reaction(_) => TicketKind::Reaction,
    };
    debug_assert!(
        matches!(root_kind, TicketKind::Manufacturing)
            == matches!(root_operation.activity, MaterialActivity::Manufacturing),
        "root operation activity must match the source Build's own recipe kind"
    );

    let now = Utc::now();
    let order = Order {
        id: OrderId::new(),
        workspace_id,
        owner_id,
        source_build_id: Some(build.id),
        source_build_revision: build.revision,
        display_name: format!("Manufacture {}", build.name),
        runs: root_operation.runs,
        recipe_fingerprint: frozen.revision.recipe_fingerprint.clone(),
        price_snapshot_id: frozen.revision.snapshot.id,
        estimated_material_cost: frozen.revision.estimated_material_cost,
        expected_revenue: frozen.revision.expected_revenue,
        estimated_margin: frozen.revision.estimated_margin,
        missing_price_count: frozen.revision.missing_price_count,
        created_at: now,
        updated_at: now,
        started_at: None,
        completed_at: None,
        canceled_at: None,
        archived_at: None,
        // Planning snapshot version 3: one operation, many requirements.
        // (Versions 1 and 2 are older Epics, still read but never written.)
        planning_snapshot_version: 3,
    };

    // One ticket per active operation (root + every non-fully-covered
    // Build/Reaction descendant), each carrying its own operation's
    // requirements as prerequisites -- duplicated from `frozen.requirements`
    // (new ids), the same "frozen requirement mirrored onto the ticket"
    // convention the root ticket already used.
    let mut plan_tickets = Vec::with_capacity(frozen.operations.len());
    // Only the root is ticketed up front; the other steps' tickets are
    // created on demand from the frozen operations.
    for operation in frozen
        .operations
        .iter()
        .filter(|operation| operation.occurrence_key.starts_with(ROOT_OCCURRENCE_PREFIX))
    {
        let prerequisites: Vec<NewTicketPrerequisite> = frozen
            .requirements
            .iter()
            .filter(|requirement| {
                requirement.operation_occurrence_key.as_deref()
                    == Some(operation.occurrence_key.as_str())
            })
            .map(requirement_to_prerequisite)
            .collect();
        let kind = match operation.activity {
            MaterialActivity::Manufacturing => TicketKind::Manufacturing,
            MaterialActivity::Reaction => TicketKind::Reaction,
        };
        // By key, not by a NULL parent: a version-3 fan-in operation has
        // no single parent either.
        let is_root = operation.occurrence_key.starts_with(ROOT_OCCURRENCE_PREFIX);
        let (market_region_id, market_location_id, price_source_id, execution_snapshot) = if is_root
        {
            // The snapshot is not persisted yet at this point (it, the
            // order, and every ticket are all written together in one
            // `create_order_plan` transaction below) -- derive the market
            // scope from the in-memory snapshot's own material lines
            // rather than `get_material_scope_for_snapshot`, which reads
            // `price_snapshot_items` back from storage and would see no
            // rows yet. Same selection rule that query encodes: the first
            // material-role line with a resolved market region.
            let market_scope = frozen
                .revision
                .snapshot
                .items
                .iter()
                .find(|item| {
                    item.item_role == iskworks_core::PlannerItemRole::Material
                        && item.market_region_id.is_some()
                })
                .map(|item| MarketScope {
                    region_id: item.market_region_id.expect("checked by find above"),
                    location_id: item.market_location_id,
                });
            (
                market_scope.map(|scope| scope.region_id),
                market_scope.and_then(|scope| scope.location_id),
                frozen.revision.snapshot.price_source_id,
                Some(root_execution_snapshot.clone()),
            )
        } else {
            (None, None, None, None)
        };

        plan_tickets.push(NewPlanTicket {
            ticket: NewTicket {
                id: TicketId::new(),
                workspace_id,
                owner_id,
                order_id: Some(order.id),
                kind,
                type_id: Some(operation.product_type_id),
                captured_name: operation.product_name.clone(),
                quantity: Some(operation.produced_quantity),
                source_build_id: Some(operation.build_id),
                estimated_unit_cost: None,
                estimated_line_total: operation.material_component_cost,
                market_region_id,
                market_location_id,
                price_source_id,
                notes: String::new(),
                assignee_character_id: None,
                execution_snapshot,
                prerequisites,
                occurrence_key: Some(operation.occurrence_key.clone()),
                parent_ticket_id: None, // resolved by the repository from `parent_occurrence_key`
                produced_quantity: Some(operation.produced_quantity),
                material_component_cost: operation.material_component_cost,
                own_installation_cost: operation.own_installation_cost,
                total_production_cost: operation.total_production_cost,
                plan_evidence: Some(operation.evidence.clone()),
            },
            parent_occurrence_key: operation.parent_occurrence_key.clone(),
        });
    }

    // The repository resolves `parent_ticket_id` against tickets inserted
    // earlier, so consumers must precede producers: a canonical DAG's op
    // order need not, so order it by descending dependency stage (root
    // first).
    let dag = derive_operation_dag(
        frozen
            .operations
            .iter()
            .map(|operation| operation.occurrence_key.as_str()),
        frozen.requirements.iter().filter_map(|requirement| {
            Some(FrozenDemandEdge {
                consumer: requirement.operation_occurrence_key.as_deref()?,
                producer: requirement.child_occurrence_key.as_deref()?,
                dependency_id: requirement.dependency_id.as_deref(),
            })
        }),
    )?;
    plan_tickets.sort_by_key(|planned| {
        let key = planned.ticket.occurrence_key.clone().unwrap_or_default();
        (
            std::cmp::Reverse(dag.stages.get(&key).copied().unwrap_or_default()),
            key,
        )
    });
    let repository = state.order_repository()?;
    let plan_result = repository
        .create_order_plan(NewOrderPlan {
            order,
            price_snapshot: frozen.revision.snapshot,
            operations: frozen.operations,
            requirements: frozen.requirements,
            tickets: plan_tickets,
            reservation: Some(NewPlanReservation {
                stages: dag.stages.clone(),
            }),
        })
        .await;
    let plan_result = match plan_result {
        Ok(plan_result) => plan_result,
        Err(OrderError::ReservationShortfall(shortfalls)) => {
            return Ok(reservation_drift_response(preview, Vec::new(), shortfalls));
        }
        Err(error) => return Err(error.into()),
    };

    // A freshly created Order has no fulfillment links yet by construction
    // (`create_order_plan` links requirements to tickets only through
    // `operation_occurrence_key`/prerequisites, never through
    // `order_requirement_fulfillments`), so every requirement's state is
    // InventorySatisfied or NeedsAction -- no need to query fulfillments
    // back out.
    let empty_fulfillments = vec![Vec::new(); plan_result.requirements.len()];
    let production_plan = production_plan_view(
        plan_result.operations,
        &plan_result.requirements,
        &plan_result.tickets,
    )?;
    let mut response = order_detail_response(
        &plan_result.order,
        plan_result.requirements,
        empty_fulfillments,
    );
    response.production_plan = production_plan;
    response.reuse_increased = reuse_increased;
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

/// `POST /api/builds/:build_id/orders` body: the live overlay command
/// (flattened, so a bare command still works) plus, optionally, the reuse
/// the client previewed. The Epic always reserves its frozen reuse.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CreateOrderRequest {
    #[serde(flatten)]
    command: PreviewBuildPlanCommand,
    /// The previewed reuse to hold the freeze to (less reuse now -> 409
    /// drift). Absent: nothing to compare; the Epic still reserves, and a
    /// shortfall under the lock is still a 409.
    #[serde(default)]
    reservation: Option<CreateOrderReservation>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CreateOrderReservation {
    /// The per-type reuse the client previewed and the user confirmed.
    #[serde(default)]
    expected_reuse: Vec<ExpectedReuse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ExpectedReuse {
    type_id: i64,
    quantity: u64,
}

/// What an Epic created from this overlay right now would reuse from free
/// stock, per type.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EpicReusePreview {
    reuse: Vec<ReuseLine>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReuseLine {
    type_id: i64,
    type_name: String,
    quantity: u64,
}

impl EpicReusePreview {
    fn of(requirements: &[NewOrderRequirement]) -> Self {
        let names: BTreeMap<i64, &str> = requirements
            .iter()
            .map(|requirement| (requirement.type_id, requirement.captured_name.as_str()))
            .collect();
        Self {
            reuse: reuse_by_type(requirements)
                .into_iter()
                .map(|(type_id, quantity)| ReuseLine {
                    type_id,
                    type_name: names.get(&type_id).copied().unwrap_or_default().to_string(),
                    quantity,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ReservationDriftEnvelope {
    error: ReservationDriftBody,
}

/// 409 when the Epic can't reserve what the user confirmed. Carries the
/// fresh preview so the client can refresh without another request.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReservationDriftBody {
    code: &'static str,
    message: &'static str,
    retryable: bool,
    preview: EpicReusePreview,
    /// Types the Epic would now reuse less of than previewed.
    decreased: Vec<ReuseChange>,
    /// Types another writer reserved or removed since the freeze.
    shortfalls: Vec<ReservationShortfall>,
}

fn reservation_drift_response(
    preview: EpicReusePreview,
    decreased: Vec<ReuseChange>,
    shortfalls: Vec<ReservationShortfall>,
) -> Response {
    // Same `{ "error": ... }` envelope as every other API error, so
    // clients read it through their usual error path.
    (
        StatusCode::CONFLICT,
        Json(ReservationDriftEnvelope {
            error: ReservationDriftBody {
            code: "reservation_drift",
            message: "Free inventory changed since the preview. Review the updated reuse and confirm again.",
            retryable: true,
            preview,
            decreased,
            shortfalls,
        },
        }),
    )
        .into_response()
}

/// `POST /api/builds/:build_id/orders/preview`: run the same freeze Create
/// Epic would, persist nothing, and return the per-type reuse the dialog
/// shows (and later sends back as `expectedReuse`).
pub(super) async fn preview_order(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(command): Json<PreviewBuildPlanCommand>,
) -> Result<Json<EpicReusePreview>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let build_id = BuildId(build_id);
    state
        .industry_repository()?
        .get_build(workspace_id, build_id)
        .await?;
    let frozen = state
        .order_plan_coordinator()?
        .freeze(workspace_id, owner_id, build_id, command)
        .await?;
    Ok(Json(EpicReusePreview::of(&frozen.requirements)))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OrderSummaryResponse {
    #[serde(flatten)]
    order: Order,
    status: OrderStatus,
    rollup: OrderRequirementRollup,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum ArchivedFilter {
    #[default]
    Active,
    Archived,
    All,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListOrdersQuery {
    #[serde(default)]
    archived: ArchivedFilter,
}

/// Board-scoped listing: the derived Epic `status`/`rollup` every card
/// needs (`OrderStatus` is still the Epic-lifecycle enum -- Blocked/Ready
/// there mean "requirements unmet/met", a separate axis from a Ticket's
/// user-controlled workflow status), without the full
/// `requirements`/`linkedTickets` detail
/// only the Order detail page needs. Reuses `fetch_order_detail` per order
/// rather than a separate derivation path -- N+1 queries, acceptable at
/// this app's single-workspace scale (same precedent as the existing
/// Board's "fetch everything once" ticket/run loading).
///
/// `archived` filters in-memory (small, personal-scale dataset, same
/// precedent as `list_tickets`'s own `showBatched` filtering) rather than
/// in SQL -- default `active` (`archivedAt IS NULL`): the Board shows only
/// active work by default.
pub(super) async fn list_orders(
    State(state): State<AppState>,
    Query(query): Query<ListOrdersQuery>,
) -> Result<Json<Vec<OrderSummaryResponse>>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let orders = state
        .order_repository()?
        .list_orders(workspace_id, owner_id)
        .await?;
    let mut summaries = Vec::with_capacity(orders.len());
    for order in &orders {
        let keep = match query.archived {
            ArchivedFilter::Active => order.archived_at.is_none(),
            ArchivedFilter::Archived => order.archived_at.is_some(),
            ArchivedFilter::All => true,
        };
        if !keep {
            continue;
        }
        let detail = fetch_order_detail(&state, workspace_id, order.id).await?;
        summaries.push(OrderSummaryResponse {
            order: detail.order,
            status: detail.status,
            rollup: detail.rollup,
        });
    }
    Ok(Json(summaries))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OrderTicketSummary {
    #[serde(flatten)]
    ticket: Ticket,
    /// Every currently-unmet prerequisite of this ticket -- the derived
    /// dependency/blocker read model, computed regardless of the ticket's
    /// own workflow `status` (a `Complete` ticket with an unmet
    /// prerequisite still lists it). Empty for Acquisition/Generic
    /// tickets, which have no prerequisites.
    blocked_by: Vec<TicketBlockerRef>,
    /// This ticket's own frozen material need -- empty for Acquisition,
    /// populated for Manufacturing/Reaction regardless of status (the
    /// Required Materials table stays visible after the ticket unblocks).
    prerequisites: Vec<TicketPrerequisite>,
    /// Derived accounting/execution state from the ticket's explicit
    /// inventory recordings -- `Some` for Acquisition/Manufacturing/
    /// Reaction, `None` for Generic (which has no recording contract at
    /// all -- absence here means "not applicable", not "missing").
    /// Orthogonal to `status`: a `complete` ticket may still be
    /// `notRecorded`.
    recording: Option<TicketRecordingSummary>,
    recordings: Vec<TicketInventoryRecording>,
}

/// Every standalone `order::Ticket` the caller owns. Fetches everything and
/// lets the Board filter client-side, matching the same "small,
/// personal-scale dataset" convention used elsewhere -- no `archived`
/// query param here either.
pub(super) async fn list_order_tickets(
    State(state): State<AppState>,
) -> Result<Json<Vec<OrderTicketSummary>>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let repository = state.order_repository()?;
    let tickets = repository.list_tickets(workspace_id, owner_id).await?;

    let mut summaries = Vec::with_capacity(tickets.len());
    for ticket in tickets {
        let prerequisites = repository.list_ticket_prerequisites(ticket.id).await?;
        // Derived independently of `ticket.status` -- dependency state is
        // reported, workflow state is user-controlled.
        let blocked_by = ticket_blockers(&*repository, workspace_id, &prerequisites).await?;
        // Derived recording summary. Acquisition: requested = ticket
        // quantity, recorded = Σ recorded_quantity. Manufacturing/Reaction:
        // requested = execution_snapshot.runs (frozen at creation for the
        // ticket's intended output), recorded = Σ runs_completed. A null
        // snapshot means a legacy ticket created before the plan was frozen
        // -- it falls back to the recorded amount, so its state can only be
        // NotRecorded or Recorded, never Partial. N+1 over the small
        // per-workspace set, same convention as `blocked_by`.
        let recordings = repository
            .list_ticket_inventory_recordings(ticket.id)
            .await?;
        let recording = match ticket.kind {
            TicketKind::Acquisition => {
                let recorded: u64 = recordings
                    .iter()
                    .filter(|recording| recording.reverted_at.is_none())
                    .filter_map(|recording| recording.recorded_quantity)
                    .sum();
                // `quantity` is guaranteed `Some` for every Acquisition
                // ticket (`tickets_quantity_required_unless_generic`).
                Some(derive_recording_summary(
                    ticket.quantity.unwrap_or_default(),
                    recorded,
                ))
            }
            TicketKind::Manufacturing | TicketKind::Reaction => {
                let recorded: u64 = recordings
                    .iter()
                    .filter(|recording| recording.reverted_at.is_none())
                    .filter_map(|recording| recording.runs_completed)
                    .sum();
                let requested = ticket
                    .execution_snapshot
                    .as_ref()
                    .map_or(recorded, |snapshot| snapshot.runs);
                Some(derive_recording_summary(requested, recorded))
            }
            // Generic has no recording contract at all -- "not recorded"
            // would misleadingly imply something is missing.
            TicketKind::Generic => None,
        };
        summaries.push(OrderTicketSummary {
            ticket,
            blocked_by,
            prerequisites,
            recording,
            recordings,
        });
    }
    Ok(Json(summaries))
}

pub(super) async fn get_order(
    State(state): State<AppState>,
    Path(order_id): Path<uuid::Uuid>,
) -> Result<Json<OrderDetailResponse>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let response = fetch_order_detail(&state, workspace_id, OrderId(order_id)).await?;
    Ok(Json(response))
}

/// Shared by `get_order` and the lifecycle actions below, which all need
/// the same derived `OrderStatus` -- `start_order` to guard `Ready`,
/// `complete_order` to guard `InProgress`, and both to return the
/// post-transition detail view without a second, separate response-shape
/// computation.
pub(super) async fn fetch_order_detail(
    state: &AppState,
    workspace_id: iskworks_core::WorkspaceId,
    order_id: OrderId,
) -> Result<OrderDetailResponse, ApiError> {
    let repository = state.order_repository()?;
    let order = repository.get_order(workspace_id, order_id).await?;
    let requirements = repository.list_order_requirements(order_id).await?;
    let order_tickets = repository
        .list_tickets_for_order(workspace_id, order_id)
        .await?;

    let mut fulfillments_per_requirement = Vec::with_capacity(requirements.len());
    for requirement in &requirements {
        let links = repository
            .list_order_requirement_fulfillments(requirement.id)
            .await?;
        let mut linked = Vec::with_capacity(links.len());
        for link in links {
            let ticket = repository.get_ticket(workspace_id, link.ticket_id).await?;
            if ticket.status != TicketStatus::Canceled {
                linked.push(LinkedTicketRef {
                    id: ticket.id,
                    display_id: ticket.display_id,
                    status: ticket.status,
                    allocated_quantity: link.allocated_quantity,
                    producer: false,
                });
            }
        }
        // A produced (Build/React) requirement is fulfilled by its frozen
        // producer step's ticket, linked by occurrence key rather than a
        // fulfillment row: count it, so the requirement reads as in hand
        // (and later satisfied) once the step has a ticket.
        if requirement.kind != RequirementKind::Buy {
            if let Some(producer) = requirement.child_occurrence_key.as_deref().and_then(|key| {
                order_tickets.iter().find(|ticket| {
                    ticket.occurrence_key.as_deref() == Some(key)
                        && ticket.status != TicketStatus::Canceled
                })
            }) {
                if !linked.iter().any(|link| link.id == producer.id) {
                    linked.push(LinkedTicketRef {
                        id: producer.id,
                        display_id: producer.display_id.clone(),
                        status: producer.status,
                        allocated_quantity: requirement.fresh_quantity,
                        producer: true,
                    });
                }
            }
        }
        fulfillments_per_requirement.push(linked);
    }

    let operations = repository.list_order_plan_operations(order_id).await?;
    let production_plan = if operations.is_empty() {
        None
    } else {
        production_plan_view(operations, &requirements, &order_tickets)?
    };
    let inventory = if order.planning_snapshot_version >= 3 {
        let mut summary = EpicInventorySummary {
            planned_reuse: requirements.iter().map(|r| r.reused_quantity).sum(),
            ..EpicInventorySummary::default()
        };
        let held: HashMap<OrderRequirementId, u64> = repository
            .requirement_reservation_totals(workspace_id, order_id)
            .await?
            .into_iter()
            .map(|totals| {
                summary.reserved += totals.reserved;
                summary.used += totals.consumed;
                (totals.requirement_id, totals.reserved + totals.consumed)
            })
            .collect();
        // Units of different items don't add up to anything meaningful;
        // count items (requirements reusing stock) instead.
        for requirement in requirements.iter().filter(|r| r.reused_quantity > 0) {
            summary.items_planned += 1;
            if held.get(&requirement.id).copied().unwrap_or(0) >= requirement.reused_quantity {
                summary.items_held += 1;
            }
        }
        Some(summary)
    } else {
        None
    };
    let mut response = order_detail_response(&order, requirements, fulfillments_per_requirement);
    response.production_plan = production_plan;
    response.inventory = inventory;
    Ok(response)
}

/// Starting is only valid once every requirement is satisfied
/// (`OrderStatus::Ready`) -- a *derived* precondition the repository
/// itself can't check (see `OrderRepository`'s own doc), so this route
/// computes the same detail view `get_order` would return and guards on
/// its `status` before calling the repository's purely stored-state
/// transition.
pub(super) async fn start_order(
    State(state): State<AppState>,
    Path(order_id): Path<uuid::Uuid>,
) -> Result<Json<OrderDetailResponse>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let order_id = OrderId(order_id);
    let detail = fetch_order_detail(&state, workspace_id, order_id).await?;
    if detail.status != OrderStatus::Ready {
        return Err(iskworks_core::order::OrderError::OrderNotStartable.into());
    }
    state
        .order_repository()?
        .start_order(workspace_id, order_id)
        .await?;
    Ok(Json(
        fetch_order_detail(&state, workspace_id, order_id).await?,
    ))
}

/// Completing is only valid from `OrderStatus::InProgress` (same
/// derived-precondition reasoning as `start_order`). It is a **purely
/// organizational** transition -- it stamps `completed_at` and nothing
/// else. Final production output is posted by the root Manufacturing
/// ticket's explicit `record-production`, never by Order completion, so
/// this route consumes no requirements, does not touch
/// `inventory_allocations`, and posts no production output. Workflow status and
/// recording state are independent: a Complete Order may have an unrecorded
/// root ticket, and a recorded root ticket may sit under a still-open Order.
pub(super) async fn complete_order(
    State(state): State<AppState>,
    Path(order_id): Path<uuid::Uuid>,
) -> Result<Json<OrderDetailResponse>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let order_id = OrderId(order_id);
    let detail = fetch_order_detail(&state, workspace_id, order_id).await?;
    if detail.status != OrderStatus::InProgress {
        return Err(iskworks_core::order::OrderError::OrderNotCompletable.into());
    }

    state
        .order_repository()?
        .complete_order(workspace_id, order_id)
        .await?;

    Ok(Json(
        fetch_order_detail(&state, workspace_id, order_id).await?,
    ))
}

/// Cancellation is a purely stored-state precondition (see
/// `OrderRepository::cancel_order`'s own doc) -- unlike `start_order`/
/// `complete_order`, no derived-status guard needs computing here first.
pub(super) async fn cancel_order(
    State(state): State<AppState>,
    Path(order_id): Path<uuid::Uuid>,
) -> Result<Json<OrderDetailResponse>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let order_id = OrderId(order_id);
    state
        .order_repository()?
        .cancel_order(workspace_id, order_id)
        .await?;
    Ok(Json(
        fetch_order_detail(&state, workspace_id, order_id).await?,
    ))
}

pub(super) async fn archive_order(
    State(state): State<AppState>,
    Path(order_id): Path<uuid::Uuid>,
) -> Result<Json<OrderDetailResponse>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let order_id = OrderId(order_id);
    state
        .order_repository()?
        .archive_order(workspace_id, order_id)
        .await?;
    Ok(Json(
        fetch_order_detail(&state, workspace_id, order_id).await?,
    ))
}

pub(super) async fn restore_order(
    State(state): State<AppState>,
    Path(order_id): Path<uuid::Uuid>,
) -> Result<Json<OrderDetailResponse>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let order_id = OrderId(order_id);
    state
        .order_repository()?
        .restore_order(workspace_id, order_id)
        .await?;
    Ok(Json(
        fetch_order_detail(&state, workspace_id, order_id).await?,
    ))
}

/// Permanently delete an Epic/Order and its frozen plan. Member tickets are
/// detached and their inventory recordings/history survive. `204` on
/// success, `404` if it does not exist.
pub(super) async fn delete_order(
    State(state): State<AppState>,
    Path(order_id): Path<uuid::Uuid>,
) -> Result<StatusCode, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    state
        .order_repository()?
        .delete_order(workspace_id, OrderId(order_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
