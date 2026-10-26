//! Ship fittings: what a fit holds, who flies it, and the stats an engine
//! calculates for it.
//!
//! Pure: the calculation itself sits behind [`FittingEngine`], implemented
//! by `iskworks-fitting` on EVEShip.fit's dogma engine. No engine types
//! appear here.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

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
}

/// A fit as the engine calculates it: a hull and what it carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitInput {
    pub ship_type_id: i64,
    pub items: Vec<FitItem>,
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
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
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

/// Something used out of something available: CPU, a rack, a bay.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub used: f64,
    pub total: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OffenseStats {
    /// Turrets, launchers and smartbombs, without reloads.
    pub weapon_dps: f64,
    pub weapon_dps_with_reload: f64,
    pub volley: f64,
    pub drone_dps: f64,
    pub total_dps: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerStats {
    pub hp: f64,
    /// Against the calculation's damage profile.
    pub ehp: f64,
    pub resists: Resists,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DroneStats {
    pub active: f64,
    pub max_active: f64,
    /// metres
    pub control_range: f64,
}

/// The stats the Fittings page shows, grouped the way its rail is.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitCalculation {
    pub stats: FitStats,
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

/// Calculates fit stats. Implemented by `iskworks-fitting`; tests fake it.
pub trait FittingEngine: Send + Sync {
    fn info(&self) -> FittingEngineInfo;

    fn calculate(
        &self,
        fit: &FitInput,
        pilot: &Pilot,
        profile: &DamageProfile,
    ) -> Result<FitCalculation, FittingError>;
}

#[cfg(test)]
mod tests;
