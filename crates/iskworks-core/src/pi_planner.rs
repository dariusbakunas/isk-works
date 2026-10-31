//! The PI Planner: which Planetary Interaction products the planets around a
//! home system can support, and how to set colonies up for them. Pure: the
//! callers load SDE data, the player's density entries and prices.
//!
//! All quantities are per hour and in `f64`; they are estimates.

mod capacity;
mod catalogue;
mod chain;
mod yields;

#[cfg(test)]
mod fixtures;

pub use capacity::{ColonyBudget, ColonyLayout, LINK_K, MAX_HEADS_PER_ECU};
pub use catalogue::{
    command_center_output, PiCatalogue, Recipe, COMMAND_CENTER_LEVELS, IMPORT_TAX_RATIO,
};
pub use chain::{expand, ChainDemand};
pub use yields::{
    duration_factor, resolve_yield, DensityEntry, SecurityBand, YieldEstimate, YieldSource,
    HEAD_VALUE_PER_DENSITY_PCT,
};
