//! Player-structure market handling: resolve structure names via ESI
//! (cached in `market_location_names`) and verify that some connected
//! character can read a structure's market, remembering who.

use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use iskworks_app::{MarketAccessResolution, MarketAccessResolver};
use iskworks_core::{
    InventoryError, MarketError, MarketLocationClassification, ResolvedMarketLocation,
};
use serde::{Deserialize, Serialize};

use crate::{workspace_context, ApiError, AppState};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveMarketLocationsRequest {
    location_ids: Vec<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolvedMarketLocationResponse {
    location_id: i64,
    location_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolveMarketLocationsResponse {
    configured: bool,
    resolved: Vec<ResolvedMarketLocationResponse>,
    unresolved_location_ids: Vec<i64>,
    eligible_character_count: u64,
    needs_reconnection: bool,
    warnings: Vec<String>,
}

async fn resolve_market_locations(
    State(state): State<AppState>,
    Json(request): Json<ResolveMarketLocationsRequest>,
) -> Result<Json<ResolveMarketLocationsResponse>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let requested = request
        .location_ids
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    if requested.is_empty()
        || requested.len() != request.location_ids.len()
        || requested.len() > 50
        || requested.iter().any(|id| *id <= 0)
    {
        return Err(ApiError::Inventory(InventoryError::Validation(
            "Select between 1 and 50 unique positive location IDs.".to_string(),
        )));
    }
    let repository = state.market_repository()?;
    let ids = requested.iter().copied().collect::<Vec<_>>();
    let cached = repository.location_names(workspace_id, &ids).await?;
    let mut resolved = cached
        .iter()
        .map(
            |(location_id, location_name)| ResolvedMarketLocationResponse {
                location_id: *location_id,
                location_name: location_name.clone(),
            },
        )
        .collect::<Vec<_>>();
    let missing = ids
        .iter()
        .copied()
        .filter(|id| !cached.contains_key(id))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(Json(ResolveMarketLocationsResponse {
            configured: state.esi_service.is_some(),
            resolved,
            unresolved_location_ids: Vec::new(),
            eligible_character_count: 0,
            needs_reconnection: false,
            warnings: Vec::new(),
        }));
    }
    let Some(service) = state.esi_service.as_ref() else {
        return Ok(Json(ResolveMarketLocationsResponse {
            configured: false,
            resolved,
            unresolved_location_ids: missing,
            eligible_character_count: 0,
            needs_reconnection: false,
            warnings: vec!["EVE SSO is not configured.".to_string()],
        }));
    };
    let resolution = service.resolve_structures(workspace_id, &missing).await?;
    let now = chrono::Utc::now();
    let locations = resolution
        .resolved
        .iter()
        .map(|item| ResolvedMarketLocation {
            location_id: item.structure.structure_id,
            location_name: item.structure.name.clone(),
            owner_id: item.structure.owner_id,
            solar_system_id: item.structure.solar_system_id,
            structure_type_id: item.structure.type_id,
            resolved_by_connection_id: item.connection_id,
            resolved_at: now,
        })
        .collect::<Vec<_>>();
    repository
        .save_location_names(workspace_id, locations)
        .await?;
    resolved.extend(
        resolution
            .resolved
            .into_iter()
            .map(|item| ResolvedMarketLocationResponse {
                location_id: item.structure.structure_id,
                location_name: item.structure.name,
            }),
    );
    resolved.sort_by_key(|item| item.location_id);
    Ok(Json(ResolveMarketLocationsResponse {
        configured: true,
        resolved,
        unresolved_location_ids: resolution.unresolved_structure_ids,
        eligible_character_count: resolution.eligible_character_count,
        needs_reconnection: resolution.needs_reconnection,
        warnings: resolution.warnings,
    }))
}

/// The three outcomes the "Add a structure" dialog needs to render, in one
/// pass: is this structure's market pricing usable at all right now, and by
/// whom. Internally tagged (`{"access":"confirmed","characterName":"..."}`
/// / `{"access":"noEligibleCharacter"}` / `{"access":"denied"}`) so the
/// frontend can switch on `access` directly.
#[derive(Debug, Serialize)]
#[serde(
    tag = "access",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum VerifyStructureMarketAccessResponse {
    Confirmed { character_name: String },
    NoEligibleCharacter,
    Denied,
}

/// Confirms (or re-confirms) that some connected character can read this
/// structure's market, and remembers who -- the manual counterpart to the
/// worker's own self-healing resolution. Verification
/// deliberately never persists the confirmed connection's fetched order
/// page: unlike a real refresh, this route's only job is the yes/no answer.
async fn verify_structure_market_access(
    State(state): State<AppState>,
    Path(location_id): Path<i64>,
) -> Result<Json<VerifyStructureMarketAccessResponse>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let repository = state.market_repository()?;
    let solar_system_id = match repository
        .classify_location(workspace_id, location_id)
        .await?
    {
        MarketLocationClassification::Structure { solar_system_id } => solar_system_id,
        MarketLocationClassification::NpcStation => {
            return Err(MarketError::Validation(
                "This is a public NPC station -- market data is already available with no character access needed.".to_string(),
            )
            .into());
        }
        MarketLocationClassification::Unknown => {
            return Err(MarketError::Validation(
                "This location hasn't been resolved yet -- resolve its name first.".to_string(),
            )
            .into());
        }
    };
    let service = state.esi_service()?;
    let preferred = repository
        .market_access_connection(workspace_id, location_id)
        .await?;
    let resolution = service
        .resolve_market_access(workspace_id, location_id, solar_system_id, preferred)
        .await?;
    Ok(Json(match resolution {
        MarketAccessResolution::Confirmed {
            connection_id,
            character_name,
            ..
        } => {
            repository
                .remember_market_access(
                    workspace_id,
                    location_id,
                    connection_id,
                    chrono::Utc::now(),
                )
                .await?;
            VerifyStructureMarketAccessResponse::Confirmed { character_name }
        }
        MarketAccessResolution::NoEligibleCharacter => {
            VerifyStructureMarketAccessResponse::NoEligibleCharacter
        }
        MarketAccessResolution::AllDenied => VerifyStructureMarketAccessResponse::Denied,
    }))
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/industry/market-locations/resolve",
            post(resolve_market_locations),
        )
        .route(
            "/api/industry/market-locations/:location_id/verify-access",
            post(verify_structure_market_access),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for a real bug: `#[serde(rename_all = "camelCase")]`
    /// on an enum only renames the *variant* names used as the `tag` value
    /// -- it does NOT cascade into a struct variant's own field names.
    /// Without the separate `rename_all_fields` attribute,
    /// `Confirmed { character_name }` serialized as `"character_name"`
    /// while the frontend (StructureMarketAccess in
    /// api/industry/market.ts) has always expected `"characterName"` --
    /// the Add Structure dialog's "✓ {character} can price this market"
    /// message has likely rendered with a blank name since this endpoint
    /// shipped.
    #[test]
    fn verify_structure_market_access_response_confirmed_field_is_camel_case() {
        let response = VerifyStructureMarketAccessResponse::Confirmed {
            character_name: "Kira Vayne".to_string(),
        };
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "access": "confirmed", "characterName": "Kira Vayne" })
        );
    }
}
