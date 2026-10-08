use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::Utc;
use iskworks_core::build_materials::MaterialActivity;
use iskworks_core::order::{
    compute_order_rollup, derive_operation_dag, derive_order_status, derive_recording_summary,
    derive_requirement_state, derive_ticket_blockers, frozen_reuse, intended_build_backed_runs,
    planned_root_output, requirement_to_prerequisite, FrozenDemandEdge, NewOrderPlan,
    NewPlanTicket, NewTicket, NewTicketPrerequisite, OperationDependency, Order, OrderError,
    OrderId, OrderRepository, OrderRequirement, OrderRequirementId, OrderRequirementRollup,
    OrderStatus, PlanOperation, RecordAcquisitionInput, RecordProductionInput,
    RecordProductionInputLine, RequirementFulfillmentState, RequirementKind,
    RequirementTicketCreation, Ticket, TicketBlockerRef, TicketId, TicketInventoryRecording,
    TicketKind, TicketMetadataUpdate, TicketPrerequisite, TicketPrerequisiteId,
    TicketRecordingSummary, TicketStatus, ROOT_OCCURRENCE_PREFIX,
};
use iskworks_core::{
    BuildId, BuildRecipe, ConnectedCharacterId, MarketScope, Money, PreviewBuildPlanCommand,
    PriceSourceId, TaskExecutionSnapshot,
};
use serde::{Deserialize, Serialize};

use crate::{workspace_context, ApiError, AppState};

mod order_lifecycle;
mod recording;
mod requirement_tickets;
mod tickets;
mod views;
use order_lifecycle::*;
use recording::*;
use requirement_tickets::*;
use tickets::*;
use views::*;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/builds/:build_id/orders", post(create_order))
        .route("/api/orders", get(list_orders))
        .route(
            "/api/tickets",
            get(list_order_tickets).post(create_ticket_route),
        )
        .route("/api/orders/:order_id", get(get_order).delete(delete_order))
        .route("/api/orders/:order_id/start", post(start_order))
        .route("/api/orders/:order_id/complete", post(complete_order))
        .route("/api/orders/:order_id/cancel", post(cancel_order))
        .route("/api/orders/:order_id/archive", post(archive_order))
        .route("/api/orders/:order_id/restore", post(restore_order))
        .route(
            "/api/tickets/:ticket_id",
            patch(update_ticket).delete(delete_ticket),
        )
        .route(
            "/api/tickets/:ticket_id/record-acquisition",
            post(record_ticket_acquisition_route),
        )
        .route(
            "/api/tickets/:ticket_id/record-production",
            post(record_ticket_production_route),
        )
        .route(
            "/api/tickets/:ticket_id/recordings/:recording_id/revert",
            post(revert_ticket_inventory_recording_route),
        )
        .route("/api/tickets/:ticket_id/start", post(start_ticket))
        .route("/api/tickets/:ticket_id/complete", post(complete_ticket))
        .route("/api/tickets/:ticket_id/cancel", post(cancel_ticket))
        .route("/api/tickets/:ticket_id/archive", post(archive_ticket))
        .route("/api/tickets/:ticket_id/restore", post(restore_ticket))
        .route(
            "/api/orders/:order_id/requirements/:requirement_id/tickets",
            post(create_ticket_for_requirement_route),
        )
        .route(
            "/api/orders/:order_id/requirements/:requirement_id/link",
            post(link_ticket_to_requirement),
        )
        .route(
            "/api/orders/:order_id/tickets/bulk",
            post(bulk_create_tickets),
        )
        .route(
            "/api/builds/:build_id/ticket-preview",
            get(preview_ticket_plan),
        )
}

#[cfg(test)]
mod tests;
