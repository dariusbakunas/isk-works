//! Market domain, value, and DTO types: scopes, order/observation records,
//! price-source config/value types, coverage/freshness structures, order-book
//! and valuation result projections, and import DTOs. Behaviour lives in the
//! sibling modules (`repository`, `service`, `import_parse`, `depth`,
//! `errors`).

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ConnectedCharacterId, Money, PriceSourceId, PriceSourceKind, WorkspaceId};

use super::errors::MarketError;

mod catalog;
mod import;
mod observation;
mod pricing;
mod scope;
pub use catalog::*;
pub use import::*;
pub use observation::*;
pub use pricing::*;
pub use scope::*;

#[cfg(test)]
mod tests;
