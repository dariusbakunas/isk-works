use super::*;

/// Inserts one `order_requirements` row from a caller-supplied
/// `NewOrderRequirement` and returns the persisted `OrderRequirement`.
/// Derives `fresh_quantity = required_quantity - reused_quantity` (the
/// `reused + fresh = required` DB CHECK backstops this). Shared by
/// `create_order` (root-only, version-1 shape) and `create_order_plan`
/// (every depth of a whole-tree version-2 freeze) so both paths persist
/// requirements identically.
pub(super) async fn insert_order_requirement(
    tx: &mut Transaction<'_, Postgres>,
    order_id: OrderId,
    requirement: NewOrderRequirement,
) -> Result<OrderRequirement, OrderError> {
    let reused_quantity = requirement
        .reused_quantity
        .min(requirement.required_quantity);
    let fresh_quantity = requirement.required_quantity - reused_quantity;
    let price_evidence = requirement
        .price_evidence
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| OrderError::Persistence(format!("invalid price evidence: {error}")))?;

    sqlx::query(
        r#"
        INSERT INTO order_requirements (
          id, order_id, type_id, captured_name, kind, source_build_id,
          required_quantity, fulfillment_scope, reused_quantity, fresh_quantity,
          estimated_unit_cost, estimated_line_total, reused_line_total,
          operation_occurrence_key, child_occurrence_key, inventory_unit_basis,
          child_produced_quantity, child_consumed_quantity, child_surplus_quantity,
          child_surplus_retained_basis, price_evidence, child_consumed_cost, dependency_id
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23)
        "#,
    )
    .bind(requirement.id.0)
    .bind(order_id.0)
    .bind(requirement.type_id)
    .bind(&requirement.captured_name)
    .bind(requirement_kind_str(requirement.kind))
    .bind(requirement.source_build_id.map(|id| id.0))
    .bind(i64_from_u64(requirement.required_quantity)?)
    .bind(fulfillment_scope_str(requirement.fulfillment_scope))
    .bind(i64_from_u64(reused_quantity)?)
    .bind(i64_from_u64(fresh_quantity)?)
    .bind(requirement.estimated_unit_cost.map(|money| money.0))
    .bind(requirement.estimated_line_total.map(|money| money.0))
    .bind(requirement.reused_line_total.map(|money| money.0))
    .bind(&requirement.operation_occurrence_key)
    .bind(&requirement.child_occurrence_key)
    .bind(requirement.inventory_unit_basis.map(|money| money.0))
    .bind(
        requirement
            .child_produced_quantity
            .map(i64_from_u64)
            .transpose()?,
    )
    .bind(
        requirement
            .child_consumed_quantity
            .map(i64_from_u64)
            .transpose()?,
    )
    .bind(
        requirement
            .child_surplus_quantity
            .map(i64_from_u64)
            .transpose()?,
    )
    .bind(requirement.child_surplus_retained_basis.map(|money| money.0))
    .bind(&price_evidence)
    .bind(requirement.child_consumed_cost.map(|money| money.0))
    .bind(&requirement.dependency_id)
    .execute(&mut **tx)
    .await
    .map_err(map_error)?;

    Ok(OrderRequirement {
        id: requirement.id,
        order_id,
        type_id: requirement.type_id,
        captured_name: requirement.captured_name,
        kind: requirement.kind,
        source_build_id: requirement.source_build_id,
        required_quantity: requirement.required_quantity,
        fulfillment_scope: requirement.fulfillment_scope,
        reused_quantity,
        fresh_quantity,
        estimated_unit_cost: requirement.estimated_unit_cost,
        estimated_line_total: requirement.estimated_line_total,
        reused_line_total: requirement.reused_line_total,
        operation_occurrence_key: requirement.operation_occurrence_key,
        child_occurrence_key: requirement.child_occurrence_key,
        inventory_unit_basis: requirement.inventory_unit_basis,
        child_produced_quantity: requirement.child_produced_quantity,
        child_consumed_quantity: requirement.child_consumed_quantity,
        child_surplus_quantity: requirement.child_surplus_quantity,
        child_surplus_retained_basis: requirement.child_surplus_retained_basis,
        child_consumed_cost: requirement.child_consumed_cost,
        dependency_id: requirement.dependency_id,
        price_evidence: requirement.price_evidence,
    })
}

/// Inserts one `ticket_prerequisites` row from a caller-supplied
/// `NewTicketPrerequisite` and returns the persisted `TicketPrerequisite`.
/// Same derivation/serialization discipline as `insert_order_requirement`;
/// shared by `create_ticket` (a single ticket's own prerequisites) and
/// `create_order_plan` (every ticket in a whole-tree freeze).
pub(super) async fn insert_ticket_prerequisite(
    tx: &mut Transaction<'_, Postgres>,
    ticket_id: TicketId,
    prerequisite: NewTicketPrerequisite,
) -> Result<TicketPrerequisite, OrderError> {
    let reused_quantity = prerequisite
        .reused_quantity
        .min(prerequisite.required_quantity);
    let fresh_quantity = prerequisite.required_quantity - reused_quantity;
    let price_evidence = prerequisite
        .price_evidence
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| OrderError::Persistence(format!("invalid price evidence: {error}")))?;

    sqlx::query(
        r#"
        INSERT INTO ticket_prerequisites (
          id, ticket_id, type_id, captured_name, kind, source_build_id,
          required_quantity, fulfillment_scope, reused_quantity, fresh_quantity,
          estimated_unit_cost, estimated_line_total, reused_line_total,
          operation_occurrence_key, child_occurrence_key, inventory_unit_basis,
          child_produced_quantity, child_consumed_quantity, child_surplus_quantity,
          child_surplus_retained_basis, price_evidence, child_consumed_cost, dependency_id
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23)
        "#,
    )
    .bind(prerequisite.id.0)
    .bind(ticket_id.0)
    .bind(prerequisite.type_id)
    .bind(&prerequisite.captured_name)
    .bind(requirement_kind_str(prerequisite.kind))
    .bind(prerequisite.source_build_id.map(|id| id.0))
    .bind(i64_from_u64(prerequisite.required_quantity)?)
    .bind(fulfillment_scope_str(prerequisite.fulfillment_scope))
    .bind(i64_from_u64(reused_quantity)?)
    .bind(i64_from_u64(fresh_quantity)?)
    .bind(prerequisite.estimated_unit_cost.map(|money| money.0))
    .bind(prerequisite.estimated_line_total.map(|money| money.0))
    .bind(prerequisite.reused_line_total.map(|money| money.0))
    .bind(&prerequisite.operation_occurrence_key)
    .bind(&prerequisite.child_occurrence_key)
    .bind(prerequisite.inventory_unit_basis.map(|money| money.0))
    .bind(
        prerequisite
            .child_produced_quantity
            .map(i64_from_u64)
            .transpose()?,
    )
    .bind(
        prerequisite
            .child_consumed_quantity
            .map(i64_from_u64)
            .transpose()?,
    )
    .bind(
        prerequisite
            .child_surplus_quantity
            .map(i64_from_u64)
            .transpose()?,
    )
    .bind(prerequisite.child_surplus_retained_basis.map(|money| money.0))
    .bind(&price_evidence)
    .bind(prerequisite.child_consumed_cost.map(|money| money.0))
    .bind(&prerequisite.dependency_id)
    .execute(&mut **tx)
    .await
    .map_err(map_error)?;

    Ok(TicketPrerequisite {
        id: prerequisite.id,
        ticket_id,
        type_id: prerequisite.type_id,
        captured_name: prerequisite.captured_name,
        kind: prerequisite.kind,
        source_build_id: prerequisite.source_build_id,
        required_quantity: prerequisite.required_quantity,
        fulfillment_scope: prerequisite.fulfillment_scope,
        reused_quantity,
        fresh_quantity,
        estimated_unit_cost: prerequisite.estimated_unit_cost,
        estimated_line_total: prerequisite.estimated_line_total,
        reused_line_total: prerequisite.reused_line_total,
        operation_occurrence_key: prerequisite.operation_occurrence_key,
        child_occurrence_key: prerequisite.child_occurrence_key,
        inventory_unit_basis: prerequisite.inventory_unit_basis,
        child_produced_quantity: prerequisite.child_produced_quantity,
        child_consumed_quantity: prerequisite.child_consumed_quantity,
        child_surplus_quantity: prerequisite.child_surplus_quantity,
        child_surplus_retained_basis: prerequisite.child_surplus_retained_basis,
        child_consumed_cost: prerequisite.child_consumed_cost,
        dependency_id: prerequisite.dependency_id,
        price_evidence: prerequisite.price_evidence,
    })
}

/// Inserts one `order_plan_operations` row from a caller-supplied
/// `NewPlanOperation` and returns the persisted `PlanOperation`. Shared
/// only by `create_order_plan` (operations only exist on the
/// whole-tree path).
pub(super) async fn insert_plan_operation(
    tx: &mut Transaction<'_, Postgres>,
    order_id: OrderId,
    operation: NewPlanOperation,
    now: DateTime<Utc>,
) -> Result<PlanOperation, OrderError> {
    let activity = match operation.activity {
        MaterialActivity::Manufacturing => "manufacturing",
        MaterialActivity::Reaction => "reaction",
    };
    let evidence = serde_json::to_value(&operation.evidence)
        .map_err(|error| OrderError::Persistence(format!("invalid plan evidence: {error}")))?;

    sqlx::query(
        r#"
        INSERT INTO order_plan_operations (
          id, order_id, occurrence_key, parent_occurrence_key, build_id, activity,
          runs, persisted_runs, product_type_id, product_name, output_per_run,
          produced_quantity, blueprint_or_formula_type_id, material_component_cost,
          own_installation_cost, total_production_cost, complete, evidence, created_at,
          consumed_quantity, surplus_quantity, surplus_retained_basis
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22)
        "#,
    )
    .bind(operation.id.0)
    .bind(order_id.0)
    .bind(&operation.occurrence_key)
    .bind(&operation.parent_occurrence_key)
    .bind(operation.build_id.0)
    .bind(activity)
    .bind(i64_from_u64(operation.runs)?)
    .bind(i64_from_u64(operation.persisted_runs)?)
    .bind(operation.product_type_id)
    .bind(&operation.product_name)
    .bind(i64_from_u64(operation.output_per_run)?)
    .bind(i64_from_u64(operation.produced_quantity)?)
    .bind(operation.blueprint_or_formula_type_id)
    .bind(operation.material_component_cost.map(|money| money.0))
    .bind(operation.own_installation_cost.map(|money| money.0))
    .bind(operation.total_production_cost.map(|money| money.0))
    .bind(operation.complete)
    .bind(&evidence)
    .bind(now)
    .bind(operation.consumed_quantity.map(i64_from_u64).transpose()?)
    .bind(operation.surplus_quantity.map(i64_from_u64).transpose()?)
    .bind(operation.surplus_retained_basis.map(|money| money.0))
    .execute(&mut **tx)
    .await
    .map_err(map_error)?;

    Ok(PlanOperation {
        id: operation.id,
        order_id,
        occurrence_key: operation.occurrence_key,
        parent_occurrence_key: operation.parent_occurrence_key,
        build_id: Some(operation.build_id),
        activity: operation.activity,
        runs: operation.runs,
        persisted_runs: operation.persisted_runs,
        product_type_id: operation.product_type_id,
        product_name: operation.product_name,
        output_per_run: operation.output_per_run,
        produced_quantity: operation.produced_quantity,
        blueprint_or_formula_type_id: operation.blueprint_or_formula_type_id,
        material_component_cost: operation.material_component_cost,
        own_installation_cost: operation.own_installation_cost,
        total_production_cost: operation.total_production_cost,
        complete: operation.complete,
        consumed_quantity: operation.consumed_quantity,
        surplus_quantity: operation.surplus_quantity,
        surplus_retained_basis: operation.surplus_retained_basis,
        evidence: operation.evidence,
        created_at: now,
    })
}

/// Inserts one `tickets` row from a caller-supplied `NewTicket` (its
/// `prerequisites` are persisted separately by the caller via
/// `insert_ticket_prerequisite`) and returns the persisted `Ticket`
/// shell (`prerequisites` not attached -- callers that need them already
/// have the `NewTicketPrerequisite` list). Shared by `create_ticket` and
/// `create_order_plan`.
pub(super) async fn insert_ticket_row(
    tx: &mut Transaction<'_, Postgres>,
    new_ticket: &NewTicket,
    now: DateTime<Utc>,
) -> Result<(String, Option<serde_json::Value>, Option<serde_json::Value>), OrderError> {
    let display_id: String =
        sqlx::query_scalar("SELECT 'ISK-' || nextval('ticket_display_id_seq')")
            .fetch_one(&mut **tx)
            .await
            .map_err(map_error)?;

    let execution_snapshot = new_ticket
        .execution_snapshot
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| {
            OrderError::Persistence(format!("failed to serialize execution snapshot: {error}"))
        })?;
    let plan_evidence = new_ticket
        .plan_evidence
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| {
            OrderError::Persistence(format!("failed to serialize plan evidence: {error}"))
        })?;

    let quantity = new_ticket.quantity.map(i64_from_u64).transpose()?;
    sqlx::query(
        r#"
        INSERT INTO tickets (
          id, workspace_id, owner_id, display_id, kind, type_id, captured_name, quantity,
          order_id, source_build_id, status, estimated_unit_cost, estimated_line_total,
          market_region_id, market_location_id, price_source_id, notes, assignee_character_id,
          execution_snapshot, occurrence_key, parent_ticket_id, produced_quantity,
          material_component_cost, own_installation_cost, total_production_cost, plan_evidence,
          created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'todo', $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $26)
        "#,
    )
    .bind(new_ticket.id.0)
    .bind(new_ticket.workspace_id.0)
    .bind(new_ticket.owner_id.0)
    .bind(&display_id)
    .bind(ticket_kind_str(new_ticket.kind))
    .bind(new_ticket.type_id)
    .bind(&new_ticket.captured_name)
    .bind(quantity)
    .bind(new_ticket.order_id.map(|id| id.0))
    .bind(new_ticket.source_build_id.map(|id| id.0))
    .bind(new_ticket.estimated_unit_cost.map(|money| money.0))
    .bind(new_ticket.estimated_line_total.map(|money| money.0))
    .bind(new_ticket.market_region_id)
    .bind(new_ticket.market_location_id)
    .bind(new_ticket.price_source_id.map(|id| id.0))
    .bind(&new_ticket.notes)
    .bind(new_ticket.assignee_character_id.map(|id| id.0))
    .bind(&execution_snapshot)
    .bind(&new_ticket.occurrence_key)
    .bind(new_ticket.parent_ticket_id.map(|id| id.0))
    .bind(new_ticket.produced_quantity.map(i64_from_u64).transpose()?)
    .bind(new_ticket.material_component_cost.map(|money| money.0))
    .bind(new_ticket.own_installation_cost.map(|money| money.0))
    .bind(new_ticket.total_production_cost.map(|money| money.0))
    .bind(&plan_evidence)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(map_error)?;

    Ok((display_id, execution_snapshot, plan_evidence))
}

/// Inserts a ticket row and its prerequisites inside the caller's
/// transaction -- shared by `create_ticket` and the atomic
/// `create_ticket_for_order_requirement`.
pub(super) async fn insert_ticket_with_prerequisites(
    tx: &mut Transaction<'_, Postgres>,
    new_ticket: NewTicket,
    now: DateTime<Utc>,
) -> Result<(Ticket, Vec<TicketPrerequisite>), OrderError> {
    // Every new ticket starts `todo` -- workflow status is purely
    // user-controlled and never depends on prerequisite state.
    // Unresolved prerequisites surface only in the derived
    // `derive_ticket_blockers` list, never in this column.
    let (display_id, _execution_snapshot, _plan_evidence) =
        insert_ticket_row(tx, &new_ticket, now).await?;

    let mut prerequisites = Vec::with_capacity(new_ticket.prerequisites.len());
    for prerequisite in new_ticket.prerequisites {
        // The caller's frozen inventory-reuse snapshot is
        // persisted as-is. For a child ticket that is always
        // `Full` / `reused_quantity = 0` (see `NewTicketPrerequisite`);
        // for the **root** Epic ticket it mirrors the Epic's own
        // requirements. Either way, persisting it reserves nothing --
        // creating a ticket still touches no `inventory_allocations` /
        // `inventory_events` / balance row. The real Consumption is
        // posted later, only by an explicit `record_ticket_production`.
        prerequisites.push(insert_ticket_prerequisite(tx, new_ticket.id, prerequisite).await?);
    }

    let ticket = Ticket {
        id: new_ticket.id,
        workspace_id: new_ticket.workspace_id,
        owner_id: new_ticket.owner_id,
        display_id,
        kind: new_ticket.kind,
        type_id: new_ticket.type_id,
        captured_name: new_ticket.captured_name,
        quantity: new_ticket.quantity,
        order_id: new_ticket.order_id,
        source_build_id: new_ticket.source_build_id,
        notes: new_ticket.notes,
        assignee_character_id: new_ticket.assignee_character_id,
        status: TicketStatus::Todo,
        estimated_unit_cost: new_ticket.estimated_unit_cost,
        estimated_line_total: new_ticket.estimated_line_total,
        actual_unit_cost: None,
        actual_line_total: None,
        market_region_id: new_ticket.market_region_id,
        market_location_id: new_ticket.market_location_id,
        price_source_id: new_ticket.price_source_id,
        acquisition_run_id: None,
        acquired_quantity: None,
        execution_snapshot: new_ticket.execution_snapshot,
        created_at: now,
        updated_at: now,
        archived_at: None,
        occurrence_key: new_ticket.occurrence_key,
        parent_ticket_id: new_ticket.parent_ticket_id,
        produced_quantity: new_ticket.produced_quantity,
        material_component_cost: new_ticket.material_component_cost,
        own_installation_cost: new_ticket.own_installation_cost,
        total_production_cost: new_ticket.total_production_cost,
        plan_evidence: new_ticket.plan_evidence,
    };
    Ok((ticket, prerequisites))
}
