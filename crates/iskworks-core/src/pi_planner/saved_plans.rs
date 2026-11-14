//! PI plans saved as frozen snapshots. The snapshot is the solve request and
//! response exactly as the player saw them; the app layer owns their shape,
//! so here they are opaque JSON. Only the set-up checklist and the name change
//! after saving.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use uuid::Uuid;

use crate::{StoreError, WorkspaceId};

/// Longest saved plan name, in characters.
pub const SAVED_PLAN_NAME_MAX: usize = 120;

/// One saved plan as the list shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedPlanSummary {
    pub id: Uuid,
    pub name: String,
    pub isk_per_month: Decimal,
    pub colonies: i32,
    /// How many colonies are ticked as set up.
    pub set_up: i32,
    pub home_system_id: i64,
    pub home_system_name: Option<String>,
    pub price_observed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A saved plan with its snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedPlan {
    pub summary: SavedPlanSummary,
    pub snapshot_version: i32,
    pub scenario: Value,
    pub solution: Value,
    /// Indexes into the snapshot's colonies, ascending.
    pub set_up_colonies: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewSavedPlan {
    pub name: String,
    pub snapshot_version: i32,
    pub scenario: Value,
    pub solution: Value,
    pub isk_per_month: Decimal,
    pub colonies: i32,
    pub home_system_id: i64,
    pub home_system_name: Option<String>,
    pub price_observed_at: Option<DateTime<Utc>>,
}

/// Fields left `None` keep their value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SavedPlanUpdate {
    pub name: Option<String>,
    pub set_up_colonies: Option<Vec<i32>>,
}

/// A trimmed name of 1 to [`SAVED_PLAN_NAME_MAX`] characters.
pub fn saved_plan_name(name: &str) -> Result<String, StoreError> {
    let name = name.trim();
    let length = name.chars().count();
    if length == 0 || length > SAVED_PLAN_NAME_MAX {
        return Err(StoreError::Validation(format!(
            "name must be 1 to {SAVED_PLAN_NAME_MAX} characters"
        )));
    }
    Ok(name.to_string())
}

/// Set-up colonies, sorted: each a colony index below `colonies`, none twice.
pub fn set_up_colonies(indexes: &[i32], colonies: i32) -> Result<Vec<i32>, StoreError> {
    let mut sorted = indexes.to_vec();
    sorted.sort_unstable();
    let duplicated = sorted.windows(2).any(|pair| pair[0] == pair[1]);
    if duplicated || sorted.iter().any(|index| *index < 0 || *index >= colonies) {
        return Err(StoreError::Validation(
            "setUpColonies must be distinct colony indexes of this plan".to_string(),
        ));
    }
    Ok(sorted)
}

#[async_trait]
pub trait PiSavedPlansRepository: Send + Sync {
    /// The workspace's plans, most recently updated first.
    async fn saved_plans(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<SavedPlanSummary>, StoreError>;
    /// `NotFound` when the plan is missing or in another workspace.
    async fn saved_plan(
        &self,
        workspace_id: WorkspaceId,
        id: Uuid,
    ) -> Result<SavedPlan, StoreError>;
    async fn create_saved_plan(
        &self,
        workspace_id: WorkspaceId,
        plan: NewSavedPlan,
    ) -> Result<SavedPlan, StoreError>;
    /// Bumps `updated_at`. `NotFound` like [`Self::saved_plan`].
    async fn update_saved_plan(
        &self,
        workspace_id: WorkspaceId,
        id: Uuid,
        update: SavedPlanUpdate,
    ) -> Result<SavedPlan, StoreError>;
    /// `NotFound` like [`Self::saved_plan`].
    async fn delete_saved_plan(
        &self,
        workspace_id: WorkspaceId,
        id: Uuid,
    ) -> Result<(), StoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_trimmed_and_bounded() {
        assert_eq!(
            saved_plan_name("  Q-3HS5 · Robotics ").unwrap(),
            "Q-3HS5 · Robotics"
        );
        assert!(saved_plan_name("   ").is_err());
        assert!(saved_plan_name(&"x".repeat(SAVED_PLAN_NAME_MAX)).is_ok());
        assert!(saved_plan_name(&"x".repeat(SAVED_PLAN_NAME_MAX + 1)).is_err());
        // Characters, not bytes.
        assert!(saved_plan_name(&"·".repeat(SAVED_PLAN_NAME_MAX)).is_ok());
    }

    #[test]
    fn set_up_colonies_are_distinct_indexes_in_range() {
        assert_eq!(set_up_colonies(&[3, 0, 1], 4).unwrap(), vec![0, 1, 3]);
        assert_eq!(set_up_colonies(&[], 4).unwrap(), Vec::<i32>::new());
        assert!(set_up_colonies(&[1, 1], 4).is_err());
        assert!(set_up_colonies(&[4], 4).is_err());
        assert!(set_up_colonies(&[-1], 4).is_err());
    }
}
