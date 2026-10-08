//! Build -> Order (Epic) -> Board domain model: the Order/Ticket types, the
//! frozen whole-tree plan, pure derived-state functions (allocation,
//! recording, ticket planning, freeze), and the `OrderRepository` port the
//! storage crate implements.
//!
//! Not glob-re-exported at the crate root; callers use
//! `iskworks_core::order::X`.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AcquisitionProgressUpdate, AcquisitionRun, AcquisitionRunId, AcquisitionRunItem, BuildId,
    ConnectedCharacterId, FulfillmentScope, InventoryEventId, InventoryEventKind, Money,
    MoneyDelta, OwnerId, PriceSnapshotId, PriceSourceId, TaskExecutionSnapshot, WorkspaceId,
};

mod aggregate;
mod allocation;
mod freeze;
mod plan;
mod recording;
mod repository;
mod ticket;
mod ticket_plan;

pub use aggregate::*;
pub use allocation::*;
pub use freeze::*;
pub use plan::*;
pub use recording::*;
pub use repository::*;
pub use ticket::*;
pub use ticket_plan::*;
