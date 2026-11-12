//! The editor's module browser: fittable types for a ship, a module's
//! charges, drones or ships, and a type's variations, each marked with
//! whether the chosen pilot can use it.

use std::collections::BTreeMap;

use iskworks_core::fitting::{
    CatalogueEntry, CatalogueKind, FitCatalogue, FittingEngine, Hardpoint, SlotKind,
};
use iskworks_core::{InventoryError, WorkspaceId};
use serde::Serialize;

use super::calculation::PilotChoice;
use super::{sde_error, FittingService};

/// What the editor's module browser asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogueQuery {
    /// Modules a ship can fit, optionally in one rack.
    Modules {
        ship_type_id: i64,
        slot: Option<SlotKind>,
    },
    /// Charges a module loads.
    Charges {
        module_type_id: i64,
    },
    Drones,
    Ships,
}

/// One fittable type, as the module browser lists it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueItem {
    pub type_id: i64,
    pub name: String,
    pub kind: CatalogueKind,
    pub slot: Option<SlotKind>,
    pub cpu: f64,
    pub powergrid: f64,
    pub calibration: f64,
    pub hardpoint: Option<Hardpoint>,
    pub rig_size: Option<i64>,
    pub charge_size: Option<i64>,
    /// Market groups from the root down.
    pub market_path: Vec<String>,
    /// Tech I, Tech II, Faction (4), …; `None` for most Tech I types.
    pub meta_group_id: Option<i64>,
    /// The type it is a variation of (a faction charge's Tech I one).
    pub variation_parent: Option<i64>,
    /// For a character pilot, whether its skills cover the type's
    /// requirements; `None` for All V / All 0.
    pub can_use: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueView {
    pub entries: Vec<CatalogueItem>,
    /// More matched than are listed; narrow the search.
    pub truncated: bool,
}

/// The most a catalogue answer lists.
const CATALOGUE_LIMIT: usize = 200;

impl FittingService {
    /// Fittable types for the editor: modules a ship fits, charges a module
    /// loads, drones or ships, matching `text`, capped at 200.
    pub async fn catalogue(
        &self,
        workspace_id: WorkspaceId,
        engine: &dyn FittingEngine,
        query: CatalogueQuery,
        text: Option<&str>,
        pilot: PilotChoice,
    ) -> Result<CatalogueView, InventoryError> {
        let catalogue = engine.catalogue();
        let entries = match query {
            CatalogueQuery::Modules { ship_type_id, slot } => {
                let ship = catalogue
                    .entry(ship_type_id)
                    .filter(|entry| entry.kind == CatalogueKind::Ship)
                    .ok_or_else(|| {
                        InventoryError::Validation(format!("type {ship_type_id} is not a ship"))
                    })?;
                catalogue.modules_for(ship, slot, text)
            }
            CatalogueQuery::Charges { module_type_id } => {
                let module = catalogue
                    .entry(module_type_id)
                    .filter(|entry| entry.kind == CatalogueKind::Module)
                    .ok_or_else(|| {
                        InventoryError::Validation(format!("type {module_type_id} is not a module"))
                    })?;
                catalogue.charges_for(module)
            }
            CatalogueQuery::Drones => catalogue.drones(text),
            CatalogueQuery::Ships => catalogue.ships(text),
        };
        let truncated = entries.len() > CATALOGUE_LIMIT;
        let page: Vec<&CatalogueEntry> = entries.into_iter().take(CATALOGUE_LIMIT).collect();
        Ok(CatalogueView {
            entries: self
                .catalogue_items(workspace_id, &catalogue, &page, pilot)
                .await?,
            truncated,
        })
    }

    /// `type_id`'s variations (its family, itself included).
    pub async fn variations(
        &self,
        workspace_id: WorkspaceId,
        engine: &dyn FittingEngine,
        type_id: i64,
        pilot: PilotChoice,
    ) -> Result<CatalogueView, InventoryError> {
        let catalogue = engine.catalogue();
        let family = catalogue.variations(type_id);
        Ok(CatalogueView {
            entries: self
                .catalogue_items(workspace_id, &catalogue, &family, pilot)
                .await?,
            truncated: false,
        })
    }

    async fn catalogue_items(
        &self,
        workspace_id: WorkspaceId,
        catalogue: &FitCatalogue,
        entries: &[&CatalogueEntry],
        pilot: PilotChoice,
    ) -> Result<Vec<CatalogueItem>, InventoryError> {
        let (_, snapshot) = self.resolve_pilot(workspace_id, pilot).await?;
        let required = match &snapshot {
            Some(_) => self
                .sde
                .required_skills(
                    &entries
                        .iter()
                        .map(|entry| entry.type_id)
                        .collect::<Vec<_>>(),
                )
                .await
                .map_err(sde_error)?,
            None => BTreeMap::new(),
        };
        Ok(entries
            .iter()
            .map(|entry| CatalogueItem {
                type_id: entry.type_id,
                name: entry.name.clone(),
                kind: entry.kind,
                slot: entry.slot,
                cpu: entry.cpu,
                powergrid: entry.powergrid,
                calibration: entry.calibration,
                hardpoint: entry.hardpoint,
                rig_size: entry.rig_size,
                charge_size: entry.charge_size,
                market_path: entry
                    .market_group_id
                    .map(|id| catalogue.market_path(id))
                    .unwrap_or_default(),
                meta_group_id: entry.meta_group_id,
                variation_parent: entry.variation_parent,
                // Prerequisites are trained before a skill, so the direct
                // requirements decide it, at active levels.
                can_use: snapshot.as_ref().map(|snapshot| {
                    required
                        .get(&entry.type_id)
                        .into_iter()
                        .flatten()
                        .all(|requirement| {
                            snapshot
                                .skill(requirement.skill_type_id)
                                .is_some_and(|skill| skill.active_level >= requirement.level)
                        })
                }),
            })
            .collect())
    }
}
