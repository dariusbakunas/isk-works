use super::*;

/// The Type Info fields of one `types.jsonl` record, or `None` when it has
/// none of them.
pub(super) fn type_details(item: &RawType) -> Option<ImportTypeDetails> {
    let non_negative = |value: &Option<serde_json::Number>| {
        value
            .clone()
            .and_then(json_number_decimal)
            .filter(|value| *value >= Decimal::ZERO)
    };
    let details = ImportTypeDetails {
        type_id: item.key,
        description: item
            .description
            .as_ref()
            .and_then(|text| text.en.clone())
            .filter(|text| !text.trim().is_empty()),
        volume_m3: non_negative(&item.volume),
        mass_kg: non_negative(&item.mass),
        capacity_m3: non_negative(&item.capacity),
        portion_size: item.portion_size.filter(|size| *size > 0),
        variation_parent_type_id: item.variation_parent_type_id,
    };
    let empty = details.description.is_none()
        && details.volume_m3.is_none()
        && details.mass_kg.is_none()
        && details.capacity_m3.is_none()
        && details.portion_size.is_none()
        && details.variation_parent_type_id.is_none();
    (!empty).then_some(details)
}

/// Keeps what the Type Info window can show:
/// - attributes that are published and have an English display name;
/// - imported types' values of those attributes;
/// - fixed reprocessing materials whose output type was imported;
/// - bonuses of imported types, numbered skill, then role, then misc.
///
/// Every list is sorted by its key.
#[allow(clippy::too_many_arguments)]
pub(super) fn normalize_type_info(
    types: &[ImportType],
    mut details: Vec<ImportTypeDetails>,
    type_dogma: &[RawTypeDogma],
    raw_attributes: Vec<RawDogmaAttribute>,
    raw_units: Vec<RawDogmaUnit>,
    raw_categories: Vec<RawDogmaAttributeCategory>,
    raw_materials: Vec<RawTypeMaterials>,
    raw_bonuses: Vec<RawTypeBonus>,
) -> Result<ImportTypeInfo, SdeError> {
    details.sort_by_key(|row| row.type_id);
    let imported = types
        .iter()
        .map(|item| item.type_id)
        .collect::<HashSet<_>>();

    let mut unit_ids = HashSet::new();
    let mut dogma_units = Vec::new();
    for unit in raw_units {
        if !unit_ids.insert(unit.key) {
            return Err(SdeError::Duplicate {
                kind: "dogma unit",
                id: unit.key,
            });
        }
        dogma_units.push(ImportDogmaUnit {
            unit_id: unit.key,
            name: unit.name,
            display_name: unit
                .display_name
                .and_then(|name| name.en)
                .filter(|name| !name.trim().is_empty()),
        });
    }
    dogma_units.sort_by_key(|unit| unit.unit_id);

    let category_names = raw_categories
        .into_iter()
        .map(|category| (category.key, category.name))
        .collect::<HashMap<_, _>>();
    let mut attribute_ids = HashSet::new();
    let mut dogma_attributes = Vec::new();
    for attribute in raw_attributes {
        if !attribute_ids.insert(attribute.key) {
            return Err(SdeError::Duplicate {
                kind: "dogma attribute",
                id: attribute.key,
            });
        }
        let display_name = attribute
            .display_name
            .and_then(|name| name.en)
            .filter(|name| !name.trim().is_empty());
        let (true, Some(display_name)) = (attribute.published, display_name) else {
            continue;
        };
        dogma_attributes.push(ImportDogmaAttribute {
            attribute_id: attribute.key,
            name: attribute.name,
            display_name,
            // Only units the table holds, so the column can reference it.
            unit_id: attribute.unit_id.filter(|id| unit_ids.contains(id)),
            category_id: attribute.category_id,
            category_name: attribute
                .category_id
                .and_then(|id| category_names.get(&id).cloned()),
            display_when_zero: attribute.display_when_zero,
            high_is_good: attribute.high_is_good,
        });
    }
    dogma_attributes.sort_by_key(|attribute| attribute.attribute_id);

    let kept_attributes = dogma_attributes
        .iter()
        .map(|attribute| attribute.attribute_id)
        .collect::<HashSet<_>>();
    let mut type_attributes = type_dogma
        .iter()
        .filter(|record| imported.contains(&record.key))
        .flat_map(|record| {
            record
                .dogma_attributes
                .iter()
                .filter(|value| kept_attributes.contains(&value.attribute_id))
                .map(|value| ImportTypeAttribute {
                    type_id: record.key,
                    attribute_id: value.attribute_id,
                    value: value.value,
                })
        })
        .collect::<Vec<_>>();
    type_attributes.sort_by_key(|row| (row.type_id, row.attribute_id));
    type_attributes.dedup_by_key(|row| (row.type_id, row.attribute_id));

    let mut material_type_ids = HashSet::new();
    let mut type_materials = Vec::new();
    for record in raw_materials {
        if !material_type_ids.insert(record.key) {
            return Err(SdeError::Duplicate {
                kind: "type materials",
                id: record.key,
            });
        }
        if !imported.contains(&record.key) {
            continue;
        }
        let mut lines = BTreeMap::<i64, i64>::new();
        for line in record.materials {
            if line.quantity > 0 && imported.contains(&line.material_type_id) {
                *lines.entry(line.material_type_id).or_default() += line.quantity;
            }
        }
        type_materials.extend(lines.into_iter().map(|(material_type_id, quantity)| {
            ImportTypeMaterial {
                type_id: record.key,
                material_type_id,
                quantity,
            }
        }));
    }
    type_materials.sort_by_key(|row| (row.type_id, row.material_type_id));

    let mut bonus_type_ids = HashSet::new();
    let mut type_bonuses = Vec::new();
    for record in raw_bonuses {
        if !bonus_type_ids.insert(record.key) {
            return Err(SdeError::Duplicate {
                kind: "type bonus",
                id: record.key,
            });
        }
        if !imported.contains(&record.key) {
            continue;
        }
        let lines = record
            .types
            .into_iter()
            .flat_map(|skill| {
                let source = TypeBonusSource::Skill(skill.skill_type_id);
                skill.bonuses.into_iter().map(move |bonus| (source, bonus))
            })
            .chain(
                record
                    .role_bonuses
                    .into_iter()
                    .map(|bonus| (TypeBonusSource::Role, bonus)),
            )
            .chain(
                record
                    .misc_bonuses
                    .into_iter()
                    .map(|bonus| (TypeBonusSource::Misc, bonus)),
            )
            .filter_map(|(source, bonus)| Some((source, bonus.text.en.clone()?, bonus)));
        for (index, (source, text, bonus)) in lines.enumerate() {
            type_bonuses.push(ImportTypeBonus {
                type_id: record.key,
                ordinal: index as i32 + 1,
                source,
                bonus: bonus.bonus.and_then(json_number_decimal),
                unit_id: bonus.unit_id.filter(|id| unit_ids.contains(id)),
                text,
                importance: bonus.importance,
                is_positive: bonus.is_positive,
            });
        }
    }
    type_bonuses.sort_by_key(|row| (row.type_id, row.ordinal));

    Ok(ImportTypeInfo {
        details,
        dogma_units,
        dogma_attributes,
        type_attributes,
        type_materials,
        type_bonuses,
    })
}
