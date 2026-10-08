use iskworks_core::classify_all;
use sqlx::PgPool;
use uuid::Uuid;

use super::sde_import::PgSdeRepository;
use super::sde_read::map_sde_sqlx_error;
use super::SdeError;

impl PgSdeRepository {
    /// Recompute `sde_type_categories` for one import from its stored market
    /// groups and types. Idempotent; replaces any existing rows.
    pub async fn rebuild_type_categories(&self, import_id: Uuid) -> Result<u64, SdeError> {
        rebuild_type_categories(&self.pool, import_id).await
    }

    /// Self-heal: an active import written before `sde_type_categories`
    /// existed (or by a run that skipped the rebuild) gets its categories
    /// computed without a full SDE re-import. Returns the rows written, or
    /// `None` when there was nothing to do.
    pub async fn ensure_active_type_categories(&self) -> Result<Option<u64>, SdeError> {
        let missing: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT i.id FROM sde_imports i
            WHERE i.active
              AND NOT EXISTS (SELECT 1 FROM sde_type_categories c WHERE c.import_id = i.id)
              AND EXISTS (SELECT 1 FROM sde_types t WHERE t.import_id = i.id)
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        match missing {
            Some(import_id) => Ok(Some(rebuild_type_categories(&self.pool, import_id).await?)),
            None => Ok(None),
        }
    }
}

async fn rebuild_type_categories(pool: &PgPool, import_id: Uuid) -> Result<u64, SdeError> {
    let groups: Vec<(i64, String, Option<i64>)> = sqlx::query_as(
        "SELECT market_group_id, name_en, parent_group_id FROM sde_market_groups WHERE import_id = $1",
    )
    .bind(import_id)
    .fetch_all(pool)
    .await
    .map_err(map_sde_sqlx_error)?;
    let types: Vec<(i64, Option<i64>, Option<String>)> = sqlx::query_as(
        "SELECT type_id, market_group_id, group_name_en FROM sde_types WHERE import_id = $1",
    )
    .bind(import_id)
    .fetch_all(pool)
    .await
    .map_err(map_sde_sqlx_error)?;
    let classified = classify_all(&groups, &types);
    let ids: Vec<i64> = classified.iter().map(|(id, _)| *id).collect();
    let categories: Vec<&str> = classified.iter().map(|(_, category)| *category).collect();

    let mut tx = pool.begin().await.map_err(map_sde_sqlx_error)?;
    sqlx::query("DELETE FROM sde_type_categories WHERE import_id = $1")
        .bind(import_id)
        .execute(&mut *tx)
        .await
        .map_err(map_sde_sqlx_error)?;
    sqlx::query(
        r#"
        INSERT INTO sde_type_categories (import_id, type_id, category)
        SELECT $1, type_id, category FROM UNNEST($2::bigint[], $3::text[]) AS t(type_id, category)
        "#,
    )
    .bind(import_id)
    .bind(&ids)
    .bind(&categories)
    .execute(&mut *tx)
    .await
    .map_err(map_sde_sqlx_error)?;
    tx.commit().await.map_err(map_sde_sqlx_error)?;
    Ok(ids.len() as u64)
}
