//! The Type Info window (EVE's "Show Info"): an item's description,
//! attributes, ship bonuses and required skills, shaped for display.
//!
//! Pure: the data comes from the active SDE import, read by the caller.

mod rich_text;

pub use rich_text::{parse_rich_text, RichSegment};
