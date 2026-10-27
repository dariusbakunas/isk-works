use std::collections::BTreeMap;

use chrono::{TimeZone, Utc};
use iskworks_sde::{RequiredSkill, SkillAttribute, SkillDefinition};

use super::*;
use crate::fitting::{FitInput, FitItem, ModuleState, SlotKind};
use crate::skills::{CharacterAttributes, CharacterSkill, CharacterSkillsSnapshot};

const SPACESHIP_COMMAND: i64 = 100;
const CALDARI_HAULER: i64 = 101;
const TRANSPORT_SHIPS: i64 = 102;
const SHIELD_UPGRADES: i64 = 103;

const SHIP: i64 = 900;
const MODULE: i64 = 901;
const CHARGE: i64 = 902;
const CARGO: i64 = 903;

fn definition(
    type_id: i64,
    name: &str,
    rank: i32,
    prerequisites: Vec<(i64, i16)>,
) -> SkillDefinition {
    SkillDefinition {
        type_id,
        name: name.to_string(),
        group_id: 257,
        group_name: "Spaceship Command".to_string(),
        rank,
        primary_attribute: SkillAttribute::Perception,
        secondary_attribute: SkillAttribute::Willpower,
        omega_only: false,
        prerequisites: prerequisites
            .into_iter()
            .map(|(skill_type_id, level)| RequiredSkill {
                skill_type_id,
                level,
            })
            .collect(),
    }
}

fn definitions() -> Vec<SkillDefinition> {
    vec![
        definition(SPACESHIP_COMMAND, "Spaceship Command", 1, Vec::new()),
        definition(
            CALDARI_HAULER,
            "Caldari Hauler",
            4,
            vec![(SPACESHIP_COMMAND, 3)],
        ),
        definition(
            TRANSPORT_SHIPS,
            "Transport Ships",
            6,
            vec![(CALDARI_HAULER, 5)],
        ),
        definition(SHIELD_UPGRADES, "Shield Upgrades", 2, Vec::new()),
    ]
}

fn required(entries: &[(i64, i64, i16)]) -> BTreeMap<i64, Vec<RequiredSkill>> {
    let mut map: BTreeMap<i64, Vec<RequiredSkill>> = BTreeMap::new();
    for (type_id, skill_type_id, level) in entries {
        map.entry(*type_id).or_default().push(RequiredSkill {
            skill_type_id: *skill_type_id,
            level: *level,
        });
    }
    map
}

/// The ship needs Transport Ships I and Caldari Hauler V; the module Shield
/// Upgrades IV; its charge Spaceship Command IV; the cargo (ignored)
/// Transport Ships V.
fn requirements_by_type() -> BTreeMap<i64, Vec<RequiredSkill>> {
    required(&[
        (SHIP, TRANSPORT_SHIPS, 1),
        (SHIP, CALDARI_HAULER, 5),
        (MODULE, SHIELD_UPGRADES, 4),
        (CHARGE, SPACESHIP_COMMAND, 4),
        (CARGO, TRANSPORT_SHIPS, 5),
    ])
}

fn fit() -> FitInput {
    FitInput {
        ship_type_id: SHIP,
        items: vec![
            FitItem {
                type_id: MODULE,
                slot_kind: SlotKind::Med,
                slot_index: Some(0),
                quantity: 1,
                state: ModuleState::Active,
                charge_type_id: Some(CHARGE),
                mutation: None,
            },
            FitItem {
                type_id: CARGO,
                slot_kind: SlotKind::Cargo,
                slot_index: None,
                quantity: 3,
                state: ModuleState::Offline,
                charge_type_id: None,
                mutation: None,
            },
        ],
    }
}

fn skill(skill_id: i64, trained_level: i16, active_level: i16) -> CharacterSkill {
    CharacterSkill {
        skill_id,
        trained_level,
        active_level,
        // Exactly the SP of the trained level, so nothing is part-trained.
        skillpoints: crate::skills::sp_for_level(
            definitions()
                .iter()
                .find(|definition| definition.type_id == skill_id)
                .unwrap()
                .rank,
            trained_level,
        ),
    }
}

fn character(skills: Vec<CharacterSkill>) -> CharacterSkillsSnapshot {
    CharacterSkillsSnapshot {
        total_sp: skills.iter().map(|skill| skill.skillpoints).sum(),
        unallocated_sp: None,
        skills,
        queue: Vec::new(),
        attributes: Some(CharacterAttributes {
            charisma: 20,
            intelligence: 20,
            memory: 20,
            perception: 20,
            willpower: 20,
        }),
    }
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 26, 12, 0, 0).unwrap()
}

fn requirements() -> FitRequirements {
    fit_requirements(&fit(), &requirements_by_type())
}

#[test]
fn requirements_union_hull_items_and_charges_but_not_cargo() {
    let requirements = requirements();
    assert_eq!(
        requirements.hull,
        vec![
            PlanEntry {
                skill_type_id: CALDARI_HAULER,
                level: 5
            },
            PlanEntry {
                skill_type_id: TRANSPORT_SHIPS,
                level: 1
            },
        ]
    );
    assert_eq!(
        requirements.all,
        vec![
            PlanEntry {
                skill_type_id: SPACESHIP_COMMAND,
                level: 4
            },
            PlanEntry {
                skill_type_id: CALDARI_HAULER,
                level: 5
            },
            PlanEntry {
                skill_type_id: TRANSPORT_SHIPS,
                level: 1
            },
            PlanEntry {
                skill_type_id: SHIELD_UPGRADES,
                level: 4
            },
        ]
    );
    assert_eq!(
        requirements.by_item,
        BTreeMap::from([(
            0,
            vec![
                PlanEntry {
                    skill_type_id: SPACESHIP_COMMAND,
                    level: 4
                },
                PlanEntry {
                    skill_type_id: SHIELD_UPGRADES,
                    level: 4
                },
            ]
        )])
    );
}

#[test]
fn a_pilot_with_every_skill_can_use_all_of_it() {
    let pilot = character(vec![
        skill(SPACESHIP_COMMAND, 5, 5),
        skill(CALDARI_HAULER, 5, 5),
        skill(TRANSPORT_SHIPS, 1, 1),
        skill(SHIELD_UPGRADES, 4, 4),
    ]);
    let flyability = evaluate_flyability(&requirements(), &definitions(), &pilot, now());
    assert_eq!(flyability.status, FlyabilityStatus::CanUseAll);
    assert_eq!(flyability.missing, Vec::new());
    assert_eq!(flyability.remaining_seconds, Some(0));
    assert_eq!(flyability.blocked_items, Vec::<usize>::new());
    assert!(!flyability.omega_required);
}

#[test]
fn a_pilot_who_flies_the_hull_but_not_a_module_is_hull_only() {
    let pilot = character(vec![
        skill(SPACESHIP_COMMAND, 4, 4),
        skill(CALDARI_HAULER, 5, 5),
        skill(TRANSPORT_SHIPS, 1, 1),
        skill(SHIELD_UPGRADES, 2, 2),
    ]);
    let flyability = evaluate_flyability(&requirements(), &definitions(), &pilot, now());
    assert_eq!(flyability.status, FlyabilityStatus::CanFlyHull);
    assert_eq!(flyability.blocked_items, vec![0]);
    assert_eq!(flyability.missing.len(), 1);
    let missing = &flyability.missing[0];
    assert_eq!(
        (
            missing.skill_type_id,
            missing.active_level,
            missing.required_level
        ),
        (SHIELD_UPGRADES, 2, 4)
    );
    assert_eq!(missing.name, "Shield Upgrades");
    assert!(missing.seconds.unwrap() > 0);
    assert_eq!(flyability.remaining_seconds, missing.seconds);
}

#[test]
fn an_untrained_pilot_misses_the_whole_closure_at_the_highest_level_needed() {
    let flyability = evaluate_flyability(
        &requirements(),
        &definitions(),
        &character(Vec::new()),
        now(),
    );
    assert_eq!(flyability.status, FlyabilityStatus::Cannot);
    assert_eq!(flyability.blocked_items, vec![0]);
    let missing: Vec<(i64, i16, i16)> = flyability
        .missing
        .iter()
        .map(|missing| {
            (
                missing.skill_type_id,
                missing.active_level,
                missing.required_level,
            )
        })
        .collect();
    // Spaceship Command is needed at IV (charge) and III (Caldari Hauler):
    // listed once, at IV. Prerequisites first, as they train.
    assert_eq!(
        missing,
        [
            (SPACESHIP_COMMAND, 0, 4),
            (CALDARI_HAULER, 0, 5),
            (TRANSPORT_SHIPS, 0, 1),
            (SHIELD_UPGRADES, 0, 4),
        ]
    );
    assert_eq!(
        flyability.remaining_seconds,
        Some(
            flyability
                .missing
                .iter()
                .map(|missing| missing.seconds.unwrap())
                .sum()
        )
    );
    assert_eq!(
        flyability.missing_skillbooks,
        vec![
            SPACESHIP_COMMAND,
            CALDARI_HAULER,
            TRANSPORT_SHIPS,
            SHIELD_UPGRADES
        ]
    );
}

#[test]
fn a_skill_an_alpha_clone_caps_needs_omega_not_training() {
    // Shield Upgrades trained to IV but usable only at II on an Alpha.
    let pilot = character(vec![
        skill(SPACESHIP_COMMAND, 4, 4),
        skill(CALDARI_HAULER, 5, 5),
        skill(TRANSPORT_SHIPS, 1, 1),
        skill(SHIELD_UPGRADES, 4, 2),
    ]);
    let flyability = evaluate_flyability(&requirements(), &definitions(), &pilot, now());
    assert_eq!(flyability.status, FlyabilityStatus::CanFlyHull);
    assert!(flyability.omega_required);
    let missing = &flyability.missing[0];
    assert_eq!(missing.skill_type_id, SHIELD_UPGRADES);
    assert!(missing.omega_required);
    assert_eq!(missing.seconds, None);
    assert_eq!(flyability.remaining_seconds, Some(0));
}

#[test]
fn a_fit_skill_plan_is_the_closure_or_what_a_pilot_still_has_to_train() {
    let whole = requirement_plan(&requirements(), &definitions(), None).unwrap();
    assert_eq!(
        whole.first(),
        Some(&PlanEntry {
            skill_type_id: SPACESHIP_COMMAND,
            level: 1
        })
    );
    assert_eq!(whole.len(), 4 + 5 + 1 + 4);

    let pilot = character(vec![
        skill(SPACESHIP_COMMAND, 4, 4),
        skill(CALDARI_HAULER, 5, 5),
        skill(TRANSPORT_SHIPS, 1, 1),
        skill(SHIELD_UPGRADES, 2, 2),
    ]);
    assert_eq!(
        requirement_plan(&requirements(), &definitions(), Some(&pilot)).unwrap(),
        vec![
            PlanEntry {
                skill_type_id: SHIELD_UPGRADES,
                level: 3
            },
            PlanEntry {
                skill_type_id: SHIELD_UPGRADES,
                level: 4
            },
        ]
    );
}
