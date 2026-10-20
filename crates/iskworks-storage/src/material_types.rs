//! Name and Runway/Consumption category for inventory types, from the active
//! SDE import. Shared by Inventory › Runway and › Consumption.

use std::collections::BTreeMap;

use iskworks_core::{material_category, InventoryError, MaterialCategory};
use sqlx::PgPool;

/// `(type_id, name, category)` for every key of `fallback_names`. A type the
/// active SDE lacks keeps its fallback (captured) name and falls into
/// "Other".
pub(crate) async fn material_types(
    pool: &PgPool,
    fallback_names: BTreeMap<i64, String>,
) -> Result<Vec<(i64, String, MaterialCategory)>, InventoryError> {
    let type_ids: Vec<i64> = fallback_names.keys().copied().collect();
    let rows: Vec<(i64, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT t.type_id, t.name_en, g.name_en, c.name_en \
         FROM sde_types t \
         JOIN sde_imports i ON i.id = t.import_id AND i.active \
         LEFT JOIN sde_groups g ON g.import_id = t.import_id AND g.group_id = t.group_id \
         LEFT JOIN sde_categories c \
           ON c.import_id = g.import_id AND c.category_id = g.category_id \
         WHERE t.type_id = ANY($1)",
    )
    .bind(&type_ids)
    .fetch_all(pool)
    .await
    .map_err(|error| InventoryError::Persistence(error.to_string()))?;
    let mut facts: BTreeMap<i64, (String, MaterialCategory)> = rows
        .into_iter()
        .map(|(type_id, name, group, category)| {
            (
                type_id,
                (
                    name,
                    material_category(group.as_deref(), category.as_deref()),
                ),
            )
        })
        .collect();
    for (type_id, name) in fallback_names {
        facts
            .entry(type_id)
            .or_insert((name, MaterialCategory::Other));
    }
    Ok(facts
        .into_iter()
        .map(|(type_id, (name, category))| (type_id, name, category))
        .collect())
}
