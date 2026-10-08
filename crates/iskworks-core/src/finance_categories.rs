//! Pure rollup of an SDE market-group path into an Analytics category.
//!
//! The rules match on market-group *names* (root first) so they read like the
//! in-game market tree. The result is materialized per type into
//! `sde_type_categories` at SDE import; nothing walks the tree per query.

use std::collections::{HashMap, HashSet};

pub const CATEGORY_OTHER: &str = "Other";

/// Every category a type can roll up to, in display-neutral order.
pub const FINANCE_CATEGORIES: &[&str] = &[
    "PLEX & Injectors",
    "Ships",
    "Modules",
    "Rigs",
    "Mutaplasmids",
    "Ammo & Charges",
    "Drones",
    "Blueprints",
    "Fuel Blocks",
    "Components",
    "Ores & Minerals",
    "Ice & Moon Products",
    "Planetary Materials",
    "Materials",
    "Research & Datacores",
    "Implants & Boosters",
    "Skills",
    "Filaments & Trade Goods",
    "Structures",
    "Cosmetics",
    CATEGORY_OTHER,
];

/// Classify a type from its market-group path (root first). Types with no
/// market group fall back to their SDE group name, then to "Other".
#[must_use]
pub fn classify_type(market_path: &[&str], group_name: Option<&str>) -> &'static str {
    if market_path.is_empty() {
        return classify_group_name(group_name);
    }
    classify_market_path(market_path).unwrap_or(CATEGORY_OTHER)
}

fn classify_market_path(path: &[&str]) -> Option<&'static str> {
    let second = path.get(1).copied();
    let third = path.get(2).copied();
    Some(match path[0] {
        "Pilot's Services" => "PLEX & Injectors",
        "Ships" => "Ships",
        "Ship Equipment" => "Modules",
        "Ship and Module Modifications" => match second {
            Some("Rigs") => "Rigs",
            Some("Mutaplasmids") => "Mutaplasmids",
            _ => "Modules",
        },
        "Ammunition & Charges" => "Ammo & Charges",
        "Drones" => "Drones",
        "Blueprints & Reactions" => "Blueprints",
        "Manufacture & Research" => match (second, third) {
            (Some("Components"), Some("Fuel Blocks")) => "Fuel Blocks",
            (Some("Components"), _) => "Components",
            (Some("Research Equipment"), _) => "Research & Datacores",
            (Some("Materials"), Some("Raw Materials" | "Minerals")) => "Ores & Minerals",
            (Some("Materials"), Some("Ice Products" | "Reaction Materials")) => {
                "Ice & Moon Products"
            }
            (Some("Materials"), Some("Planetary Materials")) => "Planetary Materials",
            (Some("Materials"), Some("R.Db")) => "Research & Datacores",
            (Some("Materials"), _) => "Materials",
            _ => "Materials",
        },
        "Implants & Boosters" => "Implants & Boosters",
        "Skills" => "Skills",
        "Trade Goods" => "Filaments & Trade Goods",
        "Structures"
        | "Structure Equipment"
        | "Structure Modifications"
        | "Planetary Infrastructure" => "Structures",
        "Ship SKINs" | "Apparel" | "Personalization" | "Special Edition Assets" => "Cosmetics",
        _ => return None,
    })
}

fn classify_group_name(group_name: Option<&str>) -> &'static str {
    let Some(name) = group_name else {
        return CATEGORY_OTHER;
    };
    let lower = name.to_lowercase();
    if lower.starts_with("abyssal") || lower.starts_with("mutated") {
        "Modules"
    } else if lower.contains("mutaplasmid") {
        "Mutaplasmids"
    } else if lower.contains("blueprint") {
        "Blueprints"
    } else {
        CATEGORY_OTHER
    }
}

/// A market group as stored in the SDE: `(id, name, parent id)`.
pub type MarketGroupRow = (i64, String, Option<i64>);
/// A type as stored in the SDE: `(type id, market group id, SDE group name)`.
pub type TypeRow = (i64, Option<i64>, Option<String>);

/// Classify every type in one pass. Market-group paths are built once per
/// group; a parent cycle or dangling parent just truncates the path.
#[must_use]
pub fn classify_all(groups: &[MarketGroupRow], types: &[TypeRow]) -> Vec<(i64, &'static str)> {
    let by_id: HashMap<i64, (&str, Option<i64>)> = groups
        .iter()
        .map(|(id, name, parent)| (*id, (name.as_str(), *parent)))
        .collect();
    let mut paths: HashMap<i64, Vec<&str>> = HashMap::new();
    types
        .iter()
        .map(|(type_id, market_group_id, group_name)| {
            let path = match market_group_id {
                Some(id) => paths
                    .entry(*id)
                    .or_insert_with(|| market_path(*id, &by_id))
                    .as_slice(),
                None => &[],
            };
            (*type_id, classify_type(path, group_name.as_deref()))
        })
        .collect()
}

fn market_path<'a>(id: i64, by_id: &HashMap<i64, (&'a str, Option<i64>)>) -> Vec<&'a str> {
    let mut path = Vec::new();
    let mut seen = HashSet::new();
    let mut current = Some(id);
    while let Some(id) = current {
        let Some((name, parent)) = by_id.get(&id) else {
            break;
        };
        if !seen.insert(id) {
            break;
        }
        path.push(*name);
        current = *parent;
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_all_walks_parents_and_falls_back() {
        let groups = vec![
            (1, "Manufacture & Research".to_string(), None),
            (2, "Components".to_string(), Some(1)),
            (3, "Fuel Blocks".to_string(), Some(2)),
            (9, "Loop A".to_string(), Some(10)),
            (10, "Loop B".to_string(), Some(9)),
        ];
        let types = vec![
            (100, Some(3), None),
            (101, Some(2), None),
            (102, None, Some("Abyssal Armor Plate".to_string())),
            (103, Some(9), None),
            (104, Some(999), None),
        ];
        let out: HashMap<_, _> = classify_all(&groups, &types).into_iter().collect();
        assert_eq!(out[&100], "Fuel Blocks");
        assert_eq!(out[&101], "Components");
        assert_eq!(out[&102], "Modules");
        assert_eq!(out[&103], "Other");
        assert_eq!(out[&104], "Other");
    }

    fn c(path: &[&str]) -> &'static str {
        classify_type(path, None)
    }

    #[test]
    fn fuel_blocks_split_from_other_components() {
        assert_eq!(
            c(&["Manufacture & Research", "Components", "Fuel Blocks"]),
            "Fuel Blocks"
        );
        assert_eq!(
            c(&[
                "Manufacture & Research",
                "Components",
                "Advanced Components"
            ]),
            "Components"
        );
    }

    #[test]
    fn fuel_block_blueprints_are_blueprints_not_fuel_blocks() {
        assert_eq!(
            c(&[
                "Blueprints & Reactions",
                "Manufacture & Research",
                "Components",
                "Fuel Blocks"
            ]),
            "Blueprints"
        );
    }

    #[test]
    fn mutaplasmids_and_rigs_split_from_other_modifications() {
        assert_eq!(
            c(&["Ship and Module Modifications", "Mutaplasmids"]),
            "Mutaplasmids"
        );
        assert_eq!(
            c(&["Ship and Module Modifications", "Rigs", "Armor Rigs"]),
            "Rigs"
        );
        assert_eq!(
            c(&["Ship and Module Modifications", "Subsystems"]),
            "Modules"
        );
    }

    #[test]
    fn materials_subtrees() {
        let m = "Manufacture & Research";
        assert_eq!(
            c(&[m, "Materials", "Raw Materials", "Standard Ores"]),
            "Ores & Minerals"
        );
        assert_eq!(c(&[m, "Materials", "Minerals"]), "Ores & Minerals");
        assert_eq!(c(&[m, "Materials", "Ice Products"]), "Ice & Moon Products");
        assert_eq!(
            c(&[m, "Materials", "Reaction Materials"]),
            "Ice & Moon Products"
        );
        assert_eq!(
            c(&[
                m,
                "Materials",
                "Planetary Materials",
                "Raw Planetary Materials"
            ]),
            "Planetary Materials"
        );
        assert_eq!(c(&[m, "Materials", "R.Db"]), "Research & Datacores");
        assert_eq!(c(&[m, "Materials", "Gas Clouds Materials"]), "Materials");
        assert_eq!(c(&[m, "Research Equipment"]), "Research & Datacores");
    }

    #[test]
    fn plex_is_separate_from_everything() {
        assert_eq!(c(&["Pilot's Services", "PLEX"]), "PLEX & Injectors");
        assert_eq!(
            c(&["Pilot's Services", "Skill Trading"]),
            "PLEX & Injectors"
        );
    }

    #[test]
    fn simple_roots() {
        assert_eq!(c(&["Ships", "Battleships"]), "Ships");
        assert_eq!(c(&["Ship Equipment", "Shield"]), "Modules");
        assert_eq!(c(&["Trade Goods", "Filaments"]), "Filaments & Trade Goods");
        assert_eq!(c(&["Ship SKINs", "Cruisers"]), "Cosmetics");
        assert_eq!(c(&["Structure Equipment", "Service Modules"]), "Structures");
        assert_eq!(c(&["Skills", "Spaceship Command"]), "Skills");
    }

    #[test]
    fn unknown_root_is_other() {
        assert_eq!(c(&["Brand New Root", "Thing"]), "Other");
    }

    #[test]
    fn types_without_market_group_fall_back_on_group_name() {
        assert_eq!(
            classify_type(&[], Some("Abyssal Shield Extender")),
            "Modules"
        );
        assert_eq!(classify_type(&[], Some("Ship Blueprint")), "Blueprints");
        assert_eq!(classify_type(&[], Some("Something Else")), "Other");
        assert_eq!(classify_type(&[], None), "Other");
    }

    #[test]
    fn every_rule_output_is_a_listed_category() {
        let samples: &[&[&str]] = &[
            &["Pilot's Services"],
            &["Ships"],
            &["Ship Equipment"],
            &["Ship and Module Modifications", "Rigs"],
            &["Ship and Module Modifications", "Mutaplasmids"],
            &["Ammunition & Charges"],
            &["Drones"],
            &["Blueprints & Reactions"],
            &["Manufacture & Research", "Components", "Fuel Blocks"],
            &["Manufacture & Research", "Components"],
            &["Manufacture & Research", "Materials", "Minerals"],
            &["Manufacture & Research", "Materials", "Ice Products"],
            &["Manufacture & Research", "Materials", "Planetary Materials"],
            &["Manufacture & Research", "Materials"],
            &["Manufacture & Research", "Research Equipment"],
            &["Implants & Boosters"],
            &["Skills"],
            &["Trade Goods"],
            &["Structures"],
            &["Apparel"],
            &["Nope"],
        ];
        for sample in samples {
            assert!(FINANCE_CATEGORIES.contains(&c(sample)), "{sample:?}");
        }
    }
}
