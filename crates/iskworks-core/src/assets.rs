use std::collections::{BTreeMap, BTreeSet};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::InventoryError;

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetBrowserSummary {
    pub location_count: i64,
    pub character_count: i64,
    pub stack_count: i64,
    pub total_quantity: i64,
    pub total_packaged_volume: String,
    pub latest_observed_at: Option<DateTime<Utc>>,
    pub unresolved_location_count: i64,
    pub sync_states: Vec<AssetSyncState>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetSyncState {
    pub connection_id: Uuid,
    pub character_name: String,
    pub connection_status: String,
    pub snapshot_status: Option<String>,
    pub observed_at: Option<DateTime<Utc>>,
    pub row_count: Option<i64>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetBrowserFilters {
    pub owners: Vec<AssetFilterOption>,
    pub characters: Vec<AssetFilterOption>,
    pub groups: Vec<AssetFilterOption>,
    pub locations: Vec<AssetFilterOption>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetFilterOption {
    pub value: String,
    pub label: String,
    pub count: i64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AssetBrowserFacets {
    pub characters: Vec<AssetFilterOption>,
    pub locations: Vec<AssetFilterOption>,
    pub asset_kinds: Vec<AssetFilterOption>,
    pub groups: Vec<AssetFilterOption>,
    pub blueprint_kinds: Vec<AssetFilterOption>,
    pub reconciliation_states: Vec<AssetFilterOption>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AssetSortColumn {
    #[default]
    Item,
    Quantity,
    PackagedVolume,
    Character,
    Location,
    Container,
    Group,
    Status,
    Observed,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AssetSortDirection {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssetKindFilter {
    Blueprint,
    Material,
    Ship,
    Container,
    Other,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssetBlueprintKindFilter {
    Original,
    Copy,
    Unknown,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssetReconciliationFilter {
    Matched,
    Difference,
    NoAccountingRecord,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlatAssetRow {
    pub eve_item_id: i64,
    pub type_id: i64,
    pub type_name: Option<String>,
    pub quantity: i64,
    pub packaged_volume: Option<String>,
    pub total_packaged_volume: Option<String>,
    pub owner_id: Uuid,
    pub owner_name: String,
    pub connection_id: Uuid,
    pub character_id: i64,
    pub character_name: String,
    pub location_id: i64,
    pub location_name: Option<String>,
    pub location_flag: String,
    pub container_item_id: Option<i64>,
    pub container_name: Option<String>,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
    pub asset_kind: String,
    pub observed_at: DateTime<Utc>,
    pub blueprint: Option<AssetBlueprintSummary>,
    pub reconciliation: AssetReconciliationSummary,
    pub is_container: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlatAssetPage {
    pub rows: Vec<FlatAssetRow>,
    pub total: i64,
    pub next_cursor: Option<String>,
    /// Workspace-wide and the same on every page, so only the first page
    /// (no cursor) carries it; later pages send `null`.
    pub summary: Option<AssetBrowserSummary>,
    /// Same as `summary`: first page only.
    pub facets: Option<AssetBrowserFacets>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlatAssetCursor {
    pub sort: AssetSortColumn,
    pub value: String,
    pub connection_id: Uuid,
    pub eve_item_id: i64,
}

impl FlatAssetCursor {
    #[must_use]
    pub fn encode(&self) -> String {
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(self).expect("cursor serialization cannot fail"))
    }

    pub fn decode(value: &str) -> Result<Self, InventoryError> {
        let bytes = URL_SAFE_NO_PAD
            .decode(value)
            .map_err(|_| InventoryError::Validation("asset cursor is invalid".to_string()))?;
        serde_json::from_slice(&bytes)
            .map_err(|_| InventoryError::Validation("asset cursor is invalid".to_string()))
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FlatAssetQuery {
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub connection_ids: Vec<Uuid>,
    #[serde(default)]
    pub location_ids: Vec<i64>,
    #[serde(default)]
    pub asset_kinds: Vec<AssetKindFilter>,
    #[serde(default)]
    pub group_ids: Vec<i64>,
    #[serde(default)]
    pub blueprint_kinds: Vec<AssetBlueprintKindFilter>,
    #[serde(default)]
    pub reconciliation_states: Vec<AssetReconciliationFilter>,
    #[serde(default)]
    pub sort: AssetSortColumn,
    #[serde(default)]
    pub order: AssetSortDirection,
    pub cursor: Option<String>,
    pub limit: Option<u16>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ValidatedFlatAssetQuery {
    pub search: String,
    pub connection_ids: Vec<Uuid>,
    pub location_ids: Vec<i64>,
    pub asset_kinds: Vec<AssetKindFilter>,
    pub group_ids: Vec<i64>,
    pub blueprint_kinds: Vec<AssetBlueprintKindFilter>,
    pub reconciliation_states: Vec<AssetReconciliationFilter>,
    pub sort: AssetSortColumn,
    pub order: AssetSortDirection,
    pub cursor: Option<FlatAssetCursor>,
    pub limit: u16,
}

impl FlatAssetQuery {
    pub fn validate(self) -> Result<ValidatedFlatAssetQuery, InventoryError> {
        let limit = self.limit.unwrap_or(100);
        if !(1..=200).contains(&limit) {
            return Err(InventoryError::Validation(
                "asset page size must be between 1 and 200".to_string(),
            ));
        }
        let cursor = self
            .cursor
            .as_deref()
            .map(FlatAssetCursor::decode)
            .transpose()?;
        if cursor
            .as_ref()
            .is_some_and(|cursor| cursor.sort != self.sort)
        {
            return Err(InventoryError::Validation(
                "asset cursor does not match the requested sort".to_string(),
            ));
        }
        Ok(ValidatedFlatAssetQuery {
            search: self.search.trim().to_string(),
            connection_ids: self.connection_ids,
            location_ids: self.location_ids,
            asset_kinds: self.asset_kinds,
            group_ids: self.group_ids,
            blueprint_kinds: self.blueprint_kinds,
            reconciliation_states: self.reconciliation_states,
            sort: self.sort,
            order: self.order,
            cursor,
            limit,
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetLocationSummary {
    pub location_id: i64,
    pub location_name: String,
    pub location_type: String,
    pub location_type_id: Option<i64>,
    pub solar_system_name: Option<String>,
    pub stack_count: i64,
    pub total_quantity: i64,
    pub owner_count: i64,
    pub character_count: i64,
    pub container_count: i64,
    pub blueprint_count: i64,
    pub latest_observed_at: DateTime<Utc>,
    pub oldest_observed_at: DateTime<Utc>,
    pub unresolved_child_count: i64,
    pub match_count: i64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetBrowserItem {
    pub eve_item_id: i64,
    pub type_id: i64,
    pub type_name: String,
    pub quantity: i64,
    pub owner_id: Uuid,
    pub owner_name: String,
    pub connection_id: Uuid,
    pub character_id: i64,
    pub character_name: String,
    pub location_id: i64,
    pub location_flag: String,
    pub parent_item_id: Option<i64>,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
    pub asset_kind: String,
    pub observed_at: DateTime<Utc>,
    pub blueprint: Option<AssetBlueprintSummary>,
    pub reconciliation: AssetReconciliationSummary,
    pub is_container: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetBlueprintSummary {
    pub kind: String,
    pub material_efficiency: i32,
    pub time_efficiency: i32,
    pub licensed_runs: Option<i32>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetReconciliationSummary {
    pub state: String,
    pub observed_owner_type_quantity: i64,
    pub accounted_owner_type_quantity: i64,
    pub scope: &'static str,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetLocationPage {
    pub location: AssetLocationSummary,
    pub items: Vec<AssetBrowserItem>,
    pub hierarchy: AssetHierarchy,
    pub offset: u32,
    pub limit: u32,
    pub total: i64,
    pub has_more: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AssetHierarchy {
    pub ordered_item_ids: Vec<i64>,
    pub depths: BTreeMap<i64, u32>,
    pub orphan_item_ids: Vec<i64>,
    pub cycle_item_ids: Vec<i64>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AssetBrowserQuery {
    #[serde(default)]
    pub query: String,
    pub owner_id: Option<Uuid>,
    pub character_id: Option<i64>,
    pub group_id: Option<i64>,
    pub asset_kind: Option<String>,
    pub sort: Option<String>,
    pub offset: Option<u32>,
    pub limit: Option<u32>,
}

pub fn build_asset_hierarchy(items: &[AssetBrowserItem]) -> AssetHierarchy {
    let by_id = items
        .iter()
        .map(|item| (item.eve_item_id, item))
        .collect::<BTreeMap<_, _>>();
    let mut children = BTreeMap::<i64, Vec<i64>>::new();
    let mut roots = Vec::new();
    let mut orphans = BTreeSet::new();
    for item in items {
        match item.parent_item_id {
            Some(parent) if by_id.contains_key(&parent) => {
                children.entry(parent).or_default().push(item.eve_item_id);
            }
            Some(_) => {
                orphans.insert(item.eve_item_id);
                roots.push(item.eve_item_id);
            }
            None => roots.push(item.eve_item_id),
        }
    }
    let sort_ids = |ids: &mut Vec<i64>| {
        ids.sort_by_key(|id| {
            let item = by_id[id];
            (
                !item.is_container,
                item.type_name.to_lowercase(),
                item.eve_item_id,
            )
        });
    };
    sort_ids(&mut roots);
    for ids in children.values_mut() {
        sort_ids(ids);
    }

    let mut hierarchy = AssetHierarchy {
        orphan_item_ids: orphans.into_iter().collect(),
        ..AssetHierarchy::default()
    };
    let mut globally_seen = BTreeSet::new();
    for root in roots {
        walk_asset(
            root,
            0,
            &children,
            &mut Vec::new(),
            &mut globally_seen,
            &mut hierarchy,
        );
    }
    for id in by_id.keys().copied() {
        if !globally_seen.contains(&id) {
            walk_asset(
                id,
                0,
                &children,
                &mut Vec::new(),
                &mut globally_seen,
                &mut hierarchy,
            );
        }
    }
    hierarchy.cycle_item_ids.sort_unstable();
    hierarchy.cycle_item_ids.dedup();
    hierarchy
}

fn walk_asset(
    id: i64,
    depth: u32,
    children: &BTreeMap<i64, Vec<i64>>,
    path: &mut Vec<i64>,
    globally_seen: &mut BTreeSet<i64>,
    hierarchy: &mut AssetHierarchy,
) {
    if path.contains(&id) {
        hierarchy.cycle_item_ids.extend(path.iter().copied());
        hierarchy.cycle_item_ids.push(id);
        return;
    }
    if !globally_seen.insert(id) {
        return;
    }
    hierarchy.ordered_item_ids.push(id);
    hierarchy.depths.insert(id, depth);
    path.push(id);
    if let Some(child_ids) = children.get(&id) {
        for child in child_ids {
            walk_asset(
                *child,
                depth.saturating_add(1),
                children,
                path,
                globally_seen,
                hierarchy,
            );
        }
    }
    path.pop();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: i64, parent: Option<i64>) -> AssetBrowserItem {
        AssetBrowserItem {
            eve_item_id: id,
            type_id: 34,
            type_name: format!("Item {id}"),
            quantity: 1,
            owner_id: Uuid::nil(),
            owner_name: "Owner".to_string(),
            connection_id: Uuid::nil(),
            character_id: 1,
            character_name: "Character".to_string(),
            location_id: parent.unwrap_or(60_003_760),
            location_flag: "Hangar".to_string(),
            parent_item_id: parent,
            group_id: Some(18),
            group_name: Some("Mineral".to_string()),
            asset_kind: "other".to_string(),
            observed_at: Utc::now(),
            blueprint: None,
            reconciliation: AssetReconciliationSummary {
                state: "unknown".to_string(),
                observed_owner_type_quantity: 1,
                accounted_owner_type_quantity: 0,
                scope: "ownerType",
            },
            is_container: false,
        }
    }

    #[test]
    fn hierarchy_preserves_nested_and_missing_parent_assets() {
        let hierarchy =
            build_asset_hierarchy(&[item(1, None), item(2, Some(1)), item(3, Some(999))]);
        assert_eq!(hierarchy.ordered_item_ids.len(), 3);
        assert_eq!(hierarchy.depths[&2], 1);
        assert_eq!(hierarchy.orphan_item_ids, vec![3]);
    }

    #[test]
    fn hierarchy_detects_cycles_without_hanging_or_dropping_assets() {
        let hierarchy = build_asset_hierarchy(&[item(1, Some(2)), item(2, Some(1))]);
        assert_eq!(hierarchy.ordered_item_ids.len(), 2);
        assert_eq!(hierarchy.cycle_item_ids, vec![1, 2]);
    }

    #[test]
    fn flat_asset_query_normalizes_defaults_and_filters() {
        let connection_id = Uuid::new_v4();
        let query: FlatAssetQuery = serde_json::from_value(serde_json::json!({
            "search": "  tritanium  ",
            "connectionIds": [connection_id],
            "locationIds": [60003760],
            "assetKinds": ["material"],
            "groupIds": [18],
            "blueprintKinds": ["copy"],
            "reconciliationStates": ["matched"],
            "sort": "packagedVolume",
            "order": "desc",
            "limit": 200
        }))
        .expect("query deserializes");

        let validated = query.validate().expect("query validates");

        assert_eq!(validated.search, "tritanium");
        assert_eq!(validated.connection_ids, vec![connection_id]);
        assert_eq!(validated.location_ids, vec![60_003_760]);
        assert_eq!(validated.asset_kinds, vec![AssetKindFilter::Material]);
        assert_eq!(validated.group_ids, vec![18]);
        assert_eq!(
            validated.blueprint_kinds,
            vec![AssetBlueprintKindFilter::Copy]
        );
        assert_eq!(
            validated.reconciliation_states,
            vec![AssetReconciliationFilter::Matched]
        );
        assert_eq!(validated.sort, AssetSortColumn::PackagedVolume);
        assert_eq!(validated.order, AssetSortDirection::Desc);
        assert_eq!(validated.limit, 200);
    }

    #[test]
    fn flat_asset_query_rejects_out_of_range_limits_and_mismatched_cursor_sort() {
        let too_large: FlatAssetQuery = serde_json::from_value(serde_json::json!({
            "limit": 201
        }))
        .expect("query deserializes");
        assert!(matches!(
            too_large.validate(),
            Err(InventoryError::Validation(_))
        ));

        let cursor = FlatAssetCursor {
            sort: AssetSortColumn::Item,
            value: "tritanium".to_string(),
            connection_id: Uuid::nil(),
            eve_item_id: 42,
        }
        .encode();
        let mismatched: FlatAssetQuery = serde_json::from_value(serde_json::json!({
            "sort": "quantity",
            "cursor": cursor
        }))
        .expect("query deserializes");
        assert!(matches!(
            mismatched.validate(),
            Err(InventoryError::Validation(_))
        ));
    }

    #[test]
    fn flat_asset_cursor_round_trips_stable_identity() {
        let expected = FlatAssetCursor {
            sort: AssetSortColumn::Observed,
            value: "2026-08-06T12:00:00Z".to_string(),
            connection_id: Uuid::new_v4(),
            eve_item_id: 9_001,
        };

        assert_eq!(
            FlatAssetCursor::decode(&expected.encode()).expect("cursor decodes"),
            expected
        );
        assert!(FlatAssetCursor::decode("not-a-cursor").is_err());
    }

    #[test]
    fn flat_asset_page_serializes_exact_values_with_camel_case_keys() {
        let page = FlatAssetPage {
            rows: Vec::new(),
            total: 0,
            next_cursor: None,
            summary: Some(AssetBrowserSummary {
                location_count: 0,
                character_count: 0,
                stack_count: 0,
                total_quantity: 0,
                total_packaged_volume: "0.0000".to_string(),
                latest_observed_at: None,
                unresolved_location_count: 0,
                sync_states: Vec::new(),
            }),
            facets: Some(AssetBrowserFacets::default()),
        };

        let value = serde_json::to_value(page).expect("page serializes");

        assert_eq!(value["summary"]["totalPackagedVolume"], "0.0000");
        assert!(value.get("nextCursor").is_some());
    }
}
