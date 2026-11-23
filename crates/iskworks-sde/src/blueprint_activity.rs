//! Read models for a blueprint's industry activities, as the game's
//! blueprint info window lists them.

use super::*;

/// An industry activity a blueprint or reaction formula supports, in the
/// order the game's info window offers them.
#[derive(Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlueprintActivityKind {
    Manufacturing,
    ResearchMaterial,
    ResearchTime,
    Copying,
    Invention,
    Reaction,
}

impl BlueprintActivityKind {
    pub const ALL: [Self; 6] = [
        Self::Manufacturing,
        Self::ResearchMaterial,
        Self::ResearchTime,
        Self::Copying,
        Self::Invention,
        Self::Reaction,
    ];

    /// The activity's key in the SDE's `blueprints.jsonl`, also how it is
    /// stored.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manufacturing => "manufacturing",
            Self::ResearchMaterial => "research_material",
            Self::ResearchTime => "research_time",
            Self::Copying => "copying",
            Self::Invention => "invention",
            Self::Reaction => "reaction",
        }
    }
}

impl FromStr for BlueprintActivityKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| format!("unknown blueprint activity: {value}"))
    }
}

/// Everything the active import knows about one blueprint's activities.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintActivities {
    /// The most runs one job, and so one copy, can hold.
    pub max_production_limit: Option<i64>,
    /// In [`BlueprintActivityKind`] order.
    pub activities: Vec<BlueprintActivity>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintActivity {
    pub kind: BlueprintActivityKind,
    /// Base time for one run, before any skill or facility bonus.
    pub duration_seconds: Option<i64>,
    /// Consumed per run, in SDE order.
    pub materials: Vec<ActivityMaterial>,
    /// What a run yields: items for manufacturing and reactions, blueprint
    /// copies for invention. Empty for research and copying.
    pub products: Vec<ActivityProduct>,
    /// In SDE order.
    pub skills: Vec<RequiredSkill>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityMaterial {
    pub type_id: i64,
    pub type_name: String,
    pub quantity: i64,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityProduct {
    pub type_id: i64,
    pub type_name: String,
    /// Units per run; for invention, the runs on the invented copy.
    pub quantity: i64,
    /// The base success chance in `(0, 1]`, on invention products only.
    pub probability: Option<Decimal>,
}
