//! Dogma attribute values as the client's Attributes tab shows them.

use std::collections::BTreeMap;

use iskworks_sde::TypeAttributeValue;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

/// Names a value can point at: unit 115 is a group, 116 a type, 119 an
/// attribute.
pub trait AttributeNames {
    fn type_name(&self, type_id: i64) -> Option<String>;
    fn group_name(&self, group_id: i64) -> Option<String>;
    fn attribute_name(&self, attribute_id: i64) -> Option<String>;
}

/// [`AttributeNames`] from maps the caller filled.
#[derive(Debug, Clone, Default)]
pub struct NameMaps {
    pub types: BTreeMap<i64, String>,
    pub groups: BTreeMap<i64, String>,
    pub attributes: BTreeMap<i64, String>,
}

impl AttributeNames for NameMaps {
    fn type_name(&self, type_id: i64) -> Option<String> {
        self.types.get(&type_id).cloned()
    }
    fn group_name(&self, group_id: i64) -> Option<String> {
        self.groups.get(&group_id).cloned()
    }
    fn attribute_name(&self, attribute_id: i64) -> Option<String> {
        self.attributes.get(&attribute_id).cloned()
    }
}

/// A value pointing at another type or group.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ValueLink {
    Type { id: i64, name: String },
    Group { id: i64, name: String },
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FormattedValue {
    pub display: String,
    pub link: Option<ValueLink>,
}

/// One Attributes row.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributeRow {
    pub attribute_id: i64,
    pub label: String,
    pub display: String,
    /// The stored value, for sorting and comparing.
    pub raw: Decimal,
    pub unit_id: Option<i64>,
    pub high_is_good: bool,
    pub link: Option<ValueLink>,
}

/// Rows under one attribute category.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributeGroup {
    pub category: String,
    pub rows: Vec<AttributeRow>,
}

/// Units the formatter treats specially (`dogmaUnits.jsonl` ids).
mod unit {
    pub const MILLISECONDS: i64 = 101;
    pub const MULTIPLIER: i64 = 104;
    pub const PERCENTAGE: i64 = 105;
    pub const INVERSE_ABSOLUTE_PERCENT: i64 = 108;
    pub const MODIFIER_PERCENT: i64 = 109;
    pub const INVERSED_MODIFIER_PERCENT: i64 = 111;
    pub const GROUP_ID: i64 = 115;
    pub const TYPE_ID: i64 = 116;
    pub const SIZE_CLASS: i64 = 117;
    pub const ATTRIBUTE_ID: i64 = 119;
    pub const REAL_PERCENT: i64 = 121;
    pub const FITTING_SLOTS: i64 = 122;
    pub const MODIFIER_RELATIVE_PERCENT: i64 = 124;
    pub const ABSOLUTE_PERCENT: i64 = 127;
    pub const SLOT: i64 = 136;
    pub const BOOLEAN: i64 = 137;
    pub const BONUS: i64 = 139;
    pub const LEVEL: i64 = 140;
    pub const HARDPOINTS: i64 = 141;
    pub const MODIFIER_REAL_PERCENT: i64 = 205;
}

/// Formats one value of `unit_id` (`unit_display` is the unit's display
/// name, e.g. `m3`).
#[must_use]
pub fn format_attribute(
    value: Decimal,
    unit_id: Option<i64>,
    unit_display: Option<&str>,
    names: &dyn AttributeNames,
) -> FormattedValue {
    let hundred = Decimal::ONE_HUNDRED;
    let plain = |display: String| FormattedValue {
        display,
        link: None,
    };
    let with_unit = |number: Decimal| match unit_display.filter(|unit| !unit.is_empty()) {
        Some(unit) => format!("{} {unit}", number_text(number)),
        None => number_text(number),
    };
    let percent = |number: Decimal| format!("{} %", number_text(number));
    let id = value.to_i64().filter(|_| value.fract().is_zero());
    // An id nothing names stays a bare id: no thousands separators.
    let raw_id = || id.map_or_else(|| number_text(value), |id| id.to_string());
    match unit_id {
        // Rounded first: a 0.01 ms reload is "0 s" (the client's "0.0s"),
        // not a tiny value worth significant digits.
        Some(unit::MILLISECONDS) => plain(format!(
            "{} s",
            number_text(
                (value / Decimal::ONE_THOUSAND)
                    .round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
            )
        )),
        Some(unit::MULTIPLIER) => plain(format!("{} x", number_text(value))),
        Some(
            unit::PERCENTAGE
            | unit::REAL_PERCENT
            | unit::MODIFIER_RELATIVE_PERCENT
            | unit::MODIFIER_REAL_PERCENT,
        ) => plain(percent(value)),
        Some(unit::INVERSE_ABSOLUTE_PERCENT | unit::INVERSED_MODIFIER_PERCENT) => {
            plain(percent((Decimal::ONE - value) * hundred))
        }
        Some(unit::MODIFIER_PERCENT) => plain(percent((value - Decimal::ONE) * hundred)),
        Some(unit::ABSOLUTE_PERCENT) => plain(percent(value * hundred)),
        Some(unit::GROUP_ID) => match id.and_then(|id| Some((id, names.group_name(id)?))) {
            Some((id, name)) => FormattedValue {
                display: name.clone(),
                link: Some(ValueLink::Group { id, name }),
            },
            None => plain(raw_id()),
        },
        Some(unit::TYPE_ID) => match id.and_then(|id| Some((id, names.type_name(id)?))) {
            Some((id, name)) => FormattedValue {
                display: name.clone(),
                link: Some(ValueLink::Type { id, name }),
            },
            None => plain(raw_id()),
        },
        Some(unit::ATTRIBUTE_ID) => plain(
            id.and_then(|id| names.attribute_name(id))
                .unwrap_or_else(raw_id),
        ),
        Some(unit::SIZE_CLASS) => plain(
            match id {
                Some(1) => "Small",
                Some(2) => "Medium",
                Some(3) => "Large",
                Some(4) => "X-Large",
                _ => return plain(number_text(value)),
            }
            .to_string(),
        ),
        Some(unit::BOOLEAN) => plain(if value.is_zero() { "No" } else { "Yes" }.to_string()),
        Some(unit::LEVEL) => plain(format!("Level {}", number_text(value))),
        Some(unit::BONUS) => plain(format!("+{}", number_text(value))),
        Some(unit::SLOT | unit::HARDPOINTS | unit::FITTING_SLOTS) => plain(number_text(value)),
        _ => plain(with_unit(value)),
    }
}

/// Grouped thousands and at most two decimals, trailing zeros dropped. A
/// value two decimals would round to 0 keeps three significant digits
/// instead (Antimatter Charge S is 0.0025 m3).
fn number_text(value: Decimal) -> String {
    let two_decimals = value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
    let rounded = if two_decimals.is_zero() && !value.is_zero() {
        value.round_sf(3).unwrap_or(value)
    } else {
        two_decimals
    }
    .normalize();
    let text = rounded.abs().to_string();
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
    let mut grouped = String::with_capacity(text.len() + whole.len() / 3);
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    let sign = if rounded.is_sign_negative() && !rounded.is_zero() {
        "-"
    } else {
        ""
    };
    match fraction {
        "" => format!("{sign}{grouped}"),
        fraction => format!("{sign}{grouped}.{fraction}"),
    }
}

/// Every attribute a type shows, grouped by category in category id
/// order (uncategorised last, as "Other"), rows in attribute id order. A
/// zero is left out unless the attribute is shown when zero.
#[must_use]
pub fn attribute_groups(
    values: &[TypeAttributeValue],
    names: &dyn AttributeNames,
) -> Vec<AttributeGroup> {
    let mut groups: BTreeMap<(bool, i64), AttributeGroup> = BTreeMap::new();
    let mut sorted: Vec<&TypeAttributeValue> = values.iter().collect();
    sorted.sort_by_key(|value| value.attribute_id);
    for value in sorted {
        if value.value.is_zero() && !value.display_when_zero {
            continue;
        }
        let formatted = format_attribute(
            value.value,
            value.unit_id,
            value.unit_display_name.as_deref(),
            names,
        );
        // CCP's category 9 is literally named "NULL" ("not going into a
        // category"); it joins the uncategorised, which sort last.
        let category = value
            .category_id
            .zip(value.category_name.as_deref())
            .filter(|(_, name)| {
                !name.trim().is_empty() && !name.trim().eq_ignore_ascii_case("null")
            });
        let key = (category.is_none(), category.map_or(0, |(id, _)| id));
        groups
            .entry(key)
            .or_insert_with(|| AttributeGroup {
                category: category
                    .map_or_else(|| "Other".to_string(), |(_, name)| name.to_string()),
                rows: Vec::new(),
            })
            .rows
            .push(AttributeRow {
                attribute_id: value.attribute_id,
                label: value.display_name.clone(),
                display: formatted.display,
                raw: value.value,
                unit_id: value.unit_id,
                high_is_good: value.high_is_good,
                link: formatted.link,
            });
    }
    groups.into_values().collect()
}

/// A ship bonus amount: `4%`, `+5`, `5x`, `5,000 m`.
#[must_use]
pub fn format_bonus(bonus: Decimal, unit_id: Option<i64>, unit_display: Option<&str>) -> String {
    let number = number_text(bonus);
    match unit_id {
        Some(unit::PERCENTAGE | unit::REAL_PERCENT | unit::MODIFIER_RELATIVE_PERCENT) => {
            format!("{number}%")
        }
        Some(unit::BONUS) => format!("+{number}"),
        Some(unit::MULTIPLIER) => format!("{number}x"),
        _ => match unit_display.filter(|unit| !unit.is_empty()) {
            Some(unit) => format!("{number} {unit}"),
            None => number,
        },
    }
}

#[cfg(test)]
mod tests;
