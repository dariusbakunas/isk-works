use chrono::{Duration, TimeZone};
use iskworks_sde::RequiredSkill;
use serde_json::json;

use super::*;

const DRONES: i64 = 3_436;
const DRONE_INTERFACING: i64 = 3_442;
const EW_DRONES: i64 = 23_566;
const SCOUT_DRONES: i64 = 3_437;
const LEADERSHIP: i64 = 3_348;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 25, 12, 0, 0).unwrap()
}

fn definition(
    type_id: i64,
    name: &str,
    group: (i64, &str),
    rank: i32,
    attributes: (SkillAttribute, SkillAttribute),
    omega_only: bool,
    prerequisites: Vec<RequiredSkill>,
) -> SkillDefinition {
    SkillDefinition {
        type_id,
        name: name.to_string(),
        group_id: group.0,
        group_name: group.1.to_string(),
        rank,
        primary_attribute: attributes.0,
        secondary_attribute: attributes.1,
        omega_only,
        prerequisites,
    }
}

fn needs(skill_type_id: i64, level: i16) -> RequiredSkill {
    RequiredSkill {
        skill_type_id,
        level,
    }
}

const DRONES_GROUP: (i64, &str) = (273, "Drones");
const LEADERSHIP_GROUP: (i64, &str) = (258, "Fleet Support");
const MEM_PER: (SkillAttribute, SkillAttribute) =
    (SkillAttribute::Memory, SkillAttribute::Perception);
const CHA_WIL: (SkillAttribute, SkillAttribute) =
    (SkillAttribute::Charisma, SkillAttribute::Willpower);

fn definitions() -> Vec<SkillDefinition> {
    vec![
        definition(
            DRONE_INTERFACING,
            "Drone Interfacing",
            DRONES_GROUP,
            5,
            MEM_PER,
            false,
            vec![needs(DRONES, 5)],
        ),
        definition(DRONES, "Drones", DRONES_GROUP, 1, MEM_PER, false, vec![]),
        definition(
            EW_DRONES,
            "EW Drone Interfacing",
            DRONES_GROUP,
            5,
            MEM_PER,
            true,
            vec![needs(DRONES, 5)],
        ),
        definition(
            SCOUT_DRONES,
            "Scout Drone Operation",
            DRONES_GROUP,
            1,
            MEM_PER,
            false,
            vec![needs(DRONES, 1)],
        ),
        definition(
            LEADERSHIP,
            "Leadership",
            LEADERSHIP_GROUP,
            1,
            CHA_WIL,
            false,
            vec![],
        ),
    ]
}

fn attributes() -> CharacterAttributes {
    CharacterAttributes {
        charisma: 19,
        intelligence: 20,
        memory: 27,
        perception: 27,
        willpower: 17,
    }
}

fn skill(skill_id: i64, trained_level: i16, skillpoints: i64) -> CharacterSkill {
    CharacterSkill {
        skill_id,
        trained_level,
        active_level: trained_level,
        skillpoints,
    }
}

fn snapshot(skills: Vec<CharacterSkill>) -> CharacterSkillsSnapshot {
    CharacterSkillsSnapshot {
        total_sp: skills.iter().map(|skill| skill.skillpoints).sum(),
        unallocated_sp: None,
        skills,
        queue: Vec::new(),
        attributes: Some(attributes()),
    }
}

fn queue_entry(
    skill_id: i64,
    finished_level: i64,
    position: i64,
    window: Option<(DateTime<Utc>, DateTime<Utc>)>,
    sp: Option<(i64, i64)>,
) -> SkillQueueEntry {
    SkillQueueEntry {
        skill_id,
        finished_level,
        queue_position: position,
        start_date: window.map(|(start, _)| start),
        finish_date: window.map(|(_, finish)| finish),
        training_start_sp: sp.map(|(start, _)| start),
        level_end_sp: sp.map(|(_, end)| end),
    }
}

// --- Skill points -----------------------------------------------------------

#[test]
fn skill_points_per_level_match_the_client() {
    assert_eq!(
        (0..=5)
            .map(|level| sp_for_level(1, level))
            .collect::<Vec<_>>(),
        vec![0, 250, 1_415, 8_000, 45_255, 256_000]
    );
    // Rank 5 level II rounds up (7,071.07 -> 7,072), as real queue data shows.
    assert_eq!(
        (1..=5)
            .map(|level| sp_for_level(5, level))
            .collect::<Vec<_>>(),
        vec![1_250, 7_072, 40_000, 226_275, 1_280_000]
    );
    assert_eq!(sp_for_level(2, 4), 90_510);
}

// --- Training rate ----------------------------------------------------------

#[test]
fn omega_trains_at_primary_plus_half_secondary_and_alpha_at_half_that() {
    let attributes = attributes();
    let omega = sp_per_minute(
        &attributes,
        SkillAttribute::Memory,
        SkillAttribute::Perception,
        CloneState::Omega,
    );
    let alpha = sp_per_minute(
        &attributes,
        SkillAttribute::Memory,
        SkillAttribute::Perception,
        CloneState::Alpha,
    );
    assert!((omega - 40.5).abs() < f64::EPSILON);
    assert!((alpha - 20.25).abs() < f64::EPSILON);
}

#[test]
fn a_capped_active_level_means_an_alpha_clone() {
    let mut capped = skill(DRONES, 5, 256_000);
    capped.active_level = 4;
    assert_eq!(
        detect_clone_state(&snapshot(vec![skill(LEADERSHIP, 5, 256_000)])),
        CloneState::Omega
    );
    assert_eq!(
        detect_clone_state(&snapshot(vec![capped])),
        CloneState::Alpha
    );
}

#[test]
fn rate_comes_from_attributes_when_they_are_synced() {
    let rate = training_rate(&snapshot(vec![]), &definitions(), now());
    assert_eq!(rate.basis, RateBasis::Attributes);
    assert!(!rate.accelerated);
}

#[test]
fn a_queue_training_faster_than_the_attributes_explain_is_accelerated() {
    // Drone Interfacing V at 40.5 SP/min takes ~24,794 min; 12,000 min is
    // twice as fast.
    let mut character = snapshot(vec![skill(DRONE_INTERFACING, 4, 226_275)]);
    character.queue = vec![queue_entry(
        DRONE_INTERFACING,
        5,
        0,
        Some((
            now() - Duration::minutes(1),
            now() + Duration::minutes(11_999),
        )),
        Some((226_275, 1_280_000)),
    )];

    let rate = training_rate(&character, &definitions(), now());

    assert_eq!(rate.basis, RateBasis::Attributes);
    assert!(rate.accelerated);
}

#[test]
fn without_attributes_the_active_queue_entry_gives_an_observed_rate() {
    let mut character = snapshot(vec![skill(DRONE_INTERFACING, 4, 226_275)]);
    character.attributes = None;
    character.queue = vec![queue_entry(
        DRONE_INTERFACING,
        5,
        0,
        Some((
            now() - Duration::minutes(1),
            now() + Duration::minutes(26_017),
        )),
        Some((226_275, 1_280_000)),
    )];

    let rate = training_rate(&character, &definitions(), now());

    match rate.basis {
        RateBasis::ObservedQueue { sp_per_minute } => assert!((sp_per_minute - 40.5).abs() < 0.01),
        other => panic!("expected an observed rate, got {other:?}"),
    }
}

#[test]
fn without_attributes_or_an_active_queue_the_rate_is_unknown() {
    let mut character = snapshot(vec![]);
    character.attributes = None;
    assert_eq!(
        training_rate(&character, &definitions(), now()).basis,
        RateBasis::Unknown
    );
}

// --- Summary parsing --------------------------------------------------------

#[test]
fn parses_the_skills_source_summary() {
    let summary = json!({
        "total_sp": 112_822_873,
        "unallocated_sp": 25_000,
        "skills": [
            { "skill_id": DRONES, "active_skill_level": 5, "trained_skill_level": 5,
              "skillpoints_in_skill": 256_000 }
        ],
        "attributes": { "charisma": 19, "intelligence": 20, "memory": 27,
                        "perception": 27, "willpower": 17, "bonus_remaps": 1 },
        "skillQueue": [
            { "skill_id": DRONE_INTERFACING, "finished_level": 5, "queue_position": 0,
              "start_date": "2026-10-25T11:59:00Z", "finish_date": "2026-11-12T00:00:00Z",
              "training_start_sp": 226_275, "level_start_sp": 226_275,
              "level_end_sp": 1_280_000 }
        ]
    });

    let snapshot = parse_skills_summary(&summary).unwrap();

    assert_eq!(snapshot.total_sp, 112_822_873);
    assert_eq!(snapshot.unallocated_sp, Some(25_000));
    assert_eq!(snapshot.skills, vec![skill(DRONES, 5, 256_000)]);
    assert_eq!(snapshot.attributes, Some(attributes()));
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(snapshot.queue[0].level_end_sp, Some(1_280_000));
}

#[test]
fn a_summary_without_attributes_or_queue_still_parses() {
    let summary = json!({ "total_sp": 1_000, "skills": [] });
    let snapshot = parse_skills_summary(&summary).unwrap();
    assert_eq!(snapshot.attributes, None);
    assert!(snapshot.queue.is_empty());
    assert_eq!(snapshot.unallocated_sp, None);
}

#[test]
fn a_summary_without_skills_is_unreadable() {
    assert!(parse_skills_summary(&json!({ "total_sp": 1 })).is_none());
}

// --- Catalogue overlay ------------------------------------------------------

fn progress(catalogue: &SkillCatalogue, type_id: i64) -> &SkillProgress {
    catalogue
        .skills
        .iter()
        .find(|skill| skill.definition.type_id == type_id)
        .and_then(|skill| skill.progress.as_ref())
        .expect("character progress")
}

/// Drones V, Drone Interfacing IV training to V at its attribute rate
/// (1,053,725 SP at 40.5 SP/min = 26,018 min, 20,000 left), and
/// Leadership II with 500 SP into level III.
fn trained_character() -> CharacterSkillsSnapshot {
    let mut character = snapshot(vec![
        skill(DRONES, 5, 256_000),
        skill(DRONE_INTERFACING, 4, 300_000),
        skill(LEADERSHIP, 2, 1_915),
    ]);
    character.queue = vec![queue_entry(
        DRONE_INTERFACING,
        5,
        0,
        Some((
            now() - Duration::minutes(6_018),
            now() + Duration::minutes(20_000),
        )),
        Some((226_275, 1_280_000)),
    )];
    character
}

#[test]
fn catalogue_states_and_times_for_a_character() {
    let catalogue = build_catalogue(&definitions(), Some(&trained_character()), now());

    let drones = progress(&catalogue, DRONES);
    assert_eq!(drones.state, SkillState::Max);
    assert_eq!(drones.seconds_to_next_level, None);

    // Training: the time is the queue's own finish, so it agrees with the
    // queue rail.
    let interfacing = progress(&catalogue, DRONE_INTERFACING);
    assert_eq!(interfacing.state, SkillState::Training);
    assert_eq!(
        interfacing.seconds_to_next_level,
        Some(Duration::minutes(20_000).num_seconds())
    );
    assert_eq!(interfacing.queued_to_level, Some(5));

    // Injected below V: remaining SP of the next level over CHA/WIL
    // (19 + 17/2 = 27.5 SP/min): (8,000 - 1,915) / 27.5 min, rounded up.
    let leadership = progress(&catalogue, LEADERSHIP);
    assert_eq!(leadership.state, SkillState::CanTrain);
    assert_eq!(leadership.seconds_to_next_level, Some(13_277));
    assert!(!leadership.estimated);

    // Not injected, prerequisites met: no time.
    let scout = progress(&catalogue, SCOUT_DRONES);
    assert_eq!(scout.state, SkillState::CanInject);
    assert_eq!(scout.trained_level, 0);
    assert_eq!(scout.seconds_to_next_level, None);
    // Omega-only is an ordinary skill for an Omega.
    assert_eq!(progress(&catalogue, EW_DRONES).state, SkillState::CanInject);
}

#[test]
fn a_queued_skill_reports_its_target_level() {
    let mut character = trained_character();
    character.queue.push(queue_entry(
        LEADERSHIP,
        4,
        1,
        Some((
            now() + Duration::minutes(20_000),
            now() + Duration::minutes(30_000),
        )),
        None,
    ));

    let catalogue = build_catalogue(&definitions(), Some(&character), now());

    let leadership = progress(&catalogue, LEADERSHIP);
    assert_eq!(leadership.state, SkillState::Queued);
    assert_eq!(leadership.queued_to_level, Some(4));
    assert_eq!(leadership.seconds_to_next_level, Some(13_277));
}

#[test]
fn unmet_prerequisites_lock_a_skill() {
    let catalogue = build_catalogue(&definitions(), Some(&snapshot(vec![])), now());

    assert_eq!(progress(&catalogue, DRONES).state, SkillState::CanInject);
    assert_eq!(
        progress(&catalogue, DRONE_INTERFACING).state,
        SkillState::Locked
    );
    assert_eq!(progress(&catalogue, SCOUT_DRONES).state, SkillState::Locked);
}

#[test]
fn an_alpha_cannot_train_omega_only_skills() {
    let mut capped = skill(LEADERSHIP, 5, 256_000);
    capped.active_level = 4;
    let character = snapshot(vec![skill(DRONES, 5, 256_000), capped]);

    let catalogue = build_catalogue(&definitions(), Some(&character), now());

    assert_eq!(
        catalogue.rate.map(|rate| rate.clone),
        Some(CloneState::Alpha)
    );
    let ew = progress(&catalogue, EW_DRONES);
    assert_eq!(ew.state, SkillState::OmegaOnly);
    assert_eq!(ew.seconds_to_next_level, None);
    // Trained to V and only capped by the clone: nothing left to train.
    let leadership = progress(&catalogue, LEADERSHIP);
    assert_eq!(leadership.state, SkillState::Max);
    assert_eq!(leadership.active_level, 4);
}

#[test]
fn estimated_times_without_attributes() {
    let mut character = trained_character();
    character.attributes = None;

    let catalogue = build_catalogue(&definitions(), Some(&character), now());

    let leadership = progress(&catalogue, LEADERSHIP);
    assert!(leadership.estimated);
    assert!(leadership.seconds_to_next_level.is_some());
}

#[test]
fn groups_count_skills_and_skill_points() {
    let catalogue = build_catalogue(&definitions(), Some(&trained_character()), now());

    assert_eq!(
        catalogue.groups,
        vec![
            CatalogueGroup {
                group_id: 273,
                name: "Drones".to_string(),
                skill_count: 4,
                max_sp: 1_280_000 + 256_000 + 1_280_000 + 256_000,
                injected_count: Some(2),
                trained_sp: Some(256_000 + 300_000),
            },
            CatalogueGroup {
                group_id: 258,
                name: "Fleet Support".to_string(),
                skill_count: 1,
                max_sp: 256_000,
                injected_count: Some(1),
                trained_sp: Some(1_915),
            },
        ]
    );
}

#[test]
fn without_a_character_the_catalogue_is_definitions_only() {
    let catalogue = build_catalogue(&definitions(), None, now());

    assert!(catalogue.rate.is_none());
    assert!(catalogue
        .skills
        .iter()
        .all(|skill| skill.progress.is_none()));
    assert_eq!(catalogue.groups[0].injected_count, None);
    let drones = catalogue
        .skills
        .iter()
        .find(|skill| skill.definition.type_id == DRONES)
        .unwrap();
    assert_eq!(drones.sp_per_level, [250, 1_415, 8_000, 45_255, 256_000]);
}
