use super::*;
use iskworks_sde::{
    ImportRequiredSkill, ImportSkill, RequiredSkill, SkillAttribute, SkillDefinition,
};
use std::collections::BTreeMap;

fn skill_type(type_id: i64, name: &str) -> ImportType {
    ImportType {
        type_id,
        name: name.to_string(),
        group_id: Some(273),
        group_name: Some("Drones".to_string()),
        market_group_id: None,
        meta_group_id: None,
        packaged_volume_m3: None,
        published: true,
    }
}

fn skill(type_id: i64, rank: i32, omega_only: bool) -> ImportSkill {
    ImportSkill {
        type_id,
        rank,
        primary_attribute: SkillAttribute::Memory,
        secondary_attribute: SkillAttribute::Perception,
        omega_only,
    }
}

fn required(type_id: i64, ordinal: i16, skill_type_id: i64, level: i16) -> ImportRequiredSkill {
    ImportRequiredSkill {
        type_id,
        ordinal,
        skill_type_id,
        level,
    }
}

/// Drones (rank 1), Drone Interfacing (rank 5, needs Drones V) and an
/// Omega-only specialization needing both; the Rifter needs Drones I.
fn skills_fixture(checksum: &str) -> NormalizedSde {
    let mut dataset = fixture_sde(checksum, "100003");
    dataset.categories.push(ImportCategory {
        category_id: 16,
        name: "Skill".into(),
        published: true,
    });
    dataset.groups.push(ImportGroup {
        group_id: 273,
        name: "Drones".into(),
        category_id: Some(16),
        published: true,
    });
    dataset.types.extend([
        skill_type(3_436, "Drones"),
        skill_type(3_442, "Drone Interfacing"),
        skill_type(12_487, "Amarr Drone Specialization"),
    ]);
    dataset.skills = vec![
        skill(3_436, 1, false),
        skill(3_442, 5, false),
        skill(12_487, 5, true),
    ];
    dataset.required_skills = vec![
        required(3_442, 1, 3_436, 5),
        required(5_876, 1, 3_436, 1),
        required(12_487, 1, 3_442, 5),
        required(12_487, 2, 3_436, 5),
    ];
    dataset
}

async fn import_active(repository: &PgSdeRepository, dataset: &NormalizedSde) {
    let import_id = repository
        .begin_import(NewImport {
            source_version: dataset.source_version.clone(),
            source_label: dataset.source_label.clone(),
            source_checksum: dataset.source_checksum.clone(),
            started_at: Utc::now(),
        })
        .await
        .unwrap();
    repository
        .write_dataset(import_id, dataset, &NoopProgressReporter)
        .await
        .unwrap();
    repository
        .activate_import(import_id, dataset.counts(), Utc::now())
        .await
        .unwrap();
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn skill_dogma_is_stored_and_an_import_without_it_is_redone(pool: PgPool) {
    let repository = PgSdeRepository::new(pool.clone());
    let requires_skill_dogma = SdeDatasetRequirements {
        skill_dogma: true,
        ..SdeDatasetRequirements::default()
    };
    // The same archive imported before skill dogma was read must be redone.
    import_active(&repository, &fixture_sde("checksum-skills", "100003")).await;
    assert!(repository
        .active_by_checksum("checksum-skills", requires_skill_dogma)
        .await
        .unwrap()
        .is_none());

    import_active(&repository, &skills_fixture("checksum-skills")).await;

    let skills: Vec<(i64, i32, String, String, bool)> = sqlx::query_as(
        "SELECT s.type_id, s.rank, s.primary_attribute, s.secondary_attribute, s.omega_only
         FROM sde_skills s JOIN sde_imports i ON i.id = s.import_id AND i.active
         ORDER BY s.type_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let memory_perception = |type_id, rank, omega_only| {
        (
            type_id,
            rank,
            "memory".to_string(),
            "perception".to_string(),
            omega_only,
        )
    };
    assert_eq!(
        skills,
        vec![
            memory_perception(3_436, 1, false),
            memory_perception(3_442, 5, false),
            memory_perception(12_487, 5, true),
        ]
    );
    let required: Vec<(i64, i16, i64, i16)> = sqlx::query_as(
        "SELECT r.type_id, r.ordinal, r.skill_type_id, r.level
         FROM sde_type_required_skills r JOIN sde_imports i ON i.id = r.import_id AND i.active
         ORDER BY r.type_id, r.ordinal",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        required,
        vec![
            (3_442, 1, 3_436, 5),
            (5_876, 1, 3_436, 1),
            (12_487, 1, 3_442, 5),
            (12_487, 2, 3_436, 5),
        ]
    );
    assert!(repository
        .active_by_checksum("checksum-skills", requires_skill_dogma)
        .await
        .unwrap()
        .is_some());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn skill_catalogue_lists_published_skills_with_prerequisites(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    let mut dataset = skills_fixture("checksum-catalogue");
    // An unpublished skill (e.g. a retired one) is not in the catalogue.
    dataset.types.push(ImportType {
        published: false,
        ..skill_type(9_001, "Retired Skill")
    });
    dataset.skills.push(skill(9_001, 1, false));
    import_active(&repository, &dataset).await;

    let catalogue = repository.skill_catalogue().await.unwrap();

    let drones = |type_id: i64, name: &str, rank, omega_only, prerequisites| SkillDefinition {
        type_id,
        name: name.to_string(),
        group_id: 273,
        group_name: "Drones".to_string(),
        rank,
        primary_attribute: SkillAttribute::Memory,
        secondary_attribute: SkillAttribute::Perception,
        omega_only,
        prerequisites,
    };
    let needs = |skill_type_id, level| RequiredSkill {
        skill_type_id,
        level,
    };
    assert_eq!(
        catalogue,
        vec![
            drones(
                12_487,
                "Amarr Drone Specialization",
                5,
                true,
                vec![needs(3_442, 5), needs(3_436, 5)],
            ),
            drones(3_442, "Drone Interfacing", 5, false, vec![needs(3_436, 5)]),
            drones(3_436, "Drones", 1, false, Vec::new()),
        ]
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn required_skills_by_type_in_ordinal_order(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    import_active(&repository, &skills_fixture("checksum-required")).await;

    let required = repository
        .required_skills(&[5_876, 12_487, 34])
        .await
        .unwrap();

    assert_eq!(
        required,
        BTreeMap::from([
            (
                5_876,
                vec![RequiredSkill {
                    skill_type_id: 3_436,
                    level: 1
                }]
            ),
            (
                12_487,
                vec![
                    RequiredSkill {
                        skill_type_id: 3_442,
                        level: 5
                    },
                    RequiredSkill {
                        skill_type_id: 3_436,
                        level: 5
                    },
                ]
            ),
        ])
    );
    assert!(repository.required_skills(&[]).await.unwrap().is_empty());
}
