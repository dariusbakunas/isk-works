use super::*;

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reverse_recipe_lookup_resolves_a_product_type_id_to_its_recipe(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    let dataset = fixture_sde("checksum-one", "123456");
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
        .write_dataset(import_id, &dataset, &NoopProgressReporter)
        .await
        .unwrap();
    repository
        .activate_import(import_id, dataset.counts(), Utc::now())
        .await
        .unwrap();

    // This fixture's reaction formula also lists product 5_876 as an
    // output alongside the manufacturing blueprint -- each method is
    // exercised independently here, not the "which kind wins" priority
    // (that's an API-layer concern, tested separately).
    assert_eq!(
        repository
            .manufacturing_blueprint_for_product(5_876)
            .await
            .unwrap(),
        Some(6_830)
    );
    assert_eq!(
        repository
            .reaction_formula_for_product(5_876)
            .await
            .unwrap(),
        Some(46_157)
    );

    // Tritanium (34) is a raw material never produced by anything in
    // this fixture.
    assert_eq!(
        repository
            .manufacturing_blueprint_for_product(34)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        repository.reaction_formula_for_product(34).await.unwrap(),
        None
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reverse_recipe_lookup_ignores_unpublished_test_recipes(pool: PgPool) {
    // Regression: the real SDE ships an unpublished CCP "Test Reaction
    // Blueprint" (type 45732) that produces Tungsten Carbide with an
    // output-per-run of 20 instead of the genuine formula's 10,000. A bare
    // `LIMIT 1` with no `published` filter resolved "Switch to BUILD" to
    // that decoy, capturing a nonsense recipe onto the linked Build. This
    // mirrors the shape with fixture ids: an unpublished blueprint (4_830)
    // and an unpublished reaction formula (45_999) that both also produce
    // Rifter (5_876), each with a lower type id than the genuine recipe so
    // an unordered `LIMIT 1` would prefer them.
    let repository = PgSdeRepository::new(pool);
    let mut dataset = fixture_sde("checksum-unpublished-decoys", "654321");
    dataset.types.push(ImportType {
        type_id: 4_830,
        name: "Test Rifter Blueprint".to_string(),
        group_id: Some(105),
        group_name: Some("Frigate Blueprint".to_string()),
        market_group_id: None,
        meta_group_id: None,
        packaged_volume_m3: None,
        published: false,
    });
    dataset.types.push(ImportType {
        type_id: 45_999,
        name: "Test Reaction Blueprint".to_string(),
        group_id: Some(1_889),
        group_name: Some("Polymer Reaction Formulas".to_string()),
        market_group_id: None,
        meta_group_id: None,
        packaged_volume_m3: None,
        published: false,
    });
    dataset.blueprints.push(ImportBlueprint {
        blueprint_type_id: 4_830,
        name: "Test Rifter Blueprint".to_string(),
        duration_seconds: Some(1),
        materials: vec![ImportMaterial {
            type_id: 34,
            quantity: 1,
            position: 0,
        }],
        products: vec![ImportProduct {
            type_id: 5_876,
            quantity: 7,
            position: 0,
        }],
    });
    dataset
        .reaction_formulas
        .push(iskworks_sde::ImportReactionFormula {
            reaction_formula_type_id: 45_999,
            name: "Test Reaction Blueprint".to_string(),
            duration_seconds: Some(1),
            materials: vec![ImportMaterial {
                type_id: 34,
                quantity: 1,
                position: 0,
            }],
            products: vec![ImportProduct {
                type_id: 5_876,
                quantity: 20,
                position: 0,
            }],
        });

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
        .write_dataset(import_id, &dataset, &NoopProgressReporter)
        .await
        .unwrap();
    repository
        .activate_import(import_id, dataset.counts(), Utc::now())
        .await
        .unwrap();

    // The genuine published recipes win despite the unpublished decoys
    // sorting first by type id.
    assert_eq!(
        repository
            .manufacturing_blueprint_for_product(5_876)
            .await
            .unwrap(),
        Some(6_830)
    );
    assert_eq!(
        repository
            .reaction_formula_for_product(5_876)
            .await
            .unwrap(),
        Some(46_157)
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn active_by_checksum_requires_reaction_formulas_when_requested(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    // Simulates an import activated before this importer parsed reaction
    // formulas -- same archive checksum, but no reaction data was ever
    // written for it.
    let mut legacy = fixture_sde("checksum-legacy", "100000");
    legacy.reaction_formulas.clear();
    legacy.reaction_rig_modifiers.clear();

    let import_id = repository
        .begin_import(NewImport {
            source_version: legacy.source_version.clone(),
            source_label: legacy.source_label.clone(),
            source_checksum: legacy.source_checksum.clone(),
            started_at: Utc::now(),
        })
        .await
        .unwrap();
    repository
        .write_dataset(import_id, &legacy, &NoopProgressReporter)
        .await
        .unwrap();
    repository
        .activate_import(import_id, legacy.counts(), Utc::now())
        .await
        .unwrap();

    // Re-importing the same archive with a binary that now knows to look
    // for reaction data must not short-circuit as already active --
    // that would leave reaction formulas permanently missing.
    let should_reimport = repository
        .active_by_checksum(
            "checksum-legacy",
            SdeDatasetRequirements {
                reaction_formulas: true,
                ..SdeDatasetRequirements::default()
            },
        )
        .await
        .unwrap();
    assert!(should_reimport.is_none());

    // A caller that doesn't need reaction data keeps the existing
    // fast no-op behavior.
    let still_matches = repository
        .active_by_checksum("checksum-legacy", SdeDatasetRequirements::default())
        .await
        .unwrap();
    assert!(still_matches.is_some());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn active_by_checksum_requires_packaged_volume_when_requested(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    let mut legacy = fixture_sde("checksum-legacy-volume", "100000");
    for item in &mut legacy.types {
        item.packaged_volume_m3 = None;
    }

    let import_id = repository
        .begin_import(NewImport {
            source_version: legacy.source_version.clone(),
            source_label: legacy.source_label.clone(),
            source_checksum: legacy.source_checksum.clone(),
            started_at: Utc::now(),
        })
        .await
        .unwrap();
    repository
        .write_dataset(import_id, &legacy, &NoopProgressReporter)
        .await
        .unwrap();
    repository
        .activate_import(import_id, legacy.counts(), Utc::now())
        .await
        .unwrap();

    let should_reimport = repository
        .active_by_checksum(
            "checksum-legacy-volume",
            SdeDatasetRequirements {
                packaged_volumes: true,
                ..SdeDatasetRequirements::default()
            },
        )
        .await
        .unwrap();
    assert!(should_reimport.is_none());

    let still_matches = repository
        .active_by_checksum("checksum-legacy-volume", SdeDatasetRequirements::default())
        .await
        .unwrap();
    assert!(still_matches.is_some());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn failed_staged_import_does_not_replace_active_dataset(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    let first = fixture_sde("checksum-one", "123456");
    let first_id = repository
        .begin_import(NewImport {
            source_version: first.source_version.clone(),
            source_label: first.source_label.clone(),
            source_checksum: first.source_checksum.clone(),
            started_at: Utc::now(),
        })
        .await
        .unwrap();
    repository
        .write_dataset(first_id, &first, &NoopProgressReporter)
        .await
        .unwrap();
    repository
        .activate_import(first_id, first.counts(), Utc::now())
        .await
        .unwrap();

    let second = repository
        .begin_import(NewImport {
            source_version: "123457".to_string(),
            source_label: "broken.zip".to_string(),
            source_checksum: "checksum-two".to_string(),
            started_at: Utc::now(),
        })
        .await
        .unwrap();
    repository
        .fail_import(second, "fixture failure")
        .await
        .unwrap();

    let active = repository.active_sde().await.unwrap().unwrap();
    assert_eq!(active.import_id, first_id);
    assert_eq!(active.source_version, "123456");
}
