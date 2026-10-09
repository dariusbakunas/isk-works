use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_core::build_materials::MaterialActivity;
use iskworks_core::order::{
    derive_operation_dag, derive_recording_summary, operation_ticket, plan_capped_reservations,
    plan_epic_reservations, plan_recorded_output, reuse_by_type, AllocationReason,
    CappedReservationPlan, FrozenDemandEdge, NewOrder, NewOrderPlan, NewOrderRequirement,
    NewPlanOperation, NewTicket, NewTicketPrerequisite, OperationTicketCreation, Order, OrderError,
    OrderId, OrderPlanResult, OrderRepository, OrderRequirement, OrderRequirementFulfillment,
    OrderRequirementFulfillmentId, OrderRequirementId, PlanOperation, PlanOperationEvidence,
    PlanOperationId, PlanRequirementEvidence, RecordAcquisitionInput, RecordAcquisitionOutcome,
    RecordProductionInput, RecordProductionOutcome, RequirementKind, RequirementReservationTotals,
    RequirementTicketCreation, ReservationNeed, RevertTicketInventoryRecordingOutcome, Ticket,
    TicketId, TicketInventoryEffect, TicketInventoryRecording, TicketInventoryRecordingId,
    TicketInventoryRecordingKind, TicketInventoryRecordingStatus, TicketKind, TicketMetadataUpdate,
    TicketPrerequisite, TicketPrerequisiteFulfillment, TicketPrerequisiteFulfillmentId,
    TicketPrerequisiteId, TicketStatus,
};
use iskworks_core::{
    allocate_acquisition_delivery, weighted_unit_cost, AcquisitionProgressUpdate, AcquisitionRun,
    AcquisitionRunId, AcquisitionRunItem, BuildId, ConnectedCharacterId, CostInputQuality,
    FulfillmentScope, InventoryEventId, InventoryEventKind, InventoryItemKey, InventoryPosting,
    MarketPricingPolicy, MarketScope, Money, MoneyDelta, OwnerId, PlannerItemRole, PriceSnapshotId,
    PriceSourceId, PricingSelectionKind, TaskExecutionSnapshot, WorkspaceId,
};
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

use crate::inventory::PgInventoryRepository;

mod acquisition;
mod error;
mod inserts;
mod inventory_posting;
mod recording_queries;
mod repository_impl;
mod reservations;
mod rows;

#[cfg(test)]
mod tests;

use error::*;
use rows::*;

#[derive(Clone)]
pub struct PgOrderRepository {
    pool: PgPool,
}

impl PgOrderRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
