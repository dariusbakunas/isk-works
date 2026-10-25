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

// --- Evaluation -------------------------------------------------------------

mod evaluation {
    use chrono::{DateTime, Duration, TimeZone, Utc};

    use super::*;
    use crate::skills::{CharacterAttributes, CharacterSkill, CharacterSkillsSnapshot};
    use crate::training::SkillQueueEntry;

    const EW_DRONES: i64 = 23_566;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 25, 12, 0, 0).unwrap()
    }

    fn definitions() -> Vec<SkillDefinition> {
        let mut definitions = super::definitions();
        let mut ew = definition(EW_DRONES, "EW Drone Interfacing", 5, vec![(DRONES, 5)]);
        ew.omega_only = true;
        definitions.push(ew);
        definitions
    }

    fn skill(skill_id: i64, trained_level: i16, skillpoints: i64) -> CharacterSkill {
        CharacterSkill {
            skill_id,
            trained_level,
            active_level: trained_level,
            skillpoints,
        }
    }

    /// 27 + 27/2 = 40.5 SP/min on MEM/PER.
    fn character(skills: Vec<CharacterSkill>) -> CharacterSkillsSnapshot {
        CharacterSkillsSnapshot {
            total_sp: skills.iter().map(|skill| skill.skillpoints).sum(),
            unallocated_sp: None,
            skills,
            queue: Vec::new(),
            attributes: Some(CharacterAttributes {
                charisma: 19,
                intelligence: 20,
                memory: 27,
                perception: 27,
                willpower: 17,
            }),
        }
    }

    fn plan() -> Vec<PlanEntry> {
        vec![
            entry(DRONES, 1),
            entry(DRONES, 2),
            entry(DRONES, 3),
            entry(SCOUT_DRONES, 1),
        ]
    }

    #[test]
    fn entries_are_done_partial_or_remaining() {
        // Drones II trained, 3,000 SP into III (needs 8,000); Scout not injected.
        let evaluation = evaluate_plan(
            &plan(),
            &definitions(),
            &character(vec![skill(DRONES, 2, 3_000)]),
            now(),
        );

        let statuses = evaluation
            .entries
            .iter()
            .map(|entry| entry.status)
            .collect::<Vec<_>>();
        assert_eq!(
            statuses,
            vec![
                PlanEntryStatus::Done,
                PlanEntryStatus::Done,
                PlanEntryStatus::Partial,
                PlanEntryStatus::Remaining,
            ]
        );
        let drones_three = &evaluation.entries[2];
        assert_eq!(drones_three.sp_total, 8_000 - 1_415);
        assert_eq!(drones_three.sp_remaining, 5_000);
        // 5,000 SP at 40.5 SP/min, rounded up.
        assert_eq!(drones_three.seconds, Some(7_408));
        assert_eq!(evaluation.entries[3].sp_remaining, 250);
        assert_eq!(evaluation.entries[3].seconds, Some(371));
        assert_eq!(evaluation.total_sp, 8_000 + 250);
        assert_eq!(evaluation.remaining_sp, 5_250);
        assert_eq!(evaluation.remaining_seconds, Some(7_408 + 371));
        assert!((evaluation.percent_complete - 3_000.0 / 8_250.0 * 100.0).abs() < 1e-9);
        assert_eq!(evaluation.missing_skillbooks, vec![SCOUT_DRONES]);
        assert!(!evaluation.blocked);
        assert!(!evaluation.estimated);
    }

    #[test]
    fn a_finished_plan_is_complete() {
        let evaluation = evaluate_plan(
            &plan(),
            &definitions(),
            &character(vec![skill(DRONES, 5, 256_000), skill(SCOUT_DRONES, 1, 250)]),
            now(),
        );

        assert_eq!(evaluation.remaining_sp, 0);
        assert_eq!(evaluation.remaining_seconds, Some(0));
        assert!((evaluation.percent_complete - 100.0).abs() < f64::EPSILON);
        assert!(evaluation.missing_skillbooks.is_empty());
    }

    #[test]
    fn the_entry_in_training_uses_the_queue_finish() {
        let mut character = character(vec![skill(DRONES, 2, 3_000)]);
        character.queue = vec![SkillQueueEntry {
            skill_id: DRONES,
            finished_level: 3,
            queue_position: 0,
            start_date: Some(now() - Duration::minutes(10)),
            finish_date: Some(now() + Duration::minutes(100)),
            training_start_sp: Some(2_595),
            level_end_sp: Some(8_000),
        }];

        let evaluation = evaluate_plan(&plan(), &definitions(), &character, now());

        assert_eq!(evaluation.entries[2].status, PlanEntryStatus::Partial);
        assert_eq!(evaluation.entries[2].seconds, Some(6_000));
    }

    #[test]
    fn an_alpha_is_blocked_on_omega_only_entries() {
        let mut capped = skill(LEADERSHIP, 5, 256_000);
        capped.active_level = 4;
        let plan = [entry(DRONES, 5), entry(EW_DRONES, 1)];

        let evaluation = evaluate_plan(
            &plan,
            &definitions(),
            &character(vec![skill(DRONES, 5, 256_000), capped]),
            now(),
        );

        assert_eq!(evaluation.entries[1].status, PlanEntryStatus::Blocked);
        assert_eq!(evaluation.entries[1].seconds, None);
        assert!(evaluation.blocked);
        // Blocked entries don't count towards the remaining time.
        assert_eq!(evaluation.remaining_seconds, Some(0));
    }

    #[test]
    fn without_a_rate_the_remaining_time_is_unknown() {
        let mut character = character(vec![]);
        character.attributes = None;

        let evaluation = evaluate_plan(&plan(), &definitions(), &character, now());

        assert_eq!(evaluation.remaining_seconds, None);
        assert_eq!(evaluation.entries[0].seconds, None);
    }
}
