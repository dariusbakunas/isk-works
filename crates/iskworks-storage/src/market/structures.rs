//! Upwell/NPC market-location persistence: known-structure listings,
//! remembered market access, location classification, and name resolution.
//! Workspace isolation and remembered-access semantics are unchanged.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use iskworks_core::{
    ConnectedCharacterId, MarketError, MarketLocationClassification, MarketStructureListing,
    ResolvedMarketLocation, WorkspaceId,
};
use rust_decimal::Decimal;
use uuid::Uuid;

use super::convert::*;
use super::rows::*;
use super::PgMarketRepository;
use crate::sde_read::SECURITY_CLASS_CASE_SQL;

impl PgMarketRepository {
    pub(super) async fn location_names(
        &self,
        workspace_id: WorkspaceId,
        location_ids: &[i64],
    ) -> Result<BTreeMap<i64, String>, MarketError> {
        if location_ids.is_empty() {
            return Ok(BTreeMap::new());
        }
        sqlx::query_as::<_, (i64, String)>(
            "SELECT location_id,location_name FROM market_location_names WHERE workspace_id=$1 AND location_id=ANY($2)",
        )
        .bind(workspace_id.0)
        .bind(location_ids)
        .fetch_all(&self.pool)
        .await
        .map(|rows| rows.into_iter().collect())
        .map_err(map_sqlx)
    }

    pub(super) async fn resolve_order_location_names(
        &self,
        workspace_id: WorkspaceId,
        location_ids: &[i64],
    ) -> Result<BTreeMap<i64, String>, MarketError> {
        if location_ids.is_empty() {
            return Ok(BTreeMap::new());
        }
        // NPC stations resolve straight from the SDE, security status and
        // all -- mirrors `resolve_scope_display`'s own station lookup, just
        // batched. Player structures aren't in the SDE at all, so anything
        // left unresolved here falls back to whatever this workspace has
        // already resolved into `market_location_names` (the Add Structure
        // flow) below.
        let stations = sqlx::query_as::<_, (i64, String, Option<Decimal>)>(
            r#"
            SELECT npc.station_id, npc.name_en, sys.security_status
            FROM sde_imports imp
            JOIN sde_npc_stations npc ON npc.import_id=imp.id
            JOIN sde_solar_systems sys
              ON sys.import_id=imp.id AND sys.solar_system_id=npc.solar_system_id
            WHERE imp.active=true AND npc.station_id=ANY($1)
            "#,
        )
        .bind(location_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        let mut resolved: BTreeMap<i64, String> = stations
            .into_iter()
            .map(|(station_id, name, security_status)| {
                (
                    station_id,
                    format_station_display_name(&name, security_status),
                )
            })
            .collect();

        let remaining: Vec<i64> = location_ids
            .iter()
            .copied()
            .filter(|location_id| !resolved.contains_key(location_id))
            .collect();
        if !remaining.is_empty() {
            let structures = sqlx::query_as::<_, (i64, String)>(
                "SELECT location_id,location_name FROM market_location_names WHERE workspace_id=$1 AND location_id=ANY($2)",
            )
            .bind(workspace_id.0)
            .bind(&remaining)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx)?;
            resolved.extend(structures);
        }
        Ok(resolved)
    }

    pub(super) async fn known_locations_in_region(
        &self,
        workspace_id: WorkspaceId,
        region_id: i64,
    ) -> Result<Vec<iskworks_core::KnownMarketLocation>, MarketError> {
        sqlx::query_as::<_, KnownMarketLocationRow>(
            r#"
            SELECT mln.location_id, mln.location_name, mln.solar_system_id,
                   sys.name_en AS solar_system_name, mln.structure_type_id
            FROM market_location_names mln
            JOIN sde_imports imp ON imp.active=true
            JOIN sde_solar_systems sys
              ON sys.import_id=imp.id AND sys.solar_system_id=mln.solar_system_id
            WHERE mln.workspace_id=$1 AND sys.region_id=$2
            ORDER BY mln.location_name
            "#,
        )
        .bind(workspace_id.0)
        .bind(region_id)
        .fetch_all(&self.pool)
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|row| iskworks_core::KnownMarketLocation {
                    location_id: row.location_id,
                    location_name: row.location_name,
                    solar_system_id: row.solar_system_id,
                    solar_system_name: row.solar_system_name,
                    structure_type_id: row.structure_type_id,
                })
                .collect()
        })
        .map_err(map_sqlx)
    }

    pub(super) async fn classify_location(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
    ) -> Result<MarketLocationClassification, MarketError> {
        let is_npc_station = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT 1::bigint
            FROM sde_imports import
            JOIN sde_npc_stations station
              ON station.import_id=import.id AND station.station_id=$1
            WHERE import.active=true
            "#,
        )
        .bind(location_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?
        .is_some();
        if is_npc_station {
            return Ok(MarketLocationClassification::NpcStation);
        }
        let structure_solar_system_id = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT solar_system_id
            FROM market_location_names
            WHERE workspace_id=$1 AND location_id=$2 AND structure_type_id IS NOT NULL
            "#,
        )
        .bind(workspace_id.0)
        .bind(location_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(match structure_solar_system_id {
            Some(solar_system_id) => MarketLocationClassification::Structure { solar_system_id },
            None => MarketLocationClassification::Unknown,
        })
    }

    pub(super) async fn market_access_connection(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
    ) -> Result<Option<ConnectedCharacterId>, MarketError> {
        let row: Option<Option<Uuid>> = sqlx::query_scalar(
            r#"
            SELECT market_access_connection_id
            FROM market_location_names
            WHERE workspace_id=$1 AND location_id=$2
            "#,
        )
        .bind(workspace_id.0)
        .bind(location_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(row.flatten().map(ConnectedCharacterId))
    }

    pub(super) async fn remember_market_access(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
        connection_id: ConnectedCharacterId,
        checked_at: DateTime<Utc>,
    ) -> Result<(), MarketError> {
        let result = sqlx::query(
            r#"
            UPDATE market_location_names
            SET market_access_connection_id=$3, market_access_checked_at=$4
            WHERE workspace_id=$1 AND location_id=$2
            "#,
        )
        .bind(workspace_id.0)
        .bind(location_id)
        .bind(connection_id.0)
        .bind(checked_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        if result.rows_affected() == 0 {
            return Err(MarketError::Validation(format!(
                "location {location_id} has not been resolved"
            )));
        }
        Ok(())
    }

    pub(super) async fn clear_market_access(
        &self,
        workspace_id: WorkspaceId,
        location_id: i64,
        checked_at: DateTime<Utc>,
    ) -> Result<(), MarketError> {
        sqlx::query(
            r#"
            UPDATE market_location_names
            SET market_access_connection_id=NULL, market_access_checked_at=$3
            WHERE workspace_id=$1 AND location_id=$2
            "#,
        )
        .bind(workspace_id.0)
        .bind(location_id)
        .bind(checked_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    pub(super) async fn list_known_structures(
        &self,
        workspace_id: WorkspaceId,
        query: &str,
    ) -> Result<Vec<MarketStructureListing>, MarketError> {
        let contains = format!("%{}%", query.trim());
        let sql = format!(
            r#"
            SELECT location.location_id, location.location_name,
                   location.structure_type_id, structure_type.name_en AS structure_type_name,
                   location.solar_system_id, system.name_en AS solar_system_name,
                   system.region_id, region.name_en AS region_name,
                   {SECURITY_CLASS_CASE_SQL} AS security_class,
                   location.market_access_connection_id,
                   connection.character_name AS access_character_name,
                   connection.status AS access_connection_status,
                   location.market_access_checked_at
            FROM market_location_names location
            LEFT JOIN sde_imports import ON import.active=true
            LEFT JOIN sde_types structure_type
              ON structure_type.import_id=import.id
             AND structure_type.type_id=location.structure_type_id
            LEFT JOIN sde_solar_systems system
              ON system.import_id=import.id
             AND system.solar_system_id=location.solar_system_id
            LEFT JOIN sde_regions region
              ON region.import_id=import.id
             AND region.region_id=system.region_id
            LEFT JOIN eve_connections connection
              ON connection.id=location.market_access_connection_id
            WHERE location.workspace_id=$1
              AND (
                $2=''
                OR lower(location.location_name) LIKE lower($3)
                OR lower(system.name_en) LIKE lower($3)
              )
            ORDER BY location.location_name
            "#
        );
        sqlx::query_as::<_, MarketStructureListingRow>(&sql)
            .bind(workspace_id.0)
            .bind(query.trim())
            .bind(contains)
            .fetch_all(&self.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(MarketStructureListingRow::into_listing)
                    .collect()
            })
            .map_err(map_sqlx)
    }

    pub(super) async fn save_location_names(
        &self,
        workspace_id: WorkspaceId,
        locations: Vec<ResolvedMarketLocation>,
    ) -> Result<(), MarketError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        for location in locations {
            sqlx::query(
                r#"
                INSERT INTO market_location_names (
                  workspace_id,location_id,location_name,owner_id,solar_system_id,
                  structure_type_id,resolved_by_connection_id,resolved_at,updated_at
                ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,now())
                ON CONFLICT (workspace_id,location_id) DO UPDATE SET
                  location_name=EXCLUDED.location_name,
                  owner_id=EXCLUDED.owner_id,
                  solar_system_id=EXCLUDED.solar_system_id,
                  structure_type_id=EXCLUDED.structure_type_id,
                  resolved_by_connection_id=EXCLUDED.resolved_by_connection_id,
                  resolved_at=EXCLUDED.resolved_at,
                  updated_at=now()
                WHERE EXCLUDED.resolved_at >= market_location_names.resolved_at
                "#,
            )
            .bind(workspace_id.0)
            .bind(location.location_id)
            .bind(location.location_name)
            .bind(location.owner_id)
            .bind(location.solar_system_id)
            .bind(location.structure_type_id)
            .bind(location.resolved_by_connection_id.0)
            .bind(location.resolved_at)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;
        }
        tx.commit().await.map_err(map_sqlx)
    }
}
