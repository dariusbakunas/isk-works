//! The fitting catalogue, built once from the engine data.

use std::collections::{BTreeMap, HashMap};

use esf_data::eve;
use iskworks_core::fitting::{
    CatalogueEntry, CatalogueKind, FitCatalogue, Hardpoint, MarketGroup, ShipSlots, SlotKind,
};

use super::FittingLoadError;

const CATEGORY_SHIP: i32 = 6;
const CATEGORY_MODULE: i32 = 7;
const CATEGORY_CHARGE: i32 = 8;
const CATEGORY_DRONE: i32 = 18;
const CATEGORY_SUBSYSTEM: i32 = 32;

/// The effect marking each rack, by name (as the engine's own EFT reader
/// places modules).
const RACK_EFFECTS: [(&str, SlotKind); 6] = [
    ("hiPower", SlotKind::High),
    ("medPower", SlotKind::Med),
    ("loPower", SlotKind::Low),
    ("rigSlot", SlotKind::Rig),
    ("subSystem", SlotKind::Subsystem),
    ("serviceSlot", SlotKind::Service),
];

const HARDPOINT_EFFECTS: [(&str, Hardpoint); 2] = [
    ("turretFitted", Hardpoint::Turret),
    ("launcherFitted", Hardpoint::Launcher),
];

/// Attribute ids the catalogue reads, by name; the numbered families
/// (`chargeGroup1`…) are every attribute with that prefix.
struct CatalogueAttributes {
    cpu: Option<i32>,
    power: Option<i32>,
    upgrade_cost: Option<i32>,
    rig_size: Option<i32>,
    charge_size: Option<i32>,
    high_slots: Option<i32>,
    med_slots: Option<i32>,
    low_slots: Option<i32>,
    rig_slots: Option<i32>,
    charge_groups: Vec<i32>,
    fits_ship_types: Vec<i32>,
    fits_ship_groups: Vec<i32>,
}

/// `prefix` followed only by digits, e.g. `chargeGroup3`, `canFitShipType10`.
fn numbered(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix)
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
}

/// The vtable slot of `Type.variation_parent_type_id`: the field after
/// `mode_type_ids` (slot 36) in the `@eveshipfit/sde` schema
/// (`specs/eve.fbs`). The data carries it, but `esf-data` 13.6.1's reader
/// was generated from an older schema without it. The catalogue data test
/// (Damage Control I and II share a family) fails if this ever moves.
const VT_VARIATION_PARENT_TYPE_ID: flatbuffers::VOffsetT = 38;

fn variation_parent(ty: &eve::Type<'_>) -> i32 {
    // SAFETY: `_tab` is a table the flatbuffers verifier accepted when the
    // data loaded, and slot 38 holds an int32 in the pinned schema; a slot
    // the table lacks reads as the default.
    unsafe { ty._tab.get::<i32>(VT_VARIATION_PARENT_TYPE_ID, Some(0)) }.unwrap_or(0)
}

/// Every attribute named `prefix` plus digits.
fn family(root: &eve::Sde<'_>, prefix: &str) -> Vec<i32> {
    root.dogma_attributes()
        .into_iter()
        .flatten()
        .filter(|attribute| numbered(attribute.name(), prefix))
        .map(|attribute| attribute.id())
        .collect()
}

impl CatalogueAttributes {
    fn resolve(root: &eve::Sde<'_>) -> Self {
        let by_name: HashMap<&str, i32> = root
            .dogma_attributes()
            .into_iter()
            .flatten()
            .map(|attribute| (attribute.name(), attribute.id()))
            .collect();
        let id = |name: &str| by_name.get(name).copied();
        Self {
            cpu: id("cpu"),
            power: id("power"),
            upgrade_cost: id("upgradeCost"),
            rig_size: id("rigSize"),
            charge_size: id("chargeSize"),
            high_slots: id("hiSlots"),
            med_slots: id("medSlots"),
            low_slots: id("lowSlots"),
            rig_slots: id("upgradeSlotsLeft"),
            charge_groups: family(root, "chargeGroup"),
            fits_ship_types: family(root, "canFitShipType"),
            fits_ship_groups: family(root, "canFitShipGroup"),
        }
    }
}

/// Every published ship, module, drone and charge, and the market groups.
pub(super) fn build(root: &eve::Sde<'_>) -> Result<FitCatalogue, FittingLoadError> {
    let effect_ids: HashMap<&str, i32> = root
        .dogma_effects()
        .into_iter()
        .flatten()
        .map(|effect| (effect.name(), effect.id()))
        .collect();
    let effect = |name: &'static str| {
        effect_ids
            .get(name)
            .copied()
            .ok_or(FittingLoadError::MissingEffect(name))
    };
    let racks: Vec<(i32, SlotKind)> = RACK_EFFECTS
        .iter()
        .map(|(name, kind)| Ok((effect(name)?, *kind)))
        .collect::<Result<_, FittingLoadError>>()?;
    let hardpoints: Vec<(i32, Hardpoint)> = HARDPOINT_EFFECTS
        .iter()
        .map(|(name, hardpoint)| Ok((effect(name)?, *hardpoint)))
        .collect::<Result<_, FittingLoadError>>()?;
    let attributes = CatalogueAttributes::resolve(root);

    let mut entries = Vec::new();
    for ty in root.types().into_iter().flatten() {
        if !ty.published() {
            continue;
        }
        let effects: Vec<i32> = ty
            .dogma_effects()
            .into_iter()
            .flatten()
            .map(|effect| effect.effect_id())
            .collect();
        let slot = racks
            .iter()
            .find(|(id, _)| effects.contains(id))
            .map(|(_, kind)| *kind);
        let kind = match ty.category_id() {
            CATEGORY_SHIP => CatalogueKind::Ship,
            CATEGORY_DRONE => CatalogueKind::Drone,
            CATEGORY_CHARGE => CatalogueKind::Charge,
            CATEGORY_MODULE | CATEGORY_SUBSYSTEM if slot.is_some() => CatalogueKind::Module,
            _ => continue,
        };
        let values: HashMap<i32, f64> = ty
            .dogma_attributes()
            .into_iter()
            .flatten()
            .map(|attribute| (attribute.attribute_id(), f64::from(attribute.value())))
            .collect();
        let value = |id: Option<i32>| id.and_then(|id| values.get(&id).copied());
        let ids = |family: &[i32]| -> Vec<i64> {
            family
                .iter()
                .filter_map(|id| values.get(id))
                .filter(|value| **value > 0.0)
                .map(|value| *value as i64)
                .collect()
        };
        let count = |id: Option<i32>| value(id).map_or(0, |value| value.max(0.0) as u32);
        let positive = |id: Option<i32>| {
            value(id)
                .filter(|value| *value > 0.0)
                .map(|value| value as i64)
        };
        entries.push(CatalogueEntry {
            type_id: i64::from(ty.id()),
            name: ty.name().to_string(),
            kind,
            group_id: i64::from(ty.group_id()),
            slot: if kind == CatalogueKind::Module {
                slot
            } else {
                None
            },
            market_group_id: Some(i64::from(ty.market_group_id())).filter(|id| *id > 0),
            meta_group_id: Some(i64::from(ty.meta_group_id())).filter(|id| *id > 0),
            variation_parent: Some(i64::from(variation_parent(&ty)))
                .filter(|id| *id > 0 && *id != i64::from(ty.id())),
            cpu: value(attributes.cpu).unwrap_or(0.0),
            powergrid: value(attributes.power).unwrap_or(0.0),
            calibration: value(attributes.upgrade_cost).unwrap_or(0.0),
            rig_size: positive(attributes.rig_size),
            hardpoint: hardpoints
                .iter()
                .find(|(id, _)| effects.contains(id))
                .map(|(_, hardpoint)| *hardpoint),
            charge_groups: ids(&attributes.charge_groups),
            charge_size: positive(attributes.charge_size),
            fits_ship_types: ids(&attributes.fits_ship_types),
            fits_ship_groups: ids(&attributes.fits_ship_groups),
            ship_slots: (kind == CatalogueKind::Ship).then(|| ShipSlots {
                high: count(attributes.high_slots),
                med: count(attributes.med_slots),
                low: count(attributes.low_slots),
                rig: count(attributes.rig_slots),
            }),
        });
    }

    let market_groups: BTreeMap<i64, MarketGroup> = root
        .market_groups()
        .into_iter()
        .flatten()
        .map(|group| {
            let id = i64::from(group.id());
            (
                id,
                MarketGroup {
                    id,
                    name: group.name().to_string(),
                    parent: Some(i64::from(group.parent_group_id())).filter(|parent| *parent > 0),
                },
            )
        })
        .collect();
    Ok(FitCatalogue::new(entries, market_groups))
}

#[cfg(test)]
mod tests {
    use super::numbered;

    #[test]
    fn numbered_families_need_digits_after_the_prefix() {
        assert!(numbered("chargeGroup1", "chargeGroup"));
        assert!(numbered("canFitShipType10", "canFitShipType"));
        assert!(!numbered("chargeGroup", "chargeGroup"));
        assert!(!numbered("chargeGroupX", "chargeGroup"));
        assert!(!numbered("chargeSize", "chargeGroup"));
    }
}
