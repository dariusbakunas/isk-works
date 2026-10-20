//! Rollup of an SDE type into an Inventory › Runway / Consumption category.
//!
//! Separate from the Finance rollup (`finance_categories`), which merges moon
//! materials and reaction intermediates into one market-tree bucket. Industry
//! inputs read better split by how they are made, so the rules match the SDE
//! *group* name, and the SDE *category* name for whole families (planetary
//! goods, ships, modules).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Display order is the declaration order.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum MaterialCategory {
    MoonMaterials,
    Minerals,
    IceProducts,
    ReactionIntermediates,
    Planetary,
    FuelBlocks,
    Components,
    ShipsAndModules,
    Other,
}

impl MaterialCategory {
    pub const ALL: [MaterialCategory; 9] = [
        MaterialCategory::MoonMaterials,
        MaterialCategory::Minerals,
        MaterialCategory::IceProducts,
        MaterialCategory::ReactionIntermediates,
        MaterialCategory::Planetary,
        MaterialCategory::FuelBlocks,
        MaterialCategory::Components,
        MaterialCategory::ShipsAndModules,
        MaterialCategory::Other,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            MaterialCategory::MoonMaterials => "Moon Materials",
            MaterialCategory::Minerals => "Minerals",
            MaterialCategory::IceProducts => "Ice Products",
            MaterialCategory::ReactionIntermediates => "Reaction Intermediates",
            MaterialCategory::Planetary => "Planetary",
            MaterialCategory::FuelBlocks => "Fuel Blocks",
            MaterialCategory::Components => "Components",
            MaterialCategory::ShipsAndModules => "Ships & Modules",
            MaterialCategory::Other => "Other",
        }
    }
}

impl fmt::Display for MaterialCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

impl FromStr for MaterialCategory {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        MaterialCategory::ALL
            .into_iter()
            .find(|category| category.label() == value)
            .ok_or_else(|| format!("unknown material category: {value}"))
    }
}

impl Serialize for MaterialCategory {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.label())
    }
}

impl<'de> Deserialize<'de> for MaterialCategory {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

const REACTION_INTERMEDIATE_GROUPS: &[&str] = &[
    "Intermediate Materials",
    "Composite",
    "Hybrid Polymers",
    "Biochemical Material",
    "Molecular-Forged Materials",
];

/// Classify a type from its SDE group and category names. Unknown or missing
/// names fall back to [`MaterialCategory::Other`].
#[must_use]
pub fn material_category(
    group_name: Option<&str>,
    category_name: Option<&str>,
) -> MaterialCategory {
    match category_name {
        Some("Planetary Commodities" | "Planetary Resources") => {
            return MaterialCategory::Planetary
        }
        Some("Ship" | "Module") => return MaterialCategory::ShipsAndModules,
        _ => {}
    }
    let Some(group) = group_name else {
        return MaterialCategory::Other;
    };
    match group {
        "Mineral" => MaterialCategory::Minerals,
        "Moon Materials" => MaterialCategory::MoonMaterials,
        "Ice Product" => MaterialCategory::IceProducts,
        "Fuel Block" => MaterialCategory::FuelBlocks,
        "Tool" => MaterialCategory::Components,
        group if REACTION_INTERMEDIATE_GROUPS.contains(&group) => {
            MaterialCategory::ReactionIntermediates
        }
        group if group.ends_with("Components") => MaterialCategory::Components,
        _ => MaterialCategory::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(group: &str, category: &str) -> MaterialCategory {
        material_category(Some(group), Some(category))
    }

    #[test]
    fn minerals_moon_materials_and_ice_match_their_groups() {
        assert_eq!(classify("Mineral", "Material"), MaterialCategory::Minerals);
        assert_eq!(
            classify("Moon Materials", "Material"),
            MaterialCategory::MoonMaterials
        );
        assert_eq!(
            classify("Ice Product", "Material"),
            MaterialCategory::IceProducts
        );
    }

    #[test]
    fn reaction_products_are_intermediates() {
        for group in REACTION_INTERMEDIATE_GROUPS {
            assert_eq!(
                classify(group, "Material"),
                MaterialCategory::ReactionIntermediates,
                "{group}"
            );
        }
    }

    #[test]
    fn planetary_goods_match_by_sde_category() {
        assert_eq!(
            classify("Specialized Commodities - Tier 3", "Planetary Commodities"),
            MaterialCategory::Planetary
        );
        assert_eq!(
            classify("Planet Solid - Raw Resource", "Planetary Resources"),
            MaterialCategory::Planetary
        );
    }

    #[test]
    fn fuel_blocks_components_and_tools() {
        assert_eq!(
            classify("Fuel Block", "Material"),
            MaterialCategory::FuelBlocks
        );
        for group in [
            "Construction Components",
            "Capital Construction Components",
            "Advanced Capital Construction Components",
            "Hybrid Tech Components",
            "Structure Components",
        ] {
            assert_eq!(
                classify(group, "Commodity"),
                MaterialCategory::Components,
                "{group}"
            );
        }
        assert_eq!(classify("Tool", "Commodity"), MaterialCategory::Components);
    }

    #[test]
    fn ships_and_modules_match_by_sde_category() {
        assert_eq!(
            classify("Dreadnought", "Ship"),
            MaterialCategory::ShipsAndModules
        );
        assert_eq!(
            classify("Armor Repair Unit", "Module"),
            MaterialCategory::ShipsAndModules
        );
    }

    #[test]
    fn anything_else_is_other() {
        assert_eq!(
            classify("Salvaged Materials", "Material"),
            MaterialCategory::Other
        );
        assert_eq!(material_category(None, None), MaterialCategory::Other);
    }

    #[test]
    fn labels_round_trip_through_serde_and_from_str() {
        for category in MaterialCategory::ALL {
            let json = serde_json::to_string(&category).unwrap();
            assert_eq!(json, format!("\"{}\"", category.label()));
            assert_eq!(
                serde_json::from_str::<MaterialCategory>(&json).unwrap(),
                category
            );
            assert_eq!(category.label().parse::<MaterialCategory>(), Ok(category));
        }
        assert!("Nonsense".parse::<MaterialCategory>().is_err());
    }
}
