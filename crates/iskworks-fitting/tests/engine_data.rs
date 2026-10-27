//! Tests against the real engine data. Run `make fitting-data` first, then
//! `cargo test -p iskworks-fitting -- --ignored`.

use std::path::Path;
use std::sync::OnceLock;

use std::collections::BTreeMap;

use iskworks_core::fitting::{
    AttributeExplanation, CapacitorState, DamageProfile, ExplainTarget, FitCalculation, FitInput,
    FitItem, FittingEngine, FittingError, HeadlineKind, ModuleState, Pilot, PilotSkills, SlotKind,
    SourceRef,
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
        mutation: None,
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
        mutation: None,
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
        mutation: None,
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

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn per_item_fitting_costs_add_up_to_the_ships() {
    let calculation = calculate(&bustard(), &Pilot::AllV);
    assert_eq!(calculation.items.len(), bustard().items.len());
    let items: Vec<_> = calculation.items.iter().map(|item| item.unwrap()).collect();
    let cpu: f64 = items.iter().map(|item| item.cpu).sum();
    let powergrid: f64 = items.iter().map(|item| item.powergrid).sum();
    let calibration: f64 = items.iter().map(|item| item.calibration).sum();
    let resources = calculation.stats.resources;
    assert_close(cpu, resources.cpu.used, "cpu");
    assert_close(powergrid, resources.powergrid.used, "powergrid");
    assert_close(calibration, resources.calibration.used, "calibration");
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn active_modules_use_capacitor_and_passive_ones_do_not() {
    let fit = bustard();
    let calculation = calculate(&fit, &Pilot::AllV);
    let stats_of = |name: &str| {
        let index = fit
            .items
            .iter()
            .position(|item| item.type_id == type_id(name))
            .unwrap();
        calculation.items[index].unwrap()
    };
    let mwd = stats_of("50MN Y-T8 Compact Microwarpdrive");
    assert_eq!(mwd.reached_state, ModuleState::Active);
    assert!(mwd.capacitor_per_second > 0.0, "{mwd:?}");
    let nanofiber = stats_of("Nanofiber Internal Structure II");
    assert_eq!(nanofiber.reached_state, ModuleState::Online);
    assert_eq!(nanofiber.max_state, ModuleState::Online);
    assert_eq!(nanofiber.capacitor_per_second, 0.0);
    // Requested active, but only one propulsion module may run.
    let afterburner = stats_of("10MN Afterburner II");
    assert_eq!(afterburner.reached_state, ModuleState::Online);
    assert_eq!(afterburner.capacitor_per_second, 0.0);
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn guns_report_range_and_their_share_of_the_damage() {
    let calculation = calculate(&gun_rifter(), &Pilot::AllV);
    let guns: Vec<_> = calculation.items.iter().map(|item| item.unwrap()).collect();
    for gun in &guns {
        assert!(gun.optimal > 0.0 && gun.falloff > 0.0, "{gun:?}");
        assert!(gun.dps > 0.0, "{gun:?}");
    }
    assert_close(
        guns.iter().map(|gun| gun.dps).sum(),
        calculation.stats.offense.weapon_dps,
        "gun dps",
    );
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn unknown_items_have_no_item_stats() {
    let mut fit = gun_rifter();
    fit.items.push(FitItem {
        type_id: 999_999_999,
        slot_kind: SlotKind::Low,
        slot_index: Some(0),
        quantity: 1,
        state: ModuleState::Online,
        charge_type_id: None,
        mutation: None,
    });
    let calculation = calculate(&fit, &Pilot::AllV);
    assert_eq!(calculation.items.len(), 4);
    assert!(calculation.items[3].is_none());
    assert!(calculation.items[0].is_some());
}

fn explain(
    fit: &FitInput,
    pilot: &Pilot,
    target: ExplainTarget,
    attribute: &str,
) -> AttributeExplanation {
    engine()
        .explain(fit, pilot, &DamageProfile::uniform(), target, attribute)
        .unwrap()
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn cpu_output_is_explained_by_cpu_management() {
    let explained = explain(
        &empty("Rifter"),
        &Pilot::AllV,
        ExplainTarget::Ship,
        "cpuOutput",
    );
    assert_close(explained.value, explained.base * 1.25, "cpu output");
    let cpu_management = type_id("CPU Management");
    assert!(
        explained.sources.iter().any(|source| source.from
            == SourceRef::Skill {
                type_id: cpu_management
            }
            && source.applied),
        "{:?}",
        explained.sources
    );
    let skill = explained
        .sources
        .iter()
        .find(|source| {
            source.from
                == SourceRef::Skill {
                    type_id: cpu_management,
                }
        })
        .unwrap();
    assert_eq!(skill.operator, "post_percent");
    assert_close(skill.value, 25.0, "CPU Management V");
    // Every source of an empty hull is a skill or the hull itself.
    assert!(explained.sources.iter().all(|source| matches!(
        source.from,
        SourceRef::Skill { .. } | SourceRef::Ship | SourceRef::Character
    )));
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn shield_capacity_lists_the_extender_and_rigs_by_their_fit_index() {
    let fit = bustard();
    let index_of = |name: &str| {
        let type_id = type_id(name);
        fit.items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.type_id == type_id)
            .map(|(index, _)| index)
            .collect::<Vec<_>>()
    };
    let explained = explain(&fit, &Pilot::AllV, ExplainTarget::Ship, "shieldCapacity");
    let items: Vec<usize> = explained
        .sources
        .iter()
        .filter_map(|source| match source.from {
            SourceRef::Item { index } => Some(index),
            _ => None,
        })
        .collect();
    for index in index_of("Republic Fleet Medium Shield Extender")
        .into_iter()
        .chain(index_of("Medium Core Defense Field Extender II"))
    {
        assert!(items.contains(&index), "{index} not in {items:?}");
    }
    assert!(explained.value > explained.base);
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn an_item_attribute_can_be_explained_and_bad_requests_are_refused() {
    let fit = gun_rifter();
    let explained = explain(&fit, &Pilot::AllV, ExplainTarget::Item(0), "maxRange");
    assert!(explained.value > 0.0);

    let engine = engine();
    let profile = DamageProfile::uniform();
    assert_eq!(
        engine
            .explain(
                &fit,
                &Pilot::AllV,
                &profile,
                ExplainTarget::Ship,
                "notAnAttribute"
            )
            .err(),
        Some(FittingError::UnknownAttribute)
    );
    assert_eq!(
        engine
            .explain(
                &fit,
                &Pilot::AllV,
                &profile,
                ExplainTarget::Item(9),
                "maxRange"
            )
            .err(),
        Some(FittingError::UnknownItem(9))
    );
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn modules_headline_what_they_do() {
    let fit = bustard();
    let calculation = calculate(&fit, &Pilot::AllV);
    let headline_of = |name: &str| {
        let index = fit
            .items
            .iter()
            .position(|item| item.type_id == type_id(name))
            .unwrap();
        calculation.items[index].unwrap().headline
    };
    let kind_of = |name: &str| headline_of(name).map(|headline| headline.kind);
    assert_eq!(
        kind_of("Multispectrum Shield Hardener II"),
        Some(HeadlineKind::Resist)
    );
    assert_eq!(
        kind_of("Republic Fleet Medium Shield Extender"),
        Some(HeadlineKind::ShieldHp)
    );
    assert_eq!(
        kind_of("50MN Y-T8 Compact Microwarpdrive"),
        Some(HeadlineKind::Velocity)
    );
    assert_eq!(kind_of("Nanofiber Internal Structure II"), None);
    for name in [
        "Multispectrum Shield Hardener II",
        "Republic Fleet Medium Shield Extender",
        "50MN Y-T8 Compact Microwarpdrive",
    ] {
        assert!(headline_of(name).unwrap().value > 0.0, "{name}");
    }
    // A single-type hardener headlines its own bonus, not an average that
    // counts the three types it doesn't touch.
    let em = headline_of("Dread Guristas EM Shield Hardener").unwrap();
    assert_eq!(em.kind, HeadlineKind::Resist);
    assert!(em.value > 40.0, "{em:?}");

    let guns = calculate(&gun_rifter(), &Pilot::AllV);
    let gun = guns.items[0].unwrap();
    assert_eq!(
        gun.headline.map(|headline| headline.kind),
        Some(HeadlineKind::Dps)
    );
    assert_close(gun.headline.unwrap().value, gun.dps, "gun headline");
}

/// A Devoter with its microwarpdrive running and two bubbles in `bubbles`.
fn bubble_devoter(bubbles: [ModuleState; 2]) -> FitInput {
    FitInput {
        items: vec![
            module(
                "50MN Quad LiF Restrained Microwarpdrive",
                SlotKind::Med,
                0,
                ModuleState::Active,
            ),
            module(
                "Warp Disruption Field Generator II",
                SlotKind::High,
                0,
                bubbles[0],
            ),
            module(
                "Warp Disruption Field Generator II",
                SlotKind::High,
                1,
                bubbles[1],
            ),
        ],
        ..empty("Devoter")
    }
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn an_active_bubble_blooms_the_signature_unpenalised_as_pyfa_does() {
    let signature = |bubbles| {
        calculate(&bubble_devoter(bubbles), &Pilot::AllV)
            .stats
            .targeting
            .signature_radius
    };
    let idle = signature([ModuleState::Online, ModuleState::Online]);
    // The microwarpdrive's +450% on the Devoter's 140 m.
    assert_close(idle, 770.0, "bubbles online");
    assert_close(
        signature([ModuleState::Active, ModuleState::Online]),
        idle * 1.5,
        "one bubble",
    );
    // pyfa's 1.73k: each bubble's +50% in full.
    assert_close(
        signature([ModuleState::Active, ModuleState::Active]),
        idle * 1.5 * 1.5,
        "two bubbles",
    );
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn the_bubble_bloom_is_explained_per_bubble() {
    let explained = explain(
        &bubble_devoter([ModuleState::Active, ModuleState::Online]),
        &Pilot::AllV,
        ExplainTarget::Ship,
        "signatureRadius",
    );
    assert_close(explained.value, 770.0 * 1.5, "explained value");
    let bubbles: Vec<_> = explained
        .sources
        .iter()
        .filter(|source| matches!(source.from, SourceRef::Item { index: 1 | 2 }))
        .map(|source| {
            (
                source.from,
                source.operator.as_str(),
                source.value,
                source.penalty,
                source.applied,
            )
        })
        .collect();
    assert_eq!(
        bubbles,
        [
            (
                SourceRef::Item { index: 1 },
                "post_percent",
                50.0,
                None,
                true
            ),
            (
                SourceRef::Item { index: 2 },
                "post_percent",
                50.0,
                None,
                false
            ),
        ]
    );
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn a_nosferatu_gains_capacitor_and_ranges_read_as_pyfa_shows_them() {
    let mut fit = bubble_devoter([ModuleState::Active, ModuleState::Active]);
    fit.items[2].charge_type_id = Some(type_id("Focused Warp Disruption Script"));
    fit.items.push(module(
        "Small Ghoul Compact Energy Nosferatu",
        SlotKind::High,
        2,
        ModuleState::Active,
    ));
    let calculation = calculate(&fit, &Pilot::AllV);
    let stats = |index: usize| calculation.items[index].unwrap();

    // pyfa's -3.75: 112.5 GJ every 30 s.
    assert_close(stats(1).capacitor_per_second, 3.75, "bubble cap");
    // The bubble's radius is its range.
    assert_close(stats(1).optimal, 25_000.0, "bubble range");
    // The focused script: -60% capacitor need, -80% cycle time, +50% range.
    assert_close(stats(2).capacitor_per_second, 45.0 / 6.0, "focused cap");
    assert_close(stats(2).optimal, 37_500.0, "focused range");

    // pyfa's +3.6 and 4km+2km: 9 GJ drained every 2.5 s.
    let nosferatu = stats(3);
    assert_close(nosferatu.capacitor_per_second, -3.6, "nosferatu cap");
    assert_close(nosferatu.optimal, 4_000.0, "nosferatu optimal");
    assert_close(nosferatu.falloff, 2_000.0, "nosferatu falloff");
}
