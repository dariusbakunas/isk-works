use super::*;
use iskworks_sde::{
    ImportDogmaAttribute, ImportDogmaUnit, ImportTypeAttribute, ImportTypeBonus, ImportTypeDetails,
    ImportTypeInfo, ImportTypeMaterial, TypeBonusSource,
};
use rust_decimal::Decimal;

/// type_id, description, volume, mass, capacity, portion size, variation parent.
type DetailsRow = (
    i64,
    Option<String>,
    Option<Decimal>,
    Option<Decimal>,
    Option<Decimal>,
    Option<i32>,
    Option<i64>,
);
/// attribute_id, display name, unit_id, unit display name, category, value.
type AttributeRow = (
    i64,
    String,
    Option<i64>,
    Option<String>,
    Option<String>,
    Decimal,
);
/// ordinal, source kind, skill, bonus, unit_id, text, importance, is_positive.
type BonusRow = (
    i32,
    String,
    Option<i64>,
    Option<Decimal>,
    Option<i64>,
    String,
    Option<i32>,
    Option<bool>,
);

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

/// The Rifter (5876) and Tritanium (34) from `fixture_sde`, with the Type
/// Info shapes of the SDE parse tests.
fn type_info_fixture(checksum: &str) -> NormalizedSde {
    let mut dataset = fixture_sde(checksum, "100004");
    dataset.type_info = ImportTypeInfo {
        details: vec![ImportTypeDetails {
            type_id: 5_876,
            description: Some("A <a href=showinfo:34>fast</a> frigate.".into()),
            volume_m3: Some(Decimal::new(27_289, 0)),
            mass_kg: Some(Decimal::new(1_067_000, 0)),
            capacity_m3: Some(Decimal::new(140, 0)),
            portion_size: Some(1),
            variation_parent_type_id: Some(587),
        }],
        dogma_units: vec![
            ImportDogmaUnit {
                unit_id: 105,
                name: "Percentage".into(),
                display_name: Some("%".into()),
            },
            ImportDogmaUnit {
                unit_id: 122,
                name: "Fitting slots".into(),
                display_name: None,
            },
        ],
        dogma_attributes: vec![
            ImportDogmaAttribute {
                attribute_id: 12,
                name: "lowSlots".into(),
                display_name: "Low Slots".into(),
                unit_id: Some(122),
                category_id: None,
                category_name: None,
                display_when_zero: true,
                high_is_good: true,
            },
            ImportDogmaAttribute {
                attribute_id: 2023,
                name: "aoeCloudSizeBonusBonus".into(),
                display_name: "Modification of Explosion Radius Bonus".into(),
                unit_id: Some(105),
                category_id: Some(7),
                category_name: Some("Miscellaneous".into()),
                display_when_zero: false,
                high_is_good: false,
            },
        ],
        type_attributes: vec![
            ImportTypeAttribute {
                type_id: 5_876,
                attribute_id: 12,
                value: Decimal::new(4, 0),
            },
            ImportTypeAttribute {
                type_id: 5_876,
                attribute_id: 2023,
                value: Decimal::new(-125, 1),
            },
        ],
        type_materials: vec![ImportTypeMaterial {
            type_id: 5_876,
            material_type_id: 34,
            quantity: 14_000,
        }],
        type_bonuses: vec![
            ImportTypeBonus {
                type_id: 5_876,
                ordinal: 1,
                source: TypeBonusSource::Skill(3_329),
                bonus: Some(Decimal::new(5, 0)),
                unit_id: Some(105),
                text: "bonus to Small Projectile Turret damage".into(),
                importance: Some(1),
                is_positive: None,
            },
            ImportTypeBonus {
                type_id: 5_876,
                ordinal: 2,
                source: TypeBonusSource::Role,
                bonus: None,
                unit_id: None,
                text: "Can fit <a href=showinfo:47254>Assault Damage Controls</a>".into(),
                importance: Some(2),
                is_positive: Some(true),
            },
        ],
    };
    dataset
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn type_info_is_stored_and_an_import_without_it_is_redone(pool: PgPool) {
    let repository = PgSdeRepository::new(pool.clone());
    let requires_type_info = SdeDatasetRequirements {
        type_info: true,
        ..SdeDatasetRequirements::default()
    };
    // The same archive imported before type info was read must be redone.
    import_active(&repository, &fixture_sde("checksum-type-info", "100004")).await;
    assert!(repository
        .active_by_checksum("checksum-type-info", requires_type_info)
        .await
        .unwrap()
        .is_none());

    import_active(&repository, &type_info_fixture("checksum-type-info")).await;

    let details: Vec<DetailsRow> = sqlx::query_as(
        "SELECT d.type_id, d.description_en, d.volume_m3, d.mass_kg, d.capacity_m3,
                    d.portion_size, d.variation_parent_type_id
             FROM sde_type_details d JOIN sde_imports i ON i.id = d.import_id AND i.active",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        details,
        vec![(
            5_876,
            Some("A <a href=showinfo:34>fast</a> frigate.".into()),
            Some(Decimal::new(27_289, 0)),
            Some(Decimal::new(1_067_000, 0)),
            Some(Decimal::new(140, 0)),
            Some(1),
            Some(587),
        )]
    );

    let attributes: Vec<AttributeRow> = sqlx::query_as(
        "SELECT a.attribute_id, a.display_name_en, a.unit_id, u.display_name, a.category_name,
                    v.value
             FROM sde_type_attributes v
             JOIN sde_imports i ON i.id = v.import_id AND i.active
             JOIN sde_dogma_attributes a
               ON a.import_id = v.import_id AND a.attribute_id = v.attribute_id
             LEFT JOIN sde_dogma_units u ON u.import_id = a.import_id AND u.unit_id = a.unit_id
             WHERE v.type_id = 5876
             ORDER BY a.attribute_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        attributes,
        vec![
            (
                12,
                "Low Slots".into(),
                Some(122),
                None,
                None,
                Decimal::new(4, 0)
            ),
            (
                2023,
                "Modification of Explosion Radius Bonus".into(),
                Some(105),
                Some("%".into()),
                Some("Miscellaneous".into()),
                Decimal::new(-125, 1),
            ),
        ]
    );

    let materials: Vec<(i64, i64, i64)> = sqlx::query_as(
        "SELECT m.type_id, m.material_type_id, m.quantity
         FROM sde_type_materials m JOIN sde_imports i ON i.id = m.import_id AND i.active",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(materials, vec![(5_876, 34, 14_000)]);

    let bonuses: Vec<BonusRow> = sqlx::query_as(
        "SELECT b.ordinal, b.source_kind, b.skill_type_id, b.bonus, b.unit_id, b.text_en,
                    b.importance, b.is_positive
             FROM sde_type_bonuses b JOIN sde_imports i ON i.id = b.import_id AND i.active
             WHERE b.type_id = 5876
             ORDER BY b.ordinal",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        bonuses,
        vec![
            (
                1,
                "skill".into(),
                Some(3_329),
                Some(Decimal::new(5, 0)),
                Some(105),
                "bonus to Small Projectile Turret damage".into(),
                Some(1),
                None,
            ),
            (
                2,
                "role".into(),
                None,
                None,
                None,
                "Can fit <a href=showinfo:47254>Assault Damage Controls</a>".into(),
                Some(2),
                Some(true),
            ),
        ]
    );

    assert!(repository
        .active_by_checksum("checksum-type-info", requires_type_info)
        .await
        .unwrap()
        .is_some());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_newer_import_keeps_the_superseded_type_info(pool: PgPool) {
    let repository = PgSdeRepository::new(pool.clone());
    import_active(&repository, &type_info_fixture("checksum-type-info-old")).await;
    import_active(&repository, &type_info_fixture("checksum-type-info-new")).await;

    let per_import: Vec<(i64,)> = sqlx::query_as(
        "SELECT count(*) FROM sde_type_attributes GROUP BY import_id ORDER BY import_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(per_import, vec![(2,), (2,)]);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn type_info_reads_come_from_the_active_import(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    import_active(&repository, &type_info_fixture("checksum-type-info-reads")).await;

    let rifter = repository.type_info_base(5_876).await.unwrap().unwrap();
    assert_eq!(
        rifter,
        iskworks_sde::TypeInfoBase {
            type_id: 5_876,
            name: "Rifter".into(),
            published: true,
            group_id: Some(25),
            group_name: Some("Frigate".into()),
            category_id: Some(6),
            category_name: Some("Ship".into()),
            meta_group_id: Some(1),
            meta_group_name: Some("Tech I".into()),
            market_group_path: vec!["Ships".into(), "Frigates".into()],
            packaged_volume_m3: None,
            description: Some("A <a href=showinfo:34>fast</a> frigate.".into()),
            volume_m3: Some(Decimal::new(27_289, 0)),
            mass_kg: Some(Decimal::new(1_067_000, 0)),
            capacity_m3: Some(Decimal::new(140, 0)),
            portion_size: Some(1),
            variation_parent_type_id: Some(587),
        }
    );
    // A type without details or a market group.
    let tritanium = repository.type_info_base(34).await.unwrap().unwrap();
    assert_eq!(tritanium.description, None);
    assert_eq!(tritanium.packaged_volume_m3, Some(Decimal::new(1, 2)));
    assert!(repository.type_info_base(999_999).await.unwrap().is_none());

    assert_eq!(
        repository.type_attributes(5_876).await.unwrap(),
        vec![
            iskworks_sde::TypeAttributeValue {
                attribute_id: 12,
                display_name: "Low Slots".into(),
                unit_id: Some(122),
                unit_display_name: None,
                category_id: None,
                category_name: None,
                display_when_zero: true,
                high_is_good: true,
                value: Decimal::new(4, 0),
            },
            iskworks_sde::TypeAttributeValue {
                attribute_id: 2023,
                display_name: "Modification of Explosion Radius Bonus".into(),
                unit_id: Some(105),
                unit_display_name: Some("%".into()),
                category_id: Some(7),
                category_name: Some("Miscellaneous".into()),
                display_when_zero: false,
                high_is_good: false,
                value: Decimal::new(-125, 1),
            },
        ]
    );

    assert_eq!(
        repository.type_bonuses(5_876).await.unwrap(),
        vec![
            iskworks_sde::TypeBonusLine {
                source: TypeBonusSource::Skill(3_329),
                bonus: Some(Decimal::new(5, 0)),
                unit_id: Some(105),
                unit_display_name: Some("%".into()),
                text: "bonus to Small Projectile Turret damage".into(),
            },
            iskworks_sde::TypeBonusLine {
                source: TypeBonusSource::Role,
                bonus: None,
                unit_id: None,
                unit_display_name: None,
                text: "Can fit <a href=showinfo:47254>Assault Damage Controls</a>".into(),
            },
        ]
    );

    assert_eq!(
        repository.type_materials(5_876).await.unwrap(),
        vec![iskworks_sde::RecipeLine {
            type_id: 34,
            type_name: "Tritanium".into(),
            quantity: 14_000,
        }]
    );

    // Tritanium goes into the Rifter Blueprint and a reaction formula.
    assert_eq!(
        repository.type_info_counts(34).await.unwrap(),
        iskworks_sde::TypeInfoCounts {
            variations: 1,
            used_in: 2,
        }
    );
    assert_eq!(
        repository.type_info_counts(5_876).await.unwrap(),
        iskworks_sde::TypeInfoCounts {
            variations: 1,
            used_in: 0,
        }
    );

    assert_eq!(
        repository.group_names(&[25, 999_999]).await.unwrap(),
        std::collections::BTreeMap::from([(25, "Frigate".to_string())])
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn variations_count_the_whole_family(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    let mut dataset = type_info_fixture("checksum-type-info-family");
    // Make the Rifter the family's parent and the Tritanium... a variation
    // of it, just to have two members.
    dataset.type_info.details[0].variation_parent_type_id = None;
    dataset.type_info.details.insert(
        0,
        ImportTypeDetails {
            type_id: 34,
            description: None,
            volume_m3: None,
            mass_kg: None,
            capacity_m3: None,
            portion_size: None,
            variation_parent_type_id: Some(5_876),
        },
    );
    import_active(&repository, &dataset).await;

    for type_id in [34, 5_876] {
        assert_eq!(
            repository
                .type_info_counts(type_id)
                .await
                .unwrap()
                .variations,
            2,
            "type {type_id}"
        );
    }
}
