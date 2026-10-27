//! What can be fitted: every ship, module, drone and charge the engine data
//! knows, with what decides where it goes (rack, rig size, allowed ships,
//! charge groups and size) and the market groups to browse it by. Pure:
//! `iskworks-fitting` builds it from the engine data.

use std::collections::BTreeMap;

use serde::Serialize;

use super::SlotKind;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CatalogueKind {
    Ship,
    Module,
    Drone,
    Charge,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Hardpoint {
    Turret,
    Launcher,
}

/// A ship's rack sizes, before modules change them.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShipSlots {
    pub high: u32,
    pub med: u32,
    pub low: u32,
    pub rig: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueEntry {
    pub type_id: i64,
    /// English, from the engine data.
    pub name: String,
    pub kind: CatalogueKind,
    pub group_id: i64,
    /// The rack a module goes in.
    pub slot: Option<SlotKind>,
    pub market_group_id: Option<i64>,
    pub meta_group_id: Option<i64>,
    /// The type it is a variation of (Tech I for most modules).
    pub variation_parent: Option<i64>,
    /// What it costs to fit (modules), before skills.
    pub cpu: f64,
    pub powergrid: f64,
    pub calibration: f64,
    /// A rig's size, or the rig size a ship takes.
    pub rig_size: Option<i64>,
    pub hardpoint: Option<Hardpoint>,
    /// The charge groups a module loads.
    pub charge_groups: Vec<i64>,
    /// A module's or a charge's size (small, medium, large, …).
    pub charge_size: Option<i64>,
    /// When either is non-empty, the module fits only these ships.
    pub fits_ship_types: Vec<i64>,
    pub fits_ship_groups: Vec<i64>,
    pub ship_slots: Option<ShipSlots>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketGroup {
    pub id: i64,
    pub name: String,
    pub parent: Option<i64>,
}

/// Every fittable type, by type id.
#[derive(Debug, Clone, Default)]
pub struct FitCatalogue {
    entries: BTreeMap<i64, CatalogueEntry>,
    market_groups: BTreeMap<i64, MarketGroup>,
}

fn matches(entry: &CatalogueEntry, query: Option<&str>) -> bool {
    query
        .map(str::trim)
        .filter(|query| !query.is_empty())
        .map_or(true, |query| {
            entry.name.to_lowercase().contains(&query.to_lowercase())
        })
}

impl FitCatalogue {
    #[must_use]
    pub fn new(entries: Vec<CatalogueEntry>, market_groups: BTreeMap<i64, MarketGroup>) -> Self {
        Self {
            entries: entries
                .into_iter()
                .map(|entry| (entry.type_id, entry))
                .collect(),
            market_groups,
        }
    }

    #[must_use]
    pub fn entry(&self, type_id: i64) -> Option<&CatalogueEntry> {
        self.entries.get(&type_id)
    }

    /// Market group names from the root down to `market_group_id`.
    #[must_use]
    pub fn market_path(&self, market_group_id: i64) -> Vec<String> {
        let mut path = Vec::new();
        let mut current = self.market_groups.get(&market_group_id);
        // The depth guard stops a malformed loop.
        while let Some(group) = current.filter(|_| path.len() < 16) {
            path.push(group.name.clone());
            current = group
                .parent
                .and_then(|parent| self.market_groups.get(&parent));
        }
        path.reverse();
        path
    }

    /// By market path, then name.
    fn sorted<'a>(&self, mut entries: Vec<&'a CatalogueEntry>) -> Vec<&'a CatalogueEntry> {
        entries.sort_by_cached_key(|entry| {
            (
                entry
                    .market_group_id
                    .map(|id| self.market_path(id).join(" / "))
                    .unwrap_or_default(),
                entry.name.clone(),
            )
        });
        entries
    }

    /// Modules `ship` can fit: in a rack the ship has (`slot` narrows it),
    /// rigs of its size, and nothing restricted to other ships.
    #[must_use]
    pub fn modules_for(
        &self,
        ship: &CatalogueEntry,
        slot: Option<SlotKind>,
        query: Option<&str>,
    ) -> Vec<&CatalogueEntry> {
        let slots = ship.ship_slots;
        let has_rack = |kind: SlotKind| match (kind, slots) {
            (SlotKind::High, Some(slots)) => slots.high > 0,
            (SlotKind::Med, Some(slots)) => slots.med > 0,
            (SlotKind::Low, Some(slots)) => slots.low > 0,
            (SlotKind::Rig, Some(slots)) => slots.rig > 0,
            // Subsystems and services: let the engine's rules judge.
            _ => true,
        };
        self.sorted(
            self.entries
                .values()
                .filter(|entry| entry.kind == CatalogueKind::Module)
                .filter(|entry| {
                    entry.slot.is_some_and(|kind| {
                        slot.map_or(true, |slot| slot == kind) && has_rack(kind)
                    })
                })
                .filter(|entry| {
                    entry.slot != Some(SlotKind::Rig) || entry.rig_size == ship.rig_size
                })
                .filter(|entry| {
                    (entry.fits_ship_types.is_empty() && entry.fits_ship_groups.is_empty())
                        || entry.fits_ship_types.contains(&ship.type_id)
                        || entry.fits_ship_groups.contains(&ship.group_id)
                })
                .filter(|entry| matches(entry, query))
                .collect(),
        )
    }

    /// Charges `module` can load: in one of its charge groups, and of its
    /// size when it has one.
    #[must_use]
    pub fn charges_for(&self, module: &CatalogueEntry) -> Vec<&CatalogueEntry> {
        self.sorted(
            self.entries
                .values()
                .filter(|entry| entry.kind == CatalogueKind::Charge)
                .filter(|entry| module.charge_groups.contains(&entry.group_id))
                .filter(|entry| {
                    module
                        .charge_size
                        .map_or(true, |size| entry.charge_size == Some(size))
                })
                .collect(),
        )
    }

    #[must_use]
    pub fn drones(&self, query: Option<&str>) -> Vec<&CatalogueEntry> {
        self.of_kind(CatalogueKind::Drone, query)
    }

    #[must_use]
    pub fn ships(&self, query: Option<&str>) -> Vec<&CatalogueEntry> {
        self.of_kind(CatalogueKind::Ship, query)
    }

    fn of_kind(&self, kind: CatalogueKind, query: Option<&str>) -> Vec<&CatalogueEntry> {
        self.sorted(
            self.entries
                .values()
                .filter(|entry| entry.kind == kind && matches(entry, query))
                .collect(),
        )
    }

    /// `type_id`'s variation family (its parent and everything sharing the
    /// parent, itself included), by name; empty for a type not listed.
    #[must_use]
    pub fn variations(&self, type_id: i64) -> Vec<&CatalogueEntry> {
        let Some(entry) = self.entry(type_id) else {
            return Vec::new();
        };
        let root = entry.variation_parent.unwrap_or(entry.type_id);
        let mut family: Vec<&CatalogueEntry> = self
            .entries
            .values()
            .filter(|candidate| {
                candidate.type_id == root || candidate.variation_parent == Some(root)
            })
            .collect();
        family.sort_by(|a, b| a.name.cmp(&b.name));
        family
    }
}

#[cfg(test)]
mod tests;
