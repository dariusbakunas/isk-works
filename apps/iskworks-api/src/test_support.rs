//! Fakes shared by this crate's unit tests.

use iskworks_core::{AppError, NewWorkspace, WorkspaceId, WorkspaceRepository, WorkspaceState};

/// A `WorkspaceRepository` with no workspace configured that refuses creation.
pub(crate) struct EmptyWorkspaceRepository;

#[async_trait::async_trait]
impl WorkspaceRepository for EmptyWorkspaceRepository {
    async fn get_workspace_state(&self) -> Result<WorkspaceState, AppError> {
        Ok(WorkspaceState::unconfigured())
    }

    async fn get_workspace_state_by_id(
        &self,
        _workspace_id: WorkspaceId,
    ) -> Result<WorkspaceState, AppError> {
        Ok(WorkspaceState::unconfigured())
    }

    async fn create_workspace(
        &self,
        _new_workspace: NewWorkspace,
    ) -> Result<WorkspaceState, AppError> {
        unreachable!()
    }
}
