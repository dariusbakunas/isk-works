use super::*;
use async_trait::async_trait;
use iskworks_core::{CharacterSourceRefreshRepository, RetryPolicy};

pub(super) fn parse_refresh_state(value: &str) -> MarketRefreshState {
    match value {
        "current" => MarketRefreshState::Current,
        "refreshing" => MarketRefreshState::Refreshing,
        "failed" => MarketRefreshState::Failed,
        _ => MarketRefreshState::Missing,
    }
}

impl PgEsiRepository {
    /// Resolve industry-job facility ids to a name (+ solar system when
    /// known) from data this workspace already has: the player-structure
    /// name cache the Add-Structure / asset-sync flows populate
    /// (`market_location_names`, workspace-scoped) and the SDE NPC-station
    /// catalogue. A pure read -- facility ids nothing has resolved yet are
    /// simply omitted, and the caller renders them as an unknown structure.
    pub async fn industry_facility_labels(
        &self,
        workspace_id: WorkspaceId,
        facility_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, super::FacilityLabel>, InventoryError> {
        if facility_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let rows: Vec<(i64, String, Option<String>)> = sqlx::query_as(
            r#"
            SELECT mln.location_id, mln.location_name, sys.name_en
            FROM market_location_names mln
            LEFT JOIN sde_imports imp ON imp.active = true
            LEFT JOIN sde_solar_systems sys
              ON sys.import_id = imp.id AND sys.solar_system_id = mln.solar_system_id
            WHERE mln.workspace_id = $1 AND mln.location_id = ANY($2)
            UNION
            SELECT npc.station_id, npc.name_en, sys.name_en
            FROM sde_imports imp
            JOIN sde_npc_stations npc ON npc.import_id = imp.id
            JOIN sde_solar_systems sys
              ON sys.import_id = imp.id AND sys.solar_system_id = npc.solar_system_id
            WHERE imp.active = true AND npc.station_id = ANY($2)
            "#,
        )
        .bind(workspace_id.0)
        .bind(facility_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(rows
            .into_iter()
            .map(|(id, name, solar_system_name)| {
                (
                    id,
                    super::FacilityLabel {
                        name,
                        solar_system_name,
                    },
                )
            })
            .collect())
    }
}

#[async_trait]
impl CharacterSourceRefreshRepository for PgEsiRepository {
    async fn register_character_sources(
        &self,
        connection_id: ConnectedCharacterId,
    ) -> Result<(), InventoryError> {
        for kind in CharacterSourceKind::all() {
            sqlx::query(
                "INSERT INTO character_source_sync_state (connection_id, source_kind)
                 VALUES ($1, $2)
                 ON CONFLICT (connection_id, source_kind) DO NOTHING",
            )
            .bind(connection_id.0)
            .bind(kind.as_db_str())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;
        }
        Ok(())
    }

    async fn begin_character_source_refresh(
        &self,
        connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        attempted_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, InventoryError> {
        let claimed: Option<(DateTime<Utc>,)> = sqlx::query_as(
            "UPDATE character_source_sync_state
             SET refresh_state = 'refreshing',
                 last_attempted_at = $3,
                 lease_expires_at = $4
             WHERE connection_id = $1 AND source_kind = $2
               AND (refresh_state <> 'refreshing' OR lease_expires_at <= $3)
               AND (next_refresh_at IS NULL OR next_refresh_at <= $3)
             RETURNING last_attempted_at",
        )
        .bind(connection_id.0)
        .bind(source_kind.as_db_str())
        .bind(attempted_at)
        .bind(lease_expires_at)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(claimed.map(|(claim,)| claim))
    }

    async fn complete_character_source_refresh(
        &self,
        connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        claim: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        summary: serde_json::Value,
        observed_at: DateTime<Utc>,
    ) -> Result<bool, InventoryError> {
        let result = sqlx::query(
            "UPDATE character_source_sync_state
             SET refresh_state = 'current',
                 summary = $5,
                 observed_at = $6,
                 next_refresh_at = $4,
                 lease_expires_at = NULL,
                 last_error = NULL,
                 consecutive_failures = 0
             WHERE connection_id = $1 AND source_kind = $2 AND last_attempted_at = $3",
        )
        .bind(connection_id.0)
        .bind(source_kind.as_db_str())
        .bind(claim)
        .bind(next_refresh_at)
        .bind(summary)
        .bind(observed_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(result.rows_affected() > 0)
    }

    async fn fail_character_source_refresh(
        &self,
        connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        claim: DateTime<Utc>,
        attempted_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        error_message: String,
    ) -> Result<bool, InventoryError> {
        // `next_refresh_at` is the caller's floor (it may carry ESI's
        // `Retry-After`); consecutive failures back off on top, per
        // `RetryPolicy::CHARACTER_SOURCE`, from the pre-update count.
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        let failures: Option<i32> = sqlx::query_scalar(
            "SELECT consecutive_failures FROM character_source_sync_state
             WHERE connection_id = $1 AND source_kind = $2 AND last_attempted_at = $3
             FOR UPDATE",
        )
        .bind(connection_id.0)
        .bind(source_kind.as_db_str())
        .bind(claim)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        let Some(failures) = failures else {
            return Ok(false);
        };
        let retry_at = RetryPolicy::CHARACTER_SOURCE.retry_at(
            attempted_at,
            failures.unsigned_abs(),
            next_refresh_at,
        );
        sqlx::query(
            "UPDATE character_source_sync_state
             SET refresh_state = 'failed',
                 last_error = $5,
                 next_refresh_at = $4,
                 consecutive_failures = consecutive_failures + 1,
                 lease_expires_at = NULL
             WHERE connection_id = $1 AND source_kind = $2 AND last_attempted_at = $3",
        )
        .bind(connection_id.0)
        .bind(source_kind.as_db_str())
        .bind(claim)
        .bind(retry_at)
        .bind(error_message)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        tx.commit().await.map_err(map_sqlx)?;
        Ok(true)
    }

    async fn due_character_sources(
        &self,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<(ConnectedCharacterId, CharacterSourceKind)>, InventoryError> {
        // Only connections that can actually sync: live (not disconnected,
        // not flagged for reconnection by a rejected refresh) and not
        // belonging to a disabled user. Anything else would sit at the head
        // of this queue forever and starve every other tenant.
        let rows: Vec<(Uuid, String)> = sqlx::query_as(
            "SELECT s.connection_id, s.source_kind
             FROM character_source_sync_state s
             JOIN eve_connections c ON c.id = s.connection_id
             WHERE s.refresh_state IN ('missing', 'current', 'failed')
               AND (s.next_refresh_at IS NULL OR s.next_refresh_at <= $1)
               AND c.status = 'connected'
               AND c.disconnected_at IS NULL
               AND NOT EXISTS (
                 SELECT 1 FROM users u
                 WHERE u.workspace_id = c.workspace_id AND u.disabled_at IS NOT NULL
               )
             ORDER BY s.next_refresh_at NULLS FIRST
             LIMIT $2",
        )
        .bind(now)
        .bind(limit.max(0))
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, kind)| {
                CharacterSourceKind::from_db_str(&kind).map(|kind| (ConnectedCharacterId(id), kind))
            })
            .collect())
    }

    async fn defer_character_sources(
        &self,
        connection_id: ConnectedCharacterId,
        until: DateTime<Utc>,
    ) -> Result<(), InventoryError> {
        sqlx::query(
            "UPDATE character_source_sync_state
             SET next_refresh_at = $2
             WHERE connection_id = $1
               AND refresh_state IN ('missing', 'current', 'failed')
               AND (next_refresh_at IS NULL OR next_refresh_at < $2)",
        )
        .bind(connection_id.0)
        .bind(until)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }
}
