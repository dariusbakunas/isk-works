use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::workspace::WorkspaceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OwnerId(pub Uuid);

impl OwnerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for OwnerId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerKind {
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owner {
    pub id: OwnerId,
    pub workspace_id: WorkspaceId,
    pub kind: OwnerKind,
    pub display_name: String,
    pub eve_owner_id: Option<i64>,
    pub hidden: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
