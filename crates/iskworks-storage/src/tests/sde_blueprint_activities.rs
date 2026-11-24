use super::*;
use iskworks_sde::{
    ActivityMaterial, ActivityProduct, BlueprintActivity, BlueprintActivityKind,
    ImportActivityProduct, ImportActivitySkill, ImportBlueprintActivity, ImportBlueprintLimit,
    RequiredSkill,
};
use rust_decimal::Decimal;

fn named_type(type_id: i64, name: &str, group: Option<(i64, &str)>) -> ImportType {
    ImportType {
        type_id,
        name: name.to_string(),
        group_id: group.map(|(id, _)| id),
        group_name: group.map(|(_, name)| name.to_string()),
        market_group_id: None,
        meta_group_id: None,
        packaged_volume_m3: None,
        published: true,
    }
}

/// The shared fixture's Rifter Blueprint (6830) with a research, a copying
/// and an invention activity.
fn activities_dataset(checksum: &str) -> NormalizedSde {
    let mut dataset = fixture_sde(checksum, "100010");
    dataset.types.extend([
        named_type(11_401, "Jaguar Blueprint", None),
        named_type(
            20_172,
            "Datacore - Minmatar Starship Engineering",
            Some((333, "Datacores")),
        ),
        named_type(
            20_424,
            "Datacore - Mechanical Engineering",
            Some((333, "Datacores")),
        ),
        named_type(3_402, "Science", None),
        named_type(3_409, "Metallurgy", None),
    ]);
    let activity = |kind, duration_seconds| ImportBlueprintActivity {
        blueprint_type_id: 6_830,
        kind,
        duration_seconds: Some(duration_seconds),
        materials: Vec::new(),
        products: Vec::new(),
        skills: Vec::new(),
    };
    dataset.blueprint_activities = vec![
        ImportBlueprintActivity {
            skills: vec![
                ImportActivitySkill {
                    skill_type_id: 3_409,
                    level: 4,
                    position: 0,
                },
                ImportActivitySkill {
                    skill_type_id: 3_402,
                    level: 3,
                    position: 1,
                },
            ],
            ..activity(BlueprintActivityKind::ResearchMaterial, 2_100)
        },
        activity(BlueprintActivityKind::Copying, 4_800),
        ImportBlueprintActivity {
            materials: vec![
                ImportMaterial {
                    type_id: 20_424,
                    quantity: 2,
                    position: 1,
                },
                ImportMaterial {
                    type_id: 20_172,
                    quantity: 2,
                    position: 0,
                },
            ],
            products: vec![ImportActivityProduct {
                type_id: 11_401,
                quantity: 1,
                probability: Some(Decimal::new(3, 1)),
                position: 0,
            }],
            ..activity(BlueprintActivityKind::Invention, 63_900)
        },
    ];
    dataset.blueprint_limits = vec![ImportBlueprintLimit {
        blueprint_type_id: 6_830,
        max_production_limit: 30,
    }];
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
async fn active_by_checksum_requires_blueprint_activities_when_requested(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    let requires = SdeDatasetRequirements {
        blueprint_activities: true,
        ..SdeDatasetRequirements::default()
    };
    // The same archive imported before activities were read must be redone.
    import_active(&repository, &fixture_sde("checksum-activities", "100010")).await;
    assert!(repository
        .blueprint_activities(6_830)
        .await
        .unwrap()
        .is_none());
    assert!(repository
        .active_by_checksum("checksum-activities", requires)
        .await
        .unwrap()
        .is_none());

    import_active(&repository, &activities_dataset("checksum-activities")).await;

    assert!(repository
        .active_by_checksum("checksum-activities", requires)
        .await
        .unwrap()
        .is_some());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn blueprint_activities_are_read_in_game_order(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    import_active(&repository, &activities_dataset("checksum-activities-read")).await;

    let read = repository
        .blueprint_activities(6_830)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(read.max_production_limit, Some(30));
    let datacore = |type_id, type_name: &str| ActivityMaterial {
        type_id,
        type_name: type_name.to_string(),
        quantity: 2,
        group_id: Some(333),
        group_name: Some("Datacores".to_string()),
    };
    assert_eq!(
        read.activities,
        vec![
            BlueprintActivity {
                kind: BlueprintActivityKind::ResearchMaterial,
                duration_seconds: Some(2_100),
                materials: Vec::new(),
                products: Vec::new(),
                skills: vec![
                    RequiredSkill {
                        skill_type_id: 3_409,
                        level: 4,
                    },
                    RequiredSkill {
                        skill_type_id: 3_402,
                        level: 3,
                    },
                ],
            },
            BlueprintActivity {
                kind: BlueprintActivityKind::Copying,
                duration_seconds: Some(4_800),
                materials: Vec::new(),
                products: Vec::new(),
                skills: Vec::new(),
            },
            BlueprintActivity {
                kind: BlueprintActivityKind::Invention,
                duration_seconds: Some(63_900),
                materials: vec![
                    datacore(20_172, "Datacore - Minmatar Starship Engineering"),
                    datacore(20_424, "Datacore - Mechanical Engineering"),
                ],
                products: vec![ActivityProduct {
                    type_id: 11_401,
                    type_name: "Jaguar Blueprint".to_string(),
                    quantity: 1,
                    probability: Some(Decimal::new(3, 1)),
                }],
                skills: Vec::new(),
            },
        ]
    );
    // A type with no activities, such as the Rifter itself.
    assert!(repository
        .blueprint_activities(587)
        .await
        .unwrap()
        .is_none());
}
