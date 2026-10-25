use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_core::skill_plans::{
    PlanEntry, SkillPlan, SkillPlanId, SkillPlanInput, SkillPlanRepository,
};
use iskworks_core::{InventoryError, WorkspaceId};
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

#[derive(Clone)]
pub struct PgSkillPlanRepository {
    pool: PgPool,
}

impl PgSkillPlanRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn persistence(error: sqlx::Error) -> InventoryError {
    InventoryError::Persistence(error.to_string())
}

#[derive(sqlx::FromRow)]
struct PlanRow {
    id: Uuid,
    name: String,
    description: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct EntryRow {
    plan_id: Uuid,
    skill_type_id: i64,
    level: i16,
}

impl PgSkillPlanRepository {
    /// The plans' entries in position order, attached to their plans.
    async fn with_entries(&self, rows: Vec<PlanRow>) -> Result<Vec<SkillPlan>, InventoryError> {
        let ids = rows.iter().map(|row| row.id).collect::<Vec<_>>();
        let mut entries = HashMap::<Uuid, Vec<PlanEntry>>::new();
        if !ids.is_empty() {
            let entry_rows = sqlx::query_as::<_, EntryRow>(
                "SELECT plan_id, skill_type_id, level
                 FROM skill_plan_entries
                 WHERE plan_id = ANY($1)
                 ORDER BY plan_id, position",
            )
            .bind(&ids)
            .fetch_all(&self.pool)
            .await
            .map_err(persistence)?;
            for row in entry_rows {
                entries.entry(row.plan_id).or_default().push(PlanEntry {
                    skill_type_id: row.skill_type_id,
                    level: row.level,
                });
            }
        }
        Ok(rows
            .into_iter()
            .map(|row| SkillPlan {
                entries: entries.remove(&row.id).unwrap_or_default(),
                id: SkillPlanId(row.id),
                name: row.name,
                description: row.description,
                created_at: row.created_at,
                updated_at: row.updated_at,
            })
            .collect())
    }
}

async fn insert_entries(
    tx: &mut Transaction<'_, Postgres>,
    plan_id: Uuid,
    entries: &[PlanEntry],
) -> Result<(), InventoryError> {
    if entries.is_empty() {
        return Ok(());
    }
    let mut query = QueryBuilder::<Postgres>::new(
        "INSERT INTO skill_plan_entries (plan_id, position, skill_type_id, level) ",
    );
    query.push_values(entries.iter().enumerate(), |mut row, (position, entry)| {
        row.push_bind(plan_id)
            .push_bind(position as i32)
            .push_bind(entry.skill_type_id)
            .push_bind(entry.level);
    });
    query
        .build()
        .execute(&mut **tx)
        .await
        .map_err(persistence)?;
    Ok(())
}

const PLAN_COLUMNS: &str = "id, name, description, created_at, updated_at";

#[async_trait]
impl SkillPlanRepository for PgSkillPlanRepository {
    async fn list_skill_plans(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<SkillPlan>, InventoryError> {
        let rows = sqlx::query_as::<_, PlanRow>(&format!(
            "SELECT {PLAN_COLUMNS} FROM skill_plans
             WHERE workspace_id = $1
             ORDER BY lower(name), id"
        ))
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(persistence)?;
        self.with_entries(rows).await
    }

    async fn skill_plan(
        &self,
        workspace_id: WorkspaceId,
        id: SkillPlanId,
    ) -> Result<Option<SkillPlan>, InventoryError> {
        let row = sqlx::query_as::<_, PlanRow>(&format!(
            "SELECT {PLAN_COLUMNS} FROM skill_plans WHERE workspace_id = $1 AND id = $2"
        ))
        .bind(workspace_id.0)
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(persistence)?;
        Ok(self.with_entries(row.into_iter().collect()).await?.pop())
    }

    async fn create_skill_plan(
        &self,
        workspace_id: WorkspaceId,
        input: &SkillPlanInput,
        now: DateTime<Utc>,
    ) -> Result<SkillPlan, InventoryError> {
        let id = Uuid::new_v4();
        let mut tx = self.pool.begin().await.map_err(persistence)?;
        let row = sqlx::query_as::<_, PlanRow>(&format!(
            "INSERT INTO skill_plans (id, workspace_id, name, description, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $5)
             RETURNING {PLAN_COLUMNS}"
        ))
        .bind(id)
        .bind(workspace_id.0)
        .bind(&input.name)
        .bind(&input.description)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(persistence)?;
        insert_entries(&mut tx, id, &input.entries).await?;
        tx.commit().await.map_err(persistence)?;
        Ok(SkillPlan {
            id: SkillPlanId(row.id),
            name: row.name,
            description: row.description,
            entries: input.entries.clone(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    async fn update_skill_plan(
        &self,
        workspace_id: WorkspaceId,
        id: SkillPlanId,
        input: &SkillPlanInput,
        now: DateTime<Utc>,
    ) -> Result<Option<SkillPlan>, InventoryError> {
        let mut tx = self.pool.begin().await.map_err(persistence)?;
        let Some(row) = sqlx::query_as::<_, PlanRow>(&format!(
            "UPDATE skill_plans SET name = $3, description = $4, updated_at = $5
             WHERE workspace_id = $1 AND id = $2
             RETURNING {PLAN_COLUMNS}"
        ))
        .bind(workspace_id.0)
        .bind(id.0)
        .bind(&input.name)
        .bind(&input.description)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await
        .map_err(persistence)?
        else {
            return Ok(None);
        };
        sqlx::query("DELETE FROM skill_plan_entries WHERE plan_id = $1")
            .bind(id.0)
            .execute(&mut *tx)
            .await
            .map_err(persistence)?;
        insert_entries(&mut tx, id.0, &input.entries).await?;
        tx.commit().await.map_err(persistence)?;
        Ok(Some(SkillPlan {
            id,
            name: row.name,
            description: row.description,
            entries: input.entries.clone(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }))
    }

    async fn delete_skill_plan(
        &self,
        workspace_id: WorkspaceId,
        id: SkillPlanId,
    ) -> Result<bool, InventoryError> {
        let deleted = sqlx::query("DELETE FROM skill_plans WHERE workspace_id = $1 AND id = $2")
            .bind(workspace_id.0)
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(persistence)?;
        Ok(deleted.rows_affected() > 0)
    }
}
