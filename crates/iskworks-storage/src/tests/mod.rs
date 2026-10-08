use super::*;
use chrono::Duration;
use iskworks_core::CreateWorkspaceCommand;
use iskworks_core::WorkspaceService;
use iskworks_core::{EveIdentity, SessionRepository, UserRepository};
use iskworks_sde::{
    ImportBlueprint, ImportCategory, ImportGroup, ImportMarketGroup, ImportMaterial,
    ImportMetaGroup, ImportProduct, ImportType, NoopProgressReporter, SdeDatasetRequirements,
};

mod auth_sessions;
mod sde_geography;
mod sde_market_catalog;
mod sde_recipes;
mod workspace;

// ---------------------------------------------------------------------------
// Full erase of a user's workspace.
// ---------------------------------------------------------------------------

mod erase;

// ---------------------------------------------------------------------------
// Invite-only onboarding.
// ---------------------------------------------------------------------------

mod invite_codes;

// ---------------------------------------------------------------------------
// Shared SDE datasets
// ---------------------------------------------------------------------------

fn fixture_sde(checksum: &str, version: &str) -> NormalizedSde {
    NormalizedSde {
        categories: vec![ImportCategory {
            category_id: 6,
            name: "Ship".into(),
            published: true,
        }],
        groups: vec![ImportGroup {
            group_id: 25,
            name: "Frigate".into(),
            category_id: Some(6),
            published: true,
        }],
        meta_groups: vec![ImportMetaGroup {
            meta_group_id: 1,
            name: "Tech I".into(),
        }],
        market_groups: vec![
            ImportMarketGroup {
                market_group_id: 4,
                name: "Ships".into(),
                parent_group_id: None,
            },
            ImportMarketGroup {
                market_group_id: 1361,
                name: "Frigates".into(),
                parent_group_id: Some(4),
            },
        ],
        source_label: "fixture.zip".to_string(),
        source_checksum: checksum.to_string(),
        source_version: version.to_string(),
        types: vec![
            ImportType {
                type_id: 34,
                name: "Tritanium".to_string(),
                group_id: Some(18),
                group_name: Some("Mineral".to_string()),
                market_group_id: Some(1857),
                meta_group_id: None,
                packaged_volume_m3: Some(rust_decimal::Decimal::new(1, 2)),
                published: true,
            },
            ImportType {
                type_id: 5_876,
                name: "Rifter".to_string(),
                group_id: Some(25),
                group_name: Some("Frigate".to_string()),
                market_group_id: Some(1361),
                meta_group_id: Some(1),
                packaged_volume_m3: None,
                published: true,
            },
            ImportType {
                type_id: 6_830,
                name: "Rifter Blueprint".to_string(),
                group_id: Some(105),
                group_name: Some("Frigate Blueprint".to_string()),
                market_group_id: Some(204),
                meta_group_id: None,
                packaged_volume_m3: None,
                published: true,
            },
            ImportType {
                type_id: 1_928,
                name: "Amarr Station".to_string(),
                group_id: Some(15),
                group_name: Some("Station".to_string()),
                market_group_id: None,
                meta_group_id: None,
                packaged_volume_m3: None,
                published: false,
            },
            ImportType {
                type_id: 35_825,
                name: "Raitaru".to_string(),
                group_id: Some(1404),
                group_name: Some("Engineering Complex".to_string()),
                market_group_id: None,
                meta_group_id: None,
                packaged_volume_m3: None,
                published: true,
            },
            ImportType {
                type_id: 36_971,
                name: "Raitaru Blueprint".to_string(),
                group_id: Some(1462),
                group_name: Some("Structure Blueprints".to_string()),
                market_group_id: None,
                meta_group_id: None,
                packaged_volume_m3: None,
                published: true,
            },
            ImportType {
                type_id: 35_827,
                name: "Sotiyo".to_string(),
                group_id: Some(1404),
                group_name: Some("Engineering Complex".to_string()),
                market_group_id: None,
                meta_group_id: None,
                packaged_volume_m3: None,
                published: true,
            },
            ImportType {
                type_id: 43_800,
                name: "Standup M-Set Basic Small Ship Manufacturing Material Efficiency I"
                    .to_string(),
                group_id: Some(1824),
                group_name: Some("Structure Engineering Rig M - Basic Small Ship ME".to_string()),
                market_group_id: None,
                meta_group_id: None,
                packaged_volume_m3: None,
                published: true,
            },
            ImportType {
                type_id: 43_801,
                name:
                    "Standup M-Set Basic Small Ship Manufacturing Material Efficiency I Blueprint"
                        .to_string(),
                group_id: Some(1708),
                group_name: Some("Structure Rig Blueprint".to_string()),
                market_group_id: None,
                meta_group_id: None,
                packaged_volume_m3: None,
                published: true,
            },
            ImportType {
                type_id: 46_157,
                name: "Methanofullerene Reaction Formula".to_string(),
                group_id: Some(1889),
                group_name: Some("Polymer Reaction Formulas".to_string()),
                market_group_id: None,
                meta_group_id: None,
                packaged_volume_m3: None,
                published: true,
            },
            ImportType {
                type_id: 46_486,
                name: "Standup M-Set Composite Reactor Material Efficiency I".to_string(),
                group_id: Some(1934),
                group_name: Some("Structure Composite Reactor Rig M - ME".to_string()),
                market_group_id: None,
                meta_group_id: None,
                packaged_volume_m3: None,
                published: true,
            },
        ],
        blueprints: vec![ImportBlueprint {
            blueprint_type_id: 6_830,
            name: "Rifter Blueprint".to_string(),
            duration_seconds: Some(600),
            materials: vec![ImportMaterial {
                type_id: 34,
                quantity: 1_000,
                position: 0,
            }],
            products: vec![ImportProduct {
                type_id: 5_876,
                quantity: 1,
                position: 0,
            }],
        }],
        solar_systems: vec![iskworks_sde::ImportSolarSystem {
            solar_system_id: 30_000_772,
            name: "C-J6MT".to_string(),
            constellation_id: None,
            region_id: Some(10_000_009),
            security_status: Some(rust_decimal::Decimal::new(-2, 1)),
            wormhole_class_id: None,
        }],
        constellations: Vec::new(),
        regions: vec![iskworks_sde::ImportRegion {
            region_id: 10_000_009,
            name: "Insmother".to_string(),
        }],
        npc_stations: vec![iskworks_sde::ImportNpcStation {
            station_id: 60_000_001,
            name: "C-J6MT I - Imperial Shipment Assembly Plant".to_string(),
            solar_system_id: 30_000_772,
            owner_corporation_id: 1_000_001,
            station_type_id: 1_928,
        }],
        structure_modifiers: vec![
            iskworks_sde::ImportStructureModifiers {
                type_id: 35_825,
                material_reduction_percent: rust_decimal::Decimal::ONE,
                time_reduction_percent: rust_decimal::Decimal::new(15, 0),
                job_cost_reduction_percent: rust_decimal::Decimal::new(3, 0),
                structure_size: Some(2),
            },
            iskworks_sde::ImportStructureModifiers {
                type_id: 35_827,
                material_reduction_percent: rust_decimal::Decimal::ONE,
                time_reduction_percent: rust_decimal::Decimal::new(30, 0),
                job_cost_reduction_percent: rust_decimal::Decimal::new(5, 0),
                structure_size: Some(4),
            },
        ],
        rig_modifiers: vec![iskworks_sde::ImportRigModifiers {
            type_id: 43_800,
            material_reduction_percent: rust_decimal::Decimal::new(2, 0),
            time_reduction_percent: rust_decimal::Decimal::new(24, 0),
            high_sec_multiplier: rust_decimal::Decimal::ONE,
            low_sec_multiplier: rust_decimal::Decimal::new(19, 1),
            null_sec_multiplier: rust_decimal::Decimal::new(21, 1),
            compatible_structure_group_ids: vec![1404],
            rig_size: Some(2),
            material_filter_ids: vec![3],
            time_filter_ids: vec![3],
        }],
        reaction_formulas: vec![iskworks_sde::ImportReactionFormula {
            reaction_formula_type_id: 46_157,
            name: "Methanofullerene Reaction Formula".to_string(),
            duration_seconds: Some(10_800),
            materials: vec![ImportMaterial {
                type_id: 34,
                quantity: 100,
                position: 0,
            }],
            products: vec![ImportProduct {
                type_id: 5_876,
                quantity: 160,
                position: 0,
            }],
        }],
        reaction_rig_modifiers: vec![iskworks_sde::ImportRigModifiers {
            type_id: 46_486,
            material_reduction_percent: rust_decimal::Decimal::new(2, 0),
            time_reduction_percent: rust_decimal::Decimal::ZERO,
            high_sec_multiplier: rust_decimal::Decimal::ONE,
            low_sec_multiplier: rust_decimal::Decimal::ONE,
            null_sec_multiplier: rust_decimal::Decimal::new(11, 1),
            compatible_structure_group_ids: vec![1406],
            rig_size: Some(2),
            material_filter_ids: vec![18],
            time_filter_ids: vec![],
        }],
        industry_target_filters: vec![
            iskworks_sde::ImportIndustryTargetFilter {
                filter_id: 3,
                name: "Ships".into(),
                category_ids: vec![6, 32],
                group_ids: vec![],
            },
            iskworks_sde::ImportIndustryTargetFilter {
                filter_id: 18,
                name: "Composite Reactions".into(),
                category_ids: vec![],
                group_ids: vec![428, 429, 4932],
            },
        ],
        planet_schematics: Vec::new(),
        planets: Vec::new(),
        skipped_blueprints: 0,
        skipped_reaction_formulas: 0,
    }
}

fn strict_opportunity_fixture() -> NormalizedSde {
    let mut dataset = fixture_sde("strict-opportunity-scopes", "2026.08");
    dataset.categories.extend([
        ImportCategory {
            category_id: 7,
            name: "Module".into(),
            published: true,
        },
        ImportCategory {
            category_id: 4,
            name: "Material".into(),
            published: true,
        },
    ]);
    dataset.market_groups.extend([
        ImportMarketGroup {
            market_group_id: 1_612,
            name: "Special Edition Ships".into(),
            parent_group_id: Some(4),
        },
        ImportMarketGroup {
            market_group_id: 1_619,
            name: "Special Edition Frigates".into(),
            parent_group_id: Some(1_612),
        },
        ImportMarketGroup {
            // Matches `RIGS_MARKET_GROUP_ID` in `opportunity.rs` exactly --
            // the T1 Rigs scope filters on this real id, not a fixture-local
            // stand-in, so this test actually exercises production code.
            market_group_id: 1_111,
            name: "Rigs".into(),
            parent_group_id: None,
        },
        ImportMarketGroup {
            market_group_id: 1_206,
            name: "Small Armor Rigs".into(),
            parent_group_id: Some(1_111),
        },
    ]);
    dataset.groups.extend([
        ImportGroup {
            group_id: 27,
            name: "Battleship".into(),
            category_id: Some(6),
            published: true,
        },
        ImportGroup {
            group_id: 324,
            name: "Assault Frigate".into(),
            category_id: Some(6),
            published: true,
        },
        ImportGroup {
            group_id: 900,
            name: "Rig Armor".into(),
            category_id: Some(7),
            published: true,
        },
        ImportGroup {
            group_id: 429,
            name: "Composite".into(),
            category_id: Some(4),
            published: true,
        },
    ]);
    dataset.meta_groups.extend([
        ImportMetaGroup {
            meta_group_id: 2,
            name: "Tech II".into(),
        },
        ImportMetaGroup {
            meta_group_id: 4,
            name: "Faction".into(),
        },
    ]);
    let products = [
        (11_174, "Kestrel Navy Issue", 25, 4, true),
        (11_394, "Wolf", 324, 2, true),
        (58_701, "Unpublished Frigate", 25, 1, false),
        (24_605, "Rokh", 27, 1, true),
        (77_114, "Metamorphosis", 25, 1, true),
    ];
    for (position, (type_id, name, group_id, meta_group_id, published)) in
        products.into_iter().enumerate()
    {
        let blueprint_type_id = 90_000 + i64::try_from(position).unwrap();
        dataset.types.push(ImportType {
            type_id,
            name: name.to_string(),
            group_id: Some(group_id),
            group_name: None,
            market_group_id: (type_id == 77_114).then_some(1_619),
            meta_group_id: Some(meta_group_id),
            packaged_volume_m3: None,
            published,
        });
        dataset.types.push(ImportType {
            type_id: blueprint_type_id,
            name: format!("{name} Blueprint"),
            group_id: Some(105),
            group_name: Some("Ship Blueprint".to_string()),
            market_group_id: None,
            meta_group_id: None,
            packaged_volume_m3: None,
            published: true,
        });
        dataset.blueprints.push(ImportBlueprint {
            blueprint_type_id,
            name: format!("{name} Blueprint"),
            duration_seconds: Some(600),
            materials: vec![ImportMaterial {
                type_id: 34,
                quantity: 1_000,
                position: 0,
            }],
            products: vec![ImportProduct {
                type_id,
                quantity: 1,
                position: 0,
            }],
        });
    }

    // A Tech I rig and its Tech II counterpart, both in the same SDE group
    // (900) but only the T1 one under the "Rigs" market-group root (5000) --
    // proves `market_group_root_ids` filtering against real recursive SQL,
    // not just the in-memory classification check.
    dataset.types.extend([
        ImportType {
            type_id: 30_987,
            name: "Small Trimark Armor Pump I".to_string(),
            group_id: Some(900),
            group_name: None,
            market_group_id: Some(1_206),
            meta_group_id: Some(1),
            packaged_volume_m3: None,
            published: true,
        },
        ImportType {
            type_id: 30_988,
            name: "Small Trimark Armor Pump II".to_string(),
            group_id: Some(900),
            group_name: None,
            market_group_id: Some(1_206),
            meta_group_id: Some(2),
            packaged_volume_m3: None,
            published: true,
        },
        ImportType {
            type_id: 90_500,
            name: "Small Trimark Armor Pump I Blueprint".to_string(),
            group_id: Some(105),
            group_name: Some("Ship Blueprint".to_string()),
            market_group_id: None,
            meta_group_id: None,
            packaged_volume_m3: None,
            published: true,
        },
        ImportType {
            type_id: 90_501,
            name: "Small Trimark Armor Pump II Blueprint".to_string(),
            group_id: Some(105),
            group_name: Some("Ship Blueprint".to_string()),
            market_group_id: None,
            meta_group_id: None,
            packaged_volume_m3: None,
            published: true,
        },
    ]);
    dataset.blueprints.extend([
        ImportBlueprint {
            blueprint_type_id: 90_500,
            name: "Small Trimark Armor Pump I Blueprint".to_string(),
            duration_seconds: Some(1_200),
            materials: vec![ImportMaterial {
                type_id: 34,
                quantity: 200,
                position: 0,
            }],
            products: vec![ImportProduct {
                type_id: 30_987,
                quantity: 1,
                position: 0,
            }],
        },
        ImportBlueprint {
            blueprint_type_id: 90_501,
            name: "Small Trimark Armor Pump II Blueprint".to_string(),
            duration_seconds: Some(1_200),
            materials: vec![ImportMaterial {
                type_id: 34,
                quantity: 200,
                position: 0,
            }],
            products: vec![ImportProduct {
                type_id: 30_988,
                quantity: 1,
                position: 0,
            }],
        },
    ]);

    // A reaction formula: product carries Category "Material" and, matching
    // the real SDE, no meta_group_id at all -- proves the Reactions scope's
    // deliberately unrestricted meta-group filter against real SQL, and
    // that `recipe_kinds: {Reaction}` actually reaches the storage layer's
    // manufacturing/reaction UNION.
    dataset.types.extend([
        ImportType {
            type_id: 16_656,
            name: "Fernite Alloy".to_string(),
            group_id: Some(429),
            group_name: None,
            market_group_id: None,
            meta_group_id: None,
            packaged_volume_m3: None,
            published: true,
        },
        ImportType {
            type_id: 46_171,
            name: "Fernite Alloy Reaction Formula".to_string(),
            group_id: Some(105),
            group_name: Some("Ship Blueprint".to_string()),
            market_group_id: None,
            meta_group_id: None,
            packaged_volume_m3: None,
            published: true,
        },
    ]);
    dataset
        .reaction_formulas
        .push(iskworks_sde::ImportReactionFormula {
            reaction_formula_type_id: 46_171,
            name: "Fernite Alloy Reaction Formula".to_string(),
            duration_seconds: Some(3_600),
            materials: vec![ImportMaterial {
                type_id: 34,
                quantity: 10,
                position: 0,
            }],
            products: vec![ImportProduct {
                type_id: 16_656,
                quantity: 100,
                position: 0,
            }],
        });

    dataset
}
