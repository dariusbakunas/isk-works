use super::*;

pub(super) fn planner_item_role_str(value: PlannerItemRole) -> &'static str {
    match value {
        PlannerItemRole::Material => "material",
        PlannerItemRole::Output => "output",
    }
}

pub(super) fn pricing_selection_kind_str(value: PricingSelectionKind) -> &'static str {
    match value {
        PricingSelectionKind::Default => "default",
        PricingSelectionKind::MarketPolicy => "market_policy",
        PricingSelectionKind::Manual => "manual",
    }
}

pub(super) fn pricing_policy_str(value: MarketPricingPolicy) -> &'static str {
    match value {
        MarketPricingPolicy::LowestSell => "lowest_sell",
        MarketPricingPolicy::HighestBuy => "highest_buy",
        MarketPricingPolicy::AcquireQuantityFromSellOrders => "acquire_quantity_from_sell_orders",
        MarketPricingPolicy::LiquidateQuantityIntoBuyOrders => "liquidate_quantity_into_buy_orders",
    }
}

pub(super) fn requirement_kind_str(kind: RequirementKind) -> &'static str {
    match kind {
        RequirementKind::Buy => "buy",
        RequirementKind::Build => "build",
        RequirementKind::React => "react",
    }
}

pub(super) fn requirement_kind_from_str(value: &str) -> Result<RequirementKind, OrderError> {
    match value {
        "buy" => Ok(RequirementKind::Buy),
        "build" => Ok(RequirementKind::Build),
        "react" => Ok(RequirementKind::React),
        other => Err(OrderError::Persistence(format!(
            "unknown order requirement kind {other}"
        ))),
    }
}

pub(super) fn fulfillment_scope_str(scope: FulfillmentScope) -> &'static str {
    match scope {
        FulfillmentScope::Missing => "missing",
        FulfillmentScope::Full => "full",
    }
}

pub(super) fn fulfillment_scope_from_str(value: &str) -> Result<FulfillmentScope, OrderError> {
    match value {
        "missing" => Ok(FulfillmentScope::Missing),
        "full" => Ok(FulfillmentScope::Full),
        other => Err(OrderError::Persistence(format!(
            "unknown fulfillment scope {other}"
        ))),
    }
}

pub(super) fn ticket_kind_str(kind: TicketKind) -> &'static str {
    match kind {
        TicketKind::Acquisition => "acquisition",
        TicketKind::Manufacturing => "manufacturing",
        TicketKind::Reaction => "reaction",
        TicketKind::Generic => "generic",
    }
}

pub(super) fn ticket_kind_from_str(value: &str) -> Result<TicketKind, OrderError> {
    match value {
        "acquisition" => Ok(TicketKind::Acquisition),
        "manufacturing" => Ok(TicketKind::Manufacturing),
        "reaction" => Ok(TicketKind::Reaction),
        "generic" => Ok(TicketKind::Generic),
        other => Err(OrderError::Persistence(format!(
            "unknown ticket kind {other}"
        ))),
    }
}

pub(super) fn ticket_status_str(status: TicketStatus) -> &'static str {
    match status {
        TicketStatus::Todo => "todo",
        TicketStatus::InProgress => "in_progress",
        TicketStatus::Complete => "complete",
        TicketStatus::Canceled => "canceled",
    }
}

pub(super) fn ticket_status_from_str(value: &str) -> Result<TicketStatus, OrderError> {
    match value {
        "todo" => Ok(TicketStatus::Todo),
        "in_progress" => Ok(TicketStatus::InProgress),
        "complete" => Ok(TicketStatus::Complete),
        "canceled" => Ok(TicketStatus::Canceled),
        other => Err(OrderError::Persistence(format!(
            "unknown ticket status {other}"
        ))),
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct OrderRow {
    pub(super) id: Uuid,
    pub(super) workspace_id: Uuid,
    pub(super) owner_id: Uuid,
    pub(super) source_build_id: Option<Uuid>,
    pub(super) source_build_revision: i64,
    pub(super) display_name: String,
    pub(super) runs: i64,
    pub(super) recipe_fingerprint: String,
    pub(super) price_snapshot_id: Uuid,
    pub(super) estimated_material_cost: Decimal,
    pub(super) expected_revenue: Option<Decimal>,
    pub(super) estimated_margin: Option<Decimal>,
    pub(super) missing_price_count: i32,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
    pub(super) started_at: Option<DateTime<Utc>>,
    pub(super) completed_at: Option<DateTime<Utc>>,
    pub(super) canceled_at: Option<DateTime<Utc>>,
    pub(super) archived_at: Option<DateTime<Utc>>,
    pub(super) planning_snapshot_version: i16,
}

impl OrderRow {
    pub(super) fn into_order(self) -> Result<Order, OrderError> {
        Ok(Order {
            id: OrderId(self.id),
            workspace_id: WorkspaceId(self.workspace_id),
            owner_id: OwnerId(self.owner_id),
            source_build_id: self.source_build_id.map(BuildId),
            source_build_revision: u64_from_i64(self.source_build_revision)?,
            display_name: self.display_name,
            runs: u64_from_i64(self.runs)?,
            recipe_fingerprint: self.recipe_fingerprint,
            price_snapshot_id: PriceSnapshotId(self.price_snapshot_id),
            estimated_material_cost: Money(self.estimated_material_cost),
            expected_revenue: self.expected_revenue.map(Money),
            estimated_margin: self.estimated_margin.map(Money),
            missing_price_count: u32::try_from(self.missing_price_count)
                .map_err(|_| OrderError::Persistence("invalid missing count".to_string()))?,
            created_at: self.created_at,
            updated_at: self.updated_at,
            started_at: self.started_at,
            completed_at: self.completed_at,
            canceled_at: self.canceled_at,
            archived_at: self.archived_at,
            planning_snapshot_version: u8::try_from(self.planning_snapshot_version)
                .map_err(|_| OrderError::Persistence("invalid snapshot version".to_string()))?,
        })
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct OrderRequirementRow {
    pub(super) id: Uuid,
    pub(super) order_id: Uuid,
    pub(super) type_id: i64,
    pub(super) captured_name: String,
    pub(super) kind: String,
    pub(super) source_build_id: Option<Uuid>,
    pub(super) required_quantity: i64,
    pub(super) fulfillment_scope: String,
    pub(super) reused_quantity: i64,
    pub(super) fresh_quantity: i64,
    pub(super) estimated_unit_cost: Option<Decimal>,
    pub(super) estimated_line_total: Option<Decimal>,
    pub(super) reused_line_total: Option<Decimal>,
    pub(super) operation_occurrence_key: Option<String>,
    pub(super) child_occurrence_key: Option<String>,
    pub(super) inventory_unit_basis: Option<Decimal>,
    pub(super) child_produced_quantity: Option<i64>,
    pub(super) child_consumed_quantity: Option<i64>,
    pub(super) child_surplus_quantity: Option<i64>,
    pub(super) child_surplus_retained_basis: Option<Decimal>,
    pub(super) price_evidence: Option<serde_json::Value>,
    pub(super) child_consumed_cost: Option<Decimal>,
    pub(super) dependency_id: Option<String>,
}

impl OrderRequirementRow {
    pub(super) fn into_requirement(self) -> Result<OrderRequirement, OrderError> {
        Ok(OrderRequirement {
            id: OrderRequirementId(self.id),
            order_id: OrderId(self.order_id),
            type_id: self.type_id,
            captured_name: self.captured_name,
            kind: requirement_kind_from_str(&self.kind)?,
            source_build_id: self.source_build_id.map(BuildId),
            required_quantity: u64_from_i64(self.required_quantity)?,
            fulfillment_scope: fulfillment_scope_from_str(&self.fulfillment_scope)?,
            reused_quantity: u64_from_i64(self.reused_quantity)?,
            fresh_quantity: u64_from_i64(self.fresh_quantity)?,
            estimated_unit_cost: self.estimated_unit_cost.map(Money),
            estimated_line_total: self.estimated_line_total.map(Money),
            reused_line_total: self.reused_line_total.map(Money),
            operation_occurrence_key: self.operation_occurrence_key,
            child_occurrence_key: self.child_occurrence_key,
            inventory_unit_basis: self.inventory_unit_basis.map(Money),
            child_produced_quantity: self.child_produced_quantity.map(u64_from_i64).transpose()?,
            child_consumed_quantity: self.child_consumed_quantity.map(u64_from_i64).transpose()?,
            child_surplus_quantity: self.child_surplus_quantity.map(u64_from_i64).transpose()?,
            child_surplus_retained_basis: self.child_surplus_retained_basis.map(Money),
            child_consumed_cost: self.child_consumed_cost.map(Money),
            dependency_id: self.dependency_id,
            price_evidence: self
                .price_evidence
                .map(|value| {
                    serde_json::from_value::<PlanRequirementEvidence>(value).map_err(|error| {
                        OrderError::Persistence(format!("invalid stored price evidence: {error}"))
                    })
                })
                .transpose()?,
        })
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct TicketRow {
    pub(super) id: Uuid,
    pub(super) workspace_id: Uuid,
    pub(super) owner_id: Uuid,
    pub(super) display_id: String,
    pub(super) kind: String,
    pub(super) type_id: Option<i64>,
    pub(super) captured_name: String,
    pub(super) quantity: Option<i64>,
    pub(super) order_id: Option<Uuid>,
    pub(super) source_build_id: Option<Uuid>,
    pub(super) notes: String,
    pub(super) assignee_character_id: Option<Uuid>,
    pub(super) status: String,
    pub(super) estimated_unit_cost: Option<Decimal>,
    pub(super) estimated_line_total: Option<Decimal>,
    pub(super) actual_unit_cost: Option<Decimal>,
    pub(super) actual_line_total: Option<Decimal>,
    pub(super) market_region_id: Option<i64>,
    pub(super) market_location_id: Option<i64>,
    pub(super) price_source_id: Option<Uuid>,
    pub(super) acquisition_run_id: Option<Uuid>,
    pub(super) acquired_quantity: Option<i64>,
    pub(super) execution_snapshot: Option<serde_json::Value>,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
    pub(super) archived_at: Option<DateTime<Utc>>,
    pub(super) occurrence_key: Option<String>,
    pub(super) parent_ticket_id: Option<Uuid>,
    pub(super) produced_quantity: Option<i64>,
    pub(super) material_component_cost: Option<Decimal>,
    pub(super) own_installation_cost: Option<Decimal>,
    pub(super) total_production_cost: Option<Decimal>,
    pub(super) plan_evidence: Option<serde_json::Value>,
}

impl TicketRow {
    pub(super) fn into_ticket(self) -> Result<Ticket, OrderError> {
        let execution_snapshot = self
            .execution_snapshot
            .map(|value| {
                serde_json::from_value::<TaskExecutionSnapshot>(value).map_err(|error| {
                    OrderError::Persistence(format!("invalid stored execution snapshot: {error}"))
                })
            })
            .transpose()?;
        let plan_evidence = self
            .plan_evidence
            .map(|value| {
                serde_json::from_value::<PlanOperationEvidence>(value).map_err(|error| {
                    OrderError::Persistence(format!("invalid stored plan evidence: {error}"))
                })
            })
            .transpose()?;
        Ok(Ticket {
            id: TicketId(self.id),
            workspace_id: WorkspaceId(self.workspace_id),
            owner_id: OwnerId(self.owner_id),
            display_id: self.display_id,
            kind: ticket_kind_from_str(&self.kind)?,
            type_id: self.type_id,
            captured_name: self.captured_name,
            quantity: self.quantity.map(u64_from_i64).transpose()?,
            order_id: self.order_id.map(OrderId),
            source_build_id: self.source_build_id.map(BuildId),
            notes: self.notes,
            assignee_character_id: self.assignee_character_id.map(ConnectedCharacterId),
            status: ticket_status_from_str(&self.status)?,
            estimated_unit_cost: self.estimated_unit_cost.map(Money),
            estimated_line_total: self.estimated_line_total.map(Money),
            actual_unit_cost: self.actual_unit_cost.map(Money),
            actual_line_total: self.actual_line_total.map(Money),
            market_region_id: self.market_region_id,
            market_location_id: self.market_location_id,
            price_source_id: self.price_source_id.map(PriceSourceId),
            acquisition_run_id: self.acquisition_run_id.map(AcquisitionRunId),
            acquired_quantity: self.acquired_quantity.map(u64_from_i64).transpose()?,
            execution_snapshot,
            created_at: self.created_at,
            updated_at: self.updated_at,
            archived_at: self.archived_at,
            occurrence_key: self.occurrence_key,
            parent_ticket_id: self.parent_ticket_id.map(TicketId),
            produced_quantity: self.produced_quantity.map(u64_from_i64).transpose()?,
            material_component_cost: self.material_component_cost.map(Money),
            own_installation_cost: self.own_installation_cost.map(Money),
            total_production_cost: self.total_production_cost.map(Money),
            plan_evidence,
        })
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct TicketPrerequisiteRow {
    pub(super) id: Uuid,
    pub(super) ticket_id: Uuid,
    pub(super) type_id: i64,
    pub(super) captured_name: String,
    pub(super) kind: String,
    pub(super) source_build_id: Option<Uuid>,
    pub(super) required_quantity: i64,
    pub(super) fulfillment_scope: String,
    pub(super) reused_quantity: i64,
    pub(super) fresh_quantity: i64,
    pub(super) estimated_unit_cost: Option<Decimal>,
    pub(super) estimated_line_total: Option<Decimal>,
    pub(super) reused_line_total: Option<Decimal>,
    pub(super) operation_occurrence_key: Option<String>,
    pub(super) child_occurrence_key: Option<String>,
    pub(super) inventory_unit_basis: Option<Decimal>,
    pub(super) child_produced_quantity: Option<i64>,
    pub(super) child_consumed_quantity: Option<i64>,
    pub(super) child_surplus_quantity: Option<i64>,
    pub(super) child_surplus_retained_basis: Option<Decimal>,
    pub(super) price_evidence: Option<serde_json::Value>,
    pub(super) child_consumed_cost: Option<Decimal>,
    pub(super) dependency_id: Option<String>,
}

impl TicketPrerequisiteRow {
    pub(super) fn into_prerequisite(self) -> Result<TicketPrerequisite, OrderError> {
        Ok(TicketPrerequisite {
            id: TicketPrerequisiteId(self.id),
            ticket_id: TicketId(self.ticket_id),
            type_id: self.type_id,
            captured_name: self.captured_name,
            kind: requirement_kind_from_str(&self.kind)?,
            source_build_id: self.source_build_id.map(BuildId),
            required_quantity: u64_from_i64(self.required_quantity)?,
            fulfillment_scope: fulfillment_scope_from_str(&self.fulfillment_scope)?,
            reused_quantity: u64_from_i64(self.reused_quantity)?,
            fresh_quantity: u64_from_i64(self.fresh_quantity)?,
            estimated_unit_cost: self.estimated_unit_cost.map(Money),
            estimated_line_total: self.estimated_line_total.map(Money),
            reused_line_total: self.reused_line_total.map(Money),
            operation_occurrence_key: self.operation_occurrence_key,
            child_occurrence_key: self.child_occurrence_key,
            inventory_unit_basis: self.inventory_unit_basis.map(Money),
            child_produced_quantity: self.child_produced_quantity.map(u64_from_i64).transpose()?,
            child_consumed_quantity: self.child_consumed_quantity.map(u64_from_i64).transpose()?,
            child_surplus_quantity: self.child_surplus_quantity.map(u64_from_i64).transpose()?,
            child_surplus_retained_basis: self.child_surplus_retained_basis.map(Money),
            child_consumed_cost: self.child_consumed_cost.map(Money),
            dependency_id: self.dependency_id,
            price_evidence: self
                .price_evidence
                .map(|value| {
                    serde_json::from_value::<PlanRequirementEvidence>(value).map_err(|error| {
                        OrderError::Persistence(format!("invalid stored price evidence: {error}"))
                    })
                })
                .transpose()?,
        })
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct PlanOperationRow {
    pub(super) id: Uuid,
    pub(super) order_id: Uuid,
    pub(super) occurrence_key: String,
    pub(super) parent_occurrence_key: Option<String>,
    pub(super) build_id: Option<Uuid>,
    pub(super) activity: String,
    pub(super) runs: i64,
    pub(super) persisted_runs: i64,
    pub(super) product_type_id: i64,
    pub(super) product_name: String,
    pub(super) output_per_run: i64,
    pub(super) produced_quantity: i64,
    pub(super) blueprint_or_formula_type_id: i64,
    pub(super) material_component_cost: Option<Decimal>,
    pub(super) own_installation_cost: Option<Decimal>,
    pub(super) total_production_cost: Option<Decimal>,
    pub(super) complete: bool,
    pub(super) evidence: serde_json::Value,
    pub(super) created_at: DateTime<Utc>,
    pub(super) consumed_quantity: Option<i64>,
    pub(super) surplus_quantity: Option<i64>,
    pub(super) surplus_retained_basis: Option<Decimal>,
}

impl PlanOperationRow {
    pub(super) fn into_operation(self) -> Result<PlanOperation, OrderError> {
        let activity = match self.activity.as_str() {
            "manufacturing" => MaterialActivity::Manufacturing,
            "reaction" => MaterialActivity::Reaction,
            other => {
                return Err(OrderError::Persistence(format!(
                    "unknown plan operation activity {other}"
                )))
            }
        };
        let evidence =
            serde_json::from_value::<PlanOperationEvidence>(self.evidence).map_err(|error| {
                OrderError::Persistence(format!("invalid stored plan operation evidence: {error}"))
            })?;
        Ok(PlanOperation {
            id: PlanOperationId(self.id),
            order_id: OrderId(self.order_id),
            occurrence_key: self.occurrence_key,
            parent_occurrence_key: self.parent_occurrence_key,
            build_id: self.build_id.map(BuildId),
            activity,
            runs: u64_from_i64(self.runs)?,
            persisted_runs: u64_from_i64(self.persisted_runs)?,
            product_type_id: self.product_type_id,
            product_name: self.product_name,
            output_per_run: u64_from_i64(self.output_per_run)?,
            produced_quantity: u64_from_i64(self.produced_quantity)?,
            blueprint_or_formula_type_id: self.blueprint_or_formula_type_id,
            material_component_cost: self.material_component_cost.map(Money),
            own_installation_cost: self.own_installation_cost.map(Money),
            total_production_cost: self.total_production_cost.map(Money),
            complete: self.complete,
            consumed_quantity: self.consumed_quantity.map(u64_from_i64).transpose()?,
            surplus_quantity: self.surplus_quantity.map(u64_from_i64).transpose()?,
            surplus_retained_basis: self.surplus_retained_basis.map(Money),
            evidence,
            created_at: self.created_at,
        })
    }
}

pub(super) fn ticket_inventory_recording_kind_from_str(
    value: &str,
) -> Result<TicketInventoryRecordingKind, OrderError> {
    match value {
        "acquisition" => Ok(TicketInventoryRecordingKind::Acquisition),
        "production" => Ok(TicketInventoryRecordingKind::Production),
        other => Err(OrderError::Persistence(format!(
            "unknown ticket inventory recording kind {other}"
        ))),
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct TicketInventoryRecordingRow {
    pub(super) id: Uuid,
    pub(super) ticket_id: Uuid,
    pub(super) kind: String,
    pub(super) recorded_quantity: Option<i64>,
    pub(super) runs_completed: Option<i64>,
    pub(super) installation_cost: Option<Decimal>,
    pub(super) output_type_id: Option<i64>,
    pub(super) output_quantity: Option<i64>,
    pub(super) location_note: String,
    pub(super) note: String,
    pub(super) recorded_at: DateTime<Utc>,
    pub(super) reverted_at: Option<DateTime<Utc>>,
}

impl TicketInventoryRecordingRow {
    pub(super) fn into_recording(self) -> Result<TicketInventoryRecording, OrderError> {
        let status = if self.reverted_at.is_some() {
            TicketInventoryRecordingStatus::Reversed
        } else {
            TicketInventoryRecordingStatus::Recorded
        };
        Ok(TicketInventoryRecording {
            id: TicketInventoryRecordingId(self.id),
            ticket_id: TicketId(self.ticket_id),
            kind: ticket_inventory_recording_kind_from_str(&self.kind)?,
            recorded_quantity: self.recorded_quantity.map(u64_from_i64).transpose()?,
            runs_completed: self.runs_completed.map(u64_from_i64).transpose()?,
            installation_cost: self.installation_cost.map(Money),
            output_type_id: self.output_type_id,
            output_quantity: self.output_quantity.map(u64_from_i64).transpose()?,
            location_note: self.location_note,
            note: self.note,
            recorded_at: self.recorded_at,
            reverted_at: self.reverted_at,
            status,
            effects: Vec::new(),
        })
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct OrderRequirementFulfillmentRow {
    pub(super) id: Uuid,
    pub(super) order_requirement_id: Uuid,
    pub(super) ticket_id: Uuid,
    pub(super) allocated_quantity: i64,
    pub(super) linked_at: DateTime<Utc>,
}

impl OrderRequirementFulfillmentRow {
    pub(super) fn into_fulfillment(self) -> Result<OrderRequirementFulfillment, OrderError> {
        Ok(OrderRequirementFulfillment {
            id: OrderRequirementFulfillmentId(self.id),
            order_requirement_id: OrderRequirementId(self.order_requirement_id),
            ticket_id: TicketId(self.ticket_id),
            allocated_quantity: u64_from_i64(self.allocated_quantity)?,
            linked_at: self.linked_at,
        })
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct TicketPrerequisiteFulfillmentRow {
    pub(super) id: Uuid,
    pub(super) ticket_prerequisite_id: Uuid,
    pub(super) fulfilling_ticket_id: Uuid,
    pub(super) allocated_quantity: i64,
    pub(super) linked_at: DateTime<Utc>,
}

impl TicketPrerequisiteFulfillmentRow {
    pub(super) fn into_fulfillment(self) -> Result<TicketPrerequisiteFulfillment, OrderError> {
        Ok(TicketPrerequisiteFulfillment {
            id: TicketPrerequisiteFulfillmentId(self.id),
            ticket_prerequisite_id: TicketPrerequisiteId(self.ticket_prerequisite_id),
            fulfilling_ticket_id: TicketId(self.fulfilling_ticket_id),
            allocated_quantity: u64_from_i64(self.allocated_quantity)?,
            linked_at: self.linked_at,
        })
    }
}
