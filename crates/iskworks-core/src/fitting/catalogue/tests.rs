use std::collections::BTreeMap;

use super::*;

const SHIP_GROUP: i64 = 380;
const OTHER_GROUP: i64 = 25;

fn module(type_id: i64, name: &str, slot: SlotKind) -> CatalogueEntry {
    CatalogueEntry {
        type_id,
        name: name.to_string(),
        kind: CatalogueKind::Module,
        group_id: 60,
        slot: Some(slot),
        market_group_id: Some(2),
        meta_group_id: None,
        variation_parent: None,
        cpu: 10.0,
        powergrid: 1.0,
        calibration: 0.0,
        rig_size: None,
        hardpoint: None,
        charge_groups: Vec::new(),
        charge_size: None,
        fits_ship_types: Vec::new(),
        fits_ship_groups: Vec::new(),
        ship_slots: None,
    }
}

fn ship() -> CatalogueEntry {
    CatalogueEntry {
        kind: CatalogueKind::Ship,
        group_id: SHIP_GROUP,
        slot: None,
        rig_size: Some(2),
        ship_slots: Some(ShipSlots {
            high: 2,
            med: 0,
            low: 3,
            rig: 2,
        }),
        ..module(12_731, "Bustard", SlotKind::High)
    }
}

fn catalogue(entries: Vec<CatalogueEntry>) -> FitCatalogue {
    FitCatalogue::new(
        entries,
        BTreeMap::from([
            (
                1,
                MarketGroup {
                    id: 1,
                    name: "Ship Equipment".to_string(),
                    parent: None,
                },
            ),
            (
                2,
                MarketGroup {
                    id: 2,
                    name: "Hull & Armor".to_string(),
                    parent: Some(1),
                },
            ),
            (
                3,
                MarketGroup {
                    id: 3,
                    name: "Damage Controls".to_string(),
                    parent: Some(2),
                },
            ),
        ]),
    )
}

fn names(entries: &[&CatalogueEntry]) -> Vec<String> {
    entries.iter().map(|entry| entry.name.clone()).collect()
}

#[test]
fn modules_for_a_ship_skip_racks_it_lacks_and_rigs_of_another_size() {
    let mut medium_rig = module(1, "Medium Rig", SlotKind::Rig);
    medium_rig.rig_size = Some(2);
    let mut large_rig = module(2, "Large Rig", SlotKind::Rig);
    large_rig.rig_size = Some(3);
    let catalogue = catalogue(vec![
        ship(),
        medium_rig,
        large_rig,
        module(3, "Shield Hardener", SlotKind::Med),
        module(4, "Damage Control", SlotKind::Low),
    ]);
    let ship = ship();
    assert_eq!(
        names(&catalogue.modules_for(&ship, None, None)),
        ["Damage Control", "Medium Rig"]
    );
    assert_eq!(
        names(&catalogue.modules_for(&ship, Some(SlotKind::Rig), None)),
        ["Medium Rig"]
    );
}

#[test]
fn modules_restricted_to_other_ships_are_left_out() {
    let mut ours = module(1, "Cloak For Us", SlotKind::Low);
    ours.fits_ship_groups = vec![SHIP_GROUP];
    let mut theirs = module(2, "Cloak For Them", SlotKind::Low);
    theirs.fits_ship_groups = vec![OTHER_GROUP];
    let mut by_type = module(3, "Bustard Only", SlotKind::Low);
    by_type.fits_ship_types = vec![12_731];
    let catalogue = catalogue(vec![ours, theirs, by_type]);
    assert_eq!(
        names(&catalogue.modules_for(&ship(), None, None)),
        ["Bustard Only", "Cloak For Us"]
    );
}

#[test]
fn a_query_matches_any_part_of_the_name_in_any_case() {
    let catalogue = catalogue(vec![
        module(1, "Damage Control II", SlotKind::Low),
        module(2, "Nanofiber Internal Structure II", SlotKind::Low),
    ]);
    assert_eq!(
        names(&catalogue.modules_for(&ship(), None, Some("damage"))),
        ["Damage Control II"]
    );
}

#[test]
fn results_sort_by_market_path_then_name() {
    let mut deep = module(1, "Alpha", SlotKind::Low);
    deep.market_group_id = Some(3);
    let shallow = module(2, "Zulu", SlotKind::Low);
    let catalogue = catalogue(vec![deep, shallow]);
    // "Ship Equipment / Hull & Armor" sorts before ".../Damage Controls".
    assert_eq!(
        names(&catalogue.modules_for(&ship(), None, None)),
        ["Zulu", "Alpha"]
    );
    assert_eq!(
        catalogue.market_path(3),
        ["Ship Equipment", "Hull & Armor", "Damage Controls"]
    );
}

#[test]
fn charges_follow_the_modules_groups_and_size() {
    let mut gun = module(1, "Gun", SlotKind::High);
    gun.charge_groups = vec![83];
    gun.charge_size = Some(1);
    let charge = |type_id, name: &str, group_id, size| CatalogueEntry {
        kind: CatalogueKind::Charge,
        slot: None,
        group_id,
        charge_size: size,
        ..module(type_id, name, SlotKind::High)
    };
    let catalogue = catalogue(vec![
        gun.clone(),
        charge(2, "EMP S", 83, Some(1)),
        charge(3, "EMP M", 83, Some(2)),
        charge(4, "Missile", 90, Some(1)),
    ]);
    assert_eq!(names(&catalogue.charges_for(&gun)), ["EMP S"]);

    gun.charge_size = None;
    assert_eq!(names(&catalogue.charges_for(&gun)), ["EMP M", "EMP S"]);
}

#[test]
fn drones_and_ships_are_listed_by_kind() {
    let drone = CatalogueEntry {
        kind: CatalogueKind::Drone,
        slot: None,
        ..module(5, "Hobgoblin II", SlotKind::High)
    };
    let catalogue = catalogue(vec![
        ship(),
        drone,
        module(1, "Damage Control", SlotKind::Low),
    ]);
    assert_eq!(names(&catalogue.drones(None)), ["Hobgoblin II"]);
    assert_eq!(names(&catalogue.ships(Some("bust"))), ["Bustard"]);
}

#[test]
fn variations_are_the_parent_and_everything_sharing_it() {
    let parent = module(1, "Damage Control I", SlotKind::Low);
    let mut t2 = module(2, "Damage Control II", SlotKind::Low);
    t2.variation_parent = Some(1);
    let mut faction = module(3, "Imperial Navy Damage Control", SlotKind::Low);
    faction.variation_parent = Some(1);
    let catalogue = catalogue(vec![
        parent,
        t2,
        faction,
        module(4, "Unrelated", SlotKind::Low),
    ]);
    for type_id in [1, 2, 3] {
        assert_eq!(
            names(&catalogue.variations(type_id)),
            [
                "Damage Control I",
                "Damage Control II",
                "Imperial Navy Damage Control"
            ],
            "{type_id}"
        );
    }
    assert!(catalogue.variations(99).is_empty());
}
