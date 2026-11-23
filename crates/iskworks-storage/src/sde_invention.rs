//! Invention data: written with an SDE import, read for the Invention page.

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
    pub(super) async fn write_invention(
        &self,
        import_id: Uuid,
        dataset: &NormalizedSde,
    ) -> Result<(), SdeError> {
        for batch in dataset.inventions.chunks(BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_blueprint_invention \
                 (import_id, blueprint_type_id, duration_seconds) ",
            );
            query.push_values(batch, |mut row, invention| {
                row.push_bind(import_id)
                    .push_bind(invention.blueprint_type_id)
                    .push_bind(invention.duration_seconds);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        let materials = dataset
            .inventions
            .iter()
            .flat_map(|invention| {
                invention
                    .materials
                    .iter()
                    .map(move |line| (invention.blueprint_type_id, line))
            })
            .collect::<Vec<_>>();
        for batch in materials.chunks(BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_blueprint_invention_materials \
                 (import_id, blueprint_type_id, material_type_id, quantity, position) ",
            );
            query.push_values(batch, |mut row, (blueprint_type_id, line)| {
                row.push_bind(import_id)
                    .push_bind(*blueprint_type_id)
                    .push_bind(line.type_id)
                    .push_bind(line.quantity)
                    .push_bind(line.position);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        let products = dataset
            .inventions
            .iter()
            .flat_map(|invention| {
                invention
                    .products
                    .iter()
                    .map(move |line| (invention.blueprint_type_id, line))
            })
            .collect::<Vec<_>>();
        for batch in products.chunks(BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_blueprint_invention_products \
                 (import_id, blueprint_type_id, product_blueprint_type_id, base_runs, \
                  probability, position) ",
            );
            query.push_values(batch, |mut row, (blueprint_type_id, line)| {
                row.push_bind(import_id)
                    .push_bind(*blueprint_type_id)
                    .push_bind(line.blueprint_type_id)
                    .push_bind(line.base_runs)
                    .push_bind(line.probability)
                    .push_bind(line.position);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        let skills = dataset
            .inventions
            .iter()
            .flat_map(|invention| {
                invention
                    .skills
                    .iter()
                    .map(move |skill| (invention.blueprint_type_id, skill))
            })
            .collect::<Vec<_>>();
        for batch in skills.chunks(BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_blueprint_invention_skills \
                 (import_id, blueprint_type_id, skill_type_id, level) ",
            );
            query.push_values(batch, |mut row, (blueprint_type_id, skill)| {
                row.push_bind(import_id)
                    .push_bind(*blueprint_type_id)
                    .push_bind(skill.skill_type_id)
                    .push_bind(skill.level);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        for batch in dataset.blueprint_copying.chunks(BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_blueprint_copying \
                 (import_id, blueprint_type_id, duration_seconds) ",
            );
            query.push_values(batch, |mut row, copying| {
                row.push_bind(import_id)
                    .push_bind(copying.blueprint_type_id)
                    .push_bind(copying.duration_seconds);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

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
              JOIN sde_blueprint_invention invention ON invention.import_id = import.id
              WHERE import.active = true
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
            JOIN sde_blueprint_invention_products product ON product.import_id = import.id
            JOIN sde_blueprint_invention invention
              ON invention.import_id = product.import_id
             AND invention.blueprint_type_id = product.blueprint_type_id
            JOIN sde_types source
              ON source.import_id = invention.import_id
             AND source.type_id = invention.blueprint_type_id
            LEFT JOIN sde_blueprint_copying copying
              ON copying.import_id = invention.import_id
             AND copying.blueprint_type_id = invention.blueprint_type_id
            WHERE import.active = true AND product.product_blueprint_type_id = $1
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
            JOIN sde_blueprint_invention_materials line ON line.import_id = import.id
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
            SELECT line.product_blueprint_type_id, blueprint.name_en,
                   built.product_type_id, built.product_name,
                   line.base_runs, line.probability
            FROM sde_imports import
            JOIN sde_blueprint_invention_products line ON line.import_id = import.id
            JOIN sde_types blueprint
              ON blueprint.import_id = line.import_id
             AND blueprint.type_id = line.product_blueprint_type_id
            LEFT JOIN LATERAL (
              SELECT p.product_type_id, item.name_en AS product_name
              FROM sde_blueprint_products p
              JOIN sde_types item
                ON item.import_id = p.import_id AND item.type_id = p.product_type_id
              WHERE p.import_id = line.import_id
                AND p.blueprint_type_id = line.product_blueprint_type_id
              ORDER BY p.position, p.product_type_id
              LIMIT 1
            ) built ON true
            WHERE import.active = true AND line.blueprint_type_id = $1
            ORDER BY line.position, line.product_blueprint_type_id
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
            JOIN sde_blueprint_invention_skills line ON line.import_id = import.id
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
                   line.product_blueprint_type_id,
                   line.blueprint_type_id, source.name_en,
                   (SELECT count(*)
                    FROM sde_blueprint_invention_products sibling
                    WHERE sibling.import_id = line.import_id
                      AND sibling.blueprint_type_id = line.blueprint_type_id),
                   CASE WHEN lower(item.name_en) = lower($2) THEN 0 ELSE 1 END AS exact
            FROM sde_imports import
            JOIN sde_blueprint_invention_products line ON line.import_id = import.id
            JOIN sde_types source
              ON source.import_id = line.import_id
             AND source.type_id = line.blueprint_type_id
            JOIN sde_blueprint_products built
              ON built.import_id = line.import_id
             AND built.blueprint_type_id = line.product_blueprint_type_id
            JOIN sde_types item
              ON item.import_id = built.import_id
             AND item.type_id = built.product_type_id
            WHERE import.active = true
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
