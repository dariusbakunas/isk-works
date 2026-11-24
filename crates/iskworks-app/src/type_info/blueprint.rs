//! A blueprint's industry activities, shaped like the game's blueprint
//! info window: per activity, its time, outcome, skills and materials.

use chrono::{DateTime, Utc};
use iskworks_core::skills::CharacterSkillsSnapshot;
use iskworks_core::{
    requirement_summary, requirement_tree, RequirementPilot, RequirementSummary, SkillNode,
};
use iskworks_sde::{BlueprintActivities, BlueprintActivityKind, SkillDefinition};
use rust_decimal::Decimal;
use serde::Serialize;

use super::TypeQuantity;

/// EVE's Blueprint category: blueprints and reaction formulas.
pub(super) const BLUEPRINT_CATEGORY_ID: i64 = 9;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeBlueprint {
    /// The most runs one job, and so one copy, can hold.
    pub max_runs: Option<i64>,
    /// The activities the blueprint has, in the game's order.
    pub activities: Vec<TypeBlueprintActivity>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeBlueprintActivity {
    pub kind: BlueprintActivityKind,
    /// Base time for one run, before skills and facility.
    pub time_seconds: Option<i64>,
    /// Items for manufacturing and reactions, blueprint copies for
    /// invention; empty for research and copying, whose outcome is the
    /// blueprint itself.
    pub outcome: Vec<TypeActivityOutcome>,
    pub skills: Vec<SkillNode>,
    /// For a character pilot: what it still has to train.
    pub missing: Option<RequirementSummary>,
    /// By item group, groups and items in SDE order.
    pub materials: Vec<TypeMaterialGroup>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeActivityOutcome {
    pub type_id: i64,
    pub name: String,
    /// Units per run; for invention, the runs on the invented copy.
    pub quantity: i64,
    /// The base success chance in `(0, 1]`, for invention only.
    pub probability: Option<Decimal>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeMaterialGroup {
    pub name: String,
    pub items: Vec<TypeQuantity>,
}

pub(super) fn blueprint_view(
    source: BlueprintActivities,
    definitions: &[SkillDefinition],
    pilot: RequirementPilot<'_>,
    character: Option<&CharacterSkillsSnapshot>,
    now: DateTime<Utc>,
) -> TypeBlueprint {
    TypeBlueprint {
        max_runs: source.max_production_limit,
        activities: source
            .activities
            .into_iter()
            .map(|activity| {
                let mut materials: Vec<TypeMaterialGroup> = Vec::new();
                for line in activity.materials {
                    let name = line.group_name.unwrap_or_else(|| "Other".to_string());
                    let item = TypeQuantity {
                        type_id: line.type_id,
                        name: line.type_name,
                        quantity: line.quantity,
                    };
                    match materials.iter_mut().find(|group| group.name == name) {
                        Some(group) => group.items.push(item),
                        None => materials.push(TypeMaterialGroup {
                            name,
                            items: vec![item],
                        }),
                    }
                }
                TypeBlueprintActivity {
                    kind: activity.kind,
                    time_seconds: activity.duration_seconds,
                    outcome: activity
                        .products
                        .into_iter()
                        .map(|product| TypeActivityOutcome {
                            type_id: product.type_id,
                            name: product.type_name,
                            quantity: product.quantity,
                            probability: product.probability,
                        })
                        .collect(),
                    skills: requirement_tree(&activity.skills, definitions, pilot),
                    missing: character.map(|snapshot| {
                        requirement_summary(&activity.skills, definitions, snapshot, now)
                    }),
                    materials,
                }
            })
            .collect(),
    }
}
