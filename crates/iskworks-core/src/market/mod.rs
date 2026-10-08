//! Market pricing: scope, domain types, the `MarketRepository` port,
//! `MarketService`, the EVE client market-export parser, and the pure
//! order-book depth / valuation helpers.
//!
//! Split into focused submodules:
//! - [`types`] — domain / value / DTO types + small type helpers.
//! - [`repository`] — the `MarketRepository` port.
//! - [`service`] — `MarketService` import / price-preview orchestration.
//! - [`import_parse`] — the pure EVE client market-export parser.
//! - [`depth`] — quantity-aware depth / valuation / summarisation helpers.
//! - [`errors`] — `MarketError`.
//!
//! This file is the public surface; every symbol callers used before the
//! split is re-exported here, so `use iskworks_core::{...}` paths (via the
//! crate-root `pub use market::*`) are unchanged.

mod depth;
mod errors;
mod import_parse;
mod repository;
mod service;
mod types;

pub use depth::*;
pub use errors::*;
pub use import_parse::*;
pub use repository::*;
pub use service::*;
pub use types::*;
