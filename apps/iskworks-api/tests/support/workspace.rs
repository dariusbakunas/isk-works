//! Shared workspace fake.

use async_trait::async_trait;
use iskworks_core::{
    AppError, NewWorkspace, OwnerId, WorkspaceId, WorkspaceRepository, WorkspaceState,
};

/// A `WorkspaceRepository` that always reports one already-configured
/// workspace and refuses re-creation. Used by every suite that just needs the
/// workspace context to exist so routing and request validation can run.
#[derive(Clone)]
pub struct ConfiguredWorkspaceRepository {
    pub state: WorkspaceState,
}

impl ConfiguredWorkspaceRepository {
    pub fn new(state: WorkspaceState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl WorkspaceRepository for ConfiguredWorkspaceRepository {
    async fn get_workspace_state(&self) -> Result<WorkspaceState, AppError> {
        Ok(self.state.clone())
    }

    async fn get_workspace_state_by_id(
        &self,
        _workspace_id: WorkspaceId,
    ) -> Result<WorkspaceState, AppError> {
        Ok(self.state.clone())
    }

    async fn create_workspace(
        &self,
        _new_workspace: NewWorkspace,
    ) -> Result<WorkspaceState, AppError> {
        Err(AppError::WorkspaceAlreadyConfigured)
    }
}

/// A configured workspace whose owner (and the workspace's `owner_id`) is
/// `owner_id`, for suites whose fixtures are owned by a known owner.
pub fn configured_workspace_owned_by(
    name: &str,
    owner_id: OwnerId,
) -> ConfiguredWorkspaceRepository {
    let mut workspace = NewWorkspace::manual(name.to_string());
    workspace.workspace.owner_id = owner_id;
    workspace.owner.id = owner_id;
    ConfiguredWorkspaceRepository::new(WorkspaceState::configured(
        workspace.workspace,
        workspace.owner,
    ))
}

/// Convenience constructor: a configured single-owner manual workspace with the
/// given display name.
pub fn configured_workspace(name: &str) -> ConfiguredWorkspaceRepository {
    let workspace = NewWorkspace::manual(name.to_string());
    ConfiguredWorkspaceRepository::new(WorkspaceState::configured(
        workspace.workspace,
        workspace.owner,
    ))
}

/// A `WorkspaceRepository` with no workspace configured that refuses creation.
pub struct EmptyWorkspaceRepository;

#[async_trait]
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
