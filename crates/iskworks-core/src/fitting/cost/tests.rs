use std::collections::BTreeMap;

use rust_decimal::Decimal;

use super::*;
use crate::fitting::{FitInput, FitItem, ModuleState, SlotKind};

const SHIP: i64 = 587;
const GUN: i64 = 2873;
const AMMO: i64 = 185;
const DRONE: i64 = 2456;
const PASTE: i64 = 28_668;
const UNPRICED: i64 = 99;

fn item(type_id: i64, slot_kind: SlotKind, slot_index: Option<i16>, quantity: u32) -> FitItem {
    FitItem {
        type_id,
        slot_kind,
        slot_index,
        quantity,
        state: ModuleState::Online,
        charge_type_id: None,
        mutation: None,
    }
}

fn fit() -> FitInput {
    let gun = |index| {
        let mut gun = item(GUN, SlotKind::High, Some(index), 1);
        gun.charge_type_id = Some(AMMO);
        gun
    };
    FitInput {
        ship_type_id: SHIP,
        items: vec![
            gun(0),
            gun(1),
            item(UNPRICED, SlotKind::Low, Some(0), 1),
            item(DRONE, SlotKind::DroneBay, None, 2),
            item(PASTE, SlotKind::Cargo, None, 100),
        ],
    }
}

fn isk(value: i64) -> Money {
    Money(Decimal::from(value))
}

fn prices() -> BTreeMap<i64, Money> {
    BTreeMap::from([
        (SHIP, isk(500_000)),
        (GUN, isk(1_000_000)),
        (AMMO, isk(10)),
        (DRONE, isk(2_000_000)),
        (PASTE, isk(30_000)),
    ])
}

#[test]
fn a_fit_costs_its_hull_modules_drones_and_cargo_by_type() {
    let cost = fit_cost(&fit(), &prices());
    let line = |type_id| {
        cost.lines
            .iter()
            .find(|line| line.type_id == type_id)
            .unwrap()
    };

    assert_eq!(line(SHIP).section, CostSection::Hull);
    assert_eq!(line(GUN).quantity, 2);
    assert_eq!(line(GUN).total, Some(isk(2_000_000)));
    assert_eq!(line(GUN).section, CostSection::Fittings);
    assert_eq!(line(DRONE).total, Some(isk(4_000_000)));
    assert_eq!(line(PASTE).total, Some(isk(3_000_000)));
    assert_eq!(cost.hull, isk(500_000));
    assert_eq!(cost.fittings, isk(2_000_000));
    assert_eq!(cost.drones, isk(4_000_000));
    assert_eq!(cost.cargo, isk(3_000_000));
    assert_eq!(cost.total, isk(9_500_000));
}

#[test]
fn unpriced_types_are_listed_and_left_out_of_the_totals() {
    let cost = fit_cost(&fit(), &prices());
    assert_eq!(cost.missing, vec![UNPRICED]);
    let unpriced = cost
        .lines
        .iter()
        .find(|line| line.type_id == UNPRICED)
        .unwrap();
    assert_eq!((unpriced.unit_price, unpriced.total), (None, None));
}

#[test]
fn loaded_charges_are_not_costed() {
    // A fit says which charge a gun loads, never how many.
    let cost = fit_cost(&fit(), &prices());
    assert!(cost.lines.iter().all(|line| line.type_id != AMMO));
}

#[test]
fn what_a_fit_needs_counts_the_hull_items_and_stacks_but_not_charges() {
    assert_eq!(
        fit_needs(&fit()),
        BTreeMap::from([(SHIP, 1), (GUN, 2), (UNPRICED, 1), (DRONE, 2), (PASTE, 100)])
    );
}

fn holding(connection: u128, location: i64, type_id: i64, quantity: i64) -> AssetHolding {
    AssetHolding {
        connection_id: crate::ConnectedCharacterId(uuid::Uuid::from_u128(connection)),
        character_name: format!("Pilot {connection}"),
        location_id: location,
        location_name: format!("Station {location}"),
        type_id,
        quantity,
    }
}

#[test]
fn coverage_counts_what_each_location_already_has_best_first() {
    let needed = BTreeMap::from([(SHIP, 1), (GUN, 2), (DRONE, 2)]);
    let coverage = asset_coverage(
        &needed,
        &[
            // Pilot 1 in station 10 has everything, and more guns than needed.
            holding(1, 10, SHIP, 1),
            holding(1, 10, GUN, 5),
            holding(1, 10, DRONE, 2),
            // Pilot 2 in station 20 has one gun.
            holding(2, 20, GUN, 1),
            // Something the fit doesn't need doesn't count.
            holding(2, 20, PASTE, 1000),
        ],
    );
    assert_eq!(coverage.len(), 2);
    assert_eq!(
        (
            coverage[0].location_id,
            coverage[0].covered,
            coverage[0].needed
        ),
        (10, 5, 5)
    );
    assert!(coverage[0].short.is_empty());
    assert_eq!(
        (
            coverage[1].location_id,
            coverage[1].covered,
            coverage[1].needed
        ),
        (20, 1, 5)
    );
    assert_eq!(
        // By type id.
        coverage[1].short,
        vec![
            Shortfall {
                type_id: SHIP,
                needed: 1,
                have: 0
            },
            Shortfall {
                type_id: DRONE,
                needed: 2,
                have: 0
            },
            Shortfall {
                type_id: GUN,
                needed: 2,
                have: 1
            },
        ]
    );
}

#[test]
fn coverage_keeps_the_ten_best_locations() {
    let needed = BTreeMap::from([(GUN, 1)]);
    let holdings: Vec<AssetHolding> = (0..15)
        .map(|location| holding(1, location, GUN, 1))
        .collect();
    assert_eq!(asset_coverage(&needed, &holdings).len(), 10);
}
