use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use thiserror::Error;

use crate::owner::{Owner, OwnerId, OwnerKind};
use crate::workspace::{Workspace, WorkspaceId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateWorkspaceCommand {
    pub name: String,
}

impl CreateWorkspaceCommand {
    pub fn validate(&self) -> Result<String, FieldErrors> {
        let trimmed = self.name.trim();
        if trimmed.is_empty() {
            let mut fields = BTreeMap::new();
            fields.insert(
                "name".to_string(),
                "Workspace name is required.".to_string(),
            );
            return Err(FieldErrors { fields });
        }
        Ok(trimmed.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceState {
    pub configured: bool,
    pub workspace: Option<Workspace>,
    pub owner: Option<Owner>,
}

impl WorkspaceState {
    pub fn unconfigured() -> Self {
        Self {
            configured: false,
            workspace: None,
            owner: None,
        }
    }

    pub fn configured(workspace: Workspace, owner: Owner) -> Self {
        Self {
            configured: true,
            workspace: Some(workspace),
            owner: Some(owner),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewWorkspace {
    pub workspace: Workspace,
    pub owner: Owner,
}

impl NewWorkspace {
    pub fn manual(name: String) -> Self {
        let now = Utc::now();
        let workspace_id = WorkspaceId::new();
        let owner_id = OwnerId::new();
        Self {
            workspace: Workspace {
                id: workspace_id,
                name: name.clone(),
                owner_id,
                default_market_region_id: None,
                default_market_location_id: None,
                created_at: now,
                updated_at: now,
            },
            owner: Owner {
                id: owner_id,
                workspace_id,
                kind: OwnerKind::Manual,
                display_name: name,
                eve_owner_id: None,
                hidden: true,
                created_at: now,
                updated_at: now,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldErrors {
    pub fields: BTreeMap<String, String>,
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("validation failed")]
    Validation(FieldErrors),
    #[error("workspace is already configured")]
    WorkspaceAlreadyConfigured,
    #[error("persistence failed: {0}")]
    Persistence(String),
}

#[async_trait]
pub trait WorkspaceRepository: Send + Sync {
    /// The single-tenant legacy lookup — "the" workspace, oldest first.
    /// Unsafe to use once more than one workspace can exist; kept only for
    /// the no-EVE-SSO-configured fallback path. Multi-tenant call sites must
    /// use `get_workspace_state_by_id` instead.
    async fn get_workspace_state(&self) -> Result<WorkspaceState, AppError>;
    async fn get_workspace_state_by_id(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<WorkspaceState, AppError>;
    async fn create_workspace(
        &self,
        new_workspace: NewWorkspace,
    ) -> Result<WorkspaceState, AppError>;
}

#[async_trait]
impl<T> WorkspaceRepository for Arc<T>
where
    T: WorkspaceRepository + ?Sized,
{
    async fn get_workspace_state(&self) -> Result<WorkspaceState, AppError> {
        (**self).get_workspace_state().await
    }

    async fn get_workspace_state_by_id(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<WorkspaceState, AppError> {
        (**self).get_workspace_state_by_id(workspace_id).await
    }

    async fn create_workspace(
        &self,
        new_workspace: NewWorkspace,
    ) -> Result<WorkspaceState, AppError> {
        (**self).create_workspace(new_workspace).await
    }
}

#[derive(Debug)]
pub struct WorkspaceService<R> {
    repository: R,
}

impl<R> WorkspaceService<R>
where
    R: WorkspaceRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn get_workspace_state(&self) -> Result<WorkspaceState, AppError> {
        self.repository.get_workspace_state().await
    }

    pub async fn get_workspace_state_by_id(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<WorkspaceState, AppError> {
        self.repository
            .get_workspace_state_by_id(workspace_id)
            .await
    }

    pub async fn create_workspace(
        &self,
        command: CreateWorkspaceCommand,
    ) -> Result<WorkspaceState, AppError> {
        let name = command.validate().map_err(AppError::Validation)?;
        let new_workspace = NewWorkspace::manual(name);
        self.repository.create_workspace(new_workspace).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn workspace_name_validation_trims_and_rejects_empty() {
        let valid = CreateWorkspaceCommand {
            name: "  Personal Industry  ".to_string(),
        };
        assert_eq!(valid.validate().unwrap(), "Personal Industry");

        let invalid = CreateWorkspaceCommand {
            name: "   ".to_string(),
        };
        let err = invalid.validate().unwrap_err();
        assert_eq!(
            err.fields.get("name").map(String::as_str),
            Some("Workspace name is required.")
        );
    }

    #[test]
    fn new_workspace_creates_hidden_manual_owner() {
        let new_workspace = NewWorkspace::manual("Personal Industry".to_string());

        assert_eq!(new_workspace.workspace.name, "Personal Industry");
        assert_eq!(new_workspace.owner.display_name, "Personal Industry");
        assert_eq!(new_workspace.owner.workspace_id, new_workspace.workspace.id);
        assert_eq!(new_workspace.owner.id, new_workspace.workspace.owner_id);
        assert_eq!(new_workspace.owner.kind, OwnerKind::Manual);
        assert!(new_workspace.owner.hidden);
    }

    #[tokio::test]
    async fn workspace_service_rejects_duplicate_setup() {
        #[derive(Clone, Default)]
        struct MemoryRepo {
            state: Arc<Mutex<Option<WorkspaceState>>>,
        }

        #[async_trait]
        impl WorkspaceRepository for MemoryRepo {
            async fn get_workspace_state(&self) -> Result<WorkspaceState, AppError> {
                Ok(self
                    .state
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or_else(WorkspaceState::unconfigured))
            }

            async fn get_workspace_state_by_id(
                &self,
                _workspace_id: WorkspaceId,
            ) -> Result<WorkspaceState, AppError> {
                self.get_workspace_state().await
            }

            async fn create_workspace(
                &self,
                new_workspace: NewWorkspace,
            ) -> Result<WorkspaceState, AppError> {
                let mut state = self.state.lock().unwrap();
                if state.is_some() {
                    return Err(AppError::WorkspaceAlreadyConfigured);
                }
                let configured =
                    WorkspaceState::configured(new_workspace.workspace, new_workspace.owner);
                *state = Some(configured.clone());
                Ok(configured)
            }
        }

        let service = WorkspaceService::new(MemoryRepo::default());
        let first = service
            .create_workspace(CreateWorkspaceCommand {
                name: "Industry".to_string(),
            })
            .await
            .unwrap();
        assert!(first.configured);

        let second = service
            .create_workspace(CreateWorkspaceCommand {
                name: "Other".to_string(),
            })
            .await
            .unwrap_err();
        assert!(matches!(second, AppError::WorkspaceAlreadyConfigured));
    }
}
