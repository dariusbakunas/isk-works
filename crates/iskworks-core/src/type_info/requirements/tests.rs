use chrono::TimeZone;
use iskworks_sde::SkillAttribute;

use super::*;
use crate::skills::{CharacterAttributes, CharacterSkill};

const SPACESHIP_COMMAND: i64 = 3327;
const CALDARI_FRIGATE: i64 = 3330;
const MECHANICS: i64 = 3392;
const POWER_GRID_MANAGEMENT: i64 = 3413;
const ASSAULT_FRIGATES: i64 = 12095;

fn definition(type_id: i64, name: &str, prerequisites: &[(i64, i16)]) -> SkillDefinition {
    SkillDefinition {
        type_id,
        name: name.to_string(),
        group_id: 257,
        group_name: "Spaceship Command".to_string(),
        rank: 1,
        primary_attribute: SkillAttribute::Perception,
        secondary_attribute: SkillAttribute::Willpower,
        omega_only: false,
        prerequisites: prerequisites
            .iter()
            .map(|&(skill_type_id, level)| RequiredSkill {
                skill_type_id,
                level,
            })
            .collect(),
    }
}

fn definitions() -> Vec<SkillDefinition> {
    vec![
        definition(SPACESHIP_COMMAND, "Spaceship Command", &[]),
        definition(
            CALDARI_FRIGATE,
            "Caldari Frigate",
            &[(SPACESHIP_COMMAND, 1)],
        ),
        definition(MECHANICS, "Mechanics", &[]),
        definition(POWER_GRID_MANAGEMENT, "Power Grid Management", &[]),
        definition(
            ASSAULT_FRIGATES,
            "Assault Frigates",
            &[(MECHANICS, 5), (SPACESHIP_COMMAND, 3)],
        ),
    ]
}

fn needs(skill_type_id: i64, level: i16) -> RequiredSkill {
    RequiredSkill {
        skill_type_id,
        level,
    }
}

/// The Harpy's Requirements tab, as in the Figma frame.
fn harpy() -> Vec<RequiredSkill> {
    vec![
        needs(CALDARI_FRIGATE, 5),
        needs(ASSAULT_FRIGATES, 1),
        needs(POWER_GRID_MANAGEMENT, 5),
    ]
}

fn skill(skill_id: i64, level: i16) -> CharacterSkill {
    CharacterSkill {
        skill_id,
        trained_level: level,
        active_level: level,
        skillpoints: 0,
    }
}

fn character(skills: Vec<CharacterSkill>) -> CharacterSkillsSnapshot {
    CharacterSkillsSnapshot {
        total_sp: 0,
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

/// `(depth, skill, level, trained)` rows, depth-first.
fn rows(nodes: &[SkillNode], depth: usize, out: &mut Vec<(usize, i64, i16, i16)>) {
    for node in nodes {
        out.push((depth, node.skill_type_id, node.level, node.trained_level));
        rows(&node.children, depth + 1, out);
    }
}

fn flat(nodes: &[SkillNode]) -> Vec<(usize, i64, i16, i16)> {
    let mut out = Vec::new();
    rows(nodes, 0, &mut out);
    out
}

#[test]
fn prerequisites_nest_under_each_requirement() {
    let tree = requirement_tree(&harpy(), &definitions(), RequirementPilot::AllV);

    assert_eq!(
        flat(&tree),
        [
            (0, CALDARI_FRIGATE, 5, 5),
            (1, SPACESHIP_COMMAND, 1, 5),
            (0, ASSAULT_FRIGATES, 1, 5),
            (1, MECHANICS, 5, 5),
            (1, SPACESHIP_COMMAND, 3, 5),
            (0, POWER_GRID_MANAGEMENT, 5, 5),
        ]
    );
    assert_eq!(tree[0].name.as_deref(), Some("Caldari Frigate"));
    assert!(tree.iter().all(SkillNode::met));
}

#[test]
fn a_character_is_checked_at_its_active_levels() {
    let mut alpha_capped = skill(MECHANICS, 5);
    alpha_capped.active_level = 4;
    let snapshot = character(vec![
        skill(SPACESHIP_COMMAND, 3),
        skill(CALDARI_FRIGATE, 4),
        alpha_capped,
        skill(POWER_GRID_MANAGEMENT, 5),
    ]);

    let tree = requirement_tree(
        &harpy(),
        &definitions(),
        RequirementPilot::Character(&snapshot),
    );

    assert_eq!(
        flat(&tree),
        [
            (0, CALDARI_FRIGATE, 5, 4),
            (1, SPACESHIP_COMMAND, 1, 3),
            (0, ASSAULT_FRIGATES, 1, 0),
            (1, MECHANICS, 5, 4),
            (1, SPACESHIP_COMMAND, 3, 3),
            (0, POWER_GRID_MANAGEMENT, 5, 5),
        ]
    );
    assert!(!tree[0].met());
    assert!(tree[0].children[0].met());
}

#[test]
fn all_zero_trains_nothing() {
    let tree = requirement_tree(&harpy(), &definitions(), RequirementPilot::AllZero);
    assert!(flat(&tree).iter().all(|row| row.3 == 0));
}

#[test]
fn a_skill_the_import_lacks_is_a_nameless_leaf() {
    let tree = requirement_tree(&[needs(77_777, 2)], &definitions(), RequirementPilot::AllV);
    assert_eq!(
        tree,
        [SkillNode {
            skill_type_id: 77_777,
            name: None,
            level: 2,
            trained_level: 5,
            children: Vec::new(),
        }]
    );
}

#[test]
fn a_prerequisite_cycle_stops_instead_of_recursing_forever() {
    let cyclic = vec![
        definition(1, "One", &[(2, 1)]),
        definition(2, "Two", &[(1, 1)]),
    ];
    let tree = requirement_tree(&[needs(1, 1)], &cyclic, RequirementPilot::AllV);
    assert_eq!(flat(&tree), [(0, 1, 1, 5), (1, 2, 1, 5)]);
}

#[test]
fn the_summary_counts_distinct_missing_skills_and_their_training_time() {
    let snapshot = character(vec![
        skill(SPACESHIP_COMMAND, 3),
        skill(CALDARI_FRIGATE, 4),
        skill(MECHANICS, 5),
        skill(POWER_GRID_MANAGEMENT, 5),
    ]);
    let now = Utc.with_ymd_and_hms(2026, 10, 28, 12, 0, 0).unwrap();

    let summary = requirement_summary(&harpy(), &definitions(), &snapshot, now);

    // Caldari Frigate V and Assault Frigates I.
    assert_eq!(summary.missing_skills, 2);
    assert!(summary.remaining_seconds.is_some_and(|seconds| seconds > 0));

    let trained = character(vec![
        skill(SPACESHIP_COMMAND, 3),
        skill(CALDARI_FRIGATE, 5),
        skill(ASSAULT_FRIGATES, 1),
        skill(MECHANICS, 5),
        skill(POWER_GRID_MANAGEMENT, 5),
    ]);
    assert_eq!(
        requirement_summary(&harpy(), &definitions(), &trained, now),
        RequirementSummary {
            missing_skills: 0,
            remaining_seconds: Some(0),
        }
    );
}
