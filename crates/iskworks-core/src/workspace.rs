use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceId(pub Uuid);

impl WorkspaceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for WorkspaceId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub owner_id: crate::owner::OwnerId,
    /// The `MarketScope` used wherever a caller needs "the workspace's own
    /// market" with no more specific selection available -- currently just
    /// Inventory's default valuation scope. `None` means unset, falling back to
    /// `DEFAULT_MARKET_SCOPE` (Jita 4-4) -- no settings UI writes these
    /// yet, so every workspace is unset today; the columns exist so one can
    /// be added later without another migration.
    pub default_market_region_id: Option<i64>,
    pub default_market_location_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Workspace {
    #[must_use]
    pub fn default_market_scope(&self) -> crate::MarketScope {
        match self.default_market_region_id {
            Some(region_id) => crate::MarketScope {
                region_id,
                location_id: self.default_market_location_id,
            },
            None => crate::DEFAULT_MARKET_SCOPE,
        }
    }
}
