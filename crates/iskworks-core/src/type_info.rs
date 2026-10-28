//! The Type Info window (EVE's "Show Info"): an item's description,
//! attributes, ship bonuses and required skills, shaped for display.
//!
//! Pure: the data comes from the active SDE import, read by the caller.

mod format;
mod requirements;
mod rich_text;
mod traits;

pub use format::{
    attribute_groups, format_attribute, format_bonus, AttributeGroup, AttributeNames, AttributeRow,
    FormattedValue, NameMaps, ValueLink,
};
pub use requirements::{
    requirement_summary, requirement_tree, RequirementPilot, RequirementSummary, SkillNode,
};
pub use rich_text::{parse_rich_text, RichSegment};
pub use traits::{trait_groups, TraitBonus, TraitGroup};
