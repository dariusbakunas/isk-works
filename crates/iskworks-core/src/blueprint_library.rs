//! The Blueprints library: every synced character blueprint in a workspace,
//! classified by the item it makes and filterable for "what can I build?".
//! Reads `blueprint_observations` only; it never calls ESI.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AssetFilterOption, BlueprintKind, BuildId, IndustryError, WorkspaceId};

pub const BLUEPRINT_LIBRARY_DEFAULT_LIMIT: u16 = 200;
pub const BLUEPRINT_LIBRARY_MAX_LIMIT: u16 = 500;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlueprintActivity {
    Manufacturing,
    Reaction,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum BlueprintLibrarySort {
    #[default]
    Blueprint,
    Product,
    Kind,
    Me,
    Te,
    Runs,
    Character,
    Location,
    Observed,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum BlueprintLibraryOrder {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct BlueprintLibraryQuery {
    pub search: String,
    pub kinds: Vec<BlueprintKind>,
    /// Product category / group, not the blueprint's own (always "Blueprint").
    pub category_ids: Vec<i64>,
    pub group_ids: Vec<i64>,
    pub connection_ids: Vec<Uuid>,
    pub location_ids: Vec<i64>,
    pub min_me: Option<u8>,
    pub min_te: Option<u8>,
    /// Originals that can still be researched: manufacturing, ME < 10 or
    /// TE < 20. Copies and reaction formulas cannot be researched.
    pub needs_research: bool,
    pub activity: Option<BlueprintActivity>,
    pub sort: BlueprintLibrarySort,
    pub order: BlueprintLibraryOrder,
    pub limit: Option<u16>,
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ValidatedBlueprintLibraryQuery {
    pub search: String,
    pub kinds: Vec<BlueprintKind>,
    pub category_ids: Vec<i64>,
    pub group_ids: Vec<i64>,
    pub connection_ids: Vec<Uuid>,
    pub location_ids: Vec<i64>,
    pub min_me: Option<u8>,
    pub min_te: Option<u8>,
    pub needs_research: bool,
    pub activity: Option<BlueprintActivity>,
    pub sort: BlueprintLibrarySort,
    pub order: BlueprintLibraryOrder,
    pub limit: u16,
    pub offset: u32,
}

impl BlueprintLibraryQuery {
    pub fn validate(self) -> Result<ValidatedBlueprintLibraryQuery, IndustryError> {
        let limit = self.limit.unwrap_or(BLUEPRINT_LIBRARY_DEFAULT_LIMIT);
        if !(1..=BLUEPRINT_LIBRARY_MAX_LIMIT).contains(&limit) {
            return Err(IndustryError::Validation(format!(
                "blueprint page size must be between 1 and {BLUEPRINT_LIBRARY_MAX_LIMIT}"
            )));
        }
        if self.min_me.is_some_and(|me| me > 10) {
            return Err(IndustryError::Validation(
                "minimum ME must be between 0 and 10".to_string(),
            ));
        }
        if self.min_te.is_some_and(|te| te > 20) {
            return Err(IndustryError::Validation(
                "minimum TE must be between 0 and 20".to_string(),
            ));
        }
        Ok(ValidatedBlueprintLibraryQuery {
            search: self.search.trim().to_string(),
            kinds: self.kinds,
            category_ids: self.category_ids,
            group_ids: self.group_ids,
            connection_ids: self.connection_ids,
            location_ids: self.location_ids,
            min_me: self.min_me,
            min_te: self.min_te,
            needs_research: self.needs_research,
            activity: self.activity,
            sort: self.sort,
            order: self.order,
            limit,
            offset: self.offset.unwrap_or(0),
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintLibraryBuildRef {
    pub build_id: BuildId,
    pub name: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintLibraryRow {
    pub observation_id: Uuid,
    pub eve_item_id: i64,
    pub blueprint_type_id: i64,
    pub blueprint_name: String,
    pub activity: BlueprintActivity,
    pub product_type_id: Option<i64>,
    pub product_name: Option<String>,
    pub product_group_id: Option<i64>,
    pub product_group_name: Option<String>,
    pub product_category_id: Option<i64>,
    pub product_category_name: Option<String>,
    pub kind: BlueprintKind,
    pub material_efficiency: u8,
    pub time_efficiency: u8,
    /// `None` for originals (unlimited) and exhausted/unknown copies.
    pub licensed_runs: Option<u64>,
    pub connection_id: Option<Uuid>,
    pub character_name: Option<String>,
    pub location_id: i64,
    pub location_name: Option<String>,
    pub location_flag: String,
    pub observed_at: DateTime<Utc>,
    pub used_by_builds: Vec<BlueprintLibraryBuildRef>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintLibrarySummary {
    pub originals: i64,
    pub copies: i64,
    pub characters: i64,
    pub latest_observed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintLibraryPage {
    pub rows: Vec<BlueprintLibraryRow>,
    /// Rows matching the filters, across all pages.
    pub total: i64,
    /// Workspace-wide, ignoring filters.
    pub summary: BlueprintLibrarySummary,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintGroupFacet {
    pub value: String,
    pub label: String,
    pub count: i64,
    /// Lets the web narrow the group list to the selected categories.
    pub category_id: Option<i64>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintLibraryFacets {
    pub categories: Vec<AssetFilterOption>,
    pub groups: Vec<BlueprintGroupFacet>,
    pub characters: Vec<AssetFilterOption>,
    pub locations: Vec<AssetFilterOption>,
}

#[async_trait]
pub trait BlueprintLibraryRepository: Send + Sync {
    async fn list(
        &self,
        workspace_id: WorkspaceId,
        query: &ValidatedBlueprintLibraryQuery,
    ) -> Result<BlueprintLibraryPage, IndustryError>;

    /// Filter options over the whole workspace set (not the filtered page).
    async fn facets(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<BlueprintLibraryFacets, IndustryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_first_page_of_two_hundred() {
        let query = BlueprintLibraryQuery {
            search: "  rifter ".to_string(),
            ..Default::default()
        }
        .validate()
        .expect("valid");

        assert_eq!(query.limit, 200);
        assert_eq!(query.offset, 0);
        assert_eq!(query.search, "rifter");
        assert_eq!(query.sort, BlueprintLibrarySort::Blueprint);
    }

    #[test]
    fn rejects_page_size_outside_one_to_five_hundred() {
        for limit in [0, 501] {
            let error = BlueprintLibraryQuery {
                limit: Some(limit),
                ..Default::default()
            }
            .validate()
            .expect_err("invalid limit");
            assert!(matches!(error, IndustryError::Validation(_)));
        }
        assert!(BlueprintLibraryQuery {
            limit: Some(500),
            ..Default::default()
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn rejects_minimum_efficiencies_beyond_the_game_caps() {
        assert!(BlueprintLibraryQuery {
            min_me: Some(11),
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(BlueprintLibraryQuery {
            min_te: Some(21),
            ..Default::default()
        }
        .validate()
        .is_err());
        let query = BlueprintLibraryQuery {
            min_me: Some(10),
            min_te: Some(20),
            ..Default::default()
        }
        .validate()
        .expect("caps are inclusive");
        assert_eq!((query.min_me, query.min_te), (Some(10), Some(20)));
    }
}
