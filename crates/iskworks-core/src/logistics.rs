//! The facility-aware Logistics projection --
//! *what needs to be where, and how much space it takes*.
//!
//! A pure reduction of the one planning walk's own allocation evidence
//! (`BuildMaterialsSummary::node_allocations` -- per consuming operation,
//! per component: required, planned inventory use, shortage, resolution,
//! producer) and its operations (`verification_operations`: each
//! operation's selected facility). It never re-nets inventory, never
//! re-plans and never mutates anything.
//!
//! **Destination** is the facility of the operation that *consumes* the
//! input -- a production operation's inputs are needed where it runs, the
//! root's where final production runs, and a Buy leaf wherever its
//! consuming operation runs. Never derived from the item itself.
//!
//! **Aggregation**: one line per `(destination, type)`. The same item needed
//! by several operations at one facility is one line; the same item needed
//! at two facilities stays two lines.
//!
//! Per line:
//! * `quantity` -- everything required at the destination;
//! * `planned_inventory_quantity` -- the planner's own allocation from
//!   stock (source location not yet tracked);
//! * `shortage_quantity = quantity - planned_inventory_quantity`, split into
//!   `acquire_quantity` (Buy / unresolved: to purchase) and
//!   `produced_quantity` (delivered by producer operations, whose facilities
//!   are listed as trustworthy source evidence);
//! * `total_volume_m3 = quantity * unit_volume_m3` (SDE packaged volume),
//!   exact decimal arithmetic, `None` when the SDE has no volume.
//!
//! Structured per destination so a future "Create hauling ticket" can act
//! on one group. No routes, no hauling, no readiness.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

use crate::build_materials::{
    MaterialBoundaryResolution, NodeMaterialAllocation, VerificationOperationInput,
};
use crate::{BuildId, FulfillmentScope};

/// How an input reaches its destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LogisticsSourceKind {
    /// Purchased externally (a Buy leaf).
    Acquire,
    /// Produced by another operation of this plan.
    Produced,
    /// Production-intended with no producer yet: to be sourced somehow.
    Unresolved,
}

/// One consuming operation's own demand behind a [`LogisticsLine`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogisticsConsumerRef {
    /// The consuming operation's `graph_node_id`.
    pub operation_id: String,
    pub build_id: BuildId,
    pub output_type_name: String,
    pub source: LogisticsSourceKind,
    pub fulfillment_scope: FulfillmentScope,
    pub required_quantity: u64,
    pub planned_inventory_quantity: u64,
    pub shortage_quantity: u64,
    pub dependency_id: String,
}

/// One producer operation delivering part of a line -- trustworthy source
/// evidence (its facility is where the goods come from).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogisticsProducerRef {
    pub operation_id: String,
    pub facility_id: Option<Uuid>,
    pub facility_name: Option<String>,
    pub solar_system: Option<String>,
    pub quantity: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogisticsLine {
    pub type_id: i64,
    pub type_name: String,
    pub quantity: u64,
    pub planned_inventory_quantity: u64,
    pub shortage_quantity: u64,
    pub acquire_quantity: u64,
    pub produced_quantity: u64,
    pub unresolved_quantity: u64,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub unit_volume_m3: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub total_volume_m3: Option<Decimal>,
    pub consumers: Vec<LogisticsConsumerRef>,
    pub producers: Vec<LogisticsProducerRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogisticsDestination {
    /// `facility:<uuid>`, or `unassigned` for operations with no facility.
    pub key: String,
    pub facility_id: Option<Uuid>,
    pub facility_name: Option<String>,
    pub solar_system: Option<String>,
    /// `graph_node_id`s of the operations running here.
    pub operation_ids: Vec<String>,
    pub lines: Vec<LogisticsLine>,
    /// Σ known line volumes.
    #[serde(with = "rust_decimal::serde::str")]
    pub total_volume_m3: Decimal,
    /// `false` when at least one line's unit volume is unknown (the total
    /// then understates the cargo).
    pub volume_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogisticsPlan {
    pub destinations: Vec<LogisticsDestination>,
    #[serde(with = "rust_decimal::serde::str")]
    pub total_volume_m3: Decimal,
    pub volume_complete: bool,
}

const UNASSIGNED: &str = "unassigned";

fn destination_key(facility_id: Option<Uuid>) -> String {
    facility_id.map_or_else(|| UNASSIGNED.to_string(), |id| format!("facility:{id}"))
}

/// Project the Logistics plan -- see the module doc. `unit_volumes` maps a
/// `type_id` to its SDE packaged volume (absent or `None` = unknown).
/// Deterministic: destinations by facility name (unassigned last), lines
/// by type name, consumers/producers by operation id.
#[must_use]
pub fn project_logistics(
    operations: &[VerificationOperationInput],
    allocations: &[NodeMaterialAllocation],
    unit_volumes: &BTreeMap<i64, Option<Decimal>>,
) -> LogisticsPlan {
    let op_by_node: BTreeMap<&str, &VerificationOperationInput> = operations
        .iter()
        .map(|op| (op.graph_node_id.as_str(), op))
        .collect();
    let mut op_by_build: BTreeMap<Uuid, &VerificationOperationInput> = BTreeMap::new();
    for op in operations {
        op_by_build.entry(op.build_id.0).or_insert(op);
    }

    struct DestinationAcc {
        facility_id: Option<Uuid>,
        facility_name: Option<String>,
        solar_system: Option<String>,
        operation_ids: Vec<String>,
        lines: BTreeMap<i64, LogisticsLine>,
    }
    let mut destinations: BTreeMap<String, DestinationAcc> = BTreeMap::new();
    for op in operations {
        let destination = destinations
            .entry(destination_key(op.facility_id))
            .or_insert_with(|| DestinationAcc {
                facility_id: op.facility_id,
                facility_name: op.facility_name.clone(),
                solar_system: op.solar_system.clone(),
                operation_ids: Vec::new(),
                lines: BTreeMap::new(),
            });
        destination.operation_ids.push(op.graph_node_id.clone());
    }

    for allocation in allocations {
        if allocation.required_quantity == 0 {
            continue;
        }
        let Some(consumer) = op_by_node.get(allocation.graph_node_id.as_str()) else {
            continue;
        };
        let source = match allocation.resolution {
            MaterialBoundaryResolution::Buy => LogisticsSourceKind::Acquire,
            MaterialBoundaryResolution::Build | MaterialBoundaryResolution::Reaction => {
                LogisticsSourceKind::Produced
            }
            MaterialBoundaryResolution::Unresolved => LogisticsSourceKind::Unresolved,
        };
        let Some(destination) = destinations.get_mut(&destination_key(consumer.facility_id)) else {
            continue;
        };
        let line = destination
            .lines
            .entry(allocation.type_id)
            .or_insert_with(|| LogisticsLine {
                type_id: allocation.type_id,
                type_name: allocation.type_name.clone(),
                quantity: 0,
                planned_inventory_quantity: 0,
                shortage_quantity: 0,
                acquire_quantity: 0,
                produced_quantity: 0,
                unresolved_quantity: 0,
                unit_volume_m3: unit_volumes.get(&allocation.type_id).copied().flatten(),
                total_volume_m3: None,
                consumers: Vec::new(),
                producers: Vec::new(),
            });
        line.quantity = line.quantity.saturating_add(allocation.required_quantity);
        line.planned_inventory_quantity = line
            .planned_inventory_quantity
            .saturating_add(allocation.allocated_quantity);
        line.shortage_quantity = line
            .shortage_quantity
            .saturating_add(allocation.shortage_quantity);
        match source {
            LogisticsSourceKind::Acquire => {
                line.acquire_quantity = line
                    .acquire_quantity
                    .saturating_add(allocation.shortage_quantity);
            }
            LogisticsSourceKind::Produced => {
                line.produced_quantity = line
                    .produced_quantity
                    .saturating_add(allocation.shortage_quantity);
                if allocation.shortage_quantity > 0 {
                    if let Some(producer) = allocation
                        .producer_build_id
                        .and_then(|id| op_by_build.get(&id.0))
                    {
                        match line
                            .producers
                            .iter_mut()
                            .find(|existing| existing.operation_id == producer.graph_node_id)
                        {
                            Some(existing) => {
                                existing.quantity = existing
                                    .quantity
                                    .saturating_add(allocation.shortage_quantity);
                            }
                            None => line.producers.push(LogisticsProducerRef {
                                operation_id: producer.graph_node_id.clone(),
                                facility_id: producer.facility_id,
                                facility_name: producer.facility_name.clone(),
                                solar_system: producer.solar_system.clone(),
                                quantity: allocation.shortage_quantity,
                            }),
                        }
                    }
                }
            }
            LogisticsSourceKind::Unresolved => {
                line.unresolved_quantity = line
                    .unresolved_quantity
                    .saturating_add(allocation.shortage_quantity);
            }
        }
        line.consumers.push(LogisticsConsumerRef {
            operation_id: consumer.graph_node_id.clone(),
            build_id: consumer.build_id,
            output_type_name: consumer.product_name.clone(),
            source,
            fulfillment_scope: allocation.scope,
            required_quantity: allocation.required_quantity,
            planned_inventory_quantity: allocation.allocated_quantity,
            shortage_quantity: allocation.shortage_quantity,
            dependency_id: allocation.dependency_id.clone(),
        });
    }

    let mut plan_total = Decimal::ZERO;
    let mut plan_complete = true;
    let mut result: Vec<LogisticsDestination> = destinations
        .into_iter()
        .filter(|(_, destination)| !destination.lines.is_empty())
        .map(|(key, mut destination)| {
            let mut total = Decimal::ZERO;
            let mut complete = true;
            let mut lines: Vec<LogisticsLine> = std::mem::take(&mut destination.lines)
                .into_values()
                .map(|mut line| {
                    line.total_volume_m3 = line
                        .unit_volume_m3
                        .and_then(|unit| unit.checked_mul(Decimal::from(line.quantity)));
                    match line.total_volume_m3 {
                        Some(volume) => total = total.saturating_add(volume),
                        None => complete = false,
                    }
                    line.consumers
                        .sort_by(|a, b| a.operation_id.cmp(&b.operation_id));
                    line.producers
                        .sort_by(|a, b| a.operation_id.cmp(&b.operation_id));
                    line
                })
                .collect();
            lines.sort_by(|a, b| {
                a.type_name
                    .cmp(&b.type_name)
                    .then_with(|| a.type_id.cmp(&b.type_id))
            });
            destination.operation_ids.sort();
            plan_total = plan_total.saturating_add(total);
            plan_complete &= complete;
            LogisticsDestination {
                key,
                facility_id: destination.facility_id,
                facility_name: destination.facility_name,
                solar_system: destination.solar_system,
                operation_ids: destination.operation_ids,
                lines,
                total_volume_m3: total,
                volume_complete: complete,
            }
        })
        .collect();
    result.sort_by(|a, b| {
        (
            a.facility_id.is_none(),
            a.facility_name.as_deref(),
            a.key.as_str(),
        )
            .cmp(&(
                b.facility_id.is_none(),
                b.facility_name.as_deref(),
                b.key.as_str(),
            ))
    });

    LogisticsPlan {
        destinations: result,
        total_volume_m3: plan_total,
        volume_complete: plan_complete,
    }
}

#[cfg(test)]
mod tests;
