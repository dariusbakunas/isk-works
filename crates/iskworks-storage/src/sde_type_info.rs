//! The Type Info reads, kept out of `sde_read.rs`; its
//! `SdeReadRepository` impl delegates here.

use iskworks_sde::{
    RecipeLine, TypeAttributeValue, TypeBonusLine, TypeBonusSource, TypeInfoBase, TypeInfoCounts,
};
use rust_decimal::Decimal;

use super::sde_import::PgSdeRepository;
use super::sde_read::map_sde_sqlx_error;
use super::*;

#[derive(sqlx::FromRow)]
struct TypeInfoBaseRow {
    type_id: i64,
    name_en: String,
    published: bool,
    group_id: Option<i64>,
    group_name: Option<String>,
    category_id: Option<i64>,
    category_name: Option<String>,
    meta_group_id: Option<i64>,
    meta_group_name: Option<String>,
    market_group_path: Vec<String>,
    packaged_volume_m3: Option<Decimal>,
    description_en: Option<String>,
    volume_m3: Option<Decimal>,
    mass_kg: Option<Decimal>,
    capacity_m3: Option<Decimal>,
    portion_size: Option<i32>,
    variation_parent_type_id: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct TypeAttributeRow {
    attribute_id: i64,
    display_name_en: String,
    unit_id: Option<i64>,
    unit_display_name: Option<String>,
    category_id: Option<i64>,
    category_name: Option<String>,
    display_when_zero: bool,
    high_is_good: bool,
    value: Decimal,
}

#[derive(sqlx::FromRow)]
struct TypeBonusRow {
    source_kind: String,
    skill_type_id: Option<i64>,
    bonus: Option<Decimal>,
    unit_id: Option<i64>,
    unit_display_name: Option<String>,
    text_en: String,
}

impl PgSdeRepository {
    pub(super) async fn read_type_info_base(
        &self,
        type_id: i64,
    ) -> Result<Option<TypeInfoBase>, SdeError> {
        let row = sqlx::query_as::<_, TypeInfoBaseRow>(
            r#"
            WITH RECURSIVE active AS (
              SELECT id FROM sde_imports WHERE active = true
            ), item AS (
              SELECT t.* FROM sde_types t JOIN active ON t.import_id = active.id
              WHERE t.type_id = $1
            ), market_path AS (
              SELECT g.market_group_id, g.name_en, g.parent_group_id, 0 depth
              FROM sde_market_groups g JOIN item
                ON g.import_id = item.import_id AND g.market_group_id = item.market_group_id
              UNION ALL
              SELECT parent.market_group_id, parent.name_en, parent.parent_group_id,
                     child.depth + 1
              FROM market_path child
              JOIN active ON true
              JOIN sde_market_groups parent
                ON parent.import_id = active.id
               AND parent.market_group_id = child.parent_group_id
              -- A cyclic hierarchy is refused at import; this only bounds it.
              WHERE child.depth < 32
            )
            SELECT item.type_id, item.name_en, item.published,
                   item.group_id, g.name_en group_name,
                   c.category_id, c.name_en category_name,
                   item.meta_group_id, m.name_en meta_group_name,
                   coalesce(
                     (SELECT array_agg(name_en ORDER BY depth DESC) FROM market_path),
                     ARRAY[]::text[]
                   ) market_group_path,
                   item.packaged_volume_m3,
                   d.description_en, d.volume_m3, d.mass_kg, d.capacity_m3, d.portion_size,
                   d.variation_parent_type_id
            FROM item
            LEFT JOIN sde_groups g
              ON g.import_id = item.import_id AND g.group_id = item.group_id
            LEFT JOIN sde_categories c
              ON c.import_id = g.import_id AND c.category_id = g.category_id
            LEFT JOIN sde_meta_groups m
              ON m.import_id = item.import_id AND m.meta_group_id = item.meta_group_id
            LEFT JOIN sde_type_details d
              ON d.import_id = item.import_id AND d.type_id = item.type_id
            "#,
        )
        .bind(type_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        Ok(row.map(|row| TypeInfoBase {
            type_id: row.type_id,
            name: row.name_en,
            published: row.published,
            group_id: row.group_id,
            group_name: row.group_name,
            category_id: row.category_id,
            category_name: row.category_name,
            meta_group_id: row.meta_group_id,
            meta_group_name: row.meta_group_name,
            market_group_path: row.market_group_path,
            packaged_volume_m3: row.packaged_volume_m3,
            description: row.description_en,
            volume_m3: row.volume_m3,
            mass_kg: row.mass_kg,
            capacity_m3: row.capacity_m3,
            portion_size: row.portion_size,
            variation_parent_type_id: row.variation_parent_type_id,
        }))
    }

    pub(super) async fn read_type_attributes(
        &self,
        type_id: i64,
    ) -> Result<Vec<TypeAttributeValue>, SdeError> {
        let rows = sqlx::query_as::<_, TypeAttributeRow>(
            r#"
            SELECT a.attribute_id, a.display_name_en, a.unit_id,
                   u.display_name unit_display_name,
                   a.category_id, a.category_name, a.display_when_zero, a.high_is_good,
                   v.value
            FROM sde_imports import
            JOIN sde_type_attributes v ON v.import_id = import.id
            JOIN sde_dogma_attributes a
              ON a.import_id = v.import_id AND a.attribute_id = v.attribute_id
            LEFT JOIN sde_dogma_units u
              ON u.import_id = a.import_id AND u.unit_id = a.unit_id
            WHERE import.active = true AND v.type_id = $1
            ORDER BY a.attribute_id
            "#,
        )
        .bind(type_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(|row| TypeAttributeValue {
                attribute_id: row.attribute_id,
                display_name: row.display_name_en,
                unit_id: row.unit_id,
                unit_display_name: row.unit_display_name,
                category_id: row.category_id,
                category_name: row.category_name,
                display_when_zero: row.display_when_zero,
                high_is_good: row.high_is_good,
                value: row.value,
            })
            .collect())
    }

    pub(super) async fn read_type_bonuses(
        &self,
        type_id: i64,
    ) -> Result<Vec<TypeBonusLine>, SdeError> {
        let rows = sqlx::query_as::<_, TypeBonusRow>(
            r#"
            SELECT b.source_kind, b.skill_type_id, b.bonus, b.unit_id,
                   u.display_name unit_display_name, b.text_en
            FROM sde_imports import
            JOIN sde_type_bonuses b ON b.import_id = import.id
            LEFT JOIN sde_dogma_units u
              ON u.import_id = b.import_id AND u.unit_id = b.unit_id
            WHERE import.active = true AND b.type_id = $1
            ORDER BY b.ordinal
            "#,
        )
        .bind(type_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        rows.into_iter()
            .map(|row| {
                let source = match (row.source_kind.as_str(), row.skill_type_id) {
                    ("skill", Some(skill_type_id)) => TypeBonusSource::Skill(skill_type_id),
                    ("role", None) => TypeBonusSource::Role,
                    ("misc", None) => TypeBonusSource::Misc,
                    (kind, _) => {
                        return Err(SdeError::Storage(format!(
                            "type {type_id} has a bonus of unknown kind {kind:?}"
                        )))
                    }
                };
                Ok(TypeBonusLine {
                    source,
                    bonus: row.bonus,
                    unit_id: row.unit_id,
                    unit_display_name: row.unit_display_name,
                    text: row.text_en,
                })
            })
            .collect()
    }

    pub(super) async fn read_type_materials(
        &self,
        type_id: i64,
    ) -> Result<Vec<RecipeLine>, SdeError> {
        let rows = sqlx::query_as::<_, (i64, String, i64)>(
            r#"
            SELECT m.material_type_id, t.name_en, m.quantity
            FROM sde_imports import
            JOIN sde_type_materials m ON m.import_id = import.id
            JOIN sde_types t ON t.import_id = m.import_id AND t.type_id = m.material_type_id
            WHERE import.active = true AND m.type_id = $1
            ORDER BY t.name_en, m.material_type_id
            "#,
        )
        .bind(type_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(|(type_id, type_name, quantity)| RecipeLine {
                type_id,
                type_name,
                quantity,
            })
            .collect())
    }

    pub(super) async fn read_type_info_counts(
        &self,
        type_id: i64,
    ) -> Result<TypeInfoCounts, SdeError> {
        let (variations, used_in) = sqlx::query_as::<_, (i64, i64)>(
            r#"
            WITH active AS (
              SELECT id FROM sde_imports WHERE active = true
            ), family AS (
              -- A type with no parent heads its own family.
              SELECT coalesce(
                (SELECT d.variation_parent_type_id FROM sde_type_details d, active
                 WHERE d.import_id = active.id AND d.type_id = $1),
                $1
              ) root
            )
            SELECT
              -- The root (when imported) plus everything pointing at it.
              (SELECT count(*) FROM sde_types t, active, family
                 WHERE t.import_id = active.id AND t.type_id = family.root)
              + (SELECT count(*) FROM sde_type_details d, active, family
                 WHERE d.import_id = active.id AND d.variation_parent_type_id = family.root),
              (SELECT count(DISTINCT b.blueprint_type_id) FROM sde_blueprint_materials b
                 JOIN active ON b.import_id = active.id
                 WHERE b.material_type_id = $1)
              + (SELECT count(DISTINCT r.reaction_formula_type_id)
                 FROM sde_reaction_formula_materials r
                 JOIN active ON r.import_id = active.id
                 WHERE r.material_type_id = $1)
            "#,
        )
        .bind(type_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        Ok(TypeInfoCounts {
            variations,
            used_in,
        })
    }

    pub(super) async fn read_group_names(
        &self,
        group_ids: &[i64],
    ) -> Result<BTreeMap<i64, String>, SdeError> {
        if group_ids.is_empty() {
            return Ok(BTreeMap::new());
        }
        let rows = sqlx::query_as::<_, (i64, String)>(
            r#"
            SELECT g.group_id, g.name_en
            FROM sde_imports import
            JOIN sde_groups g ON g.import_id = import.id
            WHERE import.active = true AND g.group_id = ANY($1)
            "#,
        )
        .bind(group_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        Ok(rows.into_iter().collect())
    }
}
