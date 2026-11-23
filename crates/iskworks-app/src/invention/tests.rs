use iskworks_sde::{InventionSkillRequirement, ManufacturingRecipe, RecipeLine};
use serde_json::json;

use super::*;

const ENCRYPTION: i64 = 21_790;
const MECHANICAL: i64 = 11_452;
const GALLENTE_STARSHIP: i64 = 11_450;
const CAPITAL_CONSTRUCTION: i64 = 22_242;
const DATACORE_MECHANICAL: i64 = 20_424;
const DATACORE_STARSHIP: i64 = 20_410;

fn requirement(skill_type_id: i64, name: &str, level: i16) -> InventionSkillRequirement {
    InventionSkillRequirement {
        skill_type_id,
        skill_name: name.to_string(),
        level,
    }
}

fn recipe(skills: Vec<InventionSkillRequirement>) -> InventionRecipe {
    InventionRecipe {
        source_blueprint_type_id: 940,
        source_blueprint_name: "Tristan Blueprint".to_string(),
        duration_seconds: Some(63_900),
        copying_duration_seconds: Some(4_800),
        materials: vec![
            RecipeLine {
                type_id: DATACORE_STARSHIP,
                type_name: "Datacore - Gallentean Starship Engineering".to_string(),
                quantity: 2,
            },
            RecipeLine {
                type_id: DATACORE_MECHANICAL,
                type_name: "Datacore - Mechanical Engineering".to_string(),
                quantity: 2,
            },
        ],
        products: Vec::new(),
        skills,
    }
}

fn frigate_skills() -> Vec<InventionSkillRequirement> {
    vec![
        requirement(ENCRYPTION, "Gallente Encryption Methods", 1),
        requirement(GALLENTE_STARSHIP, "Gallentean Starship Engineering", 1),
        requirement(MECHANICAL, "Mechanical Engineering", 1),
    ]
}

fn snapshot(levels: &[(i64, i64)]) -> CharacterSkillsSnapshot {
    let skills = levels
        .iter()
        .map(|(skill_id, level)| {
            json!({
                "skill_id": skill_id,
                "trained_skill_level": level,
                "active_skill_level": level,
                "skillpoints_in_skill": 1,
            })
        })
        .collect::<Vec<_>>();
    parse_skills_summary(&json!({ "total_sp": 1, "skills": skills })).unwrap()
}

fn resolve(
    recipe: &InventionRecipe,
    snapshot: Option<&CharacterSkillsSnapshot>,
    overrides: &BTreeMap<i64, u8>,
) -> ResolvedSkills {
    resolve_skills(recipe, snapshot, overrides)
}

#[test]
fn a_characters_levels_feed_the_chance() {
    let character = snapshot(&[(ENCRYPTION, 5), (GALLENTE_STARSHIP, 4), (MECHANICAL, 3)]);

    let resolved = resolve(
        &recipe(frigate_skills()),
        Some(&character),
        &BTreeMap::new(),
    );

    assert_eq!(
        resolved.levels,
        InventionSkills {
            encryption: 5,
            science: vec![4, 3],
        }
    );
    assert_eq!(
        resolved
            .views
            .iter()
            .map(|view| (view.skill_type_id, view.role, view.level))
            .collect::<Vec<_>>(),
        vec![
            (ENCRYPTION, InventionSkillRole::Encryption, 5),
            (GALLENTE_STARSHIP, InventionSkillRole::Science, 4),
            (MECHANICAL, InventionSkillRole::Science, 3),
        ]
    );
    assert!(resolved.missing.is_empty());
}

#[test]
fn no_character_means_every_level_is_four() {
    let resolved = resolve(&recipe(frigate_skills()), None, &BTreeMap::new());

    assert_eq!(
        resolved.levels,
        InventionSkills {
            encryption: 4,
            science: vec![4, 4],
        }
    );
    assert!(resolved
        .views
        .iter()
        .all(|view| view.character_level.is_none() && !view.overridden));
}

#[test]
fn an_override_replaces_the_characters_level_and_is_marked() {
    let character = snapshot(&[(ENCRYPTION, 2), (GALLENTE_STARSHIP, 2), (MECHANICAL, 2)]);

    let resolved = resolve(
        &recipe(frigate_skills()),
        Some(&character),
        &BTreeMap::from([(MECHANICAL, 5)]),
    );

    assert_eq!(resolved.levels.science, vec![2, 5]);
    let mechanical = &resolved.views[2];
    assert_eq!(mechanical.character_level, Some(2));
    assert!(mechanical.overridden);
}

#[test]
fn an_untrained_skill_counts_as_zero_and_blocks_the_job() {
    let character = snapshot(&[(GALLENTE_STARSHIP, 4), (MECHANICAL, 4)]);

    let resolved = resolve(
        &recipe(frigate_skills()),
        Some(&character),
        &BTreeMap::new(),
    );

    assert_eq!(resolved.levels.encryption, 0);
    assert_eq!(resolved.missing, vec!["Gallente Encryption Methods"]);
}

/// Capital Ship Construction on a capital blueprint counts towards the
/// chance like a science skill (see the core tests for the evidence).
#[test]
fn a_fourth_skill_counts_as_a_science_skill() {
    let mut skills = frigate_skills();
    skills.insert(
        1,
        requirement(CAPITAL_CONSTRUCTION, "Capital Ship Construction", 4),
    );
    let character = snapshot(&[
        (ENCRYPTION, 5),
        (GALLENTE_STARSHIP, 5),
        (MECHANICAL, 5),
        (CAPITAL_CONSTRUCTION, 3),
    ]);

    let resolved = resolve(&recipe(skills), Some(&character), &BTreeMap::new());

    assert_eq!(
        resolved.levels,
        InventionSkills {
            encryption: 5,
            science: vec![3, 5, 5],
        }
    );
    assert_eq!(resolved.views[1].role, InventionSkillRole::Science);
    assert_eq!(resolved.views[1].required_level, 4);
    assert_eq!(resolved.missing, vec!["Capital Ship Construction"]);
}

/// The encryption skill is shown first wherever the job lists it.
#[test]
fn the_encryption_skill_leads_and_the_rest_keep_their_order() {
    let skills = vec![
        requirement(MECHANICAL, "Mechanical Engineering", 1),
        requirement(ENCRYPTION, "Gallente Encryption Methods", 1),
        requirement(GALLENTE_STARSHIP, "Gallentean Starship Engineering", 1),
    ];

    let resolved = resolve(&recipe(skills), None, &BTreeMap::new());

    assert_eq!(
        resolved
            .views
            .iter()
            .map(|view| view.skill_type_id)
            .collect::<Vec<_>>(),
        vec![ENCRYPTION, MECHANICAL, GALLENTE_STARSHIP]
    );
}

#[test]
fn a_job_without_an_encryption_skill_gets_no_encryption_bonus() {
    let skills = vec![
        requirement(GALLENTE_STARSHIP, "Gallentean Starship Engineering", 1),
        requirement(MECHANICAL, "Mechanical Engineering", 1),
    ];

    let resolved = resolve(&recipe(skills), None, &BTreeMap::new());

    assert_eq!(resolved.levels.encryption, 0);
    assert_eq!(resolved.levels.science, vec![4, 4]);
}

fn captured() -> CapturedRecipe {
    CapturedRecipe::capture(
        Uuid::nil(),
        "test".to_string(),
        ManufacturingRecipe {
            blueprint_type_id: 11_378,
            blueprint_name: "Nemesis Blueprint".to_string(),
            duration_seconds: Some(600),
            materials: vec![
                RecipeLine {
                    type_id: 1,
                    type_name: "Plate".to_string(),
                    quantity: 100,
                },
                RecipeLine {
                    type_id: 2,
                    type_name: "Hull".to_string(),
                    quantity: 1,
                },
            ],
            products: vec![RecipeLine {
                type_id: 11_377,
                type_name: "Nemesis".to_string(),
                quantity: 1,
            }],
        },
    )
    .unwrap()
}

fn copy(material_efficiency: u8, runs: u64) -> InventedCopy {
    InventedCopy {
        material_efficiency,
        time_efficiency: 4,
        runs,
    }
}

fn prices(entries: &[(i64, i64)]) -> BTreeMap<i64, Money> {
    entries
        .iter()
        .map(|(type_id, price)| (*type_id, Money(Decimal::from(*price))))
        .collect()
}

fn no_class() -> ProductClassification {
    ProductClassification {
        category_id: None,
        group_id: None,
    }
}

#[test]
fn manufacturing_cost_applies_the_copys_me_to_the_materials() {
    let best_sell = prices(&[(1, 10), (2, 1_000)]);
    let cost = |me| {
        manufacturing_cost_per_run(&captured(), copy(me, 1), None, no_class(), None, &best_sell)
            .unwrap()
            .unwrap()
            .0
    };

    // 100 plates at ME 2 is 98, at ME 5 is 95; the single hull stays 1.
    assert_eq!(cost(2), Decimal::from(98 * 10 + 1_000));
    assert_eq!(cost(5), Decimal::from(95 * 10 + 1_000));
}

/// Material rounding happens over the whole job, so a ten-run copy's run is
/// costed as a tenth of a ten-run job.
#[test]
fn manufacturing_cost_is_a_share_of_a_job_over_all_the_copys_runs() {
    let mut recipe = captured();
    recipe.materials[0].quantity_per_run = 5;
    let best_sell = prices(&[(1, 10), (2, 1_000)]);
    let cost = |runs| {
        manufacturing_cost_per_run(&recipe, copy(4, runs), None, no_class(), None, &best_sell)
            .unwrap()
            .unwrap()
            .0
    };

    // One run: ceil(5 × 0.96) = 5 plates. Ten runs: ceil(50 × 0.96) = 48.
    assert_eq!(cost(1), Decimal::from(5 * 10 + 1_000));
    assert_eq!(
        cost(10),
        Decimal::from(48 * 10 + 10_000) / Decimal::from(10)
    );
}

#[test]
fn manufacturing_cost_is_unknown_when_an_input_has_no_price() {
    let cost = manufacturing_cost_per_run(
        &captured(),
        copy(2, 1),
        None,
        no_class(),
        None,
        &prices(&[(1, 10)]),
    )
    .unwrap();

    assert_eq!(cost, None);
}
