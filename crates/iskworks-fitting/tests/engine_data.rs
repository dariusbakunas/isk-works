//! Tests against the real engine data. Run `make fitting-data` first, then
//! `cargo test -p iskworks-fitting -- --ignored`.

use std::path::Path;
use std::sync::OnceLock;

use std::collections::BTreeMap;

use iskworks_core::fitting::{
    CapacitorState, DamageProfile, FitCalculation, FitInput, FitItem, FittingEngine, FittingError,
    ModuleState, Pilot, PilotSkills, SlotKind,
};
use iskworks_fitting::{DataLock, EsfFittingEngine};

const REQUIRES_DATA: &str = "requires data/fitting/sde.dat (make fitting-data)";

fn engine() -> &'static EsfFittingEngine {
    static ENGINE: OnceLock<EsfFittingEngine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/fitting/sde.dat");
        EsfFittingEngine::load(&path).unwrap_or_else(|error| panic!("{REQUIRES_DATA}: {error}"))
    })
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn the_pinned_data_loads_and_reports_its_version() {
    let lock = DataLock::pinned();
    let info = engine().engine_info();
    assert_eq!(info.data_version, lock.version);
    assert_eq!(info.major, lock.major);
    assert_eq!(info.build_number, lock.build_number);
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn every_published_skill_is_known_for_all_v() {
    // ~500 published skills in current EVE builds.
    assert!(engine().skill_count() > 400, "{}", engine().skill_count());
}

fn type_id(name: &str) -> i64 {
    engine()
        .type_id(name)
        .unwrap_or_else(|| panic!("no type named {name}"))
}

fn module(name: &str, slot_kind: SlotKind, slot_index: i16, state: ModuleState) -> FitItem {
    FitItem {
        type_id: type_id(name),
        slot_kind,
        slot_index: Some(slot_index),
        quantity: 1,
        state,
        charge_type_id: None,
    }
}

fn calculate(fit: &FitInput, pilot: &Pilot) -> FitCalculation {
    engine()
        .calculate(fit, pilot, &DamageProfile::uniform())
        .unwrap()
}

fn empty(ship: &str) -> FitInput {
    FitInput {
        ship_type_id: type_id(ship),
        items: Vec::new(),
    }
}

fn assert_close(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= expected.abs() * 1e-6,
        "{what}: {actual} != {expected}"
    );
}

/// The user's Bustard (spec §1.3), with the states a pilot would fly it in:
/// hardeners and the MWD active, the AB, cloak and nullifier online.
fn bustard() -> FitInput {
    use ModuleState::{Active, Online};
    use SlotKind::{High, Low, Med, Rig};
    FitInput {
        ship_type_id: type_id("Bustard"),
        items: vec![
            module("Warp Core Stabilizer II", Low, 0, Online),
            module("Nanofiber Internal Structure II", Low, 1, Online),
            module("Damage Control II", Low, 2, Active),
            module("Dread Guristas EM Shield Hardener", Med, 0, Active),
            module("Republic Fleet Medium Shield Extender", Med, 1, Online),
            module("Multispectrum Shield Hardener II", Med, 2, Active),
            module("50MN Y-T8 Compact Microwarpdrive", Med, 3, Active),
            module("Compact Multispectrum Shield Hardener", Med, 4, Active),
            module("10MN Afterburner II", Med, 5, Online),
            module("Improved Cloaking Device II", High, 0, Online),
            module("Compact Interdiction Nullifier", High, 1, Online),
            module("Medium Core Defense Field Extender II", Rig, 0, Online),
            module("Medium Core Defense Field Extender II", Rig, 1, Online),
        ],
    }
}

/// A Rifter with three autocannons loaded with EMP.
fn gun_rifter() -> FitInput {
    let mut fit = empty("Rifter");
    for index in 0..3 {
        let mut gun = module(
            "200mm AutoCannon II",
            SlotKind::High,
            index,
            ModuleState::Active,
        );
        gun.charge_type_id = Some(type_id("EMP S"));
        fit.items.push(gun);
    }
    fit
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn level_five_management_skills_add_a_quarter() {
    let base = calculate(&empty("Rifter"), &Pilot::AllZero).stats;
    let trained = calculate(&empty("Rifter"), &Pilot::AllV).stats;
    // CPU, Power Grid, Shield and Capacitor Management: 5% per level.
    assert_close(
        trained.resources.cpu.total,
        base.resources.cpu.total * 1.25,
        "cpu",
    );
    assert_close(
        trained.resources.powergrid.total,
        base.resources.powergrid.total * 1.25,
        "powergrid",
    );
    assert_close(
        trained.defense.shield.hp,
        base.defense.shield.hp * 1.25,
        "shield hp",
    );
    assert_close(
        trained.capacitor.capacity,
        base.capacitor.capacity * 1.25,
        "capacitor",
    );
    assert!(trained.navigation.align_seconds < base.navigation.align_seconds);
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn a_pilot_gets_only_the_skills_they_have() {
    let cpu_management = type_id("CPU Management");
    let pilot = Pilot::Skills(PilotSkills::new(BTreeMap::from([(cpu_management, 5)])).unwrap());
    let base = calculate(&empty("Rifter"), &Pilot::AllZero).stats;
    let pilot = calculate(&empty("Rifter"), &pilot).stats;
    assert_close(
        pilot.resources.cpu.total,
        base.resources.cpu.total * 1.25,
        "cpu",
    );
    assert_close(
        pilot.resources.powergrid.total,
        base.resources.powergrid.total,
        "powergrid",
    );
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn uniform_ehp_is_hp_over_the_mean_resonance() {
    let defense = calculate(&bustard(), &Pilot::AllV).stats.defense;
    for (name, layer) in [
        ("shield", defense.shield),
        ("armor", defense.armor),
        ("hull", defense.hull),
    ] {
        let resists = layer.resists;
        let mean_resonance =
            1.0 - (resists.em + resists.thermal + resists.kinetic + resists.explosive) / 4.0;
        assert_close(layer.ehp, layer.hp / mean_resonance, name);
        assert!(layer.ehp >= layer.hp, "{name}");
    }
    assert_close(
        defense.ehp,
        defense.shield.ehp + defense.armor.ehp + defense.hull.ehp,
        "total",
    );
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn the_bustard_fits_with_all_v_and_counts_its_racks() {
    let calculation = calculate(&bustard(), &Pilot::AllV);
    assert_eq!(calculation.violations, Vec::new());
    assert!(calculation.unknown_items.is_empty());
    let resources = calculation.stats.resources;
    for (rack, usage, used) in [
        ("high", resources.high_slots, 2.0),
        ("med", resources.med_slots, 6.0),
        ("low", resources.low_slots, 3.0),
        ("rig", resources.rig_slots, 2.0),
    ] {
        assert_eq!(usage.used, used, "{rack}");
        assert!(usage.total >= used, "{rack}: {usage:?}");
    }
    assert!(resources.cpu.used > 0.0 && resources.cpu.used <= resources.cpu.total);
    assert!(resources.calibration.used > 0.0);
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn the_bustard_needs_skills_an_untrained_pilot_lacks() {
    let calculation = calculate(&bustard(), &Pilot::AllZero);
    assert!(
        calculation
            .violations
            .iter()
            .any(|violation| violation.rule == "skill"),
        "{:?}",
        calculation.violations
    );
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn the_capacitor_is_stable_when_idle_and_drains_under_a_microwarpdrive() {
    let idle = calculate(&empty("Bustard"), &Pilot::AllV).stats.capacitor;
    match idle.state {
        CapacitorState::Stable { level } => assert_close(level, 1.0, "idle level"),
        CapacitorState::Depletes { .. } => panic!("an idle capacitor depletes: {idle:?}"),
    }

    let running = calculate(&bustard(), &Pilot::AllV).stats.capacitor;
    assert!(running.peak_delta < 0.0, "{running:?}");
    match running.state {
        CapacitorState::Depletes { seconds } => assert!(seconds > 0.0, "{seconds}"),
        CapacitorState::Stable { .. } => panic!("expected depletion, got {running:?}"),
    }
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn guns_deal_damage_and_use_turret_hardpoints() {
    let empty = calculate(&empty("Rifter"), &Pilot::AllV).stats;
    assert_eq!(empty.offense.weapon_dps, 0.0);
    assert_eq!(empty.offense.volley, 0.0);

    let armed = calculate(&gun_rifter(), &Pilot::AllV);
    assert_eq!(armed.violations, Vec::new());
    let stats = armed.stats;
    assert!(stats.offense.weapon_dps > 0.0, "{:?}", stats.offense);
    assert!(stats.offense.volley > 0.0, "{:?}", stats.offense);
    assert!(stats.offense.weapon_dps_with_reload <= stats.offense.weapon_dps);
    assert_close(
        stats.offense.total_dps,
        stats.offense.weapon_dps + stats.offense.drone_dps,
        "total dps",
    );
    assert_eq!(stats.resources.turret_hardpoints.used, 3.0);
    assert_eq!(
        stats.resources.turret_hardpoints.total,
        empty.resources.turret_hardpoints.total
    );
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn an_unknown_ship_is_refused_and_unknown_items_are_left_out() {
    let unknown_ship = FitInput {
        ship_type_id: 999_999_999,
        items: Vec::new(),
    };
    assert_eq!(
        engine()
            .calculate(&unknown_ship, &Pilot::AllV, &DamageProfile::uniform())
            .err(),
        Some(FittingError::UnknownShip(999_999_999))
    );

    let mut fit = gun_rifter();
    fit.items.push(FitItem {
        type_id: 999_999_999,
        slot_kind: SlotKind::Low,
        slot_index: Some(0),
        quantity: 1,
        state: ModuleState::Online,
        charge_type_id: None,
    });
    let calculation = calculate(&fit, &Pilot::AllV);
    assert_eq!(calculation.unknown_items, vec![3]);
    assert!(calculation.stats.offense.weapon_dps > 0.0);
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn targeting_navigation_and_drones_have_values() {
    let stats = calculate(&bustard(), &Pilot::AllV).stats;
    assert!(stats.targeting.max_targets >= 1.0);
    assert!(stats.targeting.lock_range > 0.0);
    assert!(stats.targeting.scan_resolution > 0.0);
    assert!(stats.targeting.sensor_strength > 0.0);
    assert!(stats.targeting.signature_radius > 0.0);
    assert!(stats.navigation.max_velocity > 0.0);
    assert!(stats.navigation.mass > 0.0);
    assert!(stats.navigation.warp_speed > 0.0);
    assert!(stats.drones.max_active >= 5.0, "{:?}", stats.drones);
    assert!(stats.drones.control_range > 0.0);
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn drone_damage_is_counted_apart_from_weapons() {
    let mut fit = empty("Tristan");
    fit.items.push(FitItem {
        type_id: type_id("Hobgoblin II"),
        slot_kind: SlotKind::DroneBay,
        slot_index: None,
        quantity: 5,
        state: ModuleState::Active,
        charge_type_id: None,
    });
    let calculation = calculate(&fit, &Pilot::AllV);
    assert_eq!(calculation.violations, Vec::new());
    let offense = calculation.stats.offense;
    assert!(offense.drone_dps > 0.0, "{offense:?}");
    assert_eq!(offense.weapon_dps, 0.0, "{offense:?}");
    assert_close(offense.total_dps, offense.drone_dps, "total dps");
    assert_eq!(calculation.stats.drones.active, 5.0);
}

/// Not an assertion: prints how long one calculation takes, for sizing the
/// per-character pilots table (spec §5.3). Run with `--release --nocapture`.
#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn calculation_timing() {
    let fit = bustard();
    let runs = 200;
    let start = std::time::Instant::now();
    for _ in 0..runs {
        calculate(&fit, &Pilot::AllV);
    }
    let per_run = start.elapsed() / runs;
    println!("one Bustard calculation (All V): {per_run:?}");
}
