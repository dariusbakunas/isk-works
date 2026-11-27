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
