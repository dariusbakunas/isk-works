//! Saved PI plans: a solve as the player saw it, kept so they can come back
//! to it while they build it, with the colonies they have set up so far.
//!
//! The snapshot is stored as received; saving only checks that it parses as
//! a solve request and response, so it can always be reopened.

use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use iskworks_core::pi_planner::{
    saved_plan_name, set_up_colonies, NewSavedPlan, PiSavedPlansRepository, SavedPlan,
    SavedPlanSummary, SavedPlanUpdate,
};
use iskworks_core::{StoreError, WorkspaceId};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::{PlannerScenarioDto, PlannerSolutionDto};

/// The shape of `scenario` and `solution` written today.
pub const SAVED_PLAN_SNAPSHOT_VERSION: i32 = 1;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePlanRequestDto {
    pub name: String,
    /// The solve request plus `characterNames` (and `eveCharacterIds` for
    /// characters prefilled from a connected one).
    pub scenario: Value,
    /// The solve response as shown.
    pub solution: Value,
}

/// What a saved scenario must parse as.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedScenario {
    #[serde(flatten)]
    scenario: PlannerScenarioDto,
    character_names: Vec<String>,
    #[serde(default)]
    eve_character_ids: Vec<Option<i64>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPlanUpdateDto {
    pub name: Option<String>,
    pub set_up_colonies: Option<Vec<i32>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPlanSummaryDto {
    pub id: Uuid,
    pub name: String,
    pub isk_per_month: String,
    pub colonies: i32,
    pub set_up: i32,
    pub home_system_id: i64,
    pub home_system_name: Option<String>,
    pub price_observed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPlanDto {
    #[serde(flatten)]
    pub summary: SavedPlanSummaryDto,
    pub snapshot_version: i32,
    pub scenario: Value,
    pub solution: Value,
    pub set_up_colonies: Vec<i32>,
}

impl From<SavedPlanSummary> for SavedPlanSummaryDto {
    fn from(summary: SavedPlanSummary) -> Self {
        Self {
            id: summary.id,
            name: summary.name,
            isk_per_month: summary.isk_per_month.to_string(),
            colonies: summary.colonies,
            set_up: summary.set_up,
            home_system_id: summary.home_system_id,
            home_system_name: summary.home_system_name,
            price_observed_at: summary.price_observed_at,
            created_at: summary.created_at,
            updated_at: summary.updated_at,
        }
    }
}

impl From<SavedPlan> for SavedPlanDto {
    fn from(plan: SavedPlan) -> Self {
        Self {
            summary: plan.summary.into(),
            snapshot_version: plan.snapshot_version,
            scenario: plan.scenario,
            solution: plan.solution,
            set_up_colonies: plan.set_up_colonies,
        }
    }
}

fn invalid(message: &str) -> StoreError {
    StoreError::Validation(message.to_string())
}

pub struct PiSavedPlansService {
    repository: Arc<dyn PiSavedPlansRepository>,
}

impl PiSavedPlansService {
    #[must_use]
    pub fn new(repository: Arc<dyn PiSavedPlansRepository>) -> Self {
        Self { repository }
    }

    /// Most recently updated first.
    pub async fn saved_plans(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<SavedPlanSummaryDto>, StoreError> {
        Ok(self
            .repository
            .saved_plans(workspace_id)
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    pub async fn saved_plan(
        &self,
        workspace_id: WorkspaceId,
        id: Uuid,
    ) -> Result<SavedPlanDto, StoreError> {
        Ok(self.repository.saved_plan(workspace_id, id).await?.into())
    }

    pub async fn save(
        &self,
        workspace_id: WorkspaceId,
        request: SavePlanRequestDto,
    ) -> Result<SavedPlanDto, StoreError> {
        let name = saved_plan_name(&request.name)?;
        let scenario: SavedScenario = serde_json::from_value(request.scenario.clone())
            .map_err(|_| invalid("scenario is not a PI Planner scenario"))?;
        let characters = scenario.scenario.characters.len();
        if scenario.character_names.len() != characters {
            return Err(invalid("characterNames must name every character"));
        }
        if !scenario.eve_character_ids.is_empty() && scenario.eve_character_ids.len() != characters
        {
            return Err(invalid("eveCharacterIds must cover every character"));
        }
        let solution: PlannerSolutionDto = serde_json::from_value(request.solution.clone())
            .map_err(|_| invalid("solution is not a PI Planner solve"))?;
        let plan = &solution.plan;
        if plan.colonies.is_empty() {
            return Err(invalid("the plan has no colonies to save"));
        }
        let colonies =
            i32::try_from(plan.colonies.len()).map_err(|_| invalid("the plan is too large"))?;
        let isk_per_month = Decimal::from_str(&plan.isk_per_month)
            .map_err(|_| invalid("the plan's ISK/month is not a number"))?;
        // The home system is the only one 0 jumps out.
        let home_system_name = plan
            .colonies
            .iter()
            .find(|colony| colony.jumps == 0 && !colony.system_name.is_empty())
            .map(|colony| colony.system_name.clone());
        let saved = self
            .repository
            .create_saved_plan(
                workspace_id,
                NewSavedPlan {
                    name,
                    snapshot_version: SAVED_PLAN_SNAPSHOT_VERSION,
                    scenario: request.scenario,
                    solution: request.solution,
                    isk_per_month,
                    colonies,
                    home_system_id: scenario.scenario.home_system_id,
                    home_system_name,
                    price_observed_at: solution.price_observed_at,
                },
            )
            .await?;
        Ok(saved.into())
    }

    /// Renames the plan and/or replaces its set-up colonies.
    pub async fn update(
        &self,
        workspace_id: WorkspaceId,
        id: Uuid,
        update: SavedPlanUpdateDto,
    ) -> Result<SavedPlanDto, StoreError> {
        let name = update.name.as_deref().map(saved_plan_name).transpose()?;
        let set_up = match update.set_up_colonies {
            Some(indexes) => {
                let plan = self.repository.saved_plan(workspace_id, id).await?;
                Some(set_up_colonies(&indexes, plan.summary.colonies)?)
            }
            None => None,
        };
        Ok(self
            .repository
            .update_saved_plan(
                workspace_id,
                id,
                SavedPlanUpdate {
                    name,
                    set_up_colonies: set_up,
                },
            )
            .await?
            .into())
    }

    pub async fn delete(&self, workspace_id: WorkspaceId, id: Uuid) -> Result<(), StoreError> {
        self.repository.delete_saved_plan(workspace_id, id).await
    }
}
