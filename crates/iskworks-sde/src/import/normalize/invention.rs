use super::*;

/// EVE's Blueprint category. Ancient relics are category 34.
pub(super) const BLUEPRINT_CATEGORY_ID: i64 = 9;
/// The only decryptors the game accepts. Ten other published "Decryptors -
/// ..." groups carry the same four attributes with values it does not use.
const GENERIC_DECRYPTOR_GROUP_ID: i64 = 1304;
const PROBABILITY_MULTIPLIER_ATTRIBUTE: i64 = 1112;
const ME_MODIFIER_ATTRIBUTE: i64 = 1113;
const TE_MODIFIER_ATTRIBUTE: i64 = 1114;
const RUN_MODIFIER_ATTRIBUTE: i64 = 1124;

/// One blueprint's `invention` activity. `Ok(None)` when it should be
/// skipped: a material or skill type is unknown, or no product is a named
/// type with base runs and a chance in `(0, 1]`. Kept types are marked
/// referenced so they survive the published-types filter.
pub(super) fn normalize_invention(
    blueprint_key: i64,
    activity: &RawActivity,
    raw_types_by_id: &HashMap<i64, RawType>,
    referenced_type_ids: &mut HashSet<i64>,
) -> Result<Option<ImportInvention>, SdeError> {
    let named = |type_id: &i64| {
        raw_types_by_id
            .get(type_id)
            .is_some_and(|item| item.name.en.is_some())
    };
    let mut type_ids = HashSet::new();
    let materials = match normalize_quantities(
        "invention material",
        blueprint_key,
        &activity.materials,
        raw_types_by_id,
        &mut type_ids,
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
    .collect::<Vec<_>>();

    let mut seen_products = HashSet::new();
    let products = activity
        .products
        .iter()
        .filter(|product| named(&product.type_id) && product.quantity > 0)
        .filter_map(|product| {
            let probability = product
                .probability
                .clone()
                .and_then(json_number_decimal)
                .filter(|chance| *chance > Decimal::ZERO && *chance <= Decimal::ONE)?;
            seen_products
                .insert(product.type_id)
                .then_some((product, probability))
        })
        .enumerate()
        .map(
            |(position, (product, probability))| ImportInventionProduct {
                blueprint_type_id: product.type_id,
                base_runs: product.quantity,
                probability,
                position: position as i32,
            },
        )
        .collect::<Vec<_>>();
    if products.is_empty() {
        return Ok(None);
    }

    let mut skills = BTreeMap::new();
    for skill in &activity.skills {
        if !named(&skill.type_id) {
            return Ok(None);
        }
        let level = skill.level.clamp(1, 5);
        skills
            .entry(skill.type_id)
            .and_modify(|current: &mut i16| *current = (*current).max(level))
            .or_insert(level);
    }
    let skills = skills
        .into_iter()
        .map(|(skill_type_id, level)| ImportInventionSkill {
            skill_type_id,
            level,
        })
        .collect::<Vec<_>>();

    referenced_type_ids.insert(blueprint_key);
    referenced_type_ids.extend(type_ids);
    referenced_type_ids.extend(products.iter().map(|product| product.blueprint_type_id));
    referenced_type_ids.extend(skills.iter().map(|skill| skill.skill_type_id));
    Ok(Some(ImportInvention {
        blueprint_type_id: blueprint_key,
        duration_seconds: activity.time.filter(|seconds| *seconds > 0),
        materials,
        products,
        skills,
    }))
}

/// The published Generic Decryptors, sorted by type. A missing attribute
/// means "no change": a multiplier of 1 or a modifier of 0.
pub(super) fn normalize_decryptors(
    records: &[RawTypeDogma],
    raw_types_by_id: &HashMap<i64, RawType>,
) -> Vec<ImportDecryptor> {
    let mut decryptors = records
        .iter()
        .filter(|record| {
            raw_types_by_id.get(&record.key).is_some_and(|item| {
                item.published
                    && item.name.en.is_some()
                    && item.group_id == Some(GENERIC_DECRYPTOR_GROUP_ID)
            })
        })
        .filter_map(|record| {
            let attribute = |id: i64| {
                record
                    .dogma_attributes
                    .iter()
                    .find(|value| value.attribute_id == id)
                    .map(|value| value.value)
            };
            let modifier = |id: i64| match attribute(id) {
                None => Some(0),
                Some(value) => value.round().to_i32(),
            };
            let probability_multiplier =
                attribute(PROBABILITY_MULTIPLIER_ATTRIBUTE).unwrap_or(Decimal::ONE);
            (probability_multiplier > Decimal::ZERO).then_some(())?;
            Some(ImportDecryptor {
                type_id: record.key,
                probability_multiplier,
                me_modifier: modifier(ME_MODIFIER_ATTRIBUTE)?,
                te_modifier: modifier(TE_MODIFIER_ATTRIBUTE)?,
                run_modifier: modifier(RUN_MODIFIER_ATTRIBUTE)?,
            })
        })
        .collect::<Vec<_>>();
    decryptors.sort_by_key(|decryptor| decryptor.type_id);
    decryptors
}
