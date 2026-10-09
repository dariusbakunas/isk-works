use super::*;

/// The canonical manual Ticket creation contract -- one endpoint,
/// discriminated by `kind`, rather than a separate route per Ticket kind.
/// Common fields (`capturedName` -- this Ticket's title, `notes`,
/// `orderId`, `assigneeCharacterId`) are always accepted; only the
/// type-specific execution fields differ per variant. Selecting an Epic or
/// Assignee here is purely organizational -- it never creates a
/// requirement fulfillment, reserves inventory, or derives status (see
/// `NewTicket`'s own construction below: no prerequisites, no execution
/// snapshot except where a kind's own recipe demands one).
#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
pub(super) enum CreateTicketRequest {
    #[serde(rename = "generic", rename_all = "camelCase")]
    Generic {
        captured_name: String,
        #[serde(default)]
        notes: String,
        #[serde(default)]
        order_id: Option<uuid::Uuid>,
        #[serde(default)]
        assignee_character_id: Option<uuid::Uuid>,
    },
    /// A manually created Acquisition ticket is immediately valid and
    /// recordable through the existing explicit `record-acquisition` path
    /// -- it needs nothing beyond `type_id`/`quantity`. It carries no
    /// `estimated_unit_cost` (there is no price snapshot to freeze one
    /// from), which `record_ticket_acquisition`'s existing cost hierarchy
    /// already treats as an ordinary, expected case (falls through to the
    /// current inventory average, then `CostRequired` if that's also
    /// unknown). `price_source_id` is optional context only used for
    /// Acquisition Run batching compatibility, never derived or required.
    #[serde(rename = "acquisition", rename_all = "camelCase")]
    Acquisition {
        captured_name: String,
        type_id: i64,
        quantity: u64,
        #[serde(default)]
        notes: String,
        #[serde(default)]
        order_id: Option<uuid::Uuid>,
        #[serde(default)]
        assignee_character_id: Option<uuid::Uuid>,
        #[serde(default)]
        price_source_id: Option<uuid::Uuid>,
    },
    /// Build-backed production tickets. Per the product rule ("Build owns
    /// the production plan; Ticket freezes that plan as intended work"),
    /// the client never sends recipe/ME/TE/facility/material data -- only
    /// which Build to freeze and, optionally, an overridden run count.
    /// Everything else (title, output quantity, execution snapshot,
    /// prerequisites) is derived server-side from the linked Build via the
    /// same `linked_build_plan_for_ticket` seam `create_ticket_for_requirement`
    /// uses for generated tickets. `runs` is never clamped here -- an
    /// out-of-bounds value surfaces the planner's own validation error
    /// rather than silently understating the intended work.
    #[serde(rename = "manufacturing", rename_all = "camelCase")]
    Manufacturing {
        build_id: uuid::Uuid,
        #[serde(default)]
        runs: Option<u64>,
        #[serde(default)]
        notes: String,
        #[serde(default)]
        order_id: Option<uuid::Uuid>,
        #[serde(default)]
        assignee_character_id: Option<uuid::Uuid>,
    },
    /// Same contract as `Manufacturing`, for a Build whose recipe is a
    /// Reaction formula rather than a manufacturing blueprint.
    #[serde(rename = "reaction", rename_all = "camelCase")]
    Reaction {
        build_id: uuid::Uuid,
        #[serde(default)]
        runs: Option<u64>,
        #[serde(default)]
        notes: String,
        #[serde(default)]
        order_id: Option<uuid::Uuid>,
        #[serde(default)]
        assignee_character_id: Option<uuid::Uuid>,
    },
}

pub(super) fn validate_ticket_title(captured_name: String) -> Result<String, ApiError> {
    if captured_name.trim().is_empty() {
        return Err(OrderError::InvalidTicketTitle.into());
    }
    Ok(captured_name)
}

/// The result of freezing a production plan from a Build: everything a
/// Manufacturing/Reaction ticket needs, whether it's about to be persisted
/// (`manual_build_backed_new_ticket`) or only shown as a read-only preview
/// (`preview_ticket_plan`). Both derive it via `resolve_build_backed_plan`
/// so creation and preview can never drift apart.
pub(super) struct BuildBackedPlan {
    build: iskworks_core::Build,
    kind: TicketKind,
    runs: u64,
    type_id: i64,
    captured_name: String,
    quantity: u64,
    prerequisites: Vec<NewTicketPrerequisite>,
    execution_snapshot: TaskExecutionSnapshot,
}

/// Fetches the selected Build and freezes its production plan at `runs`
/// (defaulting to the Build's own `runs` when omitted), via the same
/// `linked_build_plan_for_ticket` seam generated tickets use. The title is
/// derived from the Build's own primary product -- these kinds have no
/// free-form title field, matching how Acquisition derives its title from
/// the selected item rather than accepting one from the client. `runs` is
/// never clamped -- an out-of-bounds value surfaces the planner's own
/// validation error via `calculate_build_snapshot_with_coverage` rather than silently
/// understating the intended work.
///
/// Read-only: calculates against a transient clone of the Build (see
/// `linked_build_plan_for_ticket`) and never mutates the persisted Build.
pub(super) async fn resolve_build_backed_plan(
    state: &AppState,
    workspace_id: iskworks_core::WorkspaceId,
    build_id: uuid::Uuid,
    runs: Option<u64>,
) -> Result<BuildBackedPlan, ApiError> {
    let build = state
        .industry_repository()?
        .get_build(workspace_id, BuildId(build_id))
        .await?;
    let kind = match build.recipe.kind() {
        iskworks_core::BuildRecipeKind::Manufacturing => TicketKind::Manufacturing,
        iskworks_core::BuildRecipeKind::Reaction => TicketKind::Reaction,
    };
    let plan_root = state
        .industry_service()?
        .plan_root_of(workspace_id, build.id)
        .await?;
    let intended_runs = intended_build_backed_runs(plan_root, build.id, build.runs, runs)?;
    let primary_product = build.recipe.primary_product();
    let type_id = primary_product.type_id;
    let captured_name = primary_product.type_name.clone();
    let quantity = planned_root_output(intended_runs, primary_product.quantity_per_run)?;

    let (prerequisites, execution_snapshot) =
        linked_build_plan_for_ticket(state, workspace_id, &build, intended_runs).await?;

    Ok(BuildBackedPlan {
        build,
        kind,
        runs: intended_runs,
        type_id,
        captured_name,
        quantity,
        prerequisites,
        execution_snapshot,
    })
}

/// Builds a `NewTicket` for a manually created Manufacturing/Reaction
/// ticket: freezes the selected Build's production plan (see
/// `resolve_build_backed_plan`) and verifies its recipe kind matches
/// `expected_kind` -- rejecting a Manufacturing request against a Reaction
/// Build and vice versa, never letting the request's `kind` silently
/// override the Build's actual recipe.
///
/// Fully inventory-neutral: this only constructs a `NewTicket` value. No
/// requirement fulfillment, inventory event, or Build mutation is created
/// here or by any caller of this function.
#[allow(clippy::too_many_arguments)]
pub(super) async fn manual_build_backed_new_ticket(
    state: &AppState,
    workspace_id: iskworks_core::WorkspaceId,
    owner_id: iskworks_core::OwnerId,
    expected_kind: TicketKind,
    build_id: uuid::Uuid,
    runs: Option<u64>,
    notes: String,
    order_id: Option<uuid::Uuid>,
    assignee_character_id: Option<uuid::Uuid>,
) -> Result<NewTicket, ApiError> {
    let plan = resolve_build_backed_plan(state, workspace_id, build_id, runs).await?;
    if plan.kind != expected_kind {
        return Err(OrderError::TicketKindDoesNotMatchBuildRecipe.into());
    }

    Ok(NewTicket {
        id: TicketId::new(),
        workspace_id,
        owner_id,
        order_id: order_id.map(OrderId),
        kind: expected_kind,
        type_id: Some(plan.type_id),
        captured_name: plan.captured_name,
        quantity: Some(plan.quantity),
        source_build_id: Some(plan.build.id),
        estimated_unit_cost: None,
        estimated_line_total: None,
        market_region_id: None,
        market_location_id: None,
        price_source_id: None,
        notes,
        assignee_character_id: assignee_character_id.map(ConnectedCharacterId),
        execution_snapshot: Some(plan.execution_snapshot),
        prerequisites: plan.prerequisites,
        occurrence_key: None,
        parent_ticket_id: None,
        produced_quantity: None,
        material_component_cost: None,
        own_installation_cost: None,
        total_production_cost: None,
        plan_evidence: None,
    })
}

#[derive(Debug, Deserialize)]
pub(super) struct TicketPlanPreviewQuery {
    #[serde(default)]
    runs: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TicketPlanPreviewPrerequisite {
    type_id: i64,
    captured_name: String,
    kind: RequirementKind,
    required_quantity: u64,
    estimated_unit_cost: Option<Money>,
    estimated_line_total: Option<Money>,
}

/// A read-only preview of the production plan a Manufacturing/Reaction
/// ticket would freeze if created now from this Build, at `runs` (defaults
/// to the Build's own). Powers the TicketEditor's "plan to freeze" panel --
/// the exact same `linked_build_plan_for_ticket` calculation `POST
/// /api/tickets` uses at creation time, via the shared
/// `resolve_build_backed_plan`, so the preview can never drift from what
/// actually gets frozen. Never mutates the Build.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TicketPlanPreviewResponse {
    kind: TicketKind,
    build_id: uuid::Uuid,
    runs: u64,
    type_id: i64,
    captured_name: String,
    quantity: u64,
    execution_snapshot: TaskExecutionSnapshot,
    prerequisites: Vec<TicketPlanPreviewPrerequisite>,
}

pub(super) async fn preview_ticket_plan(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Query(query): Query<TicketPlanPreviewQuery>,
) -> Result<Json<TicketPlanPreviewResponse>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let plan = resolve_build_backed_plan(&state, workspace_id, build_id, query.runs).await?;
    Ok(Json(TicketPlanPreviewResponse {
        kind: plan.kind,
        build_id,
        runs: plan.runs,
        type_id: plan.type_id,
        captured_name: plan.captured_name,
        quantity: plan.quantity,
        execution_snapshot: plan.execution_snapshot,
        prerequisites: plan
            .prerequisites
            .into_iter()
            .map(|prerequisite| TicketPlanPreviewPrerequisite {
                type_id: prerequisite.type_id,
                captured_name: prerequisite.captured_name,
                kind: prerequisite.kind,
                required_quantity: prerequisite.required_quantity,
                estimated_unit_cost: prerequisite.estimated_unit_cost,
                estimated_line_total: prerequisite.estimated_line_total,
            })
            .collect(),
    }))
}

pub(super) async fn create_ticket_route(
    State(state): State<AppState>,
    Json(request): Json<CreateTicketRequest>,
) -> Result<(StatusCode, Json<Ticket>), ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let repository = state.order_repository()?;

    let new_ticket = match request {
        CreateTicketRequest::Generic {
            captured_name,
            notes,
            order_id,
            assignee_character_id,
        } => NewTicket {
            id: TicketId::new(),
            workspace_id,
            owner_id,
            order_id: order_id.map(OrderId),
            kind: TicketKind::Generic,
            type_id: None,
            captured_name: validate_ticket_title(captured_name)?,
            quantity: None,
            source_build_id: None,
            estimated_unit_cost: None,
            estimated_line_total: None,
            market_region_id: None,
            market_location_id: None,
            price_source_id: None,
            notes,
            assignee_character_id: assignee_character_id.map(ConnectedCharacterId),
            execution_snapshot: None,
            prerequisites: Vec::new(),
            occurrence_key: None,
            parent_ticket_id: None,
            produced_quantity: None,
            material_component_cost: None,
            own_installation_cost: None,
            total_production_cost: None,
            plan_evidence: None,
        },
        CreateTicketRequest::Acquisition {
            captured_name,
            type_id,
            quantity,
            notes,
            order_id,
            assignee_character_id,
            price_source_id,
        } => {
            if type_id <= 0 || quantity == 0 {
                return Err(OrderError::InvalidQuantity.into());
            }
            NewTicket {
                id: TicketId::new(),
                workspace_id,
                owner_id,
                order_id: order_id.map(OrderId),
                kind: TicketKind::Acquisition,
                type_id: Some(type_id),
                captured_name: validate_ticket_title(captured_name)?,
                quantity: Some(quantity),
                source_build_id: None,
                estimated_unit_cost: None,
                estimated_line_total: None,
                market_region_id: None,
                market_location_id: None,
                price_source_id: price_source_id.map(PriceSourceId),
                notes,
                assignee_character_id: assignee_character_id.map(ConnectedCharacterId),
                execution_snapshot: None,
                prerequisites: Vec::new(),
                occurrence_key: None,
                parent_ticket_id: None,
                produced_quantity: None,
                material_component_cost: None,
                own_installation_cost: None,
                total_production_cost: None,
                plan_evidence: None,
            }
        }
        CreateTicketRequest::Manufacturing {
            build_id,
            runs,
            notes,
            order_id,
            assignee_character_id,
        } => {
            manual_build_backed_new_ticket(
                &state,
                workspace_id,
                owner_id,
                TicketKind::Manufacturing,
                build_id,
                runs,
                notes,
                order_id,
                assignee_character_id,
            )
            .await?
        }
        CreateTicketRequest::Reaction {
            build_id,
            runs,
            notes,
            order_id,
            assignee_character_id,
        } => {
            manual_build_backed_new_ticket(
                &state,
                workspace_id,
                owner_id,
                TicketKind::Reaction,
                build_id,
                runs,
                notes,
                order_id,
                assignee_character_id,
            )
            .await?
        }
    };

    repository
        .verify_ticket_references(
            workspace_id,
            new_ticket.order_id,
            new_ticket.assignee_character_id,
            new_ticket.price_source_id,
        )
        .await?;
    let (ticket, _) = repository.create_ticket(new_ticket).await?;
    Ok((StatusCode::CREATED, Json(ticket)))
}

/// An Epic's requirements that hold their whole need in stock (reserved +
/// used), keyed by `(operation_occurrence_key, type_id)` -- the identity a
/// whole-tree ticket's prerequisite copies from its requirement. Empty for
/// an Epic without reservations (versions 1 and 2).
pub(super) async fn held_requirement_keys(
    repository: &dyn OrderRepository,
    workspace_id: iskworks_core::WorkspaceId,
    order_id: OrderId,
) -> Result<HashSet<(String, i64)>, ApiError> {
    let held: HashMap<OrderRequirementId, u64> = repository
        .requirement_reservation_totals(workspace_id, order_id)
        .await?
        .into_iter()
        .map(|totals| (totals.requirement_id, totals.reserved + totals.consumed))
        .collect();
    if held.values().all(|quantity| *quantity == 0) {
        return Ok(HashSet::new());
    }
    Ok(repository
        .list_order_requirements(order_id)
        .await?
        .into_iter()
        .filter(|requirement| {
            held.get(&requirement.id).copied().unwrap_or(0) >= requirement.required_quantity
        })
        .filter_map(|requirement| {
            requirement
                .operation_occurrence_key
                .map(|key| (key, requirement.type_id))
        })
        .collect())
}

/// Builds `derive_ticket_blockers`' input from an already-fetched
/// prerequisite list: for each, its non-canceled fulfillments' identity/
/// status -- the same N+1-acceptable-at-this-scale shape `fetch_order_detail`
/// already uses for `LinkedTicketRef`. A prerequisite whose Epic
/// requirement holds its whole need (`held`, see `held_requirement_keys`)
/// is met: that is how a step's recorded output unblocks its consumer.
pub(super) async fn ticket_blockers(
    repository: &dyn OrderRepository,
    workspace_id: iskworks_core::WorkspaceId,
    held: &HashSet<(String, i64)>,
    prerequisites: &[TicketPrerequisite],
) -> Result<Vec<TicketBlockerRef>, ApiError> {
    let prerequisites: Vec<TicketPrerequisite> = prerequisites
        .iter()
        .filter(|prerequisite| {
            !prerequisite
                .operation_occurrence_key
                .as_ref()
                .is_some_and(|key| held.contains(&(key.clone(), prerequisite.type_id)))
        })
        .cloned()
        .collect();
    let prerequisites = prerequisites.as_slice();
    let mut fulfillments_by_prerequisite = HashMap::with_capacity(prerequisites.len());
    for prerequisite in prerequisites {
        let links = repository
            .list_ticket_prerequisite_fulfillments(prerequisite.id)
            .await?;
        let mut fulfillments = Vec::with_capacity(links.len());
        for link in links {
            let fulfilling_ticket = repository
                .get_ticket(workspace_id, link.fulfilling_ticket_id)
                .await?;
            if fulfilling_ticket.status != TicketStatus::Canceled {
                fulfillments.push((
                    fulfilling_ticket.id,
                    fulfilling_ticket.display_id,
                    fulfilling_ticket.status,
                    link.allocated_quantity,
                ));
            }
        }
        fulfillments_by_prerequisite.insert(prerequisite.id, fulfillments);
    }
    Ok(derive_ticket_blockers(
        prerequisites,
        &fulfillments_by_prerequisite,
    ))
}

/// Deserializes a field as `Some(value)` (`value` itself possibly `null`)
/// when the JSON key is present at all, `None` when the key is absent --
/// the standard "omitted vs explicit null" pattern, since plain
/// `Option<T>` cannot distinguish the two (serde's default treats a
/// missing key and an explicit `null` identically). Used for every
/// nullable metadata field below so a client can omit a field to leave it
/// untouched, or send `null` to explicitly clear it.
pub(super) fn deserialize_present_field<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UpdateTicketRequest {
    /// Present -> `set_ticket_status`, a bare organizational-only write
    /// (see that method's own doc). Absent -> status untouched.
    #[serde(default)]
    status: Option<TicketStatus>,
    /// Present (even as an empty string) -> new title. Absent -> title
    /// untouched. Never nullable -- a Ticket's title is always required.
    #[serde(default)]
    captured_name: Option<String>,
    /// Present -> new notes (an empty string clears them, same as never
    /// having any). Absent -> notes untouched.
    #[serde(default)]
    notes: Option<String>,
    /// Omitted -> Epic membership untouched. `null` -> clear it (no
    /// Epic). A uuid -> set it. This is a bare organizational write --
    /// never a requirement-fulfillment/inventory/status/Build operation
    /// (moving a Ticket between Epics touches nothing else).
    #[serde(default, deserialize_with = "deserialize_present_field")]
    order_id: Option<Option<uuid::Uuid>>,
    /// Same three-value semantics as `order_id`, for the assignee.
    /// Reassigning is metadata-only -- it never touches status, recording,
    /// inventory, Epic, Build, or dependencies.
    #[serde(default, deserialize_with = "deserialize_present_field")]
    assignee_character_id: Option<Option<uuid::Uuid>>,
}

/// A bare workflow-status write, still the *only* way `status` changes --
/// moving a ticket card between Board lanes calls **only** this, never
/// `/start`, `/complete`, or `/cancel`, whose repository behavior posts
/// inventory events and cascades dependent-ticket status. Any persisted
/// status to any other, in either direction.
///
/// Also the canonical organizational metadata-update endpoint (title,
/// notes, Epic, assignee) -- see `UpdateTicketRequest`'s own field docs for
/// the omitted/`null`/set semantics. A request may set `status` and/or any
/// metadata field in the same call; each independently no-ops when absent.
/// No inventory, allocation, cascade, or Order/Run/Build mutation from
/// either half of this endpoint.
pub(super) async fn update_ticket(
    State(state): State<AppState>,
    Path(ticket_id): Path<uuid::Uuid>,
    Json(request): Json<UpdateTicketRequest>,
) -> Result<Json<Ticket>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    let repository = state.order_repository()?;
    let ticket_id = TicketId(ticket_id);

    let has_metadata = request.captured_name.is_some()
        || request.notes.is_some()
        || request.order_id.is_some()
        || request.assignee_character_id.is_some();
    if has_metadata {
        let captured_name = request
            .captured_name
            .map(validate_ticket_title)
            .transpose()?;
        repository
            .verify_ticket_references(
                workspace_id,
                request.order_id.flatten().map(OrderId),
                request
                    .assignee_character_id
                    .flatten()
                    .map(ConnectedCharacterId),
                None,
            )
            .await?;
        repository
            .update_ticket_metadata(
                workspace_id,
                ticket_id,
                TicketMetadataUpdate {
                    captured_name,
                    notes: request.notes,
                    order_id: request.order_id.map(|value| value.map(OrderId)),
                    assignee_character_id: request
                        .assignee_character_id
                        .map(|value| value.map(ConnectedCharacterId)),
                },
            )
            .await?;
    }
    let ticket = match request.status {
        Some(status) => {
            repository
                .set_ticket_status(workspace_id, ticket_id, status)
                .await?
        }
        None => repository.get_ticket(workspace_id, ticket_id).await?,
    };
    Ok(Json(ticket))
}

pub(super) async fn start_ticket(
    State(state): State<AppState>,
    Path(ticket_id): Path<uuid::Uuid>,
) -> Result<Json<iskworks_core::order::Ticket>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    Ok(Json(
        state
            .order_repository()?
            .start_ticket(workspace_id, iskworks_core::order::TicketId(ticket_id))
            .await?,
    ))
}

/// Workflow-only -- see `OrderRepository::complete_ticket`'s own doc. Takes
/// no body: completion prices and posts nothing, so there is nothing to
/// pass in. Any JSON a legacy caller still sends (e.g. a
/// stale `actualUnitCost`) is simply never read, not rejected.
pub(super) async fn complete_ticket(
    State(state): State<AppState>,
    Path(ticket_id): Path<uuid::Uuid>,
) -> Result<Json<iskworks_core::order::Ticket>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    Ok(Json(
        state
            .order_repository()?
            .complete_ticket(workspace_id, iskworks_core::order::TicketId(ticket_id))
            .await?,
    ))
}

/// Permanently delete a ticket -- including an acquisition ("shopping trip")
/// ticket -- with its workflow-owned rows. Historical inventory recordings
/// and ledger events survive. `204` on success, `404` if it does not exist.
pub(super) async fn delete_ticket(
    State(state): State<AppState>,
    Path(ticket_id): Path<uuid::Uuid>,
) -> Result<StatusCode, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    state
        .order_repository()?
        .delete_ticket(workspace_id, iskworks_core::order::TicketId(ticket_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn cancel_ticket(
    State(state): State<AppState>,
    Path(ticket_id): Path<uuid::Uuid>,
) -> Result<Json<iskworks_core::order::Ticket>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    Ok(Json(
        state
            .order_repository()?
            .cancel_ticket(workspace_id, iskworks_core::order::TicketId(ticket_id))
            .await?,
    ))
}

pub(super) async fn archive_ticket(
    State(state): State<AppState>,
    Path(ticket_id): Path<uuid::Uuid>,
) -> Result<Json<iskworks_core::order::Ticket>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    Ok(Json(
        state
            .order_repository()?
            .archive_ticket(workspace_id, iskworks_core::order::TicketId(ticket_id))
            .await?,
    ))
}

pub(super) async fn restore_ticket(
    State(state): State<AppState>,
    Path(ticket_id): Path<uuid::Uuid>,
) -> Result<Json<iskworks_core::order::Ticket>, ApiError> {
    let (workspace_id, _owner_id) = workspace_context(&state).await?;
    Ok(Json(
        state
            .order_repository()?
            .restore_ticket(workspace_id, iskworks_core::order::TicketId(ticket_id))
            .await?,
    ))
}
