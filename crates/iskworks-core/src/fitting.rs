//! Ship fittings: what a fit holds, who flies it, and the stats an engine
//! calculates for it.
//!
//! Pure: the calculation itself sits behind [`FittingEngine`], implemented
//! by `iskworks-fitting` on EVEShip.fit's dogma engine. No engine types
//! appear here.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{InventoryError, WorkspaceId};

mod catalogue;
mod cost;
mod flyability;

pub use catalogue::{
    CatalogueEntry, CatalogueKind, FitCatalogue, Hardpoint, MarketGroup, ShipSlots,
};
pub use cost::{
    fit_cost, fit_needs, AssetHolding, AssetHoldingsRepository, CostLine, CostSection, FitCost,
};
pub use flyability::{
    evaluate_flyability, fit_requirements, requirement_plan, FitRequirements, FlyabilityStatus,
    MissingSkill, PilotFlyability,
};

/// Highest trainable skill level.
const MAX_LEVEL: i16 = 5;

/// Where an item sits on (or in) the ship.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotKind {
    High,
    Med,
    Low,
    Rig,
    Subsystem,
    Service,
    FighterTube,
    FighterBay,
    DroneBay,
    Cargo,
    Implant,
    Booster,
}

impl SlotKind {
    pub const ALL: [Self; 12] = [
        Self::High,
        Self::Med,
        Self::Low,
        Self::Rig,
        Self::Subsystem,
        Self::Service,
        Self::FighterTube,
        Self::FighterBay,
        Self::DroneBay,
        Self::Cargo,
        Self::Implant,
        Self::Booster,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Med => "med",
            Self::Low => "low",
            Self::Rig => "rig",
            Self::Subsystem => "subsystem",
            Self::Service => "service",
            Self::FighterTube => "fighter_tube",
            Self::FighterBay => "fighter_bay",
            Self::DroneBay => "drone_bay",
            Self::Cargo => "cargo",
            Self::Implant => "implant",
            Self::Booster => "booster",
        }
    }

    /// Slots with positions (`slot_index`); bays and cargo hold stacks.
    #[must_use]
    pub fn is_rack(self) -> bool {
        !matches!(self, Self::FighterBay | Self::DroneBay | Self::Cargo)
    }
}

impl FromStr for SlotKind {
    type Err = UnknownName;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == name)
            .ok_or_else(|| UnknownName(name.to_string()))
    }
}

/// The state a module is asked to be in; the engine reports the state it
/// actually reached, which can be lower.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleState {
    Offline,
    Online,
    Active,
    Overload,
}

impl ModuleState {
    pub const ALL: [Self; 4] = [Self::Offline, Self::Online, Self::Active, Self::Overload];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Online => "online",
            Self::Active => "active",
            Self::Overload => "overload",
        }
    }
}

impl FromStr for ModuleState {
    type Err = UnknownName;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|state| state.as_str() == name)
            .ok_or_else(|| UnknownName(name.to_string()))
    }
}

/// A name that is not a [`SlotKind`] or [`ModuleState`].
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UnknownName(pub String);

impl fmt::Display for UnknownName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown name {:?}", self.0)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum FittingError {
    /// The engine data has no ship of this type.
    UnknownShip(i64),
    /// The engine could not write the fit out (e.g. as EFT).
    Export,
    /// No attribute of that name.
    UnknownAttribute,
    /// No item at that index the engine knows.
    UnknownItem(usize),
    InvalidDamageProfile,
    InvalidSkillLevel {
        skill_type_id: i64,
        level: i16,
    },
}

/// One fitted or carried item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitItem {
    pub type_id: i64,
    pub slot_kind: SlotKind,
    /// Position within a rack, from 0; `None` for bays and cargo.
    pub slot_index: Option<i16>,
    /// Stack size for drones, fighters and cargo; 1 for modules.
    pub quantity: u32,
    /// The requested state.
    pub state: ModuleState,
    pub charge_type_id: Option<i64>,
    /// For a mutated (Abyssal) item: what it was before, and its rolls.
    /// Kept so a fit round-trips; not editable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutation: Option<FitMutation>,
}

/// How a mutated item differs from its base type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitMutation {
    pub base_type_id: i64,
    /// The rolled value per attribute id.
    pub attributes: BTreeMap<i64, f64>,
}

/// A fit as the engine calculates it: a hull and what it carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitInput {
    pub ship_type_id: i64,
    pub items: Vec<FitItem>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FitId(pub Uuid);

/// A saved fit. Fits belong to the workspace, like skill plans.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fit {
    pub id: FitId,
    pub name: String,
    pub description: String,
    pub ship_type_id: i64,
    /// In the order they were saved.
    pub items: Vec<FitItem>,
    /// By name; see [`canonical_tags`].
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Fit {
    /// What the engine calculates.
    #[must_use]
    pub fn input(&self) -> FitInput {
        FitInput {
            ship_type_id: self.ship_type_id,
            items: self.items.clone(),
        }
    }
}

const MAX_NAME_CHARS: usize = 100;
const MAX_DESCRIPTION_CHARS: usize = 2_000;
const MAX_ITEMS: usize = 512;
const MAX_SLOT_INDEX: i16 = 31;

/// What a create or update writes: the whole fit, items replaced.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitDraft {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub ship_type_id: i64,
    pub items: Vec<FitItem>,
}

impl FitDraft {
    /// The fit's shape: names, positions and quantities. Fitting rules (CPU,
    /// racks the ship has, skills) are not checked: a fit that breaks them
    /// still saves, and its calculation reports them.
    pub fn validate(&self) -> Result<(), String> {
        let name_chars = self.name.trim().chars().count();
        if name_chars == 0 || name_chars > MAX_NAME_CHARS {
            return Err(format!("a fit name is 1 to {MAX_NAME_CHARS} characters"));
        }
        if self.description.chars().count() > MAX_DESCRIPTION_CHARS {
            return Err(format!(
                "a fit description is at most {MAX_DESCRIPTION_CHARS} characters"
            ));
        }
        if self.items.len() > MAX_ITEMS {
            return Err(format!("a fit holds at most {MAX_ITEMS} items"));
        }
        let mut taken = HashSet::new();
        for item in &self.items {
            let kind = item.slot_kind.as_str();
            if item.slot_kind.is_rack() {
                // Implants and boosters count from 1, as EVE numbers them.
                let first = match item.slot_kind {
                    SlotKind::Implant | SlotKind::Booster => 1,
                    _ => 0,
                };
                let Some(index) = item
                    .slot_index
                    .filter(|index| (first..=MAX_SLOT_INDEX).contains(index))
                else {
                    return Err(format!(
                        "a {kind} item needs a position from {first} to {MAX_SLOT_INDEX}"
                    ));
                };
                if !taken.insert((item.slot_kind, index)) {
                    return Err(format!("two items are in {kind} {index}"));
                }
            } else if item.slot_index.is_some() {
                return Err(format!("a {kind} item has no position"));
            }
            if item.quantity == 0 {
                return Err(format!("a {kind} item needs a quantity of at least 1"));
            }
            let stacks = !item.slot_kind.is_rack() || item.slot_kind == SlotKind::FighterTube;
            if !stacks && item.quantity != 1 {
                return Err(format!("a {kind} item has a quantity of 1"));
            }
            if !item.slot_kind.is_rack() && item.charge_type_id.is_some() {
                return Err(format!("a {kind} item cannot hold a charge"));
            }
        }
        Ok(())
    }
}

const MAX_TAG_CHARS: usize = 40;
const MAX_TAGS_PER_FIT: usize = 20;
const MAX_TAG_EDIT_FITS: usize = 500;

/// A tag as stored: trimmed, inner whitespace collapsed to one space, 1 to
/// 40 characters. Tags group fits in the fit browser; case doesn't make a
/// different tag.
pub fn normalize_tag(tag: &str) -> Result<String, String> {
    let tag = tag.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars = tag.chars().count();
    if chars == 0 || chars > MAX_TAG_CHARS {
        return Err(format!("a tag is 1 to {MAX_TAG_CHARS} characters"));
    }
    Ok(tag)
}

fn normalize_tags(tags: &[String]) -> Result<Vec<String>, String> {
    tags.iter().map(|tag| normalize_tag(tag)).collect()
}

/// What a fit stores for `requested`: normalised, in the casing the workspace
/// already uses (`in_use`) so "cyno" joins "Cyno", each once whatever its
/// case, by name.
pub fn canonical_tags(requested: &[String], in_use: &[String]) -> Result<Vec<String>, String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in normalize_tags(requested)? {
        let key = tag.to_lowercase();
        if tags.iter().any(|held| held.to_lowercase() == key) {
            continue;
        }
        let existing = in_use.iter().find(|used| used.to_lowercase() == key);
        tags.push(existing.cloned().unwrap_or(tag));
    }
    if tags.len() > MAX_TAGS_PER_FIT {
        return Err(format!("a fit holds at most {MAX_TAGS_PER_FIT} tags"));
    }
    tags.sort_by_key(|tag| tag.to_lowercase());
    Ok(tags)
}

/// Tags added to and removed from many fits at once.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitTagEdit {
    pub fit_ids: Vec<FitId>,
    #[serde(default)]
    pub add: Vec<String>,
    #[serde(default)]
    pub remove: Vec<String>,
}

impl FitTagEdit {
    /// The edit with its tags normalised, or why it can't be made.
    pub fn validate(&self) -> Result<Self, String> {
        if self.fit_ids.is_empty() || self.fit_ids.len() > MAX_TAG_EDIT_FITS {
            return Err(format!("a tag edit names 1 to {MAX_TAG_EDIT_FITS} fits"));
        }
        if self.add.is_empty() && self.remove.is_empty() {
            return Err("a tag edit adds or removes a tag".to_string());
        }
        if self.add.len() > MAX_TAGS_PER_FIT || self.remove.len() > MAX_TAGS_PER_FIT {
            return Err(format!(
                "a tag edit adds or removes at most {MAX_TAGS_PER_FIT} tags"
            ));
        }
        Ok(Self {
            fit_ids: self.fit_ids.clone(),
            add: normalize_tags(&self.add)?,
            remove: normalize_tags(&self.remove)?,
        })
    }
}

#[async_trait]
pub trait FitRepository: Send + Sync {
    /// Every fit of the workspace, by name.
    async fn list_fits(&self, workspace_id: WorkspaceId) -> Result<Vec<Fit>, InventoryError>;

    async fn fit(
        &self,
        workspace_id: WorkspaceId,
        id: FitId,
    ) -> Result<Option<Fit>, InventoryError>;

    async fn create_fit(
        &self,
        workspace_id: WorkspaceId,
        draft: &FitDraft,
        now: DateTime<Utc>,
    ) -> Result<Fit, InventoryError>;

    /// Replaces the fit and every item in one transaction. `None` when the
    /// fit isn't in the workspace.
    async fn update_fit(
        &self,
        workspace_id: WorkspaceId,
        id: FitId,
        draft: &FitDraft,
        now: DateTime<Utc>,
    ) -> Result<Option<Fit>, InventoryError>;

    /// `false` when the fit isn't in the workspace.
    async fn delete_fit(
        &self,
        workspace_id: WorkspaceId,
        id: FitId,
    ) -> Result<bool, InventoryError>;

    /// The tags on the workspace's fits, each once (in one of its casings).
    async fn fit_tags_in_use(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<String>, InventoryError>;

    /// Replaces the fit's tags with `tags`, written as given. `None` when the
    /// fit isn't in the workspace. Doesn't change `updated_at`.
    async fn replace_fit_tags(
        &self,
        workspace_id: WorkspaceId,
        id: FitId,
        tags: &[String],
    ) -> Result<Option<Vec<String>>, InventoryError>;

    /// Adds `add` to and removes `remove` from every listed fit of the
    /// workspace, in one transaction; other fits are skipped. Removal ignores
    /// case; adding a tag a fit holds (in any case) leaves it as it is.
    async fn edit_fit_tags(
        &self,
        workspace_id: WorkspaceId,
        ids: &[FitId],
        add: &[String],
        remove: &[String],
    ) -> Result<(), InventoryError>;
}

/// Incoming damage, for effective hitpoints. Only the ratios matter.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DamageProfile {
    em: f64,
    thermal: f64,
    kinetic: f64,
    explosive: f64,
}

impl DamageProfile {
    pub fn new(em: f64, thermal: f64, kinetic: f64, explosive: f64) -> Result<Self, FittingError> {
        let parts = [em, thermal, kinetic, explosive];
        if parts.iter().any(|part| !part.is_finite() || *part < 0.0)
            || parts.iter().sum::<f64>() <= 0.0
        {
            return Err(FittingError::InvalidDamageProfile);
        }
        Ok(Self {
            em,
            thermal,
            kinetic,
            explosive,
        })
    }

    #[must_use]
    pub fn uniform() -> Self {
        Self {
            em: 1.0,
            thermal: 1.0,
            kinetic: 1.0,
            explosive: 1.0,
        }
    }

    /// Shares of the total, in EM, thermal, kinetic, explosive order.
    #[must_use]
    pub fn normalized(&self) -> [f64; 4] {
        let parts = [self.em, self.thermal, self.kinetic, self.explosive];
        let total: f64 = parts.iter().sum();
        parts.map(|part| part / total)
    }
}

/// A named damage profile the pilot can pick.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DamageProfilePreset {
    pub id: &'static str,
    pub name: &'static str,
    #[serde(flatten)]
    pub profile: DamageProfile,
}

/// The built-in profiles, uniform first: the four pure damage types, then
/// the pirate factions' rats as pyfa's `[NPC][Asteroid]` built-ins have
/// them (pyfa `eos/saveddata/damagePattern.py`).
#[must_use]
pub fn damage_profile_presets() -> Vec<DamageProfilePreset> {
    let preset = |id, name, em, thermal, kinetic, explosive| DamageProfilePreset {
        id,
        name,
        profile: DamageProfile {
            em,
            thermal,
            kinetic,
            explosive,
        },
    };
    vec![
        preset("uniform", "Uniform", 25.0, 25.0, 25.0, 25.0),
        preset("em", "EM", 1.0, 0.0, 0.0, 0.0),
        preset("thermal", "Thermal", 0.0, 1.0, 0.0, 0.0),
        preset("kinetic", "Kinetic", 0.0, 0.0, 1.0, 0.0),
        preset("explosive", "Explosive", 0.0, 0.0, 0.0, 1.0),
        preset(
            "angel-cartel",
            "Angel Cartel",
            1838.0,
            562.0,
            2215.0,
            3838.0,
        ),
        preset("blood-raiders", "Blood Raiders", 5067.0, 4214.0, 0.0, 0.0),
        preset("guristas", "Guristas", 0.0, 1828.0, 7413.0, 0.0),
        preset(
            "sanshas-nation",
            "Sansha's Nation",
            5586.0,
            4112.0,
            0.0,
            0.0,
        ),
        preset("serpentis", "Serpentis", 0.0, 5373.0, 4813.0, 0.0),
    ]
}

/// Skill levels (0–5) by skill type id; a skill left out counts as 0.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PilotSkills(BTreeMap<i64, i16>);

impl PilotSkills {
    pub fn new(levels: BTreeMap<i64, i16>) -> Result<Self, FittingError> {
        if let Some((&skill_type_id, &level)) = levels
            .iter()
            .find(|(_, level)| !(0..=MAX_LEVEL).contains(*level))
        {
            return Err(FittingError::InvalidSkillLevel {
                skill_type_id,
                level,
            });
        }
        Ok(Self(levels))
    }

    #[must_use]
    pub fn levels(&self) -> &BTreeMap<i64, i16> {
        &self.0
    }
}

/// Whose skills a calculation uses.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Pilot {
    /// Every skill at V.
    AllV,
    /// No skills at all.
    AllZero,
    Skills(PilotSkills),
}

/// Resistances as fractions (0.75 = 75%), in damage type order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resists {
    pub em: f64,
    pub thermal: f64,
    pub kinetic: f64,
    pub explosive: f64,
}

impl Resists {
    /// From dogma resonances (damage taken: 1.0 is no resistance).
    #[must_use]
    pub fn from_resonances([em, thermal, kinetic, explosive]: [f64; 4]) -> Self {
        Self {
            em: 1.0 - em,
            thermal: 1.0 - thermal,
            kinetic: 1.0 - kinetic,
            explosive: 1.0 - explosive,
        }
    }
}

/// An idle capacitor: nothing draws on it, so it stays full.
impl Default for CapacitorState {
    fn default() -> Self {
        Self::Stable { level: 1.0 }
    }
}

/// Something used out of something available: CPU, a rack, a bay.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub used: f64,
    pub total: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceStats {
    /// teraflops
    pub cpu: Usage,
    /// MW
    pub powergrid: Usage,
    pub calibration: Usage,
    pub turret_hardpoints: Usage,
    pub launcher_hardpoints: Usage,
    pub high_slots: Usage,
    pub med_slots: Usage,
    pub low_slots: Usage,
    pub rig_slots: Usage,
    /// Mbit/s
    pub drone_bandwidth: Usage,
    /// m³
    pub drone_bay: Usage,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum CapacitorState {
    /// Settles at this share of capacity (0.735 = 73.5%).
    Stable { level: f64 },
    /// Runs dry after this many seconds.
    Depletes { seconds: f64 },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapacitorStats {
    /// GJ
    pub capacity: f64,
    pub recharge_seconds: f64,
    /// GJ/s at peak recharge, after what the active modules use;
    /// negative when they use more.
    pub peak_delta: f64,
    pub state: CapacitorState,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OffenseStats {
    /// Turrets, launchers and smartbombs, without reloads.
    pub weapon_dps: f64,
    pub weapon_dps_with_reload: f64,
    pub volley: f64,
    pub drone_dps: f64,
    pub total_dps: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerStats {
    pub hp: f64,
    /// Against the calculation's damage profile.
    pub ehp: f64,
    pub resists: Resists,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefenseStats {
    pub ehp: f64,
    pub shield: LayerStats,
    pub armor: LayerStats,
    pub hull: LayerStats,
    /// Peak passive shield regeneration, effective HP/s.
    pub shield_passive_regen: f64,
    /// Active repair, effective HP/s per layer.
    pub shield_boost: f64,
    pub armor_repair: f64,
    pub hull_repair: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetingStats {
    pub max_targets: f64,
    /// metres
    pub lock_range: f64,
    /// mm
    pub scan_resolution: f64,
    /// The strongest of the four sensor types, in points.
    pub sensor_strength: f64,
    /// metres
    pub signature_radius: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigationStats {
    /// m/s, with the fit's propulsion modules as requested.
    pub max_velocity: f64,
    /// kg
    pub mass: f64,
    pub inertia: f64,
    pub align_seconds: f64,
    /// AU/s
    pub warp_speed: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DroneStats {
    pub active: f64,
    pub max_active: f64,
    /// metres
    pub control_range: f64,
}

/// The stats the Fittings page shows, grouped the way its rail is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitStats {
    pub resources: ResourceStats,
    pub capacitor: CapacitorStats,
    pub offense: OffenseStats,
    pub defense: DefenseStats,
    pub targeting: TargetingStats,
    pub navigation: NavigationStats,
    pub drones: DroneStats,
}

/// What a fitting rule is about.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "index")]
pub enum ViolationTarget {
    Ship,
    /// Index into the fit's items.
    Item(usize),
    /// The charge of the item at this index.
    Charge(usize),
}

/// A fitting rule the fit breaks. `rule` is the engine's rule name
/// (`resource`, `skill`, `max_group`, …) and `detail` its values, passed
/// through until the UI needs them typed.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitViolation {
    pub target: ViolationTarget,
    pub rule: String,
    pub detail: serde_json::Value,
}

/// What the module table shows for one item.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemStats {
    /// The state it actually reached (can be below the requested one).
    pub reached_state: ModuleState,
    /// The highest state it can reach at all.
    pub max_state: ModuleState,
    /// teraflops
    pub cpu: f64,
    /// MW
    pub powergrid: f64,
    pub calibration: f64,
    /// GJ/s while it runs: what it uses, or negative for what a nosferatu
    /// drains into the capacitor; 0 unless it reached active.
    pub capacitor_per_second: f64,
    /// metres; a bubble's radius when it has no optimal range
    pub optimal: f64,
    /// metres; falloff, or the falloff of an energy or electronic warfare
    /// module
    pub falloff: f64,
    /// Its damage per second, without reloads (drones included).
    pub dps: f64,
    /// What it does, in one number; `None` for a module the table shows
    /// no headline for.
    pub headline: Option<Headline>,
}

/// What kind of number a module's headline is.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HeadlineKind {
    /// Damage per second.
    Dps,
    /// Shield HP per second.
    ShieldRepair,
    /// Armor HP per second.
    ArmorRepair,
    /// Its strongest resistance bonus, in percent.
    Resist,
    /// The velocity bonus, in percent.
    Velocity,
    /// Shield HP added.
    ShieldHp,
    /// Armor HP added.
    ArmorHp,
    /// Capacitor (GJ) per charge injected.
    Capacitor,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Headline {
    pub kind: HeadlineKind,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitCalculation {
    pub stats: FitStats,
    /// Index-parallel to the fit's items; `None` for an item the engine
    /// data doesn't know.
    pub items: Vec<Option<ItemStats>>,
    pub violations: Vec<FitViolation>,
    /// Indexes of items (or their charges) the engine data does not know;
    /// they are left out of the calculation.
    pub unknown_items: Vec<usize>,
}

/// Which engine data a [`FittingEngine`] calculates with.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FittingEngineInfo {
    /// The pinned data package version, e.g. `13.3650286.0`.
    pub data_version: String,
    pub major: i32,
    /// The EVE build the data was made from.
    pub build_number: i32,
}

/// What an explanation is about.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ExplainTarget {
    Ship,
    /// The item at this index of the fit.
    Item(usize),
}

/// Where a modifier comes from. Item and charge indexes are the fit's.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SourceRef {
    Ship,
    /// The ship's active mode (tactical destroyers).
    Mode,
    Character,
    Item {
        index: usize,
    },
    /// The charge loaded in the item at `index`.
    Charge {
        index: usize,
    },
    Skill {
        #[serde(rename = "typeId")]
        type_id: i64,
    },
    /// A projected effect.
    Projected {
        index: usize,
    },
    /// A buff (fleet boost).
    Buff {
        id: i64,
    },
}

/// One modifier on an attribute, in the order the engine applied them.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifierSource {
    pub from: SourceRef,
    /// The engine's operator name: `pre_assign`, `pre_mul`, `mod_add`,
    /// `post_percent`, ….
    pub operator: String,
    pub value: f64,
    /// The stacking penalty factor, when penalised.
    pub penalty: Option<f64>,
    /// False when the source's state is too low for its effect.
    pub applied: bool,
}

/// How one attribute got from its base value to its final one. How much
/// each source added isn't reported: multiplications compound and
/// stacking penalties depend on order.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributeExplanation {
    pub base: f64,
    pub value: f64,
    pub sources: Vec<ModifierSource>,
}

/// A fit read from text, before it is saved.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedFit {
    /// The name the text gives; `None` when it gives none (the client
    /// writes `.` for an unnamed fit).
    pub name: Option<String>,
    pub fit: FitInput,
}

/// A line of pasted text, numbered from 1 as the text is.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EftLine {
    pub line: usize,
    pub text: String,
}

/// Why EFT text could not be read.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum EftError {
    /// Names that match no type, every one of them.
    UnknownLines(Vec<EftLine>),
    /// Not EFT, or EFT the engine can't place (a module with no rack, a bad
    /// mutation); the message says what.
    Invalid(String),
}

/// Calculates fit stats and reads and writes fits as text. Implemented by
/// `iskworks-fitting`; tests fake it.
pub trait FittingEngine: Send + Sync {
    fn info(&self) -> FittingEngineInfo;

    /// Every fittable type and the market groups, built once from the
    /// engine data.
    fn catalogue(&self) -> std::sync::Arc<FitCatalogue>;

    /// Reads EFT (the client's copy/paste format, which pyfa also writes),
    /// with modules in the states EVE gives an imported fit.
    fn parse_eft(&self, text: &str) -> Result<ParsedFit, EftError>;

    /// Writes EFT. Items the engine data doesn't know are left out.
    fn write_eft(&self, name: &str, fit: &FitInput) -> Result<String, FittingError>;

    fn calculate(
        &self,
        fit: &FitInput,
        pilot: &Pilot,
        profile: &DamageProfile,
    ) -> Result<FitCalculation, FittingError>;

    /// Every modifier on one attribute (by its dogma name) of the ship or
    /// an item, for the "Why?" view.
    fn explain(
        &self,
        fit: &FitInput,
        pilot: &Pilot,
        profile: &DamageProfile,
        target: ExplainTarget,
        attribute: &str,
    ) -> Result<AttributeExplanation, FittingError>;
}

#[cfg(test)]
mod tests;
