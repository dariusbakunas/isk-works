//! Market reference data: regions, region locations, curated trade hubs,
//! workspace-known structures, cross-source location search, and the SDE
//! market-category tree.
//!
//! Pure reads over the active SDE dataset (regions, locations, categories),
//! plus workspace-known player structures, enriched with per-scope
//! freshness. No market observations, no coverage, no `PriceSource`
//! involved -- these exist so the market scope selector has somewhere to
//! read region/location/category options from, independent of whether any
//! pricing has ever been configured.

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::{workspace_context, ApiError, AppState};

async fn list_market_regions(
    State(state): State<AppState>,
) -> Result<Json<Vec<iskworks_sde::RegionSummary>>, ApiError> {
    Ok(Json(state.sde_repository.list_regions().await?))
}

async fn list_market_categories(
    State(state): State<AppState>,
) -> Result<Json<Vec<iskworks_core::MarketCategoryNode>>, ApiError> {
    let flat = state.sde_repository.list_market_groups().await?;
    Ok(Json(iskworks_core::build_market_category_tree(flat)))
}

async fn list_market_region_locations(
    State(state): State<AppState>,
    Path(region_id): Path<i64>,
) -> Result<Json<Vec<MarketLocationSummary>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let stations = state
        .sde_repository
        .list_npc_stations_in_region(region_id)
        .await?;
    let market_repository = state.market_repository()?;
    let structures = market_repository
        .known_locations_in_region(workspace_id, region_id)
        .await?;
    let scopes: Vec<(i64, i64)> = stations
        .iter()
        .map(|station| (region_id, station.station_id))
        .chain(
            structures
                .iter()
                .map(|structure| (region_id, structure.location_id)),
        )
        .collect();
    let freshness = market_repository
        .scope_freshness(workspace_id, &scopes)
        .await?;
    let mut locations: Vec<MarketLocationSummary> = stations
        .into_iter()
        .map(|station| {
            let station_freshness = freshness
                .get(&(region_id, station.station_id))
                .copied()
                .unwrap_or_default();
            MarketLocationSummary::from_npc_station(station, station_freshness)
        })
        .collect();
    locations.extend(structures.into_iter().map(|structure| {
        let structure_freshness = freshness
            .get(&(region_id, structure.location_id))
            .copied()
            .unwrap_or_default();
        MarketLocationSummary::from_structure(structure, structure_freshness)
    }));
    Ok(Json(locations))
}

/// The Market Scope Selector's "Major Hubs" tab -- a small, curated list of
/// well-known trade hub stations (`iskworks_core::MAJOR_TRADE_HUBS`),
/// resolved live against the active SDE and enriched with freshness. Not
/// workspace-specific data itself, but freshness is, so this still needs
/// workspace context.
async fn list_market_hubs(
    State(state): State<AppState>,
) -> Result<Json<Vec<MarketHubSummary>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let station_ids: Vec<i64> = iskworks_core::MAJOR_TRADE_HUBS
        .iter()
        .map(|hub| hub.station_id)
        .collect();
    let stations = state
        .sde_repository
        .resolve_npc_stations(&station_ids)
        .await?;
    let scopes: Vec<(i64, i64)> = stations
        .iter()
        .map(|station| (station.region_id, station.station_id))
        .collect();
    let freshness = state
        .market_repository()?
        .scope_freshness(workspace_id, &scopes)
        .await?;
    let hubs = stations
        .into_iter()
        .map(|station| {
            let short_name = iskworks_core::MAJOR_TRADE_HUBS
                .iter()
                .find(|hub| hub.station_id == station.station_id)
                .map(|hub| hub.short_name.to_string())
                .unwrap_or_else(|| station.station_name.clone());
            let station_freshness = freshness
                .get(&(station.region_id, station.station_id))
                .copied()
                .unwrap_or_default();
            MarketHubSummary {
                location_id: station.station_id,
                location_name: station.station_name,
                short_name,
                solar_system_id: station.solar_system_id,
                solar_system_name: station.solar_system_name,
                region_id: station.region_id,
                region_name: station.region_name,
                freshness: station_freshness,
            }
        })
        .collect();
    Ok(Json(hubs))
}

/// The Market Scope Selector's "My Structures" tab -- every Upwell
/// structure this workspace already knows about, with market-access and
/// freshness state. Unfiltered (no search) since the tab shows the whole
/// list.
async fn list_market_structures(
    State(state): State<AppState>,
) -> Result<Json<Vec<MarketStructureSummary>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let market_repository = state.market_repository()?;
    let structures = market_repository
        .list_known_structures(workspace_id, "")
        .await?;
    let scopes: Vec<(i64, i64)> = structures
        .iter()
        .filter_map(|structure| {
            structure
                .region_id
                .map(|region_id| (region_id, structure.location_id))
        })
        .collect();
    let freshness = market_repository
        .scope_freshness(workspace_id, &scopes)
        .await?;
    let summaries = structures
        .into_iter()
        .map(|structure| {
            let structure_freshness = structure
                .region_id
                .and_then(|region_id| freshness.get(&(region_id, structure.location_id)))
                .copied()
                .unwrap_or_default();
            MarketStructureSummary {
                location_id: structure.location_id,
                location_name: structure.location_name,
                structure_type_id: structure.structure_type_id,
                structure_type_name: structure.structure_type_name,
                solar_system_id: structure.solar_system_id,
                solar_system_name: structure.solar_system_name,
                region_id: structure.region_id,
                region_name: structure.region_name,
                security_class: structure.security_class,
                access_state: structure.access_state,
                access_character_name: structure.access_character_name,
                access_checked_at: structure.access_checked_at,
                freshness: structure_freshness,
            }
        })
        .collect();
    Ok(Json(summaries))
}

const MARKET_LOCATION_SEARCH_LIMIT_PER_KIND: u32 = 8;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarketLocationSearchQuery {
    q: String,
}

/// One search-box result across the Market Scope Selector's three concrete
/// location sources (curated hubs, NPC stations, workspace-known
/// structures) plus regions -- internally tagged
/// (`{"kind":"hub",...}`/`{"kind":"npcStation",...}`/
/// `{"kind":"structure",...}`/`{"kind":"region",...}`) so the frontend can
/// switch on `kind` directly, same convention as
/// `VerifyStructureMarketAccessResponse`. `Hub` and `NpcStation` share an
/// underlying source (`search_npc_stations`) -- a result is tagged `Hub`
/// instead of `NpcStation` purely by station-ID membership in
/// `MAJOR_TRADE_HUBS`, not a separate query.
#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum MarketLocationSearchResult {
    Hub {
        location_id: i64,
        display_name: String,
        solar_system_id: i64,
        solar_system_name: String,
        region_id: i64,
        region_name: String,
        freshness: iskworks_core::ScopeFreshness,
    },
    NpcStation {
        location_id: i64,
        display_name: String,
        solar_system_id: i64,
        solar_system_name: String,
        region_id: i64,
        region_name: String,
        security_class: String,
        freshness: iskworks_core::ScopeFreshness,
    },
    Structure {
        location_id: i64,
        display_name: String,
        structure_type_id: Option<i64>,
        structure_type_name: Option<String>,
        solar_system_id: i64,
        solar_system_name: Option<String>,
        region_id: Option<i64>,
        region_name: Option<String>,
        security_class: String,
        access_state: iskworks_core::MarketAccessState,
        access_character_name: Option<String>,
        freshness: iskworks_core::ScopeFreshness,
    },
    Region {
        region_id: i64,
        display_name: String,
    },
}

/// Global search backing the Market Scope Selector's search box -- fans out
/// to the same three sources its dedicated tabs use (`search_npc_stations`,
/// `list_known_structures` filtered by `q`, `search_regions`), each capped
/// independently, merged into one kind-tagged list. `q` shorter than 2
/// characters returns an empty list rather than searching, matching
/// `search_solar_systems`'s own convention -- avoids an expensive
/// effectively-unfiltered scan on every keystroke of a fresh search box.
async fn search_market_locations(
    State(state): State<AppState>,
    Query(query): Query<MarketLocationSearchQuery>,
) -> Result<Json<Vec<MarketLocationSearchResult>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let q = query.q.trim();
    if q.chars().count() < 2 {
        return Ok(Json(Vec::new()));
    }

    let stations = state
        .sde_repository
        .search_npc_stations(q, MARKET_LOCATION_SEARCH_LIMIT_PER_KIND)
        .await?;
    let regions = state
        .sde_repository
        .search_regions(q, MARKET_LOCATION_SEARCH_LIMIT_PER_KIND)
        .await?;
    let market_repository = state.market_repository()?;
    // list_known_structures has no built-in cap (a workspace's own
    // structure count is always small) -- bound it here to match the other
    // sources instead of adding a limit parameter only this caller needs.
    let structures: Vec<_> = market_repository
        .list_known_structures(workspace_id, q)
        .await?
        .into_iter()
        .take(MARKET_LOCATION_SEARCH_LIMIT_PER_KIND as usize)
        .collect();

    let scopes: Vec<(i64, i64)> = stations
        .iter()
        .map(|station| (station.region_id, station.station_id))
        .chain(structures.iter().filter_map(|structure| {
            structure
                .region_id
                .map(|region_id| (region_id, structure.location_id))
        }))
        .collect();
    let freshness = market_repository
        .scope_freshness(workspace_id, &scopes)
        .await?;

    let mut results = Vec::with_capacity(stations.len() + structures.len() + regions.len());
    for station in stations {
        let station_freshness = freshness
            .get(&(station.region_id, station.station_id))
            .copied()
            .unwrap_or_default();
        let hub = iskworks_core::MAJOR_TRADE_HUBS
            .iter()
            .find(|hub| hub.station_id == station.station_id);
        results.push(match hub {
            Some(hub) => MarketLocationSearchResult::Hub {
                location_id: station.station_id,
                display_name: hub.short_name.to_string(),
                solar_system_id: station.solar_system_id,
                solar_system_name: station.solar_system_name,
                region_id: station.region_id,
                region_name: station.region_name,
                freshness: station_freshness,
            },
            None => MarketLocationSearchResult::NpcStation {
                location_id: station.station_id,
                display_name: station.station_name,
                solar_system_id: station.solar_system_id,
                solar_system_name: station.solar_system_name,
                region_id: station.region_id,
                region_name: station.region_name,
                security_class: station.security_class,
                freshness: station_freshness,
            },
        });
    }
    for structure in structures {
        let structure_freshness = structure
            .region_id
            .and_then(|region_id| freshness.get(&(region_id, structure.location_id)))
            .copied()
            .unwrap_or_default();
        results.push(MarketLocationSearchResult::Structure {
            location_id: structure.location_id,
            display_name: structure.location_name,
            structure_type_id: structure.structure_type_id,
            structure_type_name: structure.structure_type_name,
            solar_system_id: structure.solar_system_id,
            solar_system_name: structure.solar_system_name,
            region_id: structure.region_id,
            region_name: structure.region_name,
            security_class: structure.security_class,
            access_state: structure.access_state,
            access_character_name: structure.access_character_name,
            freshness: structure_freshness,
        });
    }
    for region in regions {
        results.push(MarketLocationSearchResult::Region {
            region_id: region.region_id,
            display_name: region.region_name,
        });
    }
    Ok(Json(results))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MarketHubSummary {
    location_id: i64,
    location_name: String,
    short_name: String,
    solar_system_id: i64,
    solar_system_name: String,
    region_id: i64,
    region_name: String,
    freshness: iskworks_core::ScopeFreshness,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MarketStructureSummary {
    location_id: i64,
    location_name: String,
    structure_type_id: Option<i64>,
    structure_type_name: Option<String>,
    solar_system_id: i64,
    solar_system_name: Option<String>,
    region_id: Option<i64>,
    region_name: Option<String>,
    security_class: String,
    access_state: iskworks_core::MarketAccessState,
    access_character_name: Option<String>,
    access_checked_at: Option<chrono::DateTime<chrono::Utc>>,
    freshness: iskworks_core::ScopeFreshness,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
enum MarketLocationKind {
    NpcStation,
    Structure,
}

/// One selectable location under a region: either an SDE-known NPC station
/// or a workspace-known player structure, distinguished by `kind`. Station-
/// and structure-only fields are `null` on the other kind rather than
/// split into two response shapes, so the future location selector can
/// render one list without branching on which fields are present.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MarketLocationSummary {
    location_id: i64,
    location_name: String,
    kind: MarketLocationKind,
    solar_system_id: i64,
    solar_system_name: String,
    station_type_id: Option<i64>,
    station_type_name: Option<String>,
    security_class: Option<String>,
    structure_type_id: Option<i64>,
    freshness: iskworks_core::ScopeFreshness,
}

impl MarketLocationSummary {
    fn from_npc_station(
        station: iskworks_sde::NpcStationSearchResult,
        freshness: iskworks_core::ScopeFreshness,
    ) -> Self {
        Self {
            location_id: station.station_id,
            location_name: station.station_name,
            kind: MarketLocationKind::NpcStation,
            solar_system_id: station.solar_system_id,
            solar_system_name: station.solar_system_name,
            station_type_id: Some(station.station_type_id),
            station_type_name: station.station_type_name,
            security_class: Some(station.security_class),
            structure_type_id: None,
            freshness,
        }
    }

    fn from_structure(
        location: iskworks_core::KnownMarketLocation,
        freshness: iskworks_core::ScopeFreshness,
    ) -> Self {
        Self {
            location_id: location.location_id,
            location_name: location.location_name,
            kind: MarketLocationKind::Structure,
            solar_system_id: location.solar_system_id,
            solar_system_name: location.solar_system_name,
            station_type_id: None,
            station_type_name: None,
            security_class: None,
            structure_type_id: location.structure_type_id,
            freshness,
        }
    }
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/market/regions", get(list_market_regions))
        .route(
            "/api/market/regions/:region_id/locations",
            get(list_market_region_locations),
        )
        .route("/api/market/hubs", get(list_market_hubs))
        .route("/api/market/structures", get(list_market_structures))
        .route("/api/market/locations/search", get(search_market_locations))
        .route("/api/market/categories", get(list_market_categories))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Same class of bug, verified for `MarketLocationSearchResult`'s
    /// `Structure` variant, which already carries the correct
    /// `rename_all_fields` attribute -- guards against it regressing.
    #[test]
    fn market_location_search_result_structure_fields_are_camel_case() {
        let result = MarketLocationSearchResult::Structure {
            location_id: 1_049_995_520_085,
            display_name: "GEZ-IXX Keepstar".to_string(),
            structure_type_id: Some(35_834),
            structure_type_name: Some("Keepstar".to_string()),
            solar_system_id: 30_000_772,
            solar_system_name: Some("C-J6MT".to_string()),
            region_id: Some(10_000_058),
            region_name: Some("Feythabolis".to_string()),
            security_class: "wormhole".to_string(),
            access_state: iskworks_core::MarketAccessState::Confirmed,
            access_character_name: Some("Kira Vayne".to_string()),
            freshness: iskworks_core::ScopeFreshness::default(),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["kind"], "structure");
        assert_eq!(json["locationId"], 1_049_995_520_085_i64);
        assert_eq!(json["structureTypeId"], 35_834);
        assert_eq!(json["accessState"], "confirmed");
        assert_eq!(json["accessCharacterName"], "Kira Vayne");
        assert!(json.get("location_id").is_none());
        assert!(json.get("structure_type_id").is_none());
    }
}
