use super::*;

fn names() -> NameMaps {
    NameMaps {
        types: BTreeMap::from([(34, "Tritanium".to_string())]),
        groups: BTreeMap::from([(1396, "Missile Guidance Computer".to_string())]),
        attributes: BTreeMap::from([(9, "Structure Hitpoints".to_string())]),
    }
}

fn dec(text: &str) -> Decimal {
    text.parse().unwrap()
}

fn shown(value: &str, unit_id: Option<i64>, unit_display: Option<&str>) -> String {
    format_attribute(dec(value), unit_id, unit_display, &names()).display
}

fn value(
    attribute_id: i64,
    label: &str,
    unit: Option<(i64, &str)>,
    category: Option<(i64, &str)>,
    value: &str,
) -> TypeAttributeValue {
    TypeAttributeValue {
        attribute_id,
        display_name: label.to_string(),
        unit_id: unit.map(|(id, _)| id),
        unit_display_name: unit.map(|(_, display)| display.to_string()),
        category_id: category.map(|(id, _)| id),
        category_name: category.map(|(_, name)| name.to_string()),
        display_when_zero: false,
        high_is_good: true,
        value: dec(value),
    }
}

/// Missile Range Disruption Script (40334) as stored by SDE 3482594; the
/// expected rows are its in-game Attributes tab.
#[test]
fn the_script_reads_like_the_client() {
    let misc = Some((7, "Miscellaneous"));
    let values = [
        value(
            128,
            "Charge size",
            Some((117, "1=small 2=medium 3=l")),
            misc,
            "1.0",
        ),
        value(
            137,
            "Used with (Launcher Group)",
            Some((115, "groupID")),
            misc,
            "1396.0",
        ),
        value(422, "Tech Level", Some((140, "Level")), misc, "1.0"),
        value(
            2023,
            "Modification of Explosion Radius Bonus",
            Some((105, "%")),
            misc,
            "-100.0",
        ),
        value(
            2024,
            "Modification of Explosion Velocity Bonus",
            Some((105, "%")),
            misc,
            "-100.0",
        ),
        value(
            2025,
            "Modification of Missile Velocity Bonus",
            Some((105, "%")),
            misc,
            "100.0",
        ),
        value(
            2026,
            "Modification of Flight Time Bonus",
            Some((105, "%")),
            misc,
            "100.0",
        ),
    ];

    let groups = attribute_groups(&values, &names());

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].category, "Miscellaneous");
    let rows: Vec<(&str, &str)> = groups[0]
        .rows
        .iter()
        .map(|row| (row.label.as_str(), row.display.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            ("Charge size", "Small"),
            ("Used with (Launcher Group)", "Missile Guidance Computer"),
            ("Tech Level", "Level 1"),
            ("Modification of Explosion Radius Bonus", "-100 %"),
            ("Modification of Explosion Velocity Bonus", "-100 %"),
            ("Modification of Missile Velocity Bonus", "100 %"),
            ("Modification of Flight Time Bonus", "100 %"),
        ]
    );
    assert_eq!(
        groups[0].rows[1].link,
        Some(ValueLink::Group {
            id: 1396,
            name: "Missile Guidance Computer".to_string(),
        })
    );
    assert_eq!(groups[0].rows[1].raw, dec("1396.0"));
}

#[test]
fn plain_units_follow_the_number() {
    assert_eq!(shown("1.0", Some(9), Some("m3")), "1 m3");
    assert_eq!(shown("1155000", Some(2), Some("kg")), "1,155,000 kg");
    assert_eq!(shown("3.6328", Some(10), Some("m/sec")), "3.63 m/sec");
    assert_eq!(shown("0.125", Some(3), Some("sec")), "0.13 sec");
    assert_eq!(shown("-2500.5", Some(113), Some("HP")), "-2,500.5 HP");
}

#[test]
fn values_without_a_unit_are_bare_numbers() {
    assert_eq!(shown("12345.678", None, None), "12,345.68");
    // A unit the formatter doesn't know, with no display name.
    assert_eq!(shown("7", Some(103), None), "7");
}

#[test]
fn milliseconds_show_as_seconds() {
    assert_eq!(shown("12500", Some(101), Some("s")), "12.5 s");
}

#[test]
fn multipliers_and_bonuses() {
    assert_eq!(shown("1.25", Some(104), Some("x")), "1.25 x");
    assert_eq!(shown("5", Some(139), Some("+")), "+5");
}

#[test]
fn percentages_by_kind() {
    assert_eq!(shown("4", Some(105), Some("%")), "4 %");
    assert_eq!(shown("12.5", Some(121), Some("%")), "12.5 %");
    assert_eq!(shown("-15", Some(124), Some("%")), "-15 %");
    assert_eq!(shown("-15", Some(205), Some("%")), "-15 %");
    // A resonance of 0.75 is a 25 % resistance. Checked on the dev SDE: the
    // Harpy's resonances come out as the T2 Caldari profile (shield
    // 0/80/70/50 %, hull 33 %).
    // TODO(golden): pin 109/111/127 against in-game screenshots.
    assert_eq!(shown("0.75", Some(108), Some("%")), "25 %");
    assert_eq!(shown("1.1", Some(109), Some("%")), "10 %");
    assert_eq!(shown("0.9", Some(109), Some("%")), "-10 %");
    assert_eq!(shown("0.85", Some(111), Some("%")), "15 %");
    assert_eq!(shown("0.5", Some(127), Some("%")), "50 %");
}

#[test]
fn size_classes_booleans_levels_and_slots() {
    assert_eq!(shown("1", Some(117), None), "Small");
    assert_eq!(shown("2", Some(117), None), "Medium");
    assert_eq!(shown("3", Some(117), None), "Large");
    assert_eq!(shown("4", Some(117), None), "X-Large");
    assert_eq!(shown("9", Some(117), None), "9");
    assert_eq!(shown("1", Some(137), None), "Yes");
    assert_eq!(shown("0", Some(137), None), "No");
    assert_eq!(shown("5", Some(140), Some("Level")), "Level 5");
    assert_eq!(shown("4", Some(122), None), "4");
    assert_eq!(shown("3", Some(136), Some("Slot")), "3");
    assert_eq!(shown("2", Some(141), Some("hardpoints")), "2");
}

#[test]
fn ids_become_names_and_links() {
    let tritanium = format_attribute(dec("34"), Some(116), Some("typeID"), &names());
    assert_eq!(tritanium.display, "Tritanium");
    assert_eq!(
        tritanium.link,
        Some(ValueLink::Type {
            id: 34,
            name: "Tritanium".to_string(),
        })
    );
    let attribute = format_attribute(dec("9"), Some(119), Some("attributeID"), &names());
    assert_eq!(attribute.display, "Structure Hitpoints");
    assert_eq!(attribute.link, None);
    // Unknown ids stay numbers, unlinked.
    let unknown = format_attribute(dec("77777"), Some(116), Some("typeID"), &names());
    assert_eq!(unknown.display, "77777");
    assert_eq!(unknown.link, None);
}

#[test]
fn zeros_are_hidden_unless_shown_when_zero() {
    let mut low_slots = value(12, "Low Slots", Some((122, "")), Some((1, "Fitting")), "0");
    low_slots.display_when_zero = true;
    let hidden = value(
        13,
        "Medium Slots",
        Some((122, "")),
        Some((1, "Fitting")),
        "0",
    );

    let groups = attribute_groups(&[low_slots, hidden], &names());

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].rows.len(), 1);
    assert_eq!(groups[0].rows[0].display, "0");
}

#[test]
fn groups_follow_category_id_with_uncategorised_last() {
    let values = [
        value(
            9,
            "Structure Hitpoints",
            Some((113, "HP")),
            Some((4, "Structure")),
            "350",
        ),
        value(500, "No category", None, None, "1"),
        value(
            263,
            "Shield Capacity",
            Some((113, "HP")),
            Some((2, "Shield")),
            "450",
        ),
        value(
            48,
            "CPU Output",
            Some((106, "tf")),
            Some((1, "Fitting")),
            "185",
        ),
        value(
            11,
            "Powergrid Output",
            Some((107, "MW")),
            Some((1, "Fitting")),
            "50",
        ),
    ];

    let groups = attribute_groups(&values, &names());

    let layout: Vec<(&str, Vec<i64>)> = groups
        .iter()
        .map(|group| {
            (
                group.category.as_str(),
                group.rows.iter().map(|row| row.attribute_id).collect(),
            )
        })
        .collect();
    assert_eq!(
        layout,
        [
            ("Fitting", vec![11, 48]),
            ("Shield", vec![263]),
            ("Structure", vec![9]),
            ("Other", vec![500]),
        ]
    );
}

#[test]
fn bonus_amounts_read_like_ship_traits() {
    assert_eq!(format_bonus(dec("4.0"), Some(105), Some("%")), "4%");
    assert_eq!(format_bonus(dec("37.5"), Some(105), Some("%")), "37.5%");
    assert_eq!(format_bonus(dec("5"), Some(139), Some("+")), "+5");
    assert_eq!(format_bonus(dec("5.0"), Some(104), Some("x")), "5x");
    assert_eq!(format_bonus(dec("5000.0"), Some(1), Some("m")), "5,000 m");
    assert_eq!(format_bonus(dec("2.0"), Some(144), Some("AU/s")), "2 AU/s");
    assert_eq!(format_bonus(dec("3"), None, None), "3");
}

#[test]
fn the_null_category_counts_as_uncategorised() {
    // dogmaAttributeCategories 9 is literally named "NULL": "attributes
    // already checked and not going into a category".
    let values = [
        value(
            1544,
            "Max Modules Of This Group Allowed",
            None,
            Some((9, "NULL")),
            "1",
        ),
        value(500, "No category", None, None, "2"),
        value(
            422,
            "Tech Level",
            Some((140, "Level")),
            Some((7, "Miscellaneous")),
            "2",
        ),
    ];

    let groups = attribute_groups(&values, &names());

    let layout: Vec<(&str, Vec<i64>)> = groups
        .iter()
        .map(|group| {
            (
                group.category.as_str(),
                group.rows.iter().map(|row| row.attribute_id).collect(),
            )
        })
        .collect();
    assert_eq!(
        layout,
        [("Miscellaneous", vec![422]), ("Other", vec![500, 1544])]
    );
}
