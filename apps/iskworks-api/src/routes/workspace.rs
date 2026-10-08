use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use iskworks_core::{CreateWorkspaceCommand, Owner, Workspace, WorkspaceState};
use serde::{Deserialize, Serialize};

use crate::{ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new().route("/api/workspace", get(get_workspace).post(create_workspace))
}

async fn get_workspace(
    State(state): State<AppState>,
) -> Result<Json<WorkspaceStateResponse>, ApiError> {
    // When EVE SSO is configured, delegate to `workspace_context()` so this
    // returns the caller's own workspace, not whichever workspace happens to
    // be oldest — `get_workspace_state()` alone is only safe in the
    // single-tenant (no-SSO-configured) fallback below.
    let workspace_state = if state.auth_service().is_ok() {
        let (workspace_id, _) = crate::workspace_context(&state).await?;
        state
            .workspace_service()
            .get_workspace_state_by_id(workspace_id)
            .await?
    } else {
        state.workspace_service().get_workspace_state().await?
    };
    Ok(Json(WorkspaceStateResponse::from(workspace_state)))
}

async fn create_workspace(
    State(state): State<AppState>,
    Json(request): Json<CreateWorkspaceRequest>,
) -> Result<(StatusCode, Json<WorkspaceStateResponse>), ApiError> {
    let workspace_state = state
        .workspace_service()
        .create_workspace(CreateWorkspaceCommand { name: request.name })
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(WorkspaceStateResponse::from(workspace_state)),
    ))
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStateResponse {
    pub configured: bool,
    pub workspace: Option<WorkspaceDto>,
    pub owner: Option<OwnerDto>,
    pub version: String,
}

impl From<WorkspaceState> for WorkspaceStateResponse {
    fn from(value: WorkspaceState) -> Self {
        Self {
            configured: value.configured,
            workspace: value.workspace.map(WorkspaceDto::from),
            owner: value.owner.map(OwnerDto::from),
            version: crate::app_version(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDto {
    pub id: String,
    pub name: String,
    pub owner_id: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Workspace> for WorkspaceDto {
    fn from(value: Workspace) -> Self {
        Self {
            id: value.id.0.to_string(),
            name: value.name,
            owner_id: value.owner_id.0.to_string(),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerDto {
    pub id: String,
    pub workspace_id: String,
    pub kind: &'static str,
    pub display_name: String,
    pub hidden: bool,
}

impl From<Owner> for OwnerDto {
    fn from(value: Owner) -> Self {
        Self {
            id: value.id.0.to_string(),
            workspace_id: value.workspace_id.0.to_string(),
            kind: "manual",
            display_name: value.display_name,
            hidden: value.hidden,
        }
    }
}
