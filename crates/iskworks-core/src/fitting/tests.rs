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

#[test]
fn every_preset_is_a_valid_profile_with_a_unique_id() {
    let presets = damage_profile_presets();
    assert_eq!(presets[0].id, "uniform");
    assert_eq!(
        presets[0].profile.normalized(),
        DamageProfile::uniform().normalized()
    );
    let mut ids = std::collections::BTreeSet::new();
    for preset in &presets {
        let [em, thermal, kinetic, explosive] = preset.profile.normalized();
        assert!(
            DamageProfile::new(em, thermal, kinetic, explosive).is_ok(),
            "{}",
            preset.id
        );
        assert!(ids.insert(preset.id), "duplicate {}", preset.id);
    }
}

fn item(slot_kind: SlotKind, slot_index: Option<i16>) -> FitItem {
    FitItem {
        type_id: 2_048,
        slot_kind,
        slot_index,
        quantity: 1,
        state: ModuleState::Online,
        charge_type_id: None,
        mutation: None,
    }
}

fn draft(items: Vec<FitItem>) -> FitDraft {
    FitDraft {
        name: "Null Transport".to_string(),
        description: String::new(),
        ship_type_id: 12_731,
        items,
    }
}

fn refused(draft: &FitDraft) -> String {
    draft.validate().expect_err("the draft should be refused")
}

#[test]
fn a_draft_with_racks_bays_and_cargo_is_valid() {
    let mut drones = item(SlotKind::DroneBay, None);
    drones.quantity = 5;
    let mut gun = item(SlotKind::High, Some(0));
    gun.charge_type_id = Some(185);
    let draft = draft(vec![
        gun,
        item(SlotKind::High, Some(1)),
        item(SlotKind::Low, Some(0)),
        drones,
        item(SlotKind::Cargo, None),
        item(SlotKind::Implant, Some(1)),
    ]);
    assert_eq!(draft.validate(), Ok(()));
}

#[test]
fn a_draft_needs_a_name_of_at_most_a_hundred_characters() {
    let mut blank = draft(Vec::new());
    blank.name = "   ".to_string();
    assert!(refused(&blank).contains("name"));

    let mut long = draft(Vec::new());
    long.name = "x".repeat(101);
    assert!(refused(&long).contains("name"));
}

#[test]
fn a_draft_description_is_at_most_two_thousand_characters() {
    let mut long = draft(Vec::new());
    long.description = "x".repeat(2_001);
    assert!(refused(&long).contains("description"));
}

#[test]
fn a_draft_holds_at_most_512_items() {
    let items = (0..513).map(|_| item(SlotKind::Cargo, None)).collect();
    assert!(refused(&draft(items)).contains("512"));
}

#[test]
fn rack_items_need_an_index_in_range() {
    assert!(refused(&draft(vec![item(SlotKind::Med, None)])).contains("position"));
    assert!(refused(&draft(vec![item(SlotKind::Med, Some(-1))])).contains("position"));
    assert!(refused(&draft(vec![item(SlotKind::Med, Some(32))])).contains("position"));
    assert!(refused(&draft(vec![item(SlotKind::Implant, Some(0))])).contains("position"));
    assert!(refused(&draft(vec![item(SlotKind::Booster, Some(0))])).contains("position"));
}

#[test]
fn two_items_cannot_share_a_rack_position() {
    let draft = draft(vec![
        item(SlotKind::Low, Some(2)),
        item(SlotKind::Low, Some(2)),
    ]);
    assert!(refused(&draft).contains("low 2"));
}

#[test]
fn bay_and_cargo_items_have_no_index() {
    assert!(refused(&draft(vec![item(SlotKind::DroneBay, Some(0))])).contains("position"));
    assert!(refused(&draft(vec![item(SlotKind::Cargo, Some(0))])).contains("position"));
}

#[test]
fn quantities_are_positive_and_one_for_modules() {
    let mut none = item(SlotKind::Cargo, None);
    none.quantity = 0;
    assert!(refused(&draft(vec![none])).contains("quantity"));

    let mut stacked_module = item(SlotKind::High, Some(0));
    stacked_module.quantity = 2;
    assert!(refused(&draft(vec![stacked_module])).contains("quantity"));

    let mut squadron = item(SlotKind::FighterTube, Some(0));
    squadron.quantity = 9;
    assert_eq!(draft(vec![squadron]).validate(), Ok(()));
}

#[test]
fn only_rack_items_hold_charges() {
    let mut loaded_cargo = item(SlotKind::Cargo, None);
    loaded_cargo.charge_type_id = Some(185);
    assert!(refused(&draft(vec![loaded_cargo])).contains("charge"));
}

#[test]
fn a_stored_fit_is_calculated_from_its_ship_and_items() {
    let fit = Fit {
        id: FitId(uuid::Uuid::nil()),
        name: "Null Transport".to_string(),
        description: String::new(),
        ship_type_id: 12_731,
        items: vec![item(SlotKind::Low, Some(0))],
        created_at: chrono::DateTime::UNIX_EPOCH,
        updated_at: chrono::DateTime::UNIX_EPOCH,
    };
    assert_eq!(
        fit.input(),
        FitInput {
            ship_type_id: 12_731,
            items: vec![item(SlotKind::Low, Some(0))],
        }
    );
}
