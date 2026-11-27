//! Invention data for the Invention page: recipes from the blueprint
//! activity tables, and the decryptors written with an SDE import.

use super::sde_import::PgSdeRepository;
use super::sde_read::map_sde_sqlx_error;
use super::*;
use crate::sql::like_contains;
use iskworks_sde::{
    Decryptor, InventableProduct, InventionProduct, InventionRecipe, InventionSkillRequirement,
    RecipeLine,
};
use rust_decimal::Decimal;

const BATCH_SIZE: usize = 1_000;

impl PgSdeRepository {
    pub(super) async fn write_decryptors(
        &self,
        import_id: Uuid,
        dataset: &NormalizedSde,
    ) -> Result<(), SdeError> {
        for batch in dataset.decryptors.chunks(BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_decryptors \
                 (import_id, type_id, probability_multiplier, me_modifier, te_modifier, \
                  run_modifier) ",
            );
            query.push_values(batch, |mut row, decryptor| {
                row.push_bind(import_id)
                    .push_bind(decryptor.type_id)
                    .push_bind(decryptor.probability_multiplier)
                    .push_bind(decryptor.me_modifier)
                    .push_bind(decryptor.te_modifier)
                    .push_bind(decryptor.run_modifier);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }
        Ok(())
    }

    pub(super) async fn read_has_invention_data(&self) -> Result<bool, SdeError> {
        sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
              SELECT 1
              FROM sde_imports import
              JOIN sde_blueprint_activities invention ON invention.import_id = import.id
              WHERE import.active = true AND invention.activity = 'invention'
            )
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)
    }

    pub(super) async fn read_invention_recipe(
        &self,
        product_blueprint_type_id: i64,
    ) -> Result<Option<InventionRecipe>, SdeError> {
        let header = sqlx::query_as::<_, (i64, String, Option<i64>, Option<i64>)>(
            r#"
            SELECT invention.blueprint_type_id, source.name_en, invention.duration_seconds,
                   copying.duration_seconds
            FROM sde_imports import
            JOIN sde_blueprint_activity_products product
              ON product.import_id = import.id
             AND product.activity = 'invention'
            JOIN sde_blueprint_activities invention
              ON invention.import_id = product.import_id
             AND invention.blueprint_type_id = product.blueprint_type_id
             AND invention.activity = product.activity
            JOIN sde_types source
              ON source.import_id = invention.import_id
             AND source.type_id = invention.blueprint_type_id
            LEFT JOIN sde_blueprint_activities copying
              ON copying.import_id = invention.import_id
             AND copying.blueprint_type_id = invention.blueprint_type_id
             AND copying.activity = 'copying'
            WHERE import.active = true
              AND product.product_type_id = $1
              AND product.probability IS NOT NULL
              AND source.published = true
            ORDER BY invention.blueprint_type_id
            LIMIT 1
            "#,
        )
        .bind(product_blueprint_type_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        let Some((source_blueprint_type_id, source_blueprint_name, duration_seconds, copying)) =
            header
        else {
            return Ok(None);
        };

        let materials = sqlx::query_as::<_, (i64, String, i64)>(
            r#"
            SELECT line.material_type_id, item.name_en, line.quantity
            FROM sde_imports import
            JOIN sde_blueprint_activity_materials line
              ON line.import_id = import.id
             AND line.activity = 'invention'
            JOIN sde_types item
              ON item.import_id = line.import_id
             AND item.type_id = line.material_type_id
            WHERE import.active = true AND line.blueprint_type_id = $1
            ORDER BY line.position, line.material_type_id
            "#,
        )
        .bind(source_blueprint_type_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;

        // The item an invented blueprint builds is its manufacturing
        // recipe's first product.
        let products =
            sqlx::query_as::<_, (i64, String, Option<i64>, Option<String>, i64, Decimal)>(
                r#"
            SELECT line.product_type_id, blueprint.name_en,
                   built.product_type_id, built.product_name,
                   line.quantity, line.probability
            FROM sde_imports import
            JOIN sde_blueprint_activity_products line
              ON line.import_id = import.id
             AND line.activity = 'invention'
            JOIN sde_types blueprint
              ON blueprint.import_id = line.import_id
             AND blueprint.type_id = line.product_type_id
            LEFT JOIN LATERAL (
              SELECT p.product_type_id, item.name_en AS product_name
              FROM sde_blueprint_activity_products p
              JOIN sde_types item
                ON item.import_id = p.import_id AND item.type_id = p.product_type_id
              WHERE p.import_id = line.import_id
                AND p.blueprint_type_id = line.product_type_id
                AND p.activity = 'manufacturing'
              ORDER BY p.position, p.product_type_id
              LIMIT 1
            ) built ON true
            WHERE import.active = true
              AND line.blueprint_type_id = $1
              AND line.probability IS NOT NULL
            ORDER BY line.position, line.product_type_id
            "#,
            )
            .bind(source_blueprint_type_id)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sde_sqlx_error)?;

        let skills = sqlx::query_as::<_, (i64, String, i16)>(
            r#"
            SELECT line.skill_type_id, skill.name_en, line.level
            FROM sde_imports import
            JOIN sde_blueprint_activity_skills line
              ON line.import_id = import.id
             AND line.activity = 'invention'
            JOIN sde_types skill
              ON skill.import_id = line.import_id
             AND skill.type_id = line.skill_type_id
            WHERE import.active = true AND line.blueprint_type_id = $1
            ORDER BY skill.name_en, line.skill_type_id
            "#,
        )
        .bind(source_blueprint_type_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;

        Ok(Some(InventionRecipe {
            source_blueprint_type_id,
            source_blueprint_name,
            duration_seconds,
            copying_duration_seconds: copying,
            materials: materials
                .into_iter()
                .map(|(type_id, type_name, quantity)| RecipeLine {
                    type_id,
                    type_name,
                    quantity,
                })
                .collect(),
            products: products
                .into_iter()
                .map(
                    |(
                        blueprint_type_id,
                        blueprint_name,
                        product_type_id,
                        product_name,
                        base_runs,
                        probability,
                    )| InventionProduct {
                        blueprint_type_id,
                        blueprint_name,
                        product_type_id,
                        product_name,
                        base_runs,
                        probability: probability.normalize(),
                    },
                )
                .collect(),
            skills: skills
                .into_iter()
                .map(
                    |(skill_type_id, skill_name, level)| InventionSkillRequirement {
                        skill_type_id,
                        skill_name,
                        level,
                    },
                )
                .collect(),
        }))
    }

    pub(super) async fn read_decryptors(&self) -> Result<Vec<Decryptor>, SdeError> {
        let rows = sqlx::query_as::<_, (i64, String, Decimal, i32, i32, i32)>(
            r#"
            SELECT decryptor.type_id, item.name_en, decryptor.probability_multiplier,
                   decryptor.me_modifier, decryptor.te_modifier, decryptor.run_modifier
            FROM sde_imports import
            JOIN sde_decryptors decryptor ON decryptor.import_id = import.id
            JOIN sde_types item
              ON item.import_id = decryptor.import_id
             AND item.type_id = decryptor.type_id
            WHERE import.active = true
            ORDER BY item.name_en, decryptor.type_id
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    type_id,
                    name,
                    probability_multiplier,
                    me_modifier,
                    te_modifier,
                    run_modifier,
                )| {
                    Decryptor {
                        type_id,
                        name,
                        probability_multiplier: probability_multiplier.normalize(),
                        me_modifier,
                        te_modifier,
                        run_modifier,
                    }
                },
            )
            .collect())
    }

    pub(super) async fn read_inventable_products(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<InventableProduct>, SdeError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_as::<_, (i64, String, Option<String>, i64, i64, String, i64)>(
            r#"
            SELECT DISTINCT ON (exact, item.name_en, item.type_id)
                   item.type_id, item.name_en, item.group_name_en,
                   line.product_type_id,
                   line.blueprint_type_id, source.name_en,
                   (SELECT count(*)
                    FROM sde_blueprint_activity_products sibling
                    WHERE sibling.import_id = line.import_id
                      AND sibling.blueprint_type_id = line.blueprint_type_id
                      AND sibling.activity = 'invention'
                      AND sibling.probability IS NOT NULL),
                   CASE WHEN lower(item.name_en) = lower($2) THEN 0 ELSE 1 END AS exact
            FROM sde_imports import
            JOIN sde_blueprint_activity_products line
              ON line.import_id = import.id
             AND line.activity = 'invention'
            JOIN sde_types source
              ON source.import_id = line.import_id
             AND source.type_id = line.blueprint_type_id
            JOIN sde_blueprint_activity_products built
              ON built.import_id = line.import_id
             AND built.blueprint_type_id = line.product_type_id
             AND built.activity = 'manufacturing'
            JOIN sde_types item
              ON item.import_id = built.import_id
             AND item.type_id = built.product_type_id
            WHERE import.active = true
              AND line.probability IS NOT NULL
              AND source.published = true
              AND item.published = true
              AND lower(item.name_en) LIKE lower($1)
            ORDER BY exact, item.name_en, item.type_id, line.blueprint_type_id
            LIMIT $3
            "#,
        )
        .bind(like_contains(query))
        .bind(query)
        .bind(i64::from(limit.clamp(1, 50)))
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    product_type_id,
                    product_name,
                    group_name,
                    blueprint_type_id,
                    source_blueprint_type_id,
                    source_blueprint_name,
                    source_output_count,
                )| InventableProduct {
                    product_type_id,
                    product_name,
                    group_name,
                    blueprint_type_id,
                    source_blueprint_type_id,
                    source_blueprint_name,
                    source_output_count,
                },
            )
            .collect())
    }
}
