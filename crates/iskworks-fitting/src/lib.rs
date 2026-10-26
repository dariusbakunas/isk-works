//! Fit stats from EVEShip.fit's dogma engine (`esf-dogma-engine`), behind
//! `iskworks_core::fitting::FittingEngine`. The only crate that names esf
//! types.

mod engine;
mod lock;

pub use engine::{EsfFittingEngine, FittingLoadError};
pub use lock::{DataLock, LockError};
