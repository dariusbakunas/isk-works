use super::*;
use iskworks_sde::{
    ImportPlanet, ImportPlanetCommodity, ImportPlanetPinType, ImportPlanetSchematic,
    ImportPlanetSchematicType, ImportRegion, ImportSolarSystem, ImportStargate, PlanetPinKind,
};
use rust_decimal::Decimal;

fn pi_type(type_id: i64, name: &str) -> ImportType {
    ImportType {
        type_id,
        name: name.to_string(),
        group_id: None,
        group_name: None,
        market_group_id: None,
        meta_group_id: None,
        packaged_volume_m3: None,
        published: true,
    }
}

fn system(solar_system_id: i64, name: &str, security: Decimal) -> ImportSolarSystem {
    ImportSolarSystem {
        solar_system_id,
        name: name.to_string(),
        constellation_id: None,
        region_id: Some(10_000_001),
        security_status: Some(security),
        wormhole_class_id: None,
    }
}

fn gates(pairs: &[(i64, i64)]) -> Vec<ImportStargate> {
    pairs
        .iter()
        .flat_map(|&(from, to)| [(from, to), (to, from)])
        .enumerate()
        .map(|(index, (from, to))| ImportStargate {
            stargate_id: 50_000_000 + index as i64,
            solar_system_id: from,
            destination_solar_system_id: to,
        })
        .collect()
}

fn pin(type_id: i64, kind: PlanetPinKind) -> ImportPlanetPinType {
    ImportPlanetPinType {
        type_id,
        kind,
        planet_type_id: Some(2016),
        harvester_type_id: None,
        cpu_load: None,
        power_load: None,
        cpu_output: None,
        power_output: None,
        ecu_head_cpu: None,
        ecu_head_power: None,
        cpu_load_per_km: None,
        power_load_per_km: None,
        cpu_load_level_modifier: None,
        power_load_level_modifier: None,
    }
}

/// Alpha–Bravo–Charlie form a loop, Delta hangs off Charlie, and the
/// wormhole system has no gates.
fn pi_planner_dataset(checksum: &str) -> NormalizedSde {
    let mut dataset = fixture_sde(checksum, "100010");
    // The base fixture's stations sit in systems this dataset replaces.
    dataset.npc_stations.clear();
    dataset.types.extend([
        pi_type(2267, "Base Metals"),
        pi_type(2268, "Aqueous Liquids"),
        pi_type(2430, "Barren Base Metals Extractor"),
        pi_type(2473, "Barren Basic Industry Facility"),
        pi_type(2524, "Barren Command Center"),
        pi_type(3645, "Water"),
    ]);
    dataset.regions = vec![ImportRegion {
        region_id: 10_000_001,
        name: "Insmother".into(),
    }];
    dataset.solar_systems = vec![
        system(30_000_001, "Alpha", Decimal::new(-58, 2)),
        system(30_000_002, "Bravo", Decimal::new(-3, 1)),
        system(30_000_003, "Charlie", Decimal::new(-6, 1)),
        system(30_000_004, "Delta", Decimal::new(-1, 1)),
        ImportSolarSystem {
            region_id: None,
            wormhole_class_id: Some(3),
            ..system(31_000_001, "J100001", Decimal::new(-1, 0))
        },
    ];
    dataset.stargates = gates(&[
        (30_000_001, 30_000_002),
        (30_000_002, 30_000_003),
        (30_000_003, 30_000_001),
        (30_000_003, 30_000_004),
    ]);
    let planet =
        |planet_id: i64, system: i64, index: i64, name: &str, type_id: Option<i64>| ImportPlanet {
            planet_id,
            solar_system_id: system,
            celestial_index: index,
            name: name.into(),
            type_id,
            radius_m: type_id.map(|_| 5_060_000),
        };
    dataset.planets = vec![
        planet(40_000_011, 30_000_001, 1, "Alpha I", Some(2016)),
        planet(40_000_012, 30_000_001, 2, "Alpha II", Some(13)),
        planet(40_000_021, 30_000_002, 1, "Bravo I", None),
        planet(40_000_041, 30_000_004, 1, "Delta I", Some(2016)),
    ];
    dataset.planet_schematics = vec![ImportPlanetSchematic {
        schematic_id: 121,
        name: "Water".into(),
        cycle_time_seconds: 1_800,
        types: vec![
            ImportPlanetSchematicType {
                type_id: 2268,
                is_input: true,
                quantity: 3_000,
            },
            ImportPlanetSchematicType {
                type_id: 3645,
                is_input: false,
                quantity: 20,
            },
        ],
        pin_type_ids: vec![2473],
    }];
    dataset.planet_pin_types = vec![
        ImportPlanetPinType {
            harvester_type_id: Some(2267),
            cpu_load: Some(Decimal::from(200)),
            power_load: Some(Decimal::from(800)),
            ..pin(2430, PlanetPinKind::ExtractorResource)
        },
        ImportPlanetPinType {
            cpu_load: Some(Decimal::from(200)),
            power_load: Some(Decimal::from(800)),
            ..pin(2473, PlanetPinKind::Processor)
        },
        ImportPlanetPinType {
            cpu_output: Some(Decimal::from(1_675)),
            power_output: Some(Decimal::from(6_000)),
            ..pin(2524, PlanetPinKind::CommandCenter)
        },
    ];
    dataset.planet_commodities = vec![ImportPlanetCommodity {
        type_id: 3645,
        export_tax_multiplier: Decimal::from(400),
        import_tax_multiplier: Decimal::from(400),
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
async fn pi_planner_data_is_stored_and_an_import_without_it_is_redone(pool: PgPool) {
    let repository = PgSdeRepository::new(pool.clone());
    let requires = SdeDatasetRequirements {
        pi_planner_data: true,
        ..SdeDatasetRequirements::default()
    };
    // The same archive imported before the planner data was read is redone.
    let mut older = pi_planner_dataset("checksum-pi-planner");
    older.planet_pin_types.clear();
    older.planet_commodities.clear();
    for planet in &mut older.planets {
        planet.type_id = None;
        planet.radius_m = None;
    }
    import_active(&repository, &older).await;
    assert!(repository
        .active_by_checksum("checksum-pi-planner", requires)
        .await
        .unwrap()
        .is_none());

    import_active(&repository, &pi_planner_dataset("checksum-pi-planner")).await;
    assert!(repository
        .active_by_checksum("checksum-pi-planner", requires)
        .await
        .unwrap()
        .is_some());

    let planets: Vec<(i64, Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT p.planet_id, p.type_id, p.radius_m
         FROM sde_planets p JOIN sde_imports i ON i.id = p.import_id AND i.active
         ORDER BY p.planet_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(planets[0], (40_000_011, Some(2016), Some(5_060_000)));
    assert_eq!(planets[2], (40_000_021, None, None));

    let pins: Vec<(i64, String, Option<i64>, Option<Decimal>)> = sqlx::query_as(
        "SELECT p.type_id, p.kind, p.harvester_type_id, p.cpu_output
         FROM sde_planet_pin_types p JOIN sde_imports i ON i.id = p.import_id AND i.active
         ORDER BY p.type_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        pins,
        vec![
            (2430, "extractor_resource".into(), Some(2267), None),
            (2473, "processor".into(), None, None),
            (
                2524,
                "command_center".into(),
                None,
                Some(Decimal::from(1_675))
            ),
        ]
    );

    let schematic_pins: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT p.schematic_id, p.pin_type_id
         FROM sde_planet_schematic_pins p JOIN sde_imports i ON i.id = p.import_id AND i.active",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(schematic_pins, vec![(121, 2473)]);

    let commodities: Vec<(i64, Decimal, Decimal)> = sqlx::query_as(
        "SELECT c.type_id, c.export_tax_multiplier, c.import_tax_multiplier
         FROM sde_planet_commodities c JOIN sde_imports i ON i.id = c.import_id AND i.active",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        commodities,
        vec![(3645, Decimal::from(400), Decimal::from(400))]
    );
}
