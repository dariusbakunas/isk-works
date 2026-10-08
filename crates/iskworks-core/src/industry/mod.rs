use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_sde::{ManufacturingRecipe, SdeReadRepository};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    preview_facility, FacilityError, FacilityPlanPreview, FacilityPreviewCommand,
    FacilityProfileId, FacilityRole, IndustryFacilityProfile, InstallationCostBreakdown,
    MarketCoverageRegistration, ReactionFacilityPreviewCommand,
};
use crate::{OwnerId, WorkspaceId};

const MAX_RUNS: u64 = 1_000_000;

mod acquisition_run;
mod build;
mod canonical;
mod canonical_projection;
mod money;
mod plan_calculation;
mod pricing;
mod recipe;
mod repository;
mod service;
mod transient_calculation;

#[cfg(test)]
mod tests;

pub use acquisition_run::*;
pub use build::*;
pub use money::*;
pub use plan_calculation::*;
pub use pricing::*;
pub use recipe::*;
pub use repository::*;
pub use service::*;
pub use transient_calculation::*;
