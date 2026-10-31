//! Catalogue fixtures with real SDE 3482594 ids and figures: the Robotics
//! chain on Barren, Lava and Plasma planets.

use std::collections::BTreeMap;

use iskworks_sde::{
    PlanetCommodity, PlanetPinKind, PlanetPinType, PlanetSchematic, PlanetSchematicLine,
};
use rust_decimal::Decimal;

use super::PiCatalogue;

pub const BARREN: i64 = 2016;
pub const LAVA: i64 = 2015;
pub const PLASMA: i64 = 2063;
pub const GAS: i64 = 13;
pub const SHATTERED: i64 = 30_889;

pub const AQUEOUS_LIQUIDS: i64 = 2268;
pub const BASE_METALS: i64 = 2267;
pub const NOBLE_METALS: i64 = 2270;
pub const HEAVY_METALS: i64 = 2272;
pub const NON_CS_CRYSTALS: i64 = 2306;
pub const REACTIVE_METALS: i64 = 2398;
pub const PRECIOUS_METALS: i64 = 2399;
pub const TOXIC_METALS: i64 = 2400;
pub const CHIRAL_STRUCTURES: i64 = 2401;
pub const WATER: i64 = 3645;
pub const MECHANICAL_PARTS: i64 = 3689;
pub const CONSUMER_ELECTRONICS: i64 = 9836;
pub const ROBOTICS: i64 = 9848;

fn decimal(value: i64) -> Option<Decimal> {
    Some(Decimal::from(value))
}

fn pin(type_id: i64, kind: PlanetPinKind, planet_type_id: Option<i64>) -> PlanetPinType {
    PlanetPinType {
        type_id,
        kind,
        planet_type_id,
        harvester_type_id: None,
        cpu_load: None,
        power_load: None,
        cpu_output: None,
        power_output: None,
        ecu_head_cpu: None,
        ecu_head_power: None,
        cpu_load_per_km: None,
        power_load_per_km: None,
        cpu_load_level_modifier: None,
        power_load_level_modifier: None,
    }
}

fn loaded(type_id: i64, kind: PlanetPinKind, planet: i64, cpu: i64, power: i64) -> PlanetPinType {
    PlanetPinType {
        cpu_load: decimal(cpu),
        power_load: decimal(power),
        ..pin(type_id, kind, Some(planet))
    }
}

/// One planet type's structures; `ids` are its command center, ECU,
/// storage, launchpad, basic and advanced factory type ids.
fn planet_pins(planet: i64, ids: [i64; 6], resources: &[i64]) -> Vec<PlanetPinType> {
    let [command_center, ecu, storage, launchpad, basic, advanced] = ids;
    let mut pins = vec![
        PlanetPinType {
            cpu_output: decimal(1_675),
            power_output: decimal(6_000),
            ..pin(command_center, PlanetPinKind::CommandCenter, Some(planet))
        },
        PlanetPinType {
            ecu_head_cpu: decimal(110),
            ecu_head_power: decimal(550),
            ..loaded(ecu, PlanetPinKind::ExtractorControlUnit, planet, 400, 2_600)
        },
        loaded(storage, PlanetPinKind::Storage, planet, 500, 700),
        loaded(launchpad, PlanetPinKind::Launchpad, planet, 3_600, 700),
        loaded(basic, PlanetPinKind::Processor, planet, 200, 800),
        loaded(advanced, PlanetPinKind::Processor, planet, 500, 700),
    ];
    pins.extend(
        resources
            .iter()
            .enumerate()
            .map(|(index, resource)| PlanetPinType {
                harvester_type_id: Some(*resource),
                ..loaded(
                    planet * 100 + index as i64,
                    PlanetPinKind::ExtractorResource,
                    planet,
                    200,
                    800,
                )
            }),
    );
    pins
}

pub fn pin_types() -> Vec<PlanetPinType> {
    let mut pins = Vec::new();
    pins.extend(planet_pins(
        BARREN,
        [2524, 2848, 2541, 2544, 2473, 2474],
        &[AQUEOUS_LIQUIDS, BASE_METALS, 2288, 2073, NOBLE_METALS],
    ));
    pins.extend(planet_pins(
        LAVA,
        [2549, 3062, 2558, 2555, 2469, 2470],
        &[BASE_METALS, 2307, HEAVY_METALS, NON_CS_CRYSTALS, 2308],
    ));
    pins.extend(planet_pins(
        PLASMA,
        [2551, 3064, 2560, 2556, 2471, 2472],
        &[
            BASE_METALS,
            HEAVY_METALS,
            NOBLE_METALS,
            NON_CS_CRYSTALS,
            2308,
        ],
    ));
    pins.push(PlanetPinType {
        cpu_load: decimal(15),
        power_load: decimal(10),
        cpu_load_per_km: Some(Decimal::new(2, 1)),
        power_load_per_km: Some(Decimal::new(15, 2)),
        ..pin(2280, PlanetPinKind::Link, None)
    });
    pins
}

fn schematic(
    schematic_id: i64,
    cycle_time_seconds: i64,
    inputs: &[(i64, i64)],
    output: (i64, i64),
    pin_type_ids: Vec<i64>,
) -> PlanetSchematic {
    let line = |&(type_id, quantity): &(i64, i64)| PlanetSchematicLine { type_id, quantity };
    PlanetSchematic {
        schematic_id,
        name: format!("Schematic {schematic_id}"),
        cycle_time_seconds,
        inputs: inputs.iter().map(line).collect(),
        outputs: vec![line(&output)],
        pin_type_ids,
    }
}

pub fn schematics() -> Vec<PlanetSchematic> {
    let basic = || vec![2469, 2471, 2473];
    let advanced = || vec![2470, 2472, 2474];
    vec![
        schematic(
            121,
            1_800,
            &[(AQUEOUS_LIQUIDS, 3_000)],
            (WATER, 20),
            basic(),
        ),
        schematic(
            126,
            1_800,
            &[(BASE_METALS, 3_000)],
            (REACTIVE_METALS, 20),
            basic(),
        ),
        schematic(
            127,
            1_800,
            &[(NOBLE_METALS, 3_000)],
            (PRECIOUS_METALS, 20),
            basic(),
        ),
        schematic(
            128,
            1_800,
            &[(HEAVY_METALS, 3_000)],
            (TOXIC_METALS, 20),
            basic(),
        ),
        schematic(
            129,
            1_800,
            &[(NON_CS_CRYSTALS, 3_000)],
            (CHIRAL_STRUCTURES, 20),
            basic(),
        ),
        schematic(
            73,
            3_600,
            &[(REACTIVE_METALS, 40), (PRECIOUS_METALS, 40)],
            (MECHANICAL_PARTS, 5),
            advanced(),
        ),
        schematic(
            76,
            3_600,
            &[(TOXIC_METALS, 40), (CHIRAL_STRUCTURES, 40)],
            (CONSUMER_ELECTRONICS, 5),
            advanced(),
        ),
        schematic(
            97,
            3_600,
            &[(MECHANICAL_PARTS, 10), (CONSUMER_ELECTRONICS, 10)],
            (ROBOTICS, 3),
            advanced(),
        ),
    ]
}

pub fn commodities() -> BTreeMap<i64, PlanetCommodity> {
    let tiers: [(&[i64], i64); 4] = [
        (
            &[
                AQUEOUS_LIQUIDS,
                BASE_METALS,
                NOBLE_METALS,
                HEAVY_METALS,
                NON_CS_CRYSTALS,
            ],
            5,
        ),
        (
            &[
                WATER,
                REACTIVE_METALS,
                PRECIOUS_METALS,
                TOXIC_METALS,
                CHIRAL_STRUCTURES,
            ],
            400,
        ),
        (&[MECHANICAL_PARTS, CONSUMER_ELECTRONICS], 7_200),
        (&[ROBOTICS], 60_000),
    ];
    tiers
        .iter()
        .flat_map(|(types, multiplier)| {
            types.iter().map(move |type_id| {
                (
                    *type_id,
                    PlanetCommodity {
                        type_id: *type_id,
                        export_tax_multiplier: Decimal::from(*multiplier),
                        import_tax_multiplier: Decimal::from(*multiplier),
                    },
                )
            })
        })
        .collect()
}

pub fn volumes() -> BTreeMap<i64, f64> {
    let mut volumes = BTreeMap::new();
    for p0 in [
        AQUEOUS_LIQUIDS,
        BASE_METALS,
        NOBLE_METALS,
        HEAVY_METALS,
        NON_CS_CRYSTALS,
    ] {
        volumes.insert(p0, 0.005);
    }
    for p1 in [
        WATER,
        REACTIVE_METALS,
        PRECIOUS_METALS,
        TOXIC_METALS,
        CHIRAL_STRUCTURES,
    ] {
        volumes.insert(p1, 0.19);
    }
    volumes.insert(MECHANICAL_PARTS, 0.75);
    volumes.insert(CONSUMER_ELECTRONICS, 0.75);
    volumes.insert(ROBOTICS, 3.0);
    volumes
}

pub fn catalogue() -> PiCatalogue {
    PiCatalogue::build(pin_types(), schematics(), commodities(), volumes())
}
