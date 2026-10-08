use super::*;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OrderRequirementView {
    #[serde(flatten)]
    requirement: OrderRequirement,
    pub(super) state: RequirementFulfillmentState,
    /// Every non-canceled ticket fulfilling this requirement -- the data
    /// behind e.g. "Isogen x1,400 -- Executing via ACQ-0042". Usually 0 or
    /// 1 entries, but the shape is a list since the domain model supports
    /// N:1 sharing.
    pub(super) linked_tickets: Vec<LinkedTicketRef>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LinkedTicketRef {
    pub(super) id: TicketId,
    pub(super) display_id: String,
    pub(super) status: TicketStatus,
    pub(super) allocated_quantity: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OrderDetailResponse {
    #[serde(flatten)]
    pub(super) order: Order,
    pub(super) status: OrderStatus,
    pub(super) rollup: OrderRequirementRollup,
    pub(super) requirements: Vec<OrderRequirementView>,
    /// Whole-tree (version 2/3) Epics only: the frozen operation DAG,
    /// re-derived from the frozen requirement relation -- one entry per
    /// frozen operation with its production ticket and every requirement
    /// it serves. Absent for a version-1 Epic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) production_plan: Option<ProductionPlanView>,
    /// Create Epic only: types the new Epic reserved more of than the
    /// client previewed (stock arrived in between). Omitted when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) reuse_increased: Vec<ReuseChange>,
    /// Version-3 Epics: how much of the frozen reuse the Epic holds and
    /// has used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) inventory: Option<EpicInventorySummary>,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EpicInventorySummary {
    /// Σ frozen `reused_quantity`: what the plan meant to take from stock.
    pub(super) planned_reuse: u64,
    /// Σ active reservations.
    pub(super) reserved: u64,
    /// Σ consumed reservations.
    pub(super) used: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProductionPlanView {
    root_occurrence_key: String,
    /// Deterministic build order: `(stage, occurrence_key)`.
    operations: Vec<PlanOperationView>,
    dependencies: Vec<OperationDependency>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PlanOperationView {
    #[serde(flatten)]
    operation: PlanOperation,
    /// Longest producer chain beneath this operation (`0` = consumes no
    /// produced operation). Ordering evidence only -- never readiness.
    stage: u32,
    /// The one production ticket generated for this operation.
    ticket_id: Option<TicketId>,
    ticket_display_id: Option<String>,
    ticket_status: Option<TicketStatus>,
    /// Every frozen requirement this operation serves (its consumers'
    /// rows naming it as `child_occurrence_key`) -- one per demand edge.
    served_requirement_ids: Vec<OrderRequirementId>,
}

/// The frozen operation DAG view of a whole-tree Epic -- `None` for a
/// version-1 Epic (no operations). A corrupt stored relation is a typed
/// `OrderError::CorruptProductionGraph`.
pub(super) fn production_plan_view(
    operations: Vec<PlanOperation>,
    requirements: &[OrderRequirement],
    tickets: &[Ticket],
) -> Result<Option<ProductionPlanView>, OrderError> {
    if operations.is_empty() {
        return Ok(None);
    }
    let dag = derive_operation_dag(
        operations
            .iter()
            .map(|operation| operation.occurrence_key.as_str()),
        requirements.iter().filter_map(|requirement| {
            Some(FrozenDemandEdge {
                consumer: requirement.operation_occurrence_key.as_deref()?,
                producer: requirement.child_occurrence_key.as_deref()?,
                dependency_id: requirement.dependency_id.as_deref(),
            })
        }),
    )?;
    let mut by_key: HashMap<String, PlanOperation> = operations
        .into_iter()
        .map(|operation| (operation.occurrence_key.clone(), operation))
        .collect();
    let views = dag
        .order
        .iter()
        .filter_map(|key| {
            let operation = by_key.remove(key)?;
            let ticket = tickets.iter().find(|ticket| {
                ticket.occurrence_key.as_deref() == Some(key.as_str())
                    && ticket.status != TicketStatus::Canceled
            });
            Some(PlanOperationView {
                stage: dag.stages.get(key).copied().unwrap_or_default(),
                ticket_id: ticket.map(|ticket| ticket.id),
                ticket_display_id: ticket.map(|ticket| ticket.display_id.clone()),
                ticket_status: ticket.map(|ticket| ticket.status),
                served_requirement_ids: {
                    let mut ids: Vec<OrderRequirementId> = requirements
                        .iter()
                        .filter(|requirement| {
                            requirement.child_occurrence_key.as_deref() == Some(key.as_str())
                        })
                        .map(|requirement| requirement.id)
                        .collect();
                    ids.sort_by_key(|id| id.0);
                    ids
                },
                operation,
            })
        })
        .collect();
    Ok(Some(ProductionPlanView {
        root_occurrence_key: dag.root_occurrence_key,
        operations: views,
        dependencies: dag.dependencies,
    }))
}

/// `fulfillments_per_requirement` must have exactly one entry per
/// `requirements` element, in the same order (each entry the requirement's
/// non-canceled linked tickets).
pub(super) fn order_detail_response(
    order: &Order,
    requirements: Vec<OrderRequirement>,
    fulfillments_per_requirement: Vec<Vec<LinkedTicketRef>>,
) -> OrderDetailResponse {
    debug_assert_eq!(requirements.len(), fulfillments_per_requirement.len());
    let states: Vec<RequirementFulfillmentState> = requirements
        .iter()
        .zip(&fulfillments_per_requirement)
        .map(|(requirement, linked)| {
            let fulfillments: Vec<(TicketStatus, u64)> = linked
                .iter()
                .map(|ticket| (ticket.status, ticket.allocated_quantity))
                .collect();
            derive_requirement_state(requirement, &fulfillments)
        })
        .collect();
    let rollup = compute_order_rollup(&states);
    let status = derive_order_status(order, &states);
    let requirement_views = requirements
        .into_iter()
        .zip(states)
        .zip(fulfillments_per_requirement)
        .map(
            |((requirement, state), linked_tickets)| OrderRequirementView {
                requirement,
                state,
                linked_tickets,
            },
        )
        .collect();
    OrderDetailResponse {
        order: order.clone(),
        status,
        rollup,
        requirements: requirement_views,
        production_plan: None,
        reuse_increased: Vec::new(),
        inventory: None,
    }
}
