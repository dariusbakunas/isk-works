use super::import::*;
use super::*;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;

#[test]
fn parses_only_manufacturing_and_keeps_referenced_unpublished_types() {
    let source = fixture_source(blueprints_fixture()).unwrap();
    let dataset = parse(&source).unwrap();

    assert_eq!(dataset.blueprints.len(), 1);
    assert_eq!(dataset.blueprints[0].blueprint_type_id, 6830);
    assert_eq!(dataset.blueprints[0].materials[0].type_id, 34);
    assert!(dataset.types.iter().any(|item| item.type_id == 34));
    assert!(dataset.types.iter().any(|item| item.type_id == 6830));
    assert!(!dataset.types.iter().any(|item| item.type_id == 9999));
    assert_eq!(dataset.source_version, "123456");
    assert!(dataset.reaction_formulas.is_empty());
}

#[test]
fn packaged_volume_takes_precedence_over_regular_volume() {
    let dataset = parse(&fixture_source(blueprints_fixture()).unwrap()).unwrap();
    let rifter = dataset
        .types
        .iter()
        .find(|item| item.type_id == 5876)
        .unwrap();

    assert_eq!(rifter.packaged_volume_m3, Some(Decimal::new(2500, 1)));
}

#[test]
fn packaged_volume_falls_back_to_regular_volume() {
    let dataset = parse(&fixture_source(blueprints_fixture()).unwrap()).unwrap();
    let tritanium = dataset
        .types
        .iter()
        .find(|item| item.type_id == 34)
        .unwrap();

    assert_eq!(tritanium.packaged_volume_m3, Some(Decimal::new(1, 2)));
}

#[test]
fn packaged_volume_is_none_when_sde_has_no_volume() {
    let dataset = parse(&fixture_source(blueprints_fixture()).unwrap()).unwrap();
    let blueprint = dataset
        .types
        .iter()
        .find(|item| item.type_id == 6830)
        .unwrap();

    assert_eq!(blueprint.packaged_volume_m3, None);
}

#[test]
fn parses_reaction_activity_into_reaction_formulas_not_blueprints() {
    let source = fixture_source_with_reaction_type(reaction_blueprint_fixture()).unwrap();
    let dataset = parse(&source).unwrap();

    assert!(dataset.blueprints.is_empty());
    assert_eq!(dataset.reaction_formulas.len(), 1);
    let formula = &dataset.reaction_formulas[0];
    assert_eq!(formula.reaction_formula_type_id, 46157);
    assert_eq!(formula.name, "Methanofullerene Reaction Formula");
    assert_eq!(formula.duration_seconds, Some(10800));
    assert_eq!(
        formula.materials,
        vec![ImportMaterial {
            type_id: 34,
            quantity: 100,
            position: 0
        }]
    );
    assert_eq!(
        formula.products,
        vec![ImportProduct {
            type_id: 5876,
            quantity: 160,
            position: 0
        }]
    );
    assert_eq!(dataset.skipped_reaction_formulas, 0);
}

#[test]
fn skips_reaction_formulas_with_unknown_material_types() {
    let blueprint = r#"{"_key":46157,"activities":{"reaction":{"materials":[{"typeID":404,"quantity":1}],"products":[{"typeID":5876,"quantity":1}]}}}"#;
    let dataset = parse(&fixture_source_with_reaction_type(blueprint).unwrap()).unwrap();
    assert!(dataset.reaction_formulas.is_empty());
    assert_eq!(dataset.skipped_reaction_formulas, 1);
}

#[test]
fn skips_reaction_definitions_without_products() {
    let blueprint =
        r#"{"_key":46157,"activities":{"reaction":{"materials":[{"typeID":34,"quantity":1}]}}}"#;
    let dataset = parse(&fixture_source_with_reaction_type(blueprint).unwrap()).unwrap();
    assert!(dataset.reaction_formulas.is_empty());
    assert_eq!(dataset.skipped_reaction_formulas, 1);
}

#[test]
fn detects_reaction_formulas_by_content_not_a_separate_file() {
    let manufacturing_only = fixture_source(blueprints_fixture()).unwrap();
    assert!(!manufacturing_only.has_reaction_formulas());

    let with_reactions = fixture_source_with_reaction_type(reaction_blueprint_fixture()).unwrap();
    assert!(with_reactions.has_reaction_formulas());
}

#[test]
fn parses_solar_system_names_when_present() {
    let source = fixture_source_with_solar_systems(blueprints_fixture()).unwrap();
    let dataset = parse(&source).unwrap();

    assert_eq!(
        dataset.solar_systems,
        vec![ImportSolarSystem {
            solar_system_id: 30_004_878,
            name: "C-J6MT".to_string(),
            constellation_id: None,
            region_id: None,
            security_status: Some(Decimal::from_str("-0.2").unwrap()),
            wormhole_class_id: None,
        }]
    );
}

#[test]
fn formats_station_orbits_with_eve_roman_numerals() {
    assert_eq!(roman_numeral(4), "IV");
    assert_eq!(roman_numeral(9), "IX");
    assert_eq!(roman_numeral(14), "XIV");
}

#[test]
fn normalizes_structure_and_security_adjusted_rig_inputs() {
    let types = HashMap::from([
        (
            35_827,
            RawType {
                key: 35_827,
                name: LocalizedString {
                    en: Some("Sotiyo".to_string()),
                },
                group_id: Some(1404),
                market_group_id: None,
                meta_group_id: None,
                packaged_volume: None,
                volume: None,
                published: true,
            },
        ),
        (
            37_155,
            RawType {
                key: 37_155,
                name: LocalizedString {
                    en: Some("Standup rig".to_string()),
                },
                group_id: Some(1824),
                market_group_id: None,
                meta_group_id: None,
                packaged_volume: None,
                volume: None,
                published: true,
            },
        ),
    ]);
    let (structures, rigs, reaction_rigs) = normalize_facility_dogma(
        vec![
            RawTypeDogma {
                key: 35_827,
                dogma_attributes: vec![
                    RawDogmaValue {
                        attribute_id: 2600,
                        value: Decimal::new(99, 2),
                    },
                    RawDogmaValue {
                        attribute_id: 2601,
                        value: Decimal::new(95, 2),
                    },
                    RawDogmaValue {
                        attribute_id: 2602,
                        value: Decimal::new(70, 2),
                    },
                    RawDogmaValue {
                        attribute_id: 1547,
                        value: Decimal::new(4, 0),
                    },
                ],
            },
            RawTypeDogma {
                key: 37_155,
                dogma_attributes: vec![
                    RawDogmaValue {
                        attribute_id: 2593,
                        value: Decimal::new(-24, 0),
                    },
                    RawDogmaValue {
                        attribute_id: 2594,
                        value: Decimal::new(-24, 1),
                    },
                    RawDogmaValue {
                        attribute_id: 2357,
                        value: Decimal::new(21, 1),
                    },
                    RawDogmaValue {
                        attribute_id: 1299,
                        value: Decimal::new(1404, 0),
                    },
                    RawDogmaValue {
                        attribute_id: 1547,
                        value: Decimal::new(4, 0),
                    },
                ],
            },
        ],
        &types,
        &HashMap::from([(
            37_155,
            RigFilterIds {
                manufacturing_material: vec![5],
                manufacturing_time: vec![5],
                ..RigFilterIds::default()
            },
        )]),
    );

    assert_eq!(structures[0].material_reduction_percent, Decimal::ONE);
    assert_eq!(structures[0].job_cost_reduction_percent, Decimal::new(5, 0));
    assert_eq!(structures[0].time_reduction_percent, Decimal::new(30, 0));
    assert_eq!(structures[0].structure_size, Some(4));
    assert_eq!(rigs[0].material_reduction_percent, Decimal::new(24, 1));
    assert_eq!(rigs[0].time_reduction_percent, Decimal::new(24, 0));
    assert_eq!(rigs[0].null_sec_multiplier, Decimal::new(21, 1));
    assert_eq!(rigs[0].compatible_structure_group_ids, vec![1404]);
    assert_eq!(rigs[0].rig_size, Some(4));
    // filterIDs resolved from the modifier-source map above.
    assert_eq!(rigs[0].material_filter_ids, vec![5]);
    assert_eq!(rigs[0].time_filter_ids, vec![5]);
    assert!(reaction_rigs.is_empty());
}

#[test]
fn normalizes_reaction_rig_inputs_and_skips_structure_level_reaction_bonus() {
    // Real SDE data (verified 2026-08): Athanor/Tatara Refineries (group
    // 1406) carry none of the manufacturing structure-bonus attributes
    // (2600/2601/2602), so they never produce an ImportStructureModifiers
    // entry -- reactions have no base structure discount, only rigs do.
    // Reaction rigs use RefRigTimeBonus (2713) / RefRigMatBonus (2714),
    // not the manufacturing attributeEngRigTimeBonus/MatBonus (2593/2594)
    // -- e.g. real type 46486 "Standup M-Set Composite Reactor Material
    // Efficiency I" has RefRigMatBonus=-2.0 and canFitShipGroup03=1406.
    let types = HashMap::from([
        (
            35_835,
            RawType {
                key: 35_835,
                name: LocalizedString {
                    en: Some("Athanor".to_string()),
                },
                group_id: Some(1406),
                market_group_id: None,
                meta_group_id: None,
                packaged_volume: None,
                volume: None,
                published: true,
            },
        ),
        (
            46_486,
            RawType {
                key: 46_486,
                name: LocalizedString {
                    en: Some("Standup M-Set Composite Reactor Material Efficiency I".to_string()),
                },
                group_id: Some(1934),
                market_group_id: None,
                meta_group_id: None,
                packaged_volume: None,
                volume: None,
                published: true,
            },
        ),
    ]);
    let (structures, rigs, reaction_rigs) = normalize_facility_dogma(
        vec![
            RawTypeDogma {
                key: 35_835,
                dogma_attributes: vec![],
            },
            RawTypeDogma {
                key: 46_486,
                dogma_attributes: vec![
                    RawDogmaValue {
                        attribute_id: 2714,
                        value: Decimal::new(-20, 1),
                    },
                    RawDogmaValue {
                        attribute_id: 2357,
                        value: Decimal::new(11, 1),
                    },
                    RawDogmaValue {
                        attribute_id: 1300,
                        value: Decimal::new(1406, 0),
                    },
                    RawDogmaValue {
                        attribute_id: 1547,
                        value: Decimal::new(2, 0),
                    },
                ],
            },
        ],
        &types,
        &HashMap::from([(
            46_486,
            RigFilterIds {
                reaction_material: vec![18],
                ..RigFilterIds::default()
            },
        )]),
    );

    // Athanor is still captured as a structure (it's in STRUCTURE_GROUP_IDS
    // via group 1406), but with all-zero modifiers since it carries none
    // of the manufacturing bonus attributes -- confirming there's no
    // separate reaction structure bonus to lose by not special-casing it.
    assert_eq!(structures.len(), 1);
    assert_eq!(structures[0].material_reduction_percent, Decimal::ZERO);
    assert_eq!(structures[0].job_cost_reduction_percent, Decimal::ZERO);
    assert_eq!(structures[0].time_reduction_percent, Decimal::ZERO);
    assert!(rigs.is_empty());
    assert_eq!(reaction_rigs.len(), 1);
    assert_eq!(reaction_rigs[0].type_id, 46_486);
    assert_eq!(
        reaction_rigs[0].material_reduction_percent,
        Decimal::new(2, 0)
    );
    assert_eq!(reaction_rigs[0].time_reduction_percent, Decimal::ZERO);
    assert_eq!(reaction_rigs[0].null_sec_multiplier, Decimal::new(11, 1));
    assert_eq!(reaction_rigs[0].compatible_structure_group_ids, vec![1406]);
    assert_eq!(reaction_rigs[0].rig_size, Some(2));
    // Composite-reaction rig: material bonus restricted to filter 18, no
    // separate time filter (this is an ME-only rig).
    assert_eq!(reaction_rigs[0].material_filter_ids, vec![18]);
    assert!(reaction_rigs[0].time_filter_ids.is_empty());
}

#[test]
fn parses_industry_target_filters_and_resolves_rig_filter_ids() {
    let source = fixture_source_inner(
        blueprints_fixture(),
        None,
        None,
        &[
            (
                "industryTargetFilters.jsonl",
                concat!(
                    r#"{"_key":5,"groupIDs":[25,31,420],"name":"Small T1 Ships"}"#,
                    "\n",
                    r#"{"_key":18,"groupIDs":[428,429,4932],"name":"Composite Reactions"}"#,
                ),
            ),
            (
                "industryModifierSources.jsonl",
                concat!(
                    // structure: unrestricted base bonus, no filterID
                    r#"{"_key":35825,"manufacturing":{"material":[{"dogmaAttributeID":2600}]}}"#,
                    "\n",
                    // small-ship manufacturing rig -> filter 5 on both bonuses
                    r#"{"_key":43714,"manufacturing":{"material":[{"dogmaAttributeID":2544,"filterID":5}],"time":[{"dogmaAttributeID":2545,"filterID":5}]}}"#,
                    "\n",
                    // composite-reaction rig -> filter 18, material only (ME-only rig)
                    r#"{"_key":46486,"reaction":{"material":[{"dogmaAttributeID":2716,"filterID":18}]}}"#,
                ),
            ),
            (
                "typeDogma.jsonl",
                concat!(
                    r#"{"_key":43714,"dogmaAttributes":[{"attributeID":2594,"value":-5.04},{"attributeID":2593,"value":-50.4}]}"#,
                    "\n",
                    r#"{"_key":46486,"dogmaAttributes":[{"attributeID":2714,"value":-2.0}]}"#,
                ),
            ),
        ],
    )
    .unwrap();

    let dataset = parse(&source).unwrap();

    assert_eq!(dataset.industry_target_filters.len(), 2);
    assert_eq!(dataset.industry_target_filters[0].filter_id, 5);
    assert_eq!(
        dataset.industry_target_filters[0].group_ids,
        vec![25, 31, 420]
    );

    let ship_rig = dataset
        .rig_modifiers
        .iter()
        .find(|rig| rig.type_id == 43_714)
        .expect("small-ship rig present");
    assert_eq!(ship_rig.material_filter_ids, vec![5]);
    assert_eq!(ship_rig.time_filter_ids, vec![5]);

    let reaction_rig = dataset
        .reaction_rig_modifiers
        .iter()
        .find(|rig| rig.type_id == 46_486)
        .expect("reaction rig present");
    assert_eq!(reaction_rig.material_filter_ids, vec![18]);
    assert!(reaction_rig.time_filter_ids.is_empty());
}

/// Regression: EVE's `industryModifierSources` lists one entry per covered
/// `filterID` for a multi-scope rig. Real data -- type 46496 "Standup L-Set
/// Reactor Efficiency I" references reaction filters 16, 18 and 17 (Hybrid +
/// Composite + Biochemical = every reaction) on both its material and time
/// bonus, and type 43704 "Standup XL-Set Structure and Component
/// Manufacturing Efficiency I" references manufacturing filters 14, 13, 12
/// and 15. The importer must keep every id, not just the first.
#[test]
fn multi_scope_rig_keeps_every_referenced_filter_id() {
    let source = fixture_source_inner(
        blueprints_fixture(),
        None,
        None,
        &[
            (
                "industryTargetFilters.jsonl",
                concat!(
                    r#"{"_key":12,"categoryIDs":[23,39,40,65,66],"groupIDs":[536,1136,4736],"name":"Structures"}"#,
                    "\n",
                    r#"{"_key":13,"groupIDs":[873],"name":"Capital Components"}"#,
                    "\n",
                    r#"{"_key":14,"groupIDs":[332,334,716,964],"name":"Components"}"#,
                    "\n",
                    r#"{"_key":15,"groupIDs":[913],"name":"Advanced Capital Components"}"#,
                    "\n",
                    r#"{"_key":16,"groupIDs":[974],"name":"Hybrid Reactions"}"#,
                    "\n",
                    r#"{"_key":17,"groupIDs":[712,4096],"name":"Biochemical Reactions"}"#,
                    "\n",
                    r#"{"_key":18,"groupIDs":[428,429,4932],"name":"Composite Reactions"}"#,
                ),
            ),
            (
                "industryModifierSources.jsonl",
                concat!(
                    // L-Set reactor: one material + one time entry per covered filter.
                    r#"{"_key":46496,"reaction":{"material":[{"dogmaAttributeID":2716,"filterID":16},{"dogmaAttributeID":2718,"filterID":18},{"dogmaAttributeID":2720,"filterID":17}],"time":[{"dogmaAttributeID":2715,"filterID":16},{"dogmaAttributeID":2717,"filterID":18},{"dogmaAttributeID":2719,"filterID":17}]}}"#,
                    "\n",
                    // XL-Set structure/component: multi-filter manufacturing analogue.
                    r#"{"_key":43704,"manufacturing":{"material":[{"dogmaAttributeID":2544,"filterID":14},{"dogmaAttributeID":2544,"filterID":13},{"dogmaAttributeID":2544,"filterID":12},{"dogmaAttributeID":2544,"filterID":15}],"time":[{"dogmaAttributeID":2545,"filterID":14},{"dogmaAttributeID":2545,"filterID":13},{"dogmaAttributeID":2545,"filterID":12},{"dogmaAttributeID":2545,"filterID":15}]}}"#,
                ),
            ),
            (
                "typeDogma.jsonl",
                concat!(
                    r#"{"_key":46496,"dogmaAttributes":[{"attributeID":2714,"value":-2.0},{"attributeID":2713,"value":-20.0}]}"#,
                    "\n",
                    r#"{"_key":43704,"dogmaAttributes":[{"attributeID":2594,"value":-2.0},{"attributeID":2593,"value":-20.0}]}"#,
                ),
            ),
        ],
    )
    .unwrap();

    let dataset = parse(&source).unwrap();

    let reactor = dataset
        .reaction_rig_modifiers
        .iter()
        .find(|rig| rig.type_id == 46_496)
        .expect("L-Set reactor rig present");
    assert_eq!(reactor.material_filter_ids, vec![16, 17, 18]);
    assert_eq!(reactor.time_filter_ids, vec![16, 17, 18]);

    let structure_rig = dataset
        .rig_modifiers
        .iter()
        .find(|rig| rig.type_id == 43_704)
        .expect("XL-Set structure/component rig present");
    assert_eq!(structure_rig.material_filter_ids, vec![12, 13, 14, 15]);
    assert_eq!(structure_rig.time_filter_ids, vec![12, 13, 14, 15]);
}

#[test]
fn industry_target_filters_are_empty_when_the_sde_lacks_the_dataset() {
    let dataset = parse(&fixture_source(blueprints_fixture()).unwrap()).unwrap();
    assert!(dataset.industry_target_filters.is_empty());
    assert!(dataset
        .rig_modifiers
        .iter()
        .all(|rig| rig.material_filter_ids.is_empty() && rig.time_filter_ids.is_empty()));
}

#[test]
fn rejects_missing_required_file() {
    let mut bytes = Cursor::new(Vec::new());
    {
        let mut archive = zip::ZipWriter::new(&mut bytes);
        archive
            .start_file("types.jsonl", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"").unwrap();
        archive.finish().unwrap();
    }
    let error =
        SdeSource::from_zip_reader(Cursor::new(bytes.into_inner()), "bad.zip", "abc").unwrap_err();
    assert!(matches!(error, SdeError::MissingFile("groups.jsonl")));
}

#[test]
fn reports_json_line_for_malformed_source() {
    let source = fixture_source("{not-json").unwrap();
    let error = parse(&source).unwrap_err();
    assert!(matches!(
        error,
        SdeError::Json {
            file: "blueprints.jsonl",
            line: 1,
            ..
        }
    ));
}

#[test]
fn skips_blueprints_with_unknown_material_types() {
    let blueprint = r#"{"_key":6830,"activities":{"manufacturing":{"materials":[{"typeID":404,"quantity":1}],"products":[{"typeID":5876,"quantity":1}]}}}"#;
    let dataset = parse(&fixture_source(blueprint).unwrap()).unwrap();
    assert!(dataset.blueprints.is_empty());
    assert_eq!(dataset.skipped_blueprints, 1);
}

#[test]
fn rejects_non_positive_manufacturing_quantities() {
    let blueprint = r#"{"_key":6830,"activities":{"manufacturing":{"materials":[{"typeID":34,"quantity":0}],"products":[{"typeID":5876,"quantity":1}]}}}"#;
    let error = parse(&fixture_source(blueprint).unwrap()).unwrap_err();
    assert!(matches!(
        error,
        SdeError::InvalidQuantity {
            kind: "material",
            blueprint_type_id: 6830,
            type_id: 34
        }
    ));
}

#[test]
fn skips_manufacturing_definitions_without_products() {
    let blueprint = r#"{"_key":6830,"activities":{"manufacturing":{"materials":[{"typeID":34,"quantity":1}]}}}"#;
    let dataset = parse(&fixture_source(blueprint).unwrap()).unwrap();
    assert!(dataset.blueprints.is_empty());
    assert_eq!(dataset.skipped_blueprints, 1);
}

#[test]
fn parses_planet_schematics_and_drops_unresolvable_lines() {
    let source = fixture_source_inner(
        blueprints_fixture(),
        None,
        None,
        &[
            (
                "planetSchematics.jsonl",
                concat!(
                    r#"{"_key":65,"cycleTime":3600,"name":{"en":"Superconductors"},"pins":[2470],"types":[{"_key":34,"isInput":true,"quantity":40},{"_key":777,"isInput":true,"quantity":40},{"_key":5876,"isInput":false,"quantity":5}]}"#,
                    "\n",
                    r#"{"_key":66,"cycleTime":1800,"name":{"en":"Ghost"},"types":[{"_key":34,"isInput":true,"quantity":1},{"_key":778,"isInput":false,"quantity":1}]}"#
                ),
            ),
            ("mapPlanets.jsonl", ""),
        ],
    )
    .unwrap();
    assert!(source.has_planetary_data());

    let dataset = parse(&source).unwrap();

    assert_eq!(
        dataset.planet_schematics,
        vec![ImportPlanetSchematic {
            schematic_id: 65,
            name: "Superconductors".to_string(),
            cycle_time_seconds: 3600,
            types: vec![
                ImportPlanetSchematicType {
                    type_id: 34,
                    is_input: true,
                    quantity: 40,
                },
                ImportPlanetSchematicType {
                    type_id: 5876,
                    is_input: false,
                    quantity: 5,
                },
            ],
        }]
    );
}

#[test]
fn names_planets_from_their_system_and_celestial_index() {
    let source = fixture_source_inner(
        blueprints_fixture(),
        Some(r#"{"_key":30004878,"name":{"en":"C-J6MT"},"securityStatus":-0.2}"#),
        None,
        &[(
            "mapPlanets.jsonl",
            concat!(
                r#"{"_key":40000002,"celestialIndex":4,"solarSystemID":30004878,"statistics":{"density":1.0}}"#,
                "\n",
                r#"{"_key":40000099,"celestialIndex":1,"solarSystemID":31000001}"#
            ),
        )],
    )
    .unwrap();
    assert!(!source.has_planetary_data());

    let dataset = parse(&source).unwrap();

    assert_eq!(
        dataset.planets,
        vec![ImportPlanet {
            planet_id: 40_000_002,
            solar_system_id: 30_004_878,
            celestial_index: 4,
            name: "C-J6MT IV".to_string(),
        }]
    );
}

#[test]
fn planetary_data_is_empty_when_the_sde_lacks_it() {
    let dataset = parse(&fixture_source(blueprints_fixture()).unwrap()).unwrap();
    assert!(dataset.planet_schematics.is_empty());
    assert!(dataset.planets.is_empty());
}

fn fixture_source(blueprints: &str) -> Result<SdeSource, SdeError> {
    fixture_source_inner(blueprints, None, None, &[])
}

fn fixture_source_with_solar_systems(blueprints: &str) -> Result<SdeSource, SdeError> {
    fixture_source_inner(
        blueprints,
        Some(r#"{"_key":30004878,"name":{"en":"C-J6MT"},"securityStatus":-0.2}"#),
        None,
        &[],
    )
}

/// Adds a "Methanofullerene Reaction Formula" type (real type ID 46157,
/// real reaction-formula group ID 1889) so reaction-activity fixtures can
/// resolve their own name, alongside the existing material/product types.
fn fixture_source_with_reaction_type(blueprints: &str) -> Result<SdeSource, SdeError> {
    fixture_source_inner(
        blueprints,
        None,
        Some(
            r#"{"_key":46157,"groupID":1889,"name":{"en":"Methanofullerene Reaction Formula"},"published":true}"#,
        ),
        &[],
    )
}

fn fixture_source_inner(
    blueprints: &str,
    solar_systems: Option<&str>,
    extra_type: Option<&str>,
    extra_files: &[(&str, &str)],
) -> Result<SdeSource, SdeError> {
    let mut bytes = Cursor::new(Vec::new());
    {
        let mut archive = zip::ZipWriter::new(&mut bytes);
        let options = SimpleFileOptions::default();
        let types_jsonl = concat!(
                r#"{"_key":34,"groupID":18,"name":{"en":"Tritanium"},"published":false,"volume":0.01}"#,
                "\n",
                r#"{"_key":5876,"groupID":25,"marketGroupID":61,"metaGroupID":1,"name":{"en":"Rifter"},"published":true,"packagedVolume":250.0,"volume":27289.0}"#,
                "\n",
                r#"{"_key":6830,"groupID":105,"name":{"en":"Rifter Blueprint"},"published":true}"#,
                "\n",
                r#"{"_key":9999,"groupID":18,"name":{"en":"Unused Legacy Type"},"published":false}"#
            )
            .to_string();
        let types_jsonl = match extra_type {
            Some(extra) => format!("{types_jsonl}\n{extra}"),
            None => types_jsonl,
        };
        for (name, contents) in [
            ("types.jsonl", types_jsonl.as_str()),
            (
                "groups.jsonl",
                concat!(
                    r#"{"_key":18,"name":{"en":"Mineral"}}"#,
                    "\n",
                    r#"{"_key":25,"categoryID":6,"name":{"en":"Frigate"},"published":true}"#,
                    "\n",
                    r#"{"_key":105,"name":{"en":"Frigate Blueprint"}}"#,
                    "\n",
                    r#"{"_key":1889,"name":{"en":"Polymer Reaction Formulas"}}"#
                ),
            ),
            ("blueprints.jsonl", blueprints),
            (
                "categories.jsonl",
                r#"{"_key":6,"name":{"en":"Ship"},"published":true}"#,
            ),
            ("metaGroups.jsonl", r#"{"_key":1,"name":{"en":"Tech I"}}"#),
            (
                "marketGroups.jsonl",
                concat!(
                    r#"{"_key":4,"name":{"en":"Ships"}}"#,
                    "\n",
                    r#"{"_key":61,"name":{"en":"Minmatar"},"parentGroupID":4}"#
                ),
            ),
            ("_sde.jsonl", r#"{"buildNumber":123456}"#),
        ] {
            archive.start_file(name, options).unwrap();
            archive.write_all(contents.as_bytes()).unwrap();
        }
        if let Some(solar_systems) = solar_systems {
            archive.start_file(SOLAR_SYSTEMS_FILE, options).unwrap();
            archive.write_all(solar_systems.as_bytes()).unwrap();
        }
        for (name, contents) in extra_files {
            archive.start_file(*name, options).unwrap();
            archive.write_all(contents.as_bytes()).unwrap();
        }
        archive.finish().unwrap();
    }
    SdeSource::from_zip_reader(Cursor::new(bytes.into_inner()), "fixture.zip", "checksum")
}

fn blueprints_fixture() -> &'static str {
    r#"{"_key":6830,"activities":{"manufacturing":{"time":600,"materials":[{"typeID":34,"quantity":1000}],"products":[{"typeID":5876,"quantity":1}]},"invention":{"materials":[],"products":[]}}}"#
}

#[test]
fn parses_authoritative_classification_metadata() {
    let source = fixture_source(blueprints_fixture()).unwrap();
    assert!(source.has_classification_metadata());

    let parsed = parse(&source).unwrap();
    assert_eq!(
        parsed.categories,
        vec![ImportCategory {
            category_id: 6,
            name: "Ship".into(),
            published: true
        }]
    );
    assert_eq!(
        parsed.meta_groups,
        vec![ImportMetaGroup {
            meta_group_id: 1,
            name: "Tech I".into()
        }]
    );
    assert_eq!(
        parsed.market_groups,
        vec![
            ImportMarketGroup {
                market_group_id: 4,
                name: "Ships".into(),
                parent_group_id: None
            },
            ImportMarketGroup {
                market_group_id: 61,
                name: "Minmatar".into(),
                parent_group_id: Some(4)
            },
        ]
    );
    let frigate = parsed
        .groups
        .iter()
        .find(|group| group.group_id == 25)
        .unwrap();
    assert_eq!(frigate.category_id, Some(6));
    assert!(frigate.published);
    let rifter = parsed
        .types
        .iter()
        .find(|item| item.type_id == 5_876)
        .unwrap();
    assert_eq!(rifter.meta_group_id, Some(1));
    assert_eq!(rifter.market_group_id, Some(61));
    assert_eq!(parsed.counts().classified_types, 1);
}

/// Shape verified against the real SDE: reaction formulas live in
/// blueprints.jsonl under an activities.reaction key with the same
/// materials/products/time/skills shape as manufacturing (e.g. real type
/// 46157, Methanofullerene Reaction Formula).
fn reaction_blueprint_fixture() -> &'static str {
    r#"{"_key":46157,"activities":{"reaction":{"time":10800,"materials":[{"typeID":34,"quantity":100}],"products":[{"typeID":5876,"quantity":160}],"skills":[{"typeID":45746,"level":3}]}}}"#
}
