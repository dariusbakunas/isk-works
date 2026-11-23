//! Read models for the SDE's invention data.

use super::*;

/// What one invention attempt on a source (Tech I) blueprint consumes and
/// can yield.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventionRecipe {
    pub source_blueprint_type_id: i64,
    pub source_blueprint_name: String,
    pub duration_seconds: Option<i64>,
    /// Time to copy one run of the source blueprint, when the SDE has it.
    pub copying_duration_seconds: Option<i64>,
    /// Consumed per attempt, whether it succeeds or not.
    pub materials: Vec<RecipeLine>,
    /// Every blueprint copy the source can yield, in SDE order. The player
    /// picks one per job.
    pub products: Vec<InventionProduct>,
    pub skills: Vec<InventionSkillRequirement>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventionProduct {
    /// The invented blueprint.
    pub blueprint_type_id: i64,
    pub blueprint_name: String,
    /// The item that blueprint manufactures, when the import has its
    /// manufacturing recipe.
    pub product_type_id: Option<i64>,
    pub product_name: Option<String>,
    /// Runs on the invented copy before any decryptor.
    pub base_runs: i64,
    /// The base success chance, in `(0, 1]`.
    pub probability: Decimal,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventionSkillRequirement {
    pub skill_type_id: i64,
    pub skill_name: String,
    pub level: i16,
}

/// A decryptor's effect on an invention attempt: the chance is multiplied,
/// the copy's ME, TE and runs are added to.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Decryptor {
    pub type_id: i64,
    pub name: String,
    pub probability_multiplier: Decimal,
    pub me_modifier: i32,
    pub te_modifier: i32,
    pub run_modifier: i32,
}

/// An item whose blueprint can be invented.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventableProduct {
    pub product_type_id: i64,
    pub product_name: String,
    pub group_name: Option<String>,
    /// The invented blueprint, which manufactures the product.
    pub blueprint_type_id: i64,
    pub source_blueprint_type_id: i64,
    pub source_blueprint_name: String,
    /// How many blueprints the source can invent; above 1 the player picks.
    pub source_output_count: i64,
}
