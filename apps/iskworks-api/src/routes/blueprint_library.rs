use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use iskworks_core::{
    BlueprintActivity, BlueprintKind, BlueprintLibraryFacets, BlueprintLibraryOrder,
    BlueprintLibraryPage, BlueprintLibraryQuery, BlueprintLibrarySort, IndustryError,
};
use serde::Deserialize;

use crate::{workspace_context, ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/industry/blueprints", get(list_blueprints))
        .route("/api/industry/blueprints/filters", get(blueprint_filters))
}

/// Multi-selects arrive comma-delimited (`kinds=original,copy`), like Assets.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct BlueprintLibraryHttpQuery {
    #[serde(default)]
    search: String,
    kinds: Option<String>,
    category_ids: Option<String>,
    group_ids: Option<String>,
    connection_ids: Option<String>,
    location_ids: Option<String>,
    min_me: Option<u8>,
    min_te: Option<u8>,
    #[serde(default)]
    needs_research: bool,
    activity: Option<BlueprintActivity>,
    #[serde(default)]
    sort: BlueprintLibrarySort,
    #[serde(default)]
    order: BlueprintLibraryOrder,
    limit: Option<u16>,
    offset: Option<u32>,
}

impl BlueprintLibraryHttpQuery {
    fn into_core(self) -> Result<BlueprintLibraryQuery, IndustryError> {
        Ok(BlueprintLibraryQuery {
            search: self.search,
            kinds: parse_csv(self.kinds.as_deref(), "kind", |entry| match entry {
                "original" => Some(BlueprintKind::Original),
                "copy" => Some(BlueprintKind::Copy),
                "unknown" => Some(BlueprintKind::Unknown),
                _ => None,
            })?,
            category_ids: parse_csv(self.category_ids.as_deref(), "category", parse_id)?,
            group_ids: parse_csv(self.group_ids.as_deref(), "group", parse_id)?,
            connection_ids: parse_csv(self.connection_ids.as_deref(), "character", |entry| {
                entry.parse().ok()
            })?,
            location_ids: parse_csv(self.location_ids.as_deref(), "location", parse_id)?,
            min_me: self.min_me,
            min_te: self.min_te,
            needs_research: self.needs_research,
            activity: self.activity,
            sort: self.sort,
            order: self.order,
            limit: self.limit,
            offset: self.offset,
        })
    }
}

fn parse_id(entry: &str) -> Option<i64> {
    entry.parse().ok()
}

fn parse_csv<T>(
    value: Option<&str>,
    label: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Result<Vec<T>, IndustryError> {
    value
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .split(',')
                .map(|entry| {
                    parse(entry).ok_or_else(|| {
                        IndustryError::Validation(format!("blueprint {label} filter is invalid"))
                    })
                })
                .collect()
        })
        .unwrap_or_else(|| Ok(Vec::new()))
}

async fn list_blueprints(
    State(state): State<AppState>,
    Query(query): Query<BlueprintLibraryHttpQuery>,
) -> Result<Json<BlueprintLibraryPage>, ApiError> {
    let query = query.into_core()?.validate()?;
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .blueprint_library_repository()?
            .list(workspace_id, &query)
            .await?,
    ))
}

async fn blueprint_filters(
    State(state): State<AppState>,
) -> Result<Json<BlueprintLibraryFacets>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .blueprint_library_repository()?
            .facets(workspace_id)
            .await?,
    ))
}
