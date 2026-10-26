use std::collections::BTreeMap;

use super::*;

#[test]
fn the_uniform_profile_splits_damage_evenly() {
    let profile = DamageProfile::uniform();
    assert_eq!(profile.normalized(), [0.25, 0.25, 0.25, 0.25]);
}

#[test]
fn a_profile_normalizes_to_shares_of_its_total() {
    let profile = DamageProfile::new(2.0, 1.0, 1.0, 0.0).unwrap();
    assert_eq!(profile.normalized(), [0.5, 0.25, 0.25, 0.0]);
}

#[test]
fn a_profile_refuses_negative_empty_and_non_finite_damage() {
    assert_eq!(
        DamageProfile::new(-1.0, 1.0, 1.0, 1.0),
        Err(FittingError::InvalidDamageProfile)
    );
    assert_eq!(
        DamageProfile::new(0.0, 0.0, 0.0, 0.0),
        Err(FittingError::InvalidDamageProfile)
    );
    assert_eq!(
        DamageProfile::new(f64::NAN, 1.0, 1.0, 1.0),
        Err(FittingError::InvalidDamageProfile)
    );
    assert_eq!(
        DamageProfile::new(f64::INFINITY, 1.0, 1.0, 1.0),
        Err(FittingError::InvalidDamageProfile)
    );
}

#[test]
fn pilot_skills_accept_levels_zero_to_five() {
    let skills = PilotSkills::new(BTreeMap::from([(3_300, 0), (3_426, 5)])).unwrap();
    assert_eq!(skills.levels(), &BTreeMap::from([(3_300, 0), (3_426, 5)]));
}

#[test]
fn pilot_skills_refuse_levels_outside_zero_to_five() {
    assert_eq!(
        PilotSkills::new(BTreeMap::from([(3_300, 6)])),
        Err(FittingError::InvalidSkillLevel {
            skill_type_id: 3_300,
            level: 6
        })
    );
    assert_eq!(
        PilotSkills::new(BTreeMap::from([(3_300, -1)])),
        Err(FittingError::InvalidSkillLevel {
            skill_type_id: 3_300,
            level: -1
        })
    );
}

#[test]
fn slot_kinds_and_states_use_the_api_names() {
    let names = [
        (SlotKind::High, "high"),
        (SlotKind::Med, "med"),
        (SlotKind::Low, "low"),
        (SlotKind::Rig, "rig"),
        (SlotKind::Subsystem, "subsystem"),
        (SlotKind::Service, "service"),
        (SlotKind::FighterTube, "fighter_tube"),
        (SlotKind::FighterBay, "fighter_bay"),
        (SlotKind::DroneBay, "drone_bay"),
        (SlotKind::Cargo, "cargo"),
        (SlotKind::Implant, "implant"),
        (SlotKind::Booster, "booster"),
    ];
    for (kind, name) in names {
        assert_eq!(serde_json::to_value(kind).unwrap(), name);
        assert_eq!(kind.as_str(), name);
        assert_eq!(name.parse::<SlotKind>(), Ok(kind));
    }
    assert!("medium".parse::<SlotKind>().is_err());

    let states = [
        (ModuleState::Offline, "offline"),
        (ModuleState::Online, "online"),
        (ModuleState::Active, "active"),
        (ModuleState::Overload, "overload"),
    ];
    for (state, name) in states {
        assert_eq!(serde_json::to_value(state).unwrap(), name);
        assert_eq!(state.as_str(), name);
        assert_eq!(name.parse::<ModuleState>(), Ok(state));
    }
}

#[test]
fn racks_are_the_slot_kinds_with_positions() {
    assert!(SlotKind::High.is_rack());
    assert!(SlotKind::Rig.is_rack());
    assert!(SlotKind::FighterTube.is_rack());
    assert!(!SlotKind::DroneBay.is_rack());
    assert!(!SlotKind::Cargo.is_rack());
    assert!(!SlotKind::FighterBay.is_rack());
}

#[test]
fn resists_are_one_minus_resonance() {
    let resists = Resists::from_resonances([1.0, 0.5, 0.25, 0.0]);
    assert_eq!(
        resists,
        Resists {
            em: 0.0,
            thermal: 0.5,
            kinetic: 0.75,
            explosive: 1.0
        }
    );
}
