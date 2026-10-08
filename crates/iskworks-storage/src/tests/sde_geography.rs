use super::*;

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn staged_sde_import_activates_and_supports_search_and_recipe_lookup(pool: PgPool) {
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

    let active = repository.active_sde().await.unwrap().unwrap();
    assert_eq!(active.source_version, "123456");
    assert_eq!(active.counts.blueprints, 1);
    assert_eq!(active.counts.categories, 1);
    assert_eq!(active.counts.classified_types, 1);
    let reusable = repository
        .active_by_checksum(
            "checksum-one",
            SdeDatasetRequirements {
                classification_metadata: true,
                ..SdeDatasetRequirements::default()
            },
        )
        .await
        .unwrap();
    assert!(reusable.is_some());
    let categorized: i64 =
        sqlx::query_scalar("SELECT count(*) FROM sde_type_categories WHERE import_id = $1")
            .bind(import_id)
            .fetch_one(repository.pool())
            .await
            .unwrap();
    assert_eq!(categorized, dataset.types.len() as i64);
    // Already populated by the import, so the self-heal has nothing to do.
    assert_eq!(
        repository.ensure_active_type_categories().await.unwrap(),
        None
    );
    sqlx::query("DELETE FROM sde_type_categories")
        .execute(repository.pool())
        .await
        .unwrap();
    assert_eq!(
        repository.ensure_active_type_categories().await.unwrap(),
        Some(dataset.types.len() as u64)
    );
    let stored_group: (String, Option<i64>) = sqlx::query_as(
        "SELECT name_en, category_id FROM sde_groups WHERE import_id = $1 AND group_id = 25",
    )
    .bind(import_id)
    .fetch_one(repository.pool())
    .await
    .unwrap();
    assert_eq!(stored_group, ("Frigate".into(), Some(6)));
    let candidates = repository
        .manufacturable_candidates(&ManufacturableCandidateScope::default())
        .await
        .unwrap();
    assert_eq!(candidates.len(), 2);
    assert!(candidates.iter().any(|candidate| matches!(
        candidate.identity,
        CandidateRecipeIdentity::Manufacturing {
            blueprint_type_id: 6_830
        }
    )));
    assert!(candidates.iter().any(|candidate| matches!(
        candidate.identity,
        CandidateRecipeIdentity::Reaction {
            reaction_formula_type_id: 46_157
        }
    )));
    assert!(candidates.iter().all(|candidate| {
        candidate.materials.len() == 1
            && candidate.products.len() == 1
            && candidate.classification.category_id == Some(6)
            && candidate.classification.meta_group_id == Some(1)
    }));
    let results = repository
        .search_manufacturing_blueprints("rift", 20)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].product_name, "Rifter");
    let types = repository.search_types("trit", 20).await.unwrap();
    assert_eq!(types.len(), 1);
    assert_eq!(types[0].type_name, "Tritanium");

    // `type_reference` -- the verification workbook's bulk `Types` read:
    // one join across sde_types / sde_groups / sde_categories.
    let references = repository.type_reference(&[34, 5_876, -1]).await.unwrap();
    let tritanium = references.get(&34).expect("Tritanium reference row");
    assert_eq!(tritanium.type_name.as_deref(), Some("Tritanium"));
    assert_eq!(tritanium.group_id, Some(18));
    assert_eq!(
        tritanium.packaged_volume_m3,
        Some(rust_decimal::Decimal::new(1, 2))
    );
    let rifter = references.get(&5_876).expect("Rifter reference row");
    assert_eq!(
        rifter.group_name.as_deref(),
        Some("Frigate"),
        "group joined"
    );
    assert_eq!(
        rifter.category_name.as_deref(),
        Some("Ship"),
        "category joined"
    );
    assert!(!references.contains_key(&-1), "unknown type_id omitted");
    assert!(repository.type_reference(&[]).await.unwrap().is_empty());
    let recipe = repository
        .manufacturing_recipe(6_830)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recipe.materials[0].type_name, "Tritanium");
    assert_eq!(recipe.materials[0].quantity, 1_000);
    let systems = repository.search_solar_systems("C-J", 10).await.unwrap();
    assert_eq!(systems.len(), 1);
    assert_eq!(systems[0].solar_system_name, "C-J6MT");
    assert_eq!(systems[0].security_class, "nullSec");
    let stations = repository
        .search_npc_stations("Assembly", 10)
        .await
        .unwrap();
    assert_eq!(stations.len(), 1);
    assert_eq!(
        stations[0].station_type_name.as_deref(),
        Some("Amarr Station")
    );
    assert_eq!(stations[0].security_class, "nullSec");
    let structures = repository
        .search_structure_types("Raitaru", 10)
        .await
        .unwrap();
    assert_eq!(structures.len(), 1);
    assert_eq!(structures[0].type_name, "Raitaru");
    assert_eq!(
        repository
            .search_structure_types("", 250)
            .await
            .unwrap()
            .len(),
        2
    );
    let structure_modifiers = repository
        .structure_manufacturing_modifiers(35_825)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(structure_modifiers.material_reduction_percent, "1");
    let rig_modifiers = repository
        .rig_manufacturing_modifiers(43_800, "nullSec", Some(35_825))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rig_modifiers.material_reduction_percent, "4.2");
    assert_eq!(rig_modifiers.time_reduction_percent, "50.4");
    assert_eq!(rig_modifiers.compatible_with_structure, Some(true));
    // filterID 3 ("Ships") resolves to its category set via the joins.
    let material_filter = rig_modifiers.applies_to.material.unwrap();
    assert_eq!(material_filter.filter_id, 3);
    assert_eq!(material_filter.category_ids, vec![6, 32]);
    assert_eq!(rig_modifiers.applies_to.time.unwrap().filter_id, 3);
    let filters = repository.industry_target_filters().await.unwrap();
    assert_eq!(
        filters.iter().map(|f| f.filter_id).collect::<Vec<_>>(),
        vec![3, 18]
    );
    let rigs = repository
        .search_structure_rigs("Standup M-Set", 10, Some(35_825))
        .await
        .unwrap();
    assert_eq!(rigs.len(), 1);
    assert_eq!(
        rigs[0].type_name,
        "Standup M-Set Basic Small Ship Manufacturing Material Efficiency I"
    );
    assert_eq!(
        repository
            .search_structure_rigs("", 250, None)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(repository
        .search_structure_rigs("Standup M-Set", 10, Some(35_827))
        .await
        .unwrap()
        .is_empty());
}

/// `list_regions`/`list_npc_stations_in_region`/`list_market_groups` are
/// pure SDE reference-data reads, unrelated to `PriceSource` or market
/// coverage -- this dataset seeds two independent regions (The Forge,
/// Insmother) each with their own solar system and NPC station, plus a
/// two-level market-group hierarchy, specifically to prove region-scoped
/// reads never leak across regions and the hierarchy nests correctly.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn region_and_category_reads_are_scoped_correctly_and_never_leak_across_regions(
    pool: PgPool,
) {
    let repository = PgSdeRepository::new(pool);
    let dataset = geography_fixture();
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

    let regions = repository.list_regions().await.unwrap();
    assert_eq!(
        regions,
        vec![
            iskworks_sde::RegionSummary {
                region_id: 10_000_009,
                region_name: "Insmother".to_string(),
            },
            iskworks_sde::RegionSummary {
                region_id: 10_000_048,
                region_name: "Placid".to_string(),
            },
            iskworks_sde::RegionSummary {
                region_id: 10_000_002,
                region_name: "The Forge".to_string(),
            },
        ]
    );

    let forge_locations = repository
        .list_npc_stations_in_region(10_000_002)
        .await
        .unwrap();
    assert_eq!(forge_locations.len(), 1);
    assert_eq!(forge_locations[0].station_id, 60_003_760);
    assert_eq!(
        forge_locations[0].station_name,
        "Jita IV - Moon 4 - Caldari Navy Assembly Plant"
    );
    assert_eq!(forge_locations[0].solar_system_id, 30_000_142);
    assert_eq!(forge_locations[0].solar_system_name, "Jita");

    let insmother_locations = repository
        .list_npc_stations_in_region(10_000_009)
        .await
        .unwrap();
    assert_eq!(insmother_locations.len(), 1);
    assert_eq!(insmother_locations[0].station_id, 60_000_001);
    assert_eq!(insmother_locations[0].solar_system_name, "C-J6MT");

    // Neither region's station list may contain the other region's station.
    assert!(!insmother_locations
        .iter()
        .any(|station| station.station_id == 60_003_760));
    assert!(!forge_locations
        .iter()
        .any(|station| station.station_id == 60_000_001));

    // A region with no stations (or that doesn't exist) returns empty, not
    // an error.
    assert!(repository
        .list_npc_stations_in_region(999)
        .await
        .unwrap()
        .is_empty());

    let groups = repository.list_market_groups().await.unwrap();
    let tree = iskworks_core::build_market_category_tree(groups);
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Ships");
    assert_eq!(tree[0].market_group_id, 4);
    assert_eq!(tree[0].children.len(), 1);
    assert_eq!(tree[0].children[0].name, "Frigates");
    assert_eq!(tree[0].children[0].market_group_id, 1_361);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn planetary_sde_data_round_trips_through_the_active_import(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    let geography = geography_fixture();
    let mut dataset = fixture_sde("checksum-planetary", "100001");
    dataset.regions = geography.regions;
    dataset.constellations = geography.constellations;
    dataset.solar_systems = geography.solar_systems;
    dataset.planet_schematics = vec![iskworks_sde::ImportPlanetSchematic {
        schematic_id: 65,
        name: "Superconductors".into(),
        cycle_time_seconds: 3_600,
        types: vec![
            iskworks_sde::ImportPlanetSchematicType {
                type_id: 34,
                is_input: true,
                quantity: 40,
            },
            iskworks_sde::ImportPlanetSchematicType {
                type_id: 5_876,
                is_input: false,
                quantity: 5,
            },
        ],
    }];
    dataset.planets = vec![iskworks_sde::ImportPlanet {
        planet_id: 40_009_077,
        solar_system_id: 30_000_142,
        celestial_index: 4,
        name: "Jita IV".into(),
    }];
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

    let schematics = repository.planet_schematics(&[65, 999]).await.unwrap();
    assert_eq!(schematics.len(), 1);
    assert_eq!(schematics[&65].cycle_time_seconds, 3_600);
    assert_eq!(
        schematics[&65].inputs,
        vec![iskworks_sde::PlanetSchematicLine {
            type_id: 34,
            quantity: 40
        }]
    );
    assert_eq!(schematics[&65].outputs[0].type_id, 5_876);

    let planets = repository
        .planet_references(&[40_009_077, 1])
        .await
        .unwrap();
    assert_eq!(planets.len(), 1);
    assert_eq!(planets[&40_009_077].name, "Jita IV");
    assert_eq!(planets[&40_009_077].solar_system_name, "Jita");

    let reusable = repository
        .active_by_checksum(
            "checksum-planetary",
            SdeDatasetRequirements {
                planetary_data: true,
                ..SdeDatasetRequirements::default()
            },
        )
        .await
        .unwrap();
    assert!(reusable.is_some());
}

/// `resolve_npc_stations` looks stations up by ID directly, regardless of
/// name/region -- the Market Scope Selector's "Major Hubs" tab needs this
/// to resolve its curated `MAJOR_TRADE_HUBS` station-ID list. A requested
/// ID absent from the active SDE (`999_999_999`) simply doesn't appear in
/// the result rather than erroring.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn resolve_npc_stations_looks_up_by_id_and_includes_region_context(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    let dataset = geography_fixture();
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

    let stations = repository
        .resolve_npc_stations(&[60_003_760, 999_999_999])
        .await
        .unwrap();

    assert_eq!(stations.len(), 1);
    assert_eq!(stations[0].station_id, 60_003_760);
    assert_eq!(
        stations[0].station_name,
        "Jita IV - Moon 4 - Caldari Navy Assembly Plant"
    );
    assert_eq!(stations[0].region_id, 10_000_002);
    assert_eq!(stations[0].region_name, "The Forge");
    assert_eq!(stations[0].security_class, "highSec");
}

/// `search_npc_stations` matches on the station's own name: a query matching a station name finds it, a query
/// matching only its *solar system's* name (not the station's own name)
/// still finds it, and a query matching neither returns an empty,
/// non-erroring result.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn search_npc_stations_matches_station_name_or_solar_system_name(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    let dataset = geography_fixture();
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

    let by_station_name = repository.search_npc_stations("Jita", 10).await.unwrap();
    assert_eq!(by_station_name.len(), 1);
    assert_eq!(by_station_name[0].station_id, 60_003_760);

    // "Origin Trade Post" (in the Botane system) doesn't itself contain
    // "Botane" -- this only finds it via the joined system name.
    let by_system_name = repository.search_npc_stations("Botane", 10).await.unwrap();
    assert_eq!(by_system_name.len(), 1);
    assert_eq!(by_system_name[0].station_id, 60_000_002);
    assert_eq!(by_system_name[0].station_name, "Origin Trade Post");
    assert_eq!(by_system_name[0].solar_system_name, "Botane");

    let no_match = repository
        .search_npc_stations("Nonexistent Query", 10)
        .await
        .unwrap();
    assert!(no_match.is_empty());
}

/// `list_regions`' market-relevance filter: keeps ordinary k-space (`region_id` 10_000_001..=
/// 10_000_069), excludes Anoikis/wormhole regions and everything at or
/// above Pochven's `region_id` (10_000_070) -- see the implementation's own
/// doc comment for the live-data reasoning behind the exact boundary.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn list_regions_excludes_wormhole_and_non_standard_regions(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    let dataset = region_eligibility_fixture();
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

    let regions = repository.list_regions().await.unwrap();

    assert_eq!(
        regions,
        vec![iskworks_sde::RegionSummary {
            region_id: 10_000_002,
            region_name: "The Forge".to_string(),
        }],
        "only the ordinary k-space region should survive -- Anoikis, \
         Pochven, and the far-out special region must all be excluded"
    );
}

/// `search_regions` applies the same
/// market-relevance range as `list_regions` -- a query that would otherwise
/// match an excluded region's name never returns it, and a query too short
/// to search returns empty rather than the whole eligible set.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn search_regions_applies_the_same_eligibility_range_as_list_regions(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    let dataset = region_eligibility_fixture();
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

    let matches = repository.search_regions("Forge", 10).await.unwrap();
    assert_eq!(
        matches,
        vec![iskworks_sde::RegionSummary {
            region_id: 10_000_002,
            region_name: "The Forge".to_string(),
        }]
    );

    // "A-R00001" is the excluded Anoikis region's own name -- an exact
    // substring match on an ineligible region must still return nothing.
    assert!(repository
        .search_regions("A-R00001", 10)
        .await
        .unwrap()
        .is_empty());

    // Too short to search at all.
    assert!(repository.search_regions("F", 10).await.unwrap().is_empty());
}

fn region_eligibility_fixture() -> iskworks_sde::NormalizedSde {
    iskworks_sde::NormalizedSde {
        source_label: "region-eligibility-fixture.zip".to_string(),
        source_checksum: "region-eligibility-fixture-checksum".to_string(),
        source_version: "1".to_string(),
        types: Vec::new(),
        categories: Vec::new(),
        groups: Vec::new(),
        meta_groups: Vec::new(),
        market_groups: Vec::new(),
        blueprints: Vec::new(),
        solar_systems: Vec::new(),
        constellations: Vec::new(),
        regions: vec![
            iskworks_sde::ImportRegion {
                region_id: 10_000_002,
                name: "The Forge".to_string(),
            },
            iskworks_sde::ImportRegion {
                region_id: 10_000_070,
                name: "Pochven".to_string(),
            },
            iskworks_sde::ImportRegion {
                region_id: 10_001_004,
                name: "Exordium".to_string(),
            },
            iskworks_sde::ImportRegion {
                region_id: 11_000_001,
                name: "A-R00001".to_string(),
            },
        ],
        npc_stations: Vec::new(),
        structure_modifiers: Vec::new(),
        rig_modifiers: Vec::new(),
        reaction_formulas: Vec::new(),
        reaction_rig_modifiers: Vec::new(),
        industry_target_filters: Vec::new(),
        planet_schematics: Vec::new(),
        planets: Vec::new(),
        skipped_blueprints: 0,
        skipped_reaction_formulas: 0,
    }
}

fn geography_fixture() -> iskworks_sde::NormalizedSde {
    iskworks_sde::NormalizedSde {
        source_label: "geography-fixture.zip".to_string(),
        source_checksum: "geography-fixture-checksum".to_string(),
        source_version: "1".to_string(),
        types: Vec::new(),
        categories: Vec::new(),
        groups: Vec::new(),
        meta_groups: Vec::new(),
        market_groups: vec![
            ImportMarketGroup {
                market_group_id: 4,
                name: "Ships".to_string(),
                parent_group_id: None,
            },
            ImportMarketGroup {
                market_group_id: 1_361,
                name: "Frigates".to_string(),
                parent_group_id: Some(4),
            },
        ],
        blueprints: Vec::new(),
        solar_systems: vec![
            iskworks_sde::ImportSolarSystem {
                solar_system_id: 30_000_142,
                name: "Jita".to_string(),
                constellation_id: None,
                region_id: Some(10_000_002),
                security_status: Some(rust_decimal::Decimal::new(9, 1)),
                wormhole_class_id: None,
            },
            iskworks_sde::ImportSolarSystem {
                solar_system_id: 30_000_772,
                name: "C-J6MT".to_string(),
                constellation_id: None,
                region_id: Some(10_000_009),
                security_status: Some(rust_decimal::Decimal::new(-2, 1)),
                wormhole_class_id: None,
            },
            // A system whose only station's name does NOT contain the
            // system's own name -- proves search_npc_stations matches on
            // system name too, not just by station-name-prefix luck (most
            // real stations happen to start with their system's name, but
            // nothing guarantees it, especially for structures).
            iskworks_sde::ImportSolarSystem {
                solar_system_id: 30_000_852,
                name: "Botane".to_string(),
                constellation_id: None,
                region_id: Some(10_000_048),
                security_status: Some(rust_decimal::Decimal::new(8, 1)),
                wormhole_class_id: None,
            },
        ],
        constellations: Vec::new(),
        regions: vec![
            iskworks_sde::ImportRegion {
                region_id: 10_000_002,
                name: "The Forge".to_string(),
            },
            iskworks_sde::ImportRegion {
                region_id: 10_000_009,
                name: "Insmother".to_string(),
            },
            iskworks_sde::ImportRegion {
                region_id: 10_000_048,
                name: "Placid".to_string(),
            },
        ],
        npc_stations: vec![
            iskworks_sde::ImportNpcStation {
                station_id: 60_003_760,
                name: "Jita IV - Moon 4 - Caldari Navy Assembly Plant".to_string(),
                solar_system_id: 30_000_142,
                owner_corporation_id: 1_000_035,
                station_type_id: 1_529,
            },
            iskworks_sde::ImportNpcStation {
                station_id: 60_000_001,
                name: "C-J6MT I - Test Outpost".to_string(),
                solar_system_id: 30_000_772,
                owner_corporation_id: 1_000_001,
                station_type_id: 1_928,
            },
            iskworks_sde::ImportNpcStation {
                station_id: 60_000_002,
                name: "Origin Trade Post".to_string(),
                solar_system_id: 30_000_852,
                owner_corporation_id: 1_000_002,
                station_type_id: 1_529,
            },
        ],
        structure_modifiers: Vec::new(),
        rig_modifiers: Vec::new(),
        reaction_formulas: Vec::new(),
        reaction_rig_modifiers: Vec::new(),
        industry_target_filters: Vec::new(),
        planet_schematics: Vec::new(),
        planets: Vec::new(),
        skipped_blueprints: 0,
        skipped_reaction_formulas: 0,
    }
}

/// Regression fixture for the `wormhole_class_id IS NOT NULL != "in
/// Anoikis"` bug: mixes an ordinary highsec system (Jita, no
/// `wormhole_class_id`), an ordinary lowsec system (no `wormhole_class_id`),
/// a real k-space lowsec system that carries EVE's "shattered wormhole"
/// *environmental effect* flag (Erstet, `wormhole_class_id = 8`,
/// `security_status = 0.4494` -- verified live against the dev DB's
/// imported SDE: `solar_system_id 30_003_425`, region 10_000_042
/// Metropolis), and a real Anoikis system identified only by region
/// membership (Sentinel MZ, region 11_000_033, `wormhole_class_id = 14`,
/// also verified live). Erstet is the important row: the old
/// `wormhole_class_id IS NOT NULL` rule would have called it "wormhole" --
/// it must classify as "lowSec".
fn security_classification_fixture() -> iskworks_sde::NormalizedSde {
    iskworks_sde::NormalizedSde {
        source_label: "security-classification-fixture.zip".to_string(),
        source_checksum: "security-classification-fixture-checksum".to_string(),
        source_version: "1".to_string(),
        types: Vec::new(),
        categories: Vec::new(),
        groups: Vec::new(),
        meta_groups: Vec::new(),
        market_groups: Vec::new(),
        blueprints: Vec::new(),
        solar_systems: vec![
            iskworks_sde::ImportSolarSystem {
                solar_system_id: 30_000_142,
                name: "Jita".to_string(),
                constellation_id: None,
                region_id: Some(10_000_002),
                security_status: Some(rust_decimal::Decimal::new(9, 1)),
                wormhole_class_id: None,
            },
            iskworks_sde::ImportSolarSystem {
                solar_system_id: 30_090_001,
                name: "Fixture Lowsec System".to_string(),
                constellation_id: None,
                region_id: Some(10_000_990),
                security_status: Some(rust_decimal::Decimal::new(3, 1)),
                wormhole_class_id: None,
            },
            iskworks_sde::ImportSolarSystem {
                solar_system_id: 30_003_425,
                name: "Erstet".to_string(),
                constellation_id: None,
                region_id: Some(10_000_042),
                security_status: Some(rust_decimal::Decimal::new(4_494, 4)),
                wormhole_class_id: Some(8),
            },
            iskworks_sde::ImportSolarSystem {
                solar_system_id: 31_000_001,
                name: "Sentinel MZ".to_string(),
                constellation_id: None,
                region_id: Some(11_000_033),
                security_status: Some(rust_decimal::Decimal::new(-99, 2)),
                wormhole_class_id: Some(14),
            },
        ],
        constellations: Vec::new(),
        regions: vec![
            iskworks_sde::ImportRegion {
                region_id: 10_000_002,
                name: "The Forge".to_string(),
            },
            iskworks_sde::ImportRegion {
                region_id: 10_000_990,
                name: "Fixture Region".to_string(),
            },
            iskworks_sde::ImportRegion {
                region_id: 10_000_042,
                name: "Metropolis".to_string(),
            },
            iskworks_sde::ImportRegion {
                region_id: 11_000_033,
                name: "K-R00033".to_string(),
            },
        ],
        npc_stations: vec![
            iskworks_sde::ImportNpcStation {
                station_id: 60_003_760,
                name: "Jita IV - Moon 4 - Caldari Navy Assembly Plant".to_string(),
                solar_system_id: 30_000_142,
                owner_corporation_id: 1_000_035,
                station_type_id: 1_529,
            },
            iskworks_sde::ImportNpcStation {
                station_id: 60_090_001,
                name: "Erstet Fixture Station".to_string(),
                solar_system_id: 30_003_425,
                owner_corporation_id: 1_000_001,
                station_type_id: 1_529,
            },
        ],
        structure_modifiers: Vec::new(),
        rig_modifiers: Vec::new(),
        reaction_formulas: Vec::new(),
        reaction_rig_modifiers: Vec::new(),
        industry_target_filters: Vec::new(),
        planet_schematics: Vec::new(),
        planets: Vec::new(),
        skipped_blueprints: 0,
        skipped_reaction_formulas: 0,
    }
}

/// The regression test for the security-classification bug found while
/// planning the Market Scope Selector: `wormhole_class_id IS NOT NULL` is
/// neither necessary nor sufficient for "this system is in wormhole/Anoikis
/// space" (see `SECURITY_CLASS_CASE_SQL`'s doc comment in `sde_read.rs`).
/// Covers every production entry point that shares the corrected CASE
/// expression: `search_solar_systems`, `search_npc_stations`, and
/// `list_npc_stations_in_region`.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn security_class_is_not_derived_from_the_nullable_wormhole_class_id_column(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    let dataset = security_classification_fixture();
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

    // Jita: ordinary highsec, no wormhole_class_id -- must stay correct.
    let jita = repository.search_solar_systems("Jita", 10).await.unwrap();
    assert_eq!(jita.len(), 1);
    assert_eq!(jita[0].security_class, "highSec");

    // An ordinary lowsec system with no wormhole_class_id -- the plain
    // security_status branches must stay correct too.
    let lowsec = repository
        .search_solar_systems("Fixture Lowsec", 10)
        .await
        .unwrap();
    assert_eq!(lowsec.len(), 1);
    assert_eq!(lowsec[0].security_class, "lowSec");

    // Erstet: a real k-space lowsec system carrying wormhole_class_id=8
    // (EVE's "shattered wormhole" environmental effect, not Anoikis
    // membership). This is the exact shape of the original bug -- the old
    // `wormhole_class_id IS NOT NULL` rule would have returned "wormhole"
    // here. It must return "lowSec".
    let erstet = repository.search_solar_systems("Erstet", 10).await.unwrap();
    assert_eq!(erstet.len(), 1);
    assert_eq!(erstet[0].security_class, "lowSec");

    // Sentinel MZ: a real Anoikis system (region 11_000_033) whose
    // wormhole_class_id (14) is non-null here, but the classification must
    // come from region membership, not this column -- proven by the two
    // cases above already showing the column alone is neither necessary
    // nor sufficient.
    let anoikis = repository
        .search_solar_systems("Sentinel MZ", 10)
        .await
        .unwrap();
    assert_eq!(anoikis.len(), 1);
    assert_eq!(anoikis[0].security_class, "wormhole");

    // Same corrected semantics via search_npc_stations...
    let stations = repository
        .search_npc_stations("Erstet Fixture", 10)
        .await
        .unwrap();
    assert_eq!(stations.len(), 1);
    assert_eq!(stations[0].security_class, "lowSec");

    // ...and via list_npc_stations_in_region.
    let region_locations = repository
        .list_npc_stations_in_region(10_000_042)
        .await
        .unwrap();
    assert_eq!(region_locations.len(), 1);
    assert_eq!(region_locations[0].station_id, 60_090_001);
    assert_eq!(region_locations[0].security_class, "lowSec");
}
