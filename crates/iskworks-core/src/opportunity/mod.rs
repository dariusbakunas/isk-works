//! Opportunity scanning: the profitability-scope catalog, request
//! normalization, per-candidate projection, ranking, evidence classification,
//! and `OpportunityQueryService`.
//!
//! Split into focused submodules; `opportunity_quality` (special-edition
//! exclusion, strong/qualified/weak evidence mapping, thin-book thresholds,
//! excluded-cost diagnostics) stays a separate sibling extraction. This file
//! re-exports every symbol callers used before the split, so the crate-root
//! `pub use opportunity::*` and all `use iskworks_core::{...}` paths are
//! unchanged.

mod candidate_projection;
mod command;
mod evidence;
mod metrics;
mod query_service;
mod ranking;
mod scope_catalog;
mod valuation;

#[cfg(test)]
mod tests_common;

pub use candidate_projection::*;
pub use command::*;
pub use evidence::*;
pub use metrics::*;
pub use query_service::*;
pub use ranking::*;
pub use scope_catalog::*;
pub use valuation::*;
