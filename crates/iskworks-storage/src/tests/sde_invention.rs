use super::*;
use iskworks_sde::{
    Decryptor, ImportActivitySkill, ImportDecryptor, InventionProduct, InventionSkillRequirement,
    RecipeLine,
};
use rust_decimal::Decimal;

fn named_type(type_id: i64, name: &str, group_name: &str) -> ImportType {
    ImportType {
        type_id,
        name: name.to_string(),
        group_id: None,
        group_name: Some(group_name.to_string()),
        market_group_id: None,
        meta_group_id: None,
        packaged_volume_m3: None,
        published: true,
    }
}

fn manufacturing(blueprint_type_id: i64, product_type_id: i64) -> ImportBlueprintActivity {
    ImportBlueprintActivity {
        kind: BlueprintActivityKind::Manufacturing,
        skills: Vec::new(),
        blueprint_type_id,
        duration_seconds: Some(600),
        materials: vec![ImportMaterial {
            type_id: 34,
            quantity: 10,
            position: 0,
        }],
        products: vec![ImportActivityProduct {
            probability: None,
            type_id: product_type_id,
            quantity: 1,
            position: 0,
        }],
    }
}

/// The shared fixture's Rifter Blueprint (6830) invents the Jaguar and Wolf
/// blueprints, as in the game.
fn invention_dataset(checksum: &str) -> NormalizedSde {
    let mut dataset = fixture_sde(checksum, "100009");
    dataset.types.extend([
        named_type(11_371, "Wolf", "Assault Frigate"),
        named_type(11_372, "Wolf Blueprint", "Frigate Blueprint"),
        named_type(11_400, "Jaguar", "Assault Frigate"),
        named_type(11_401, "Jaguar Blueprint", "Frigate Blueprint"),
        named_type(
            20_172,
            "Datacore - Minmatar Starship Engineering",
            "Datacores",
        ),
        named_type(20_424, "Datacore - Mechanical Engineering", "Datacores"),
        named_type(11_454, "Minmatar Starship Engineering", "Science"),
        named_type(11_452, "Mechanical Engineering", "Science"),
        named_type(21_791, "Minmatar Encryption Methods", "Science"),
        named_type(34_201, "Accelerant Decryptor", "Generic Decryptor"),
        named_type(34_206, "Process Decryptor", "Generic Decryptor"),
    ]);
    dataset
        .blueprint_activities
        .extend([manufacturing(11_372, 11_371), manufacturing(11_401, 11_400)]);
    dataset.blueprint_activities.extend([
        ImportBlueprintActivity {
            kind: BlueprintActivityKind::Invention,
            blueprint_type_id: 6_830,
            duration_seconds: Some(63_900),
            materials: vec![
                ImportMaterial {
                    type_id: 20_172,
                    quantity: 2,
                    position: 0,
                },
                ImportMaterial {
                    type_id: 20_424,
                    quantity: 2,
                    position: 1,
                },
            ],
            products: vec![
                ImportActivityProduct {
                    type_id: 11_401,
                    quantity: 1,
                    probability: Some(Decimal::new(3, 1)),
                    position: 0,
                },
                ImportActivityProduct {
                    type_id: 11_372,
                    quantity: 1,
                    probability: Some(Decimal::new(3, 1)),
                    position: 1,
                },
            ],
            skills: [11_452, 11_454, 21_791]
                .into_iter()
                .enumerate()
                .map(|(position, skill_type_id)| ImportActivitySkill {
                    skill_type_id,
                    level: 1,
                    position: position as i32,
                })
                .collect(),
        },
        ImportBlueprintActivity {
            blueprint_type_id: 6_830,
            kind: BlueprintActivityKind::Copying,
            duration_seconds: Some(4_800),
            materials: Vec::new(),
            products: Vec::new(),
            skills: Vec::new(),
        },
    ]);
    dataset.decryptors = vec![
        ImportDecryptor {
            type_id: 34_201,
            probability_multiplier: Decimal::new(12, 1),
            me_modifier: 2,
            te_modifier: 10,
            run_modifier: 1,
        },
        ImportDecryptor {
            type_id: 34_206,
            probability_multiplier: Decimal::new(11, 1),
            me_modifier: 3,
            te_modifier: 6,
            run_modifier: 0,
        },
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
async fn active_by_checksum_requires_invention_when_requested(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    let requires_invention = SdeDatasetRequirements {
        invention: true,
        ..SdeDatasetRequirements::default()
    };
    // The same archive imported before invention was read must be redone.
    import_active(&repository, &fixture_sde("checksum-invention", "100009")).await;
    assert!(!repository.has_invention_data().await.unwrap());
    assert!(repository
        .active_by_checksum("checksum-invention", requires_invention)
        .await
        .unwrap()
        .is_none());
    assert!(repository
        .active_by_checksum("checksum-invention", SdeDatasetRequirements::default())
        .await
        .unwrap()
        .is_some());

    import_active(&repository, &invention_dataset("checksum-invention")).await;

    assert!(repository.has_invention_data().await.unwrap());
    assert!(repository
        .active_by_checksum("checksum-invention", requires_invention)
        .await
        .unwrap()
        .is_some());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn invention_recipe_is_read_from_the_invented_blueprint(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    import_active(&repository, &invention_dataset("checksum-invention-read")).await;

    let recipe = repository.invention_recipe(11_372).await.unwrap().unwrap();

    assert_eq!(recipe.source_blueprint_type_id, 6_830);
    assert_eq!(recipe.source_blueprint_name, "Rifter Blueprint");
    assert_eq!(recipe.duration_seconds, Some(63_900));
    assert_eq!(recipe.copying_duration_seconds, Some(4_800));
    assert_eq!(
        recipe.materials,
        vec![
            RecipeLine {
                type_id: 20_172,
                type_name: "Datacore - Minmatar Starship Engineering".to_string(),
                quantity: 2,
            },
            RecipeLine {
                type_id: 20_424,
                type_name: "Datacore - Mechanical Engineering".to_string(),
                quantity: 2,
            },
        ]
    );
    // Both outputs of the source, in SDE order, whichever one was asked for.
    assert_eq!(
        recipe.products,
        vec![
            InventionProduct {
                blueprint_type_id: 11_401,
                blueprint_name: "Jaguar Blueprint".to_string(),
                product_type_id: Some(11_400),
                product_name: Some("Jaguar".to_string()),
                base_runs: 1,
                probability: Decimal::new(3, 1),
            },
            InventionProduct {
                blueprint_type_id: 11_372,
                blueprint_name: "Wolf Blueprint".to_string(),
                product_type_id: Some(11_371),
                product_name: Some("Wolf".to_string()),
                base_runs: 1,
                probability: Decimal::new(3, 1),
            },
        ]
    );
    assert_eq!(
        recipe.skills,
        vec![
            InventionSkillRequirement {
                skill_type_id: 11_452,
                skill_name: "Mechanical Engineering".to_string(),
                level: 1,
            },
            InventionSkillRequirement {
                skill_type_id: 21_791,
                skill_name: "Minmatar Encryption Methods".to_string(),
                level: 1,
            },
            InventionSkillRequirement {
                skill_type_id: 11_454,
                skill_name: "Minmatar Starship Engineering".to_string(),
                level: 1,
            },
        ]
    );
    // The source blueprint itself is not something invention yields.
    assert!(repository.invention_recipe(6_830).await.unwrap().is_none());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn decryptors_are_read_by_name(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    import_active(&repository, &invention_dataset("checksum-decryptors")).await;

    assert_eq!(
        repository.decryptors().await.unwrap(),
        vec![
            Decryptor {
                type_id: 34_201,
                name: "Accelerant Decryptor".to_string(),
                probability_multiplier: Decimal::new(12, 1),
                me_modifier: 2,
                te_modifier: 10,
                run_modifier: 1,
            },
            Decryptor {
                type_id: 34_206,
                name: "Process Decryptor".to_string(),
                probability_multiplier: Decimal::new(11, 1),
                me_modifier: 3,
                te_modifier: 6,
                run_modifier: 0,
            },
        ]
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn inventable_product_search_finds_invented_items_only(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    import_active(&repository, &invention_dataset("checksum-invention-search")).await;

    let wolf = repository
        .search_inventable_products("wolf", 10)
        .await
        .unwrap();
    assert_eq!(wolf.len(), 1);
    assert_eq!(wolf[0].product_type_id, 11_371);
    assert_eq!(wolf[0].product_name, "Wolf");
    assert_eq!(wolf[0].group_name.as_deref(), Some("Assault Frigate"));
    assert_eq!(wolf[0].blueprint_type_id, 11_372);
    assert_eq!(wolf[0].source_blueprint_type_id, 6_830);
    assert_eq!(wolf[0].source_blueprint_name, "Rifter Blueprint");
    assert_eq!(wolf[0].source_output_count, 2);

    // The Rifter is built from the source blueprint, not invented.
    assert!(repository
        .search_inventable_products("rifter", 10)
        .await
        .unwrap()
        .is_empty());
    // The match is on the item's name, anywhere in it and ignoring case.
    let jaguar = repository
        .search_inventable_products("AGU", 10)
        .await
        .unwrap();
    assert_eq!(
        jaguar
            .iter()
            .map(|item| (item.product_name.as_str(), item.blueprint_type_id))
            .collect::<Vec<_>>(),
        vec![("Jaguar", 11_401)]
    );
    assert!(repository
        .search_inventable_products("  ", 10)
        .await
        .unwrap()
        .is_empty());
}
