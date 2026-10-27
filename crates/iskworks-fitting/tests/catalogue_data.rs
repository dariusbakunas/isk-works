//! The fitting catalogue against the real engine data. Run `make
//! fitting-data` first, then `cargo test -p iskworks-fitting -- --ignored`.

use std::path::Path;
use std::sync::OnceLock;

use iskworks_core::fitting::{
    CatalogueEntry, CatalogueKind, FitCatalogue, FittingEngine, Hardpoint, SlotKind,
};
use iskworks_fitting::EsfFittingEngine;

fn engine() -> &'static EsfFittingEngine {
    static ENGINE: OnceLock<EsfFittingEngine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/fitting/sde.dat");
        EsfFittingEngine::load(&path)
            .unwrap_or_else(|error| panic!("requires data/fitting/sde.dat: {error}"))
    })
}

fn catalogue() -> std::sync::Arc<FitCatalogue> {
    engine().catalogue()
}

fn entry(name: &str) -> CatalogueEntry {
    let type_id = engine()
        .type_id(name)
        .unwrap_or_else(|| panic!("no type {name}"));
    catalogue()
        .entry(type_id)
        .unwrap_or_else(|| panic!("{name} is not in the catalogue"))
        .clone()
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn ships_carry_their_rig_size_and_slot_counts() {
    let bustard = entry("Bustard");
    assert_eq!(bustard.kind, CatalogueKind::Ship);
    assert_eq!(bustard.rig_size, Some(2));
    let slots = bustard.ship_slots.expect("a ship has slot counts");
    assert_eq!((slots.high, slots.med, slots.low, slots.rig), (2, 6, 3, 2));
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn modules_carry_their_rack_size_and_fitting_costs() {
    let rig = entry("Medium Core Defense Field Extender II");
    assert_eq!(
        (rig.kind, rig.slot, rig.rig_size),
        (CatalogueKind::Module, Some(SlotKind::Rig), Some(2))
    );
    assert!(rig.calibration > 0.0);

    let gun = entry("200mm AutoCannon II");
    assert_eq!(
        (gun.slot, gun.hardpoint),
        (Some(SlotKind::High), Some(Hardpoint::Turret))
    );
    assert!(gun.cpu > 0.0 && gun.powergrid > 0.0);
    let emp = entry("EMP S");
    assert_eq!(emp.kind, CatalogueKind::Charge);
    assert!(
        gun.charge_groups.contains(&emp.group_id),
        "{:?} vs {}",
        gun.charge_groups,
        emp.group_id
    );
    assert_eq!(gun.charge_size, Some(1));
    assert_eq!(emp.charge_size, Some(1));

    assert_eq!(entry("Damage Control II").slot, Some(SlotKind::Low));
    assert_eq!(
        entry("Multispectrum Shield Hardener II").slot,
        Some(SlotKind::Med)
    );
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn drones_are_listed_and_cargo_only_items_are_not() {
    assert_eq!(entry("Hobgoblin II").kind, CatalogueKind::Drone);
    // A mineral fits nowhere; it only rides in cargo. (Nanite Repair
    // Paste would not do: it is a charge, for ancillary armor repairers.)
    let tritanium = engine().type_id("Tritanium").unwrap();
    assert!(catalogue().entry(tritanium).is_none());
    assert_eq!(entry("Nanite Repair Paste").kind, CatalogueKind::Charge);
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn variations_share_a_parent() {
    let catalogue = catalogue();
    let dc2 = entry("Damage Control II");
    let names: Vec<String> = catalogue
        .variations(dc2.type_id)
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    assert!(names.contains(&"Damage Control I".to_string()), "{names:?}");
    assert!(
        names.contains(&"Damage Control II".to_string()),
        "{names:?}"
    );
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn market_paths_reach_a_root() {
    let dc2 = entry("Damage Control II");
    let path = catalogue().market_path(dc2.market_group_id.unwrap());
    assert!(path.len() >= 2, "{path:?}");
    assert_eq!(
        path.first().map(String::as_str),
        Some("Ship Equipment"),
        "{path:?}"
    );
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn the_bustard_takes_medium_rigs_but_not_large_ones() {
    let catalogue = catalogue();
    let bustard = entry("Bustard");
    let rigs = catalogue.modules_for(
        &bustard,
        Some(SlotKind::Rig),
        Some("Core Defense Field Extender"),
    );
    let names: Vec<&str> = rigs.iter().map(|entry| entry.name.as_str()).collect();
    assert!(
        names.contains(&"Medium Core Defense Field Extender II"),
        "{names:?}"
    );
    assert!(
        !names.iter().any(|name| name.starts_with("Large ")),
        "{names:?}"
    );
    assert!(
        !names.iter().any(|name| name.starts_with("Small ")),
        "{names:?}"
    );

    let gun = entry("200mm AutoCannon II");
    let charges: Vec<String> = catalogue
        .charges_for(&gun)
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    assert!(charges.contains(&"EMP S".to_string()), "{charges:?}");
    assert!(!charges.contains(&"EMP M".to_string()), "{charges:?}");
}

/// Not an assertion: how big the catalogue is and how long the editor's
/// queries take. Run with `--release --nocapture`.
#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn catalogue_query_timing() {
    let catalogue = catalogue();
    let bustard = entry("Bustard");
    let gun = entry("200mm AutoCannon II");
    let start = std::time::Instant::now();
    let modules = catalogue.modules_for(&bustard, None, None).len();
    let all_modules = start.elapsed();
    let start = std::time::Instant::now();
    let charges = catalogue.charges_for(&gun).len();
    let charge_query = start.elapsed();
    println!(
        "catalogue: {} ships, {modules} modules fit the Bustard ({all_modules:?}), {charges} charges for the gun ({charge_query:?})",
        catalogue.ships(None).len()
    );
}
