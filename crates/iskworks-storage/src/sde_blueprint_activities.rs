//! A blueprint's industry activities: written with an SDE import, read for
//! the Type Info window.

use super::sde_import::PgSdeRepository;
use super::sde_read::map_sde_sqlx_error;
use super::*;
use iskworks_sde::{
    ActivityMaterial, ActivityProduct, BlueprintActivities, BlueprintActivity,
    BlueprintActivityKind, RequiredSkill,
};
use rust_decimal::Decimal;

const BATCH_SIZE: usize = 1_000;

fn stored_kind(activity: &str) -> Result<BlueprintActivityKind, SdeError> {
    activity.parse().map_err(SdeError::Storage)
}

/// The activity a material, product or skill line belongs to.
fn slot<'a>(
    activities: &'a mut [BlueprintActivity],
    activity: &str,
) -> Result<&'a mut BlueprintActivity, SdeError> {
    let kind = stored_kind(activity)?;
    activities
        .iter_mut()
        .find(|candidate| candidate.kind == kind)
        .ok_or_else(|| SdeError::Storage(format!("{activity} line without its activity")))
}

impl PgSdeRepository {
    pub(super) async fn write_blueprint_activities(
        &self,
        import_id: Uuid,
        dataset: &NormalizedSde,
    ) -> Result<(), SdeError> {
        for batch in dataset.blueprint_activities.chunks(BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_blueprint_activities \
                 (import_id, blueprint_type_id, activity, duration_seconds) ",
            );
            query.push_values(batch, |mut row, activity| {
                row.push_bind(import_id)
                    .push_bind(activity.blueprint_type_id)
                    .push_bind(activity.kind.as_str())
                    .push_bind(activity.duration_seconds);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        let materials = dataset
            .blueprint_activities
            .iter()
            .flat_map(|activity| activity.materials.iter().map(move |line| (activity, line)))
            .collect::<Vec<_>>();
        for batch in materials.chunks(BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_blueprint_activity_materials \
                 (import_id, blueprint_type_id, activity, material_type_id, quantity, position) ",
            );
            query.push_values(batch, |mut row, (activity, line)| {
                row.push_bind(import_id)
                    .push_bind(activity.blueprint_type_id)
                    .push_bind(activity.kind.as_str())
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
            .blueprint_activities
            .iter()
            .flat_map(|activity| activity.products.iter().map(move |line| (activity, line)))
            .collect::<Vec<_>>();
        for batch in products.chunks(BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_blueprint_activity_products \
                 (import_id, blueprint_type_id, activity, product_type_id, quantity, \
                  probability, position) ",
            );
            query.push_values(batch, |mut row, (activity, line)| {
                row.push_bind(import_id)
                    .push_bind(activity.blueprint_type_id)
                    .push_bind(activity.kind.as_str())
                    .push_bind(line.type_id)
                    .push_bind(line.quantity)
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
            .blueprint_activities
            .iter()
            .flat_map(|activity| activity.skills.iter().map(move |skill| (activity, skill)))
            .collect::<Vec<_>>();
        for batch in skills.chunks(BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_blueprint_activity_skills \
                 (import_id, blueprint_type_id, activity, skill_type_id, level, position) ",
            );
            query.push_values(batch, |mut row, (activity, skill)| {
                row.push_bind(import_id)
                    .push_bind(activity.blueprint_type_id)
                    .push_bind(activity.kind.as_str())
                    .push_bind(skill.skill_type_id)
                    .push_bind(skill.level)
                    .push_bind(skill.position);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        for batch in dataset.blueprint_limits.chunks(BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_blueprint_limits \
                 (import_id, blueprint_type_id, max_production_limit) ",
            );
            query.push_values(batch, |mut row, limit| {
                row.push_bind(import_id)
                    .push_bind(limit.blueprint_type_id)
                    .push_bind(limit.max_production_limit);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }
        Ok(())
    }

    pub(super) async fn read_blueprint_activities(
        &self,
        blueprint_type_id: i64,
    ) -> Result<Option<BlueprintActivities>, SdeError> {
        let headers = sqlx::query_as::<_, (String, Option<i64>)>(
            r#"
            SELECT activity.activity, activity.duration_seconds
            FROM sde_imports import
            JOIN sde_blueprint_activities activity ON activity.import_id = import.id
            WHERE import.active = true AND activity.blueprint_type_id = $1
            "#,
        )
        .bind(blueprint_type_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        if headers.is_empty() {
            return Ok(None);
        }

        let materials =
            sqlx::query_as::<_, (String, i64, String, i64, Option<i64>, Option<String>)>(
                r#"
            SELECT line.activity, line.material_type_id, item.name_en, line.quantity,
                   item.group_id, item.group_name_en
            FROM sde_imports import
            JOIN sde_blueprint_activity_materials line ON line.import_id = import.id
            JOIN sde_types item
              ON item.import_id = line.import_id
             AND item.type_id = line.material_type_id
            WHERE import.active = true AND line.blueprint_type_id = $1
            ORDER BY line.position, line.material_type_id
            "#,
            )
            .bind(blueprint_type_id)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sde_sqlx_error)?;

        let products = sqlx::query_as::<_, (String, i64, String, i64, Option<Decimal>)>(
            r#"
            SELECT line.activity, line.product_type_id, item.name_en, line.quantity,
                   line.probability
            FROM sde_imports import
            JOIN sde_blueprint_activity_products line ON line.import_id = import.id
            JOIN sde_types item
              ON item.import_id = line.import_id
             AND item.type_id = line.product_type_id
            WHERE import.active = true AND line.blueprint_type_id = $1
            ORDER BY line.position, line.product_type_id
            "#,
        )
        .bind(blueprint_type_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;

        let skills = sqlx::query_as::<_, (String, i64, i16)>(
            r#"
            SELECT line.activity, line.skill_type_id, line.level
            FROM sde_imports import
            JOIN sde_blueprint_activity_skills line ON line.import_id = import.id
            WHERE import.active = true AND line.blueprint_type_id = $1
            ORDER BY line.position, line.skill_type_id
            "#,
        )
        .bind(blueprint_type_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;

        let max_production_limit = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT limits.max_production_limit
            FROM sde_imports import
            JOIN sde_blueprint_limits limits ON limits.import_id = import.id
            WHERE import.active = true AND limits.blueprint_type_id = $1
            "#,
        )
        .bind(blueprint_type_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;

        let mut activities = headers
            .into_iter()
            .map(|(activity, duration_seconds)| {
                Ok(BlueprintActivity {
                    kind: stored_kind(&activity)?,
                    duration_seconds,
                    materials: Vec::new(),
                    products: Vec::new(),
                    skills: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>, SdeError>>()?;
        activities.sort_by_key(|activity| activity.kind);
        for (activity, type_id, type_name, quantity, group_id, group_name) in materials {
            slot(&mut activities, &activity)?
                .materials
                .push(ActivityMaterial {
                    type_id,
                    type_name,
                    quantity,
                    group_id,
                    group_name,
                });
        }
        for (activity, type_id, type_name, quantity, probability) in products {
            slot(&mut activities, &activity)?
                .products
                .push(ActivityProduct {
                    type_id,
                    type_name,
                    quantity,
                    probability,
                });
        }
        for (activity, skill_type_id, level) in skills {
            slot(&mut activities, &activity)?
                .skills
                .push(RequiredSkill {
                    skill_type_id,
                    level,
                });
        }

        Ok(Some(BlueprintActivities {
            max_production_limit,
            activities,
        }))
    }
}
