use super::*;

pub(super) fn normalize_named_classifications(
    records: Vec<RawCategory>,
    kind: &'static str,
) -> Result<Vec<ImportCategory>, SdeError> {
    let mut ids = HashSet::new();
    let mut values = Vec::new();
    for record in records {
        if !ids.insert(record.key) {
            return Err(SdeError::Duplicate {
                kind,
                id: record.key,
            });
        }
        if let Some(name) = record.name.en {
            values.push(ImportCategory {
                category_id: record.key,
                name,
                published: record.published,
            });
        }
    }
    values.sort_by_key(|value| value.category_id);
    Ok(values)
}

pub(super) fn normalize_meta_groups(
    records: Vec<RawMetaGroup>,
) -> Result<Vec<ImportMetaGroup>, SdeError> {
    let mut ids = HashSet::new();
    let mut values = Vec::new();
    for record in records {
        if !ids.insert(record.key) {
            return Err(SdeError::Duplicate {
                kind: "meta group",
                id: record.key,
            });
        }
        if let Some(name) = record.name.en {
            values.push(ImportMetaGroup {
                meta_group_id: record.key,
                name,
            });
        }
    }
    values.sort_by_key(|value| value.meta_group_id);
    Ok(values)
}

pub(super) fn normalize_market_groups(
    records: Vec<RawMarketGroup>,
) -> Result<Vec<ImportMarketGroup>, SdeError> {
    let mut ids = HashSet::new();
    let mut values = Vec::new();
    for record in records {
        if !ids.insert(record.key) {
            return Err(SdeError::Duplicate {
                kind: "market group",
                id: record.key,
            });
        }
        if let Some(name) = record.name.en {
            values.push(ImportMarketGroup {
                market_group_id: record.key,
                name,
                parent_group_id: record.parent_group_id,
            });
        }
    }
    values.sort_by_key(|value| value.market_group_id);
    let mut ordered = Vec::with_capacity(values.len());
    let mut emitted = HashSet::new();
    while !values.is_empty() {
        let before = values.len();
        let mut remaining = Vec::new();
        for value in values {
            if value
                .parent_group_id
                .map_or(true, |parent_id| emitted.contains(&parent_id))
            {
                emitted.insert(value.market_group_id);
                ordered.push(value);
            } else {
                remaining.push(value);
            }
        }
        if remaining.len() == before {
            return Err(SdeError::CyclicReference("market group hierarchy"));
        }
        values = remaining;
    }
    Ok(ordered)
}

#[cfg(test)]
mod market_group_ordering_tests;

pub fn parse(source: &SdeSource) -> Result<NormalizedSde, SdeError> {
    let raw_groups: Vec<RawGroup> = parse_jsonl("groups.jsonl", source.file("groups.jsonl")?)?;
    let raw_categories: Vec<RawCategory> = parse_optional_jsonl(source, CATEGORIES_FILE)?;
    let raw_meta_groups: Vec<RawMetaGroup> = parse_optional_jsonl(source, META_GROUPS_FILE)?;
    let raw_market_groups: Vec<RawMarketGroup> = parse_optional_jsonl(source, MARKET_GROUPS_FILE)?;
    let raw_types: Vec<RawType> = parse_jsonl("types.jsonl", source.file("types.jsonl")?)?;
    let raw_blueprints: Vec<RawBlueprint> =
        parse_jsonl("blueprints.jsonl", source.file("blueprints.jsonl")?)?;
    let raw_solar_systems: Vec<RawSolarSystem> = source
        .files
        .get(SOLAR_SYSTEMS_FILE)
        .map(|contents| parse_jsonl(SOLAR_SYSTEMS_FILE, contents))
        .transpose()?
        .unwrap_or_default();
    let raw_constellations: Vec<RawConstellation> =
        parse_optional_jsonl(source, CONSTELLATIONS_FILE)?;
    let raw_regions: Vec<RawRegion> = parse_optional_jsonl(source, REGIONS_FILE)?;
    let raw_stations: Vec<RawNpcStation> = parse_optional_jsonl(source, NPC_STATIONS_FILE)?;
    let raw_corporations: Vec<RawNamedRecord> =
        parse_optional_jsonl(source, NPC_CORPORATIONS_FILE)?;
    let raw_operations: Vec<RawStationOperation> =
        parse_optional_jsonl(source, STATION_OPERATIONS_FILE)?;
    let raw_moons: Vec<RawMoon> = parse_optional_jsonl(source, MOONS_FILE)?;
    let raw_type_dogma: Vec<RawTypeDogma> = source
        .files
        .get(TYPE_DOGMA_FILE)
        .map(|contents| parse_jsonl(TYPE_DOGMA_FILE, contents))
        .transpose()?
        .unwrap_or_default();
    let raw_industry_target_filters: Vec<RawIndustryTargetFilter> =
        parse_optional_jsonl(source, INDUSTRY_TARGET_FILTERS_FILE)?;
    let raw_industry_modifier_sources: Vec<RawIndustryModifierSource> =
        parse_optional_jsonl(source, INDUSTRY_MODIFIER_SOURCES_FILE)?;
    let raw_planet_schematics: Vec<RawPlanetSchematic> =
        parse_optional_jsonl(source, PLANET_SCHEMATICS_FILE)?;
    let raw_planets: Vec<RawPlanet> = parse_optional_jsonl(source, MAP_PLANETS_FILE)?;
    let rig_filter_ids: HashMap<i64, RigFilterIds> = raw_industry_modifier_sources
        .into_iter()
        .map(|source| {
            // A multi-scope rig ("all reactions", "structures and
            // components", ...) has one `material`/`time` entry per covered
            // filter -- keep them all, not just the first.
            let all = |entries: &[RawIndustryModifierEntry]| {
                let mut ids: Vec<i64> = entries.iter().filter_map(|e| e.filter_id).collect();
                ids.sort_unstable();
                ids.dedup();
                ids
            };
            (
                source.key,
                RigFilterIds {
                    manufacturing_material: all(&source.manufacturing.material),
                    manufacturing_time: all(&source.manufacturing.time),
                    reaction_material: all(&source.reaction.material),
                    reaction_time: all(&source.reaction.time),
                },
            )
        })
        .collect();
    let mut industry_target_filters = raw_industry_target_filters
        .into_iter()
        .map(|filter| {
            let dedup = |mut ids: Vec<i64>| {
                ids.sort_unstable();
                ids.dedup();
                ids
            };
            ImportIndustryTargetFilter {
                filter_id: filter.key,
                name: filter.name,
                category_ids: dedup(filter.category_ids),
                group_ids: dedup(filter.group_ids),
            }
        })
        .collect::<Vec<_>>();
    industry_target_filters.sort_by_key(|filter| filter.filter_id);

    let mut group_names = HashMap::new();
    let mut groups = Vec::new();
    for group in raw_groups {
        if group_names
            .insert(group.key, group.name.en.clone())
            .is_some()
        {
            return Err(SdeError::Duplicate {
                kind: "group",
                id: group.key,
            });
        }
        if let Some(name) = group.name.en {
            groups.push(ImportGroup {
                group_id: group.key,
                name,
                category_id: group.category_id,
                published: group.published,
            });
        }
    }
    groups.sort_by_key(|value| value.group_id);
    let categories = normalize_named_classifications(raw_categories, "category")?;
    let meta_groups = normalize_meta_groups(raw_meta_groups)?;
    let market_groups = normalize_market_groups(raw_market_groups)?;
    if source.has_classification_metadata() {
        let category_ids = categories
            .iter()
            .map(|value| value.category_id)
            .collect::<HashSet<_>>();
        for group in &groups {
            if let Some(id) = group.category_id {
                if !category_ids.contains(&id) {
                    return Err(SdeError::UnknownReference {
                        relation: "group category",
                        source_id: group.group_id,
                        referenced_id: id,
                    });
                }
            }
        }
        let market_ids = market_groups
            .iter()
            .map(|value| value.market_group_id)
            .collect::<HashSet<_>>();
        for group in &market_groups {
            if let Some(id) = group.parent_group_id {
                if !market_ids.contains(&id) {
                    return Err(SdeError::UnknownReference {
                        relation: "market group parent",
                        source_id: group.market_group_id,
                        referenced_id: id,
                    });
                }
            }
        }
    }

    let mut raw_types_by_id = HashMap::new();
    for item in raw_types {
        let id = item.key;
        if raw_types_by_id.insert(id, item).is_some() {
            return Err(SdeError::Duplicate { kind: "type", id });
        }
    }

    let mut referenced_type_ids = HashSet::new();
    let mut blueprints = Vec::new();
    let mut blueprint_ids = HashSet::new();
    let mut skipped_blueprints = 0_u64;
    let mut reaction_formulas = Vec::new();
    let mut skipped_reaction_formulas = 0_u64;

    for blueprint in raw_blueprints {
        if !blueprint_ids.insert(blueprint.key) {
            return Err(SdeError::Duplicate {
                kind: "blueprint",
                id: blueprint.key,
            });
        }
        let name = raw_types_by_id
            .get(&blueprint.key)
            .and_then(|item| item.name.en.clone());

        if let Some(activity) = blueprint.activities.get("manufacturing") {
            match &name {
                None => skipped_blueprints += 1,
                Some(name) => {
                    match parse_activity_recipe(blueprint.key, activity, &raw_types_by_id)? {
                        None => skipped_blueprints += 1,
                        Some((duration_seconds, materials, products, recipe_type_ids)) => {
                            referenced_type_ids.extend(recipe_type_ids);
                            blueprints.push(ImportBlueprint {
                                blueprint_type_id: blueprint.key,
                                name: name.clone(),
                                duration_seconds,
                                materials,
                                products,
                            });
                        }
                    }
                }
            }
        }

        if let Some(activity) = blueprint.activities.get("reaction") {
            match &name {
                None => skipped_reaction_formulas += 1,
                Some(name) => {
                    match parse_activity_recipe(blueprint.key, activity, &raw_types_by_id)? {
                        None => skipped_reaction_formulas += 1,
                        Some((duration_seconds, materials, products, recipe_type_ids)) => {
                            referenced_type_ids.extend(recipe_type_ids);
                            reaction_formulas.push(ImportReactionFormula {
                                reaction_formula_type_id: blueprint.key,
                                name: name.clone(),
                                duration_seconds,
                                materials,
                                products,
                            });
                        }
                    }
                }
            }
        }
    }

    let planet_schematics = normalize_planet_schematics(
        raw_planet_schematics,
        &raw_types_by_id,
        &mut referenced_type_ids,
    )?;

    let (structure_modifiers, rig_modifiers, reaction_rig_modifiers) =
        normalize_facility_dogma(raw_type_dogma, &raw_types_by_id, &rig_filter_ids);

    let mut types = raw_types_by_id
        .into_values()
        .filter(|item| item.published || referenced_type_ids.contains(&item.key))
        .filter_map(|item| {
            let name = item.name.en?;
            Some(ImportType {
                type_id: item.key,
                name,
                group_id: item.group_id,
                group_name: item
                    .group_id
                    .and_then(|id| group_names.get(&id).cloned())
                    .flatten(),
                market_group_id: item.market_group_id,
                meta_group_id: item.meta_group_id,
                packaged_volume_m3: item
                    .packaged_volume
                    .and_then(json_number_decimal)
                    .filter(|volume| *volume >= Decimal::ZERO)
                    .or_else(|| {
                        item.volume
                            .and_then(json_number_decimal)
                            .filter(|volume| *volume >= Decimal::ZERO)
                    }),
                published: item.published,
            })
        })
        .collect::<Vec<_>>();

    types.sort_by_key(|item| item.type_id);
    if source.has_classification_metadata() {
        let meta_ids = meta_groups
            .iter()
            .map(|value| value.meta_group_id)
            .collect::<HashSet<_>>();
        for item in &types {
            if let Some(id) = item.meta_group_id {
                if !meta_ids.contains(&id) {
                    return Err(SdeError::UnknownReference {
                        relation: "type meta group",
                        source_id: item.type_id,
                        referenced_id: id,
                    });
                }
            }
        }
    }
    blueprints.sort_by_key(|item| item.blueprint_type_id);
    let mut solar_systems = raw_solar_systems
        .into_iter()
        .filter_map(|system| {
            system.name.en.map(|name| ImportSolarSystem {
                solar_system_id: system.key,
                name,
                constellation_id: system.constellation_id,
                region_id: system.region_id,
                security_status: system
                    .security_status
                    .and_then(|value| Decimal::from_str(&value.to_string()).ok()),
                wormhole_class_id: system.wormhole_class_id,
            })
        })
        .collect::<Vec<_>>();
    solar_systems.sort_by_key(|system| system.solar_system_id);
    let system_names = solar_systems
        .iter()
        .map(|system| (system.solar_system_id, system.name.clone()))
        .collect::<HashMap<_, _>>();
    let planets = normalize_planets(raw_planets, &system_names)?;
    let corporations = raw_corporations
        .into_iter()
        .filter_map(|record| record.name.en.map(|name| (record.key, name)))
        .collect::<HashMap<_, _>>();
    let operations = raw_operations
        .into_iter()
        .filter_map(|record| record.operation_name.en.map(|name| (record.key, name)))
        .collect::<HashMap<_, _>>();
    let moon_ids = raw_moons
        .into_iter()
        .map(|moon| moon.key)
        .collect::<HashSet<_>>();
    let mut npc_stations = raw_stations
        .into_iter()
        .filter_map(|station| {
            let system = system_names.get(&station.solar_system_id)?;
            let corporation = corporations.get(&station.owner_id)?;
            let operation = operations.get(&station.operation_id)?;
            let orbit = station.celestial_index.map(|planet| {
                match (
                    station.orbit_id.is_some_and(|id| moon_ids.contains(&id)),
                    station.orbit_index,
                ) {
                    (true, Some(moon)) => {
                        format!("{} - Moon {moon}", roman_numeral(planet))
                    }
                    _ => roman_numeral(planet),
                }
            });
            Some(ImportNpcStation {
                station_id: station.key,
                name: orbit.map_or_else(
                    || format!("{system} - {corporation} {operation}"),
                    |orbit| format!("{system} {orbit} - {corporation} {operation}"),
                ),
                solar_system_id: station.solar_system_id,
                owner_corporation_id: station.owner_id,
                station_type_id: station.type_id,
            })
        })
        .collect::<Vec<_>>();
    npc_stations.sort_by_key(|station| station.station_id);
    let mut constellations = raw_constellations
        .into_iter()
        .filter_map(|value| {
            Some(ImportConstellation {
                constellation_id: value.key,
                name: value.name.en?,
                region_id: value.region_id,
            })
        })
        .collect::<Vec<_>>();
    constellations.sort_by_key(|value| value.constellation_id);
    let mut regions = raw_regions
        .into_iter()
        .filter_map(|value| {
            Some(ImportRegion {
                region_id: value.key,
                name: value.name.en?,
            })
        })
        .collect::<Vec<_>>();
    regions.sort_by_key(|value| value.region_id);
    Ok(NormalizedSde {
        source_label: source.source_label.clone(),
        source_checksum: source.source_checksum.clone(),
        source_version: source.source_version.clone(),
        types,
        categories,
        groups,
        meta_groups,
        market_groups,
        blueprints,
        solar_systems,
        constellations,
        regions,
        npc_stations,
        structure_modifiers,
        rig_modifiers,
        reaction_formulas,
        reaction_rig_modifiers,
        industry_target_filters,
        planet_schematics,
        planets,
        skipped_blueprints,
        skipped_reaction_formulas,
    })
}

/// Keeps every schematic whose output type resolves; drops individual
/// input/output rows whose type is unknown (or unnamed) rather than failing
/// the whole import. Resolved types are marked referenced so they survive the
/// published-types filter.
pub(super) fn normalize_planet_schematics(
    records: Vec<RawPlanetSchematic>,
    raw_types_by_id: &HashMap<i64, RawType>,
    referenced_type_ids: &mut HashSet<i64>,
) -> Result<Vec<ImportPlanetSchematic>, SdeError> {
    let mut ids = HashSet::new();
    let mut values = Vec::new();
    for record in records {
        if !ids.insert(record.key) {
            return Err(SdeError::Duplicate {
                kind: "planet schematic",
                id: record.key,
            });
        }
        let Some(name) = record.name.en else {
            continue;
        };
        let mut types = record
            .types
            .into_iter()
            .filter(|line| line.quantity > 0)
            .filter(|line| {
                raw_types_by_id
                    .get(&line.key)
                    .is_some_and(|item| item.name.en.is_some())
            })
            .map(|line| ImportPlanetSchematicType {
                type_id: line.key,
                is_input: line.is_input,
                quantity: line.quantity,
            })
            .collect::<Vec<_>>();
        if record.cycle_time <= 0 || !types.iter().any(|line| !line.is_input) {
            continue;
        }
        types.sort_by_key(|line| (!line.is_input, line.type_id));
        types.dedup_by_key(|line| (line.is_input, line.type_id));
        referenced_type_ids.extend(types.iter().map(|line| line.type_id));
        values.push(ImportPlanetSchematic {
            schematic_id: record.key,
            name,
            cycle_time_seconds: record.cycle_time,
            types,
        });
    }
    values.sort_by_key(|value| value.schematic_id);
    Ok(values)
}

pub(super) fn normalize_planets(
    records: Vec<RawPlanet>,
    system_names: &HashMap<i64, String>,
) -> Result<Vec<ImportPlanet>, SdeError> {
    let mut ids = HashSet::new();
    let mut values = Vec::new();
    for record in records {
        if !ids.insert(record.key) {
            return Err(SdeError::Duplicate {
                kind: "planet",
                id: record.key,
            });
        }
        let Some(system) = system_names.get(&record.solar_system_id) else {
            continue;
        };
        values.push(ImportPlanet {
            planet_id: record.key,
            solar_system_id: record.solar_system_id,
            celestial_index: record.celestial_index,
            name: format!("{system} {}", roman_numeral(record.celestial_index)),
        });
    }
    values.sort_by_key(|value| value.planet_id);
    Ok(values)
}

/// Parses one blueprint activity (manufacturing or reaction; both share the
/// same materials/products/time shape) into recipe lines. Returns `Ok(None)`
/// when the activity should be skipped (unresolvable material/product type,
/// or no products), matching the skip semantics for manufacturing
/// blueprints.
#[allow(clippy::type_complexity)]
pub(crate) fn parse_activity_recipe(
    blueprint_key: i64,
    activity: &RawActivity,
    raw_types_by_id: &HashMap<i64, RawType>,
) -> Result<
    Option<(
        Option<i64>,
        Vec<ImportMaterial>,
        Vec<ImportProduct>,
        HashSet<i64>,
    )>,
    SdeError,
> {
    let mut recipe_type_ids = HashSet::from([blueprint_key]);

    let materials = match normalize_quantities(
        "material",
        blueprint_key,
        &activity.materials,
        raw_types_by_id,
        &mut recipe_type_ids,
    ) {
        Ok(lines) => lines,
        Err(SdeError::UnknownType { .. }) => return Ok(None),
        Err(error) => return Err(error),
    }
    .into_iter()
    .map(|(type_id, quantity, position)| ImportMaterial {
        type_id,
        quantity,
        position,
    })
    .collect();
    let products = match normalize_quantities(
        "product",
        blueprint_key,
        &activity.products,
        raw_types_by_id,
        &mut recipe_type_ids,
    ) {
        Ok(lines) => lines,
        Err(SdeError::UnknownType { .. }) => return Ok(None),
        Err(error) => return Err(error),
    }
    .into_iter()
    .map(|(type_id, quantity, position)| ImportProduct {
        type_id,
        quantity,
        position,
    })
    .collect::<Vec<_>>();
    if products.is_empty() {
        return Ok(None);
    }

    Ok(Some((
        activity.time.filter(|seconds| *seconds > 0),
        materials,
        products,
        recipe_type_ids,
    )))
}

/// Manufacturing rigs carry both a base structure discount (2600/2601/2602 on
/// the Engineering Complex itself) and a rig discount. Reaction (Refinery)
/// structures carry no equivalent base discount at all in the SDE -- every
/// reaction rig type checked has neither attribute, and Athanor/Tatara carry
/// no material/time/job-cost dogma attributes whatsoever -- so reactions get
/// no `ImportStructureModifiers` counterpart; the whole discount comes from
/// rigs (`RefRigTimeBonus`/`RefRigMatBonus` below).
pub(crate) fn normalize_facility_dogma(
    records: Vec<RawTypeDogma>,
    types: &HashMap<i64, RawType>,
    rig_filter_ids: &HashMap<i64, RigFilterIds>,
) -> (
    Vec<ImportStructureModifiers>,
    Vec<ImportRigModifiers>,
    Vec<ImportRigModifiers>,
) {
    const STRUCTURE_GROUP_IDS: [i64; 8] = [1404, 1406, 1408, 1657, 2015, 2016, 2017, 4744];
    let mut structures = Vec::new();
    let mut rigs = Vec::new();
    let mut reaction_rigs = Vec::new();
    for record in records {
        let attributes = record
            .dogma_attributes
            .into_iter()
            .map(|value| (value.attribute_id, value.value))
            .collect::<HashMap<_, _>>();
        if types
            .get(&record.key)
            .and_then(|item| item.group_id)
            .is_some_and(|group_id| STRUCTURE_GROUP_IDS.contains(&group_id))
        {
            let reduction = |id| {
                (Decimal::ONE - attributes.get(&id).copied().unwrap_or(Decimal::ONE))
                    * Decimal::ONE_HUNDRED
            };
            structures.push(ImportStructureModifiers {
                type_id: record.key,
                material_reduction_percent: reduction(2600),
                job_cost_reduction_percent: reduction(2601),
                time_reduction_percent: reduction(2602),
                structure_size: attributes.get(&1547).and_then(ToPrimitive::to_i64),
            });
        }
        let compatible_structure_group_ids = || {
            let mut ids = [1298, 1299, 1300]
                .iter()
                .filter_map(|id| attributes.get(id).and_then(ToPrimitive::to_i64))
                .collect::<Vec<_>>();
            ids.sort_unstable();
            ids.dedup();
            ids
        };
        let filters = rig_filter_ids.get(&record.key).cloned().unwrap_or_default();
        if attributes.contains_key(&2593) || attributes.contains_key(&2594) {
            rigs.push(ImportRigModifiers {
                type_id: record.key,
                material_reduction_percent: -attributes
                    .get(&2594)
                    .copied()
                    .unwrap_or(Decimal::ZERO),
                time_reduction_percent: -attributes.get(&2593).copied().unwrap_or(Decimal::ZERO),
                high_sec_multiplier: attributes.get(&2355).copied().unwrap_or(Decimal::ONE),
                low_sec_multiplier: attributes.get(&2356).copied().unwrap_or(Decimal::ONE),
                null_sec_multiplier: attributes.get(&2357).copied().unwrap_or(Decimal::ONE),
                compatible_structure_group_ids: compatible_structure_group_ids(),
                rig_size: attributes.get(&1547).and_then(ToPrimitive::to_i64),
                material_filter_ids: filters.manufacturing_material.clone(),
                time_filter_ids: filters.manufacturing_time.clone(),
            });
        }
        if attributes.contains_key(&2713) || attributes.contains_key(&2714) {
            reaction_rigs.push(ImportRigModifiers {
                type_id: record.key,
                material_reduction_percent: -attributes
                    .get(&2714)
                    .copied()
                    .unwrap_or(Decimal::ZERO),
                time_reduction_percent: -attributes.get(&2713).copied().unwrap_or(Decimal::ZERO),
                high_sec_multiplier: attributes.get(&2355).copied().unwrap_or(Decimal::ONE),
                low_sec_multiplier: attributes.get(&2356).copied().unwrap_or(Decimal::ONE),
                null_sec_multiplier: attributes.get(&2357).copied().unwrap_or(Decimal::ONE),
                compatible_structure_group_ids: compatible_structure_group_ids(),
                rig_size: attributes.get(&1547).and_then(ToPrimitive::to_i64),
                material_filter_ids: filters.reaction_material.clone(),
                time_filter_ids: filters.reaction_time.clone(),
            });
        }
    }
    structures.sort_by_key(|item| item.type_id);
    rigs.sort_by_key(|item| item.type_id);
    reaction_rigs.sort_by_key(|item| item.type_id);
    (structures, rigs, reaction_rigs)
}

pub(crate) fn normalize_quantities(
    kind: &'static str,
    blueprint_type_id: i64,
    values: &[RawTypeQuantity],
    types: &HashMap<i64, RawType>,
    referenced_type_ids: &mut HashSet<i64>,
) -> Result<Vec<(i64, i64, i32)>, SdeError> {
    let mut normalized = BTreeMap::<i64, i64>::new();
    for value in values {
        if value.quantity <= 0 {
            return Err(SdeError::InvalidQuantity {
                kind,
                blueprint_type_id,
                type_id: value.type_id,
            });
        }
        if !types.contains_key(&value.type_id) {
            return Err(SdeError::UnknownType {
                blueprint_type_id,
                type_id: value.type_id,
            });
        }
        referenced_type_ids.insert(value.type_id);
        *normalized.entry(value.type_id).or_default() += value.quantity;
    }
    Ok(normalized
        .into_iter()
        .enumerate()
        .map(|(position, (type_id, quantity))| (type_id, quantity, position as i32))
        .collect())
}
