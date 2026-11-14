//! PI Planner plans saved as frozen snapshots, per workspace.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_core::pi_planner::{
    NewSavedPlan, PiSavedPlansRepository, SavedPlan, SavedPlanSummary, SavedPlanUpdate,
};
use iskworks_core::{StoreError, WorkspaceId};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::types::Json;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct PgPiSavedPlansRepository {
    pool: PgPool,
}

impl PgPiSavedPlansRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn persistence(error: sqlx::Error) -> StoreError {
    StoreError::Persistence(error.to_string())
}

const SUMMARY_COLUMNS: &str = "id, name, isk_per_month, colonies,
    cardinality(set_up_colonies) AS set_up, home_system_id, home_system_name,
    price_observed_at, created_at, updated_at";
const PLAN_COLUMNS: &str = "id, name, isk_per_month, colonies,
    cardinality(set_up_colonies) AS set_up, home_system_id, home_system_name,
    price_observed_at, created_at, updated_at,
    snapshot_version, scenario, solution, set_up_colonies";

#[derive(sqlx::FromRow)]
struct SummaryRow {
    id: Uuid,
    name: String,
    isk_per_month: Decimal,
    colonies: i32,
    set_up: i32,
    home_system_id: i64,
    home_system_name: Option<String>,
    price_observed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<SummaryRow> for SavedPlanSummary {
    fn from(row: SummaryRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            isk_per_month: row.isk_per_month,
            colonies: row.colonies,
            set_up: row.set_up,
            home_system_id: row.home_system_id,
            home_system_name: row.home_system_name,
            price_observed_at: row.price_observed_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct PlanRow {
    #[sqlx(flatten)]
    summary: SummaryRow,
    snapshot_version: i32,
    scenario: Json<Value>,
    solution: Json<Value>,
    set_up_colonies: Vec<i32>,
}

impl From<PlanRow> for SavedPlan {
    fn from(row: PlanRow) -> Self {
        Self {
            summary: row.summary.into(),
            snapshot_version: row.snapshot_version,
            scenario: row.scenario.0,
            solution: row.solution.0,
            set_up_colonies: row.set_up_colonies,
        }
    }
}

#[async_trait]
impl PiSavedPlansRepository for PgPiSavedPlansRepository {
    async fn saved_plans(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<SavedPlanSummary>, StoreError> {
        let rows: Vec<SummaryRow> = sqlx::query_as(&format!(
            "SELECT {SUMMARY_COLUMNS} FROM pi_saved_plans
             WHERE workspace_id = $1
             ORDER BY updated_at DESC, id"
        ))
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(persistence)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn saved_plan(
        &self,
        workspace_id: WorkspaceId,
        id: Uuid,
    ) -> Result<SavedPlan, StoreError> {
        let row: Option<PlanRow> = sqlx::query_as(&format!(
            "SELECT {PLAN_COLUMNS} FROM pi_saved_plans WHERE workspace_id = $1 AND id = $2"
        ))
        .bind(workspace_id.0)
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(persistence)?;
        row.map(Into::into).ok_or(StoreError::NotFound)
    }

    async fn create_saved_plan(
        &self,
        workspace_id: WorkspaceId,
        plan: NewSavedPlan,
    ) -> Result<SavedPlan, StoreError> {
        let row: PlanRow = sqlx::query_as(&format!(
            "INSERT INTO pi_saved_plans (workspace_id, name, snapshot_version, scenario, solution,
                 isk_per_month, colonies, home_system_id, home_system_name, price_observed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             RETURNING {PLAN_COLUMNS}"
        ))
        .bind(workspace_id.0)
        .bind(&plan.name)
        .bind(plan.snapshot_version)
        .bind(Json(&plan.scenario))
        .bind(Json(&plan.solution))
        .bind(plan.isk_per_month)
        .bind(plan.colonies)
        .bind(plan.home_system_id)
        .bind(&plan.home_system_name)
        .bind(plan.price_observed_at)
        .fetch_one(&self.pool)
        .await
        .map_err(persistence)?;
        Ok(row.into())
    }

    async fn update_saved_plan(
        &self,
        workspace_id: WorkspaceId,
        id: Uuid,
        update: SavedPlanUpdate,
    ) -> Result<SavedPlan, StoreError> {
        // clock_timestamp(): two updates in one transaction still order.
        let row: Option<PlanRow> = sqlx::query_as(&format!(
            "UPDATE pi_saved_plans
             SET name = COALESCE($3, name),
                 set_up_colonies = COALESCE($4, set_up_colonies),
                 updated_at = clock_timestamp()
             WHERE workspace_id = $1 AND id = $2
             RETURNING {PLAN_COLUMNS}"
        ))
        .bind(workspace_id.0)
        .bind(id)
        .bind(&update.name)
        .bind(&update.set_up_colonies)
        .fetch_optional(&self.pool)
        .await
        .map_err(persistence)?;
        row.map(Into::into).ok_or(StoreError::NotFound)
    }

    async fn delete_saved_plan(
        &self,
        workspace_id: WorkspaceId,
        id: Uuid,
    ) -> Result<(), StoreError> {
        let deleted = sqlx::query("DELETE FROM pi_saved_plans WHERE workspace_id = $1 AND id = $2")
            .bind(workspace_id.0)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(persistence)?
            .rows_affected();
        if deleted == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}
