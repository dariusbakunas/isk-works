use super::*;

/// Dogma values of one type, keyed by attribute name.
type NamedDogma<'a> = HashMap<&'a str, Decimal>;

/// Each type's dogma, keyed by attribute name, for the types `wanted` keeps.
/// Attributes are matched by name, not id. Empty without `dogmaAttributes`.
pub(super) fn named_dogma<'a>(
    raw_type_dogma: &[RawTypeDogma],
    attribute_names: &'a HashMap<i64, String>,
    wanted: impl Fn(i64) -> bool,
) -> HashMap<i64, NamedDogma<'a>> {
    raw_type_dogma
        .iter()
        .filter(|dogma| wanted(dogma.key))
        .map(|dogma| {
            let values = dogma
                .dogma_attributes
                .iter()
                .filter_map(|value| {
                    attribute_names
                        .get(&value.attribute_id)
                        .map(|name| (name.as_str(), value.value))
                })
                .collect();
            (dogma.key, values)
        })
        .collect()
}

fn type_id_value(dogma: &NamedDogma<'_>, name: &str) -> Option<i64> {
    dogma
        .get(name)
        .and_then(|value| value.round().to_i64())
        .filter(|id| *id > 0)
}

/// Published category 41 structures, sorted by type id. Their types are
/// marked referenced so they survive the published-types filter.
pub(super) fn normalize_planet_pin_types(
    raw_types_by_id: &HashMap<i64, RawType>,
    raw_type_dogma: &[RawTypeDogma],
    attribute_names: &HashMap<i64, String>,
    referenced_type_ids: &mut HashSet<i64>,
) -> Vec<ImportPlanetPinType> {
    let kind_of = |type_id: i64| {
        raw_types_by_id
            .get(&type_id)
            .filter(|item| item.published && item.name.en.is_some())
            .and_then(|item| item.group_id)
            .and_then(PlanetPinKind::from_group_id)
    };
    let dogma = named_dogma(raw_type_dogma, attribute_names, |id| kind_of(id).is_some());
    let mut values = dogma
        .into_iter()
        .filter_map(|(type_id, dogma)| {
            Some(ImportPlanetPinType {
                type_id,
                kind: kind_of(type_id)?,
                planet_type_id: type_id_value(&dogma, "planetRestriction"),
                harvester_type_id: type_id_value(&dogma, "harvesterType"),
                cpu_load: dogma.get("cpuLoad").copied(),
                power_load: dogma.get("powerLoad").copied(),
                cpu_output: dogma.get("cpuOutput").copied(),
                power_output: dogma.get("powerOutput").copied(),
                ecu_head_cpu: dogma.get("ecuExtractorHeadCPU").copied(),
                ecu_head_power: dogma.get("ecuExtractorHeadPower").copied(),
                cpu_load_per_km: dogma.get("cpuLoadPerKm").copied(),
                power_load_per_km: dogma.get("powerLoadPerKm").copied(),
                cpu_load_level_modifier: dogma.get("cpuLoadLevelModifier").copied(),
                power_load_level_modifier: dogma.get("powerLoadLevelModifier").copied(),
            })
        })
        .collect::<Vec<_>>();
    values.sort_by_key(|value| value.type_id);
    referenced_type_ids.extend(values.iter().map(|value| value.type_id));
    values
}

/// Every named type with an `exportTaxMultiplier`, sorted by type id. A
/// missing `importTaxMultiplier` falls back to the export one, as the SDE has
/// them equal. The types are marked referenced.
pub(super) fn normalize_planet_commodities(
    raw_types_by_id: &HashMap<i64, RawType>,
    raw_type_dogma: &[RawTypeDogma],
    attribute_names: &HashMap<i64, String>,
    referenced_type_ids: &mut HashSet<i64>,
) -> Vec<ImportPlanetCommodity> {
    let named = |type_id: i64| {
        raw_types_by_id
            .get(&type_id)
            .is_some_and(|item| item.name.en.is_some())
    };
    let mut values = named_dogma(raw_type_dogma, attribute_names, named)
        .into_iter()
        .filter_map(|(type_id, dogma)| {
            let export_tax_multiplier = *dogma.get("exportTaxMultiplier")?;
            Some(ImportPlanetCommodity {
                type_id,
                export_tax_multiplier,
                import_tax_multiplier: dogma
                    .get("importTaxMultiplier")
                    .copied()
                    .unwrap_or(export_tax_multiplier),
            })
        })
        .collect::<Vec<_>>();
    values.sort_by_key(|value| value.type_id);
    referenced_type_ids.extend(values.iter().map(|value| value.type_id));
    values
}
