use iskworks_sde::{RequiredSkill, SkillAttribute, SkillDefinition};

use super::*;

const DRONES: i64 = 3_436;
const DRONE_INTERFACING: i64 = 3_442;
const SCOUT_DRONES: i64 = 3_437;
const LEADERSHIP: i64 = 3_348;

fn definition(
    type_id: i64,
    name: &str,
    rank: i32,
    prerequisites: Vec<(i64, i16)>,
) -> SkillDefinition {
    SkillDefinition {
        type_id,
        name: name.to_string(),
        group_id: 273,
        group_name: "Drones".to_string(),
        rank,
        primary_attribute: SkillAttribute::Memory,
        secondary_attribute: SkillAttribute::Perception,
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
        definition(DRONES, "Drones", 1, vec![]),
        definition(SCOUT_DRONES, "Scout Drone Operation", 1, vec![(DRONES, 1)]),
        definition(
            DRONE_INTERFACING,
            "Drone Interfacing",
            5,
            vec![(DRONES, 5), (SCOUT_DRONES, 2)],
        ),
        definition(LEADERSHIP, "Leadership", 1, vec![]),
    ]
}

fn entry(skill_type_id: i64, level: i16) -> PlanEntry {
    PlanEntry {
        skill_type_id,
        level,
    }
}

#[test]
fn closure_lists_prerequisites_deepest_first_then_lower_levels() {
    let definitions = definitions();
    let graph = SkillGraph::new(&definitions);

    let closure = prerequisite_closure(&graph, entry(DRONE_INTERFACING, 2)).unwrap();

    assert_eq!(
        closure,
        vec![
            entry(DRONES, 1),
            entry(DRONES, 2),
            entry(DRONES, 3),
            entry(DRONES, 4),
            entry(DRONES, 5),
            entry(SCOUT_DRONES, 1),
            entry(SCOUT_DRONES, 2),
            entry(DRONE_INTERFACING, 1),
        ]
    );
}

#[test]
fn normalizing_inserts_missing_prerequisites_before_the_entry() {
    let definitions = definitions();
    let graph = SkillGraph::new(&definitions);

    let normalized =
        normalize_plan(&[entry(LEADERSHIP, 1), entry(SCOUT_DRONES, 2)], &graph).unwrap();

    assert_eq!(
        normalized.entries,
        vec![
            entry(LEADERSHIP, 1),
            entry(DRONES, 1),
            entry(SCOUT_DRONES, 1),
            entry(SCOUT_DRONES, 2),
        ]
    );
    assert_eq!(
        normalized.inserted,
        vec![entry(DRONES, 1), entry(SCOUT_DRONES, 1)]
    );
}

#[test]
fn a_prerequisite_already_earlier_in_the_plan_is_not_repeated() {
    let definitions = definitions();
    let graph = SkillGraph::new(&definitions);
    let plan = [
        entry(DRONES, 1),
        entry(DRONES, 2),
        entry(SCOUT_DRONES, 1),
        entry(SCOUT_DRONES, 1),
    ];

    let normalized = normalize_plan(&plan, &graph).unwrap();

    assert_eq!(
        normalized.entries,
        vec![entry(DRONES, 1), entry(DRONES, 2), entry(SCOUT_DRONES, 1)]
    );
    assert!(normalized.inserted.is_empty());
}

#[test]
fn a_prerequisite_placed_later_is_an_order_violation() {
    let definitions = definitions();
    let graph = SkillGraph::new(&definitions);

    let error = normalize_plan(&[entry(SCOUT_DRONES, 1), entry(DRONES, 1)], &graph).unwrap_err();

    assert_eq!(
        error,
        SkillPlanError::OrderViolation {
            entry: entry(SCOUT_DRONES, 1),
            requires: entry(DRONES, 1),
        }
    );
    // A lower level of the same skill counts too.
    let error = normalize_plan(&[entry(DRONES, 2), entry(DRONES, 1)], &graph).unwrap_err();
    assert_eq!(
        error,
        SkillPlanError::OrderViolation {
            entry: entry(DRONES, 2),
            requires: entry(DRONES, 1),
        }
    );
}

#[test]
fn unknown_skills_and_bad_levels_are_rejected() {
    let definitions = definitions();
    let graph = SkillGraph::new(&definitions);

    assert_eq!(
        normalize_plan(&[entry(99, 1)], &graph).unwrap_err(),
        SkillPlanError::UnknownSkill(99)
    );
    for level in [0, 6] {
        assert_eq!(
            normalize_plan(&[entry(DRONES, level)], &graph).unwrap_err(),
            SkillPlanError::InvalidLevel(entry(DRONES, level))
        );
    }
}
