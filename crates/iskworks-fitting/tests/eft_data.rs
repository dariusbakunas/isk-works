//! EFT import/export against the real engine data. Run `make fitting-data`
//! first, then `cargo test -p iskworks-fitting -- --ignored`.

use std::path::Path;
use std::sync::OnceLock;

use iskworks_core::fitting::{
    EftError, EftLine, FitInput, FitItem, FittingEngine, ModuleState, SlotKind,
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

/// The user's Bustard, exactly as the client copied it (2026-10-26).
const BUSTARD: &str = "[Bustard, .]
Warp Core Stabilizer II
Nanofiber Internal Structure II
Damage Control II

Dread Guristas EM Shield Hardener
Republic Fleet Medium Shield Extender
Multispectrum Shield Hardener II
50MN Y-T8 Compact Microwarpdrive
Compact Multispectrum Shield Hardener
10MN Afterburner II

Improved Cloaking Device II
Compact Interdiction Nullifier

Medium Core Defense Field Extender II
Medium Core Defense Field Extender II
";

/// A pyfa-style export with charges, drones and cargo.
const TRISTAN: &str = "[Tristan, Pyfa test]
Drone Damage Amplifier II
Damage Control II /OFFLINE

1MN Afterburner II
Warp Scrambler II

75mm Gatling Rail II, Antimatter Charge S
75mm Gatling Rail II, Antimatter Charge S

Small Hybrid Burst Aerator I


Hobgoblin II x5

Nanite Repair Paste x100
";

fn type_id(name: &str) -> i64 {
    engine()
        .type_id(name)
        .unwrap_or_else(|| panic!("no type named {name}"))
}

fn rack(fit: &FitInput, kind: SlotKind) -> Vec<&FitItem> {
    fit.items
        .iter()
        .filter(|item| item.slot_kind == kind)
        .collect()
}

fn find<'a>(fit: &'a FitInput, name: &str) -> &'a FitItem {
    let type_id = type_id(name);
    fit.items
        .iter()
        .find(|item| item.type_id == type_id)
        .unwrap_or_else(|| panic!("{name} is not in the fit"))
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn the_bustard_imports_into_its_racks_with_eves_states() {
    let parsed = engine().parse_eft(BUSTARD).unwrap();
    assert_eq!(parsed.name, None, "a '.' name is no name");
    let fit = parsed.fit;
    assert_eq!(fit.ship_type_id, type_id("Bustard"));
    assert_eq!(rack(&fit, SlotKind::Low).len(), 3);
    assert_eq!(rack(&fit, SlotKind::Med).len(), 6);
    assert_eq!(rack(&fit, SlotKind::High).len(), 2);
    assert_eq!(rack(&fit, SlotKind::Rig).len(), 2);
    let indexes: Vec<_> = rack(&fit, SlotKind::Med)
        .iter()
        .map(|item| item.slot_index)
        .collect();
    assert_eq!(indexes, (0..6).map(Some).collect::<Vec<_>>());

    // EVE's import rules: one propulsion module active, the cloak online.
    assert_eq!(
        find(&fit, "50MN Y-T8 Compact Microwarpdrive").state,
        ModuleState::Active
    );
    assert_eq!(find(&fit, "10MN Afterburner II").state, ModuleState::Online);
    assert_eq!(
        find(&fit, "Improved Cloaking Device II").state,
        ModuleState::Online
    );
    assert_eq!(
        find(&fit, "Dread Guristas EM Shield Hardener").state,
        ModuleState::Active
    );
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn passive_modules_and_rigs_import_online() {
    let fit = engine().parse_eft(BUSTARD).unwrap().fit;
    // Damage Control is passive in current data; the engine data decides.
    for passive in [
        "Damage Control II",
        "Nanofiber Internal Structure II",
        "Republic Fleet Medium Shield Extender",
        "Medium Core Defense Field Extender II",
    ] {
        assert_eq!(find(&fit, passive).state, ModuleState::Online, "{passive}");
    }
    assert_eq!(
        find(&fit, "Multispectrum Shield Hardener II").state,
        ModuleState::Active
    );
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn charges_offline_modules_drones_and_cargo_import() {
    let parsed = engine().parse_eft(TRISTAN).unwrap();
    assert_eq!(parsed.name.as_deref(), Some("Pyfa test"));
    let fit = parsed.fit;

    for gun in rack(&fit, SlotKind::High) {
        assert_eq!(gun.charge_type_id, Some(type_id("Antimatter Charge S")));
    }
    assert_eq!(find(&fit, "Damage Control II").state, ModuleState::Offline);

    let drones = rack(&fit, SlotKind::DroneBay);
    assert_eq!(drones.iter().map(|stack| stack.quantity).sum::<u32>(), 5);
    assert!(drones
        .iter()
        .all(|stack| stack.type_id == type_id("Hobgoblin II")));
    assert!(drones
        .iter()
        .any(|stack| stack.state == ModuleState::Active));

    let paste = find(&fit, "Nanite Repair Paste");
    assert_eq!((paste.slot_kind, paste.quantity), (SlotKind::Cargo, 100));
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn an_export_imports_back_to_the_same_fit() {
    for text in [BUSTARD, TRISTAN] {
        let parsed = engine().parse_eft(text).unwrap();
        let exported = engine().write_eft("Round trip", &parsed.fit).unwrap();
        assert!(
            exported.starts_with("[Bustard, Round trip]")
                || exported.starts_with("[Tristan, Round trip]"),
            "{exported}"
        );
        let again = engine().parse_eft(&exported).unwrap();
        assert_eq!(again.fit, parsed.fit, "{exported}");
    }
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn every_unknown_line_is_reported_with_its_number() {
    let text = BUSTARD
        .replace("Dread Guristas EM", "Dread Gurista EM")
        .replace("10MN Afterburner II", "10MN Aferburner II");
    assert_eq!(
        engine().parse_eft(&text).err(),
        Some(EftError::UnknownLines(vec![
            EftLine {
                line: 6,
                text: "Dread Gurista EM Shield Hardener".to_string()
            },
            EftLine {
                line: 11,
                text: "10MN Aferburner II".to_string()
            },
        ]))
    );
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn an_unknown_ship_or_charge_is_an_unknown_line() {
    let text = TRISTAN.replace("[Tristan,", "[Tristram,").replacen(
        "Antimatter Charge S",
        "Antimater Charge S",
        1,
    );
    let Some(EftError::UnknownLines(lines)) = engine().parse_eft(&text).err() else {
        panic!("expected unknown lines");
    };
    assert_eq!(
        lines.iter().map(|line| line.line).collect::<Vec<_>>(),
        [1, 8]
    );
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn text_that_is_not_eft_is_refused() {
    assert!(matches!(
        engine().parse_eft("Hello there").err(),
        Some(EftError::Invalid(_))
    ));
    assert!(matches!(
        engine().parse_eft("").err(),
        Some(EftError::Invalid(_))
    ));
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn empty_slots_keep_the_positions_after_them() {
    let text = "[Rifter, Gaps]\n[Empty Low slot]\nDamage Control II\n";
    let fit = engine().parse_eft(text).unwrap().fit;
    assert_eq!(find(&fit, "Damage Control II").slot_index, Some(1));
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn an_export_writes_only_the_state_suffix_the_client_knows() {
    for text in [BUSTARD, TRISTAN] {
        let fit = engine().parse_eft(text).unwrap().fit;
        let exported = engine().write_eft("Client", &fit).unwrap();
        for suffix in ["/online", "/active", "/overload", "/offline"] {
            assert!(!exported.contains(suffix), "{suffix} in:\n{exported}");
        }
    }
    let tristan = engine().parse_eft(TRISTAN).unwrap().fit;
    let exported = engine().write_eft("Client", &tristan).unwrap();
    assert!(
        exported
            .lines()
            .any(|line| line == "Damage Control II /OFFLINE"),
        "{exported}"
    );
}
