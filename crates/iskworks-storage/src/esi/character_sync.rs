use super::*;

fn parse_refresh_state(value: &str) -> MarketRefreshState {
    match value {
        "current" => MarketRefreshState::Current,
        "refreshing" => MarketRefreshState::Refreshing,
        "failed" => MarketRefreshState::Failed,
        _ => MarketRefreshState::Missing,
    }
}

impl PgEsiRepository {
    pub async fn register_character_sources(
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

    pub async fn begin_character_source_refresh(
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

    pub async fn complete_character_source_refresh(
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

    pub async fn fail_character_source_refresh(
        &self,
        connection_id: ConnectedCharacterId,
        source_kind: CharacterSourceKind,
        claim: DateTime<Utc>,
        attempted_at: DateTime<Utc>,
        next_refresh_at: DateTime<Utc>,
        error_message: String,
    ) -> Result<bool, InventoryError> {
        // `next_refresh_at` is the caller's floor (it may carry ESI's
        // `Retry-After`); consecutive failures back off 5, 10, 20...
        // minutes on top, capped at 6 hours. The right-hand side sees the
        // pre-update count.
        let result = sqlx::query(
            "UPDATE character_source_sync_state
             SET refresh_state = 'failed',
                 last_error = $5,
                 next_refresh_at = GREATEST(
                   $4,
                   $6 + LEAST(
                     interval '5 minutes' * power(2, LEAST(consecutive_failures, 16)),
                     interval '6 hours'
                   )
                 ),
                 consecutive_failures = consecutive_failures + 1,
                 lease_expires_at = NULL
             WHERE connection_id = $1 AND source_kind = $2 AND last_attempted_at = $3",
        )
        .bind(connection_id.0)
        .bind(source_kind.as_db_str())
        .bind(claim)
        .bind(next_refresh_at)
        .bind(error_message)
        .bind(attempted_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn due_character_sources(
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

    /// Pushes this connection's due-or-sooner sources out to `until`, for a
    /// transient failure that affects all of them (the token refresh). Rows
    /// already scheduled later, or mid-refresh, are left alone.
    pub async fn defer_character_sources(
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

    pub async fn character_source_state(
        &self,
        connection_id: ConnectedCharacterId,
    ) -> Result<Vec<CharacterSourceSyncState>, InventoryError> {
        #[allow(clippy::type_complexity)]
        let rows: Vec<(
            String,
            String,
            Option<serde_json::Value>,
            Option<DateTime<Utc>>,
            Option<DateTime<Utc>>,
            Option<DateTime<Utc>>,
            Option<String>,
        )> = sqlx::query_as(
            "SELECT source_kind, refresh_state, summary, observed_at,
                    last_attempted_at, next_refresh_at, last_error
             FROM character_source_sync_state
             WHERE connection_id = $1",
        )
        .bind(connection_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(rows
            .into_iter()
            .filter_map(
                |(
                    kind,
                    state,
                    summary,
                    observed_at,
                    last_attempted_at,
                    next_refresh_at,
                    last_error,
                )| {
                    CharacterSourceKind::from_db_str(&kind).map(|source_kind| {
                        CharacterSourceSyncState {
                            connection_id,
                            source_kind,
                            refresh_state: parse_refresh_state(&state),
                            summary,
                            observed_at,
                            last_attempted_at,
                            next_refresh_at,
                            last_error,
                        }
                    })
                },
            )
            .collect())
    }

    /// Reads whatever names are already cached in `eve_entity_names` for the
    /// given IDs -- corporation/solar-system names for the roster included,
    /// but this table isn't character-sync-specific (`cache_entity_names`
    /// already backs wallet-client-name resolution). A pure DB read: unknown
    /// IDs are simply absent from the result, not resolved via ESI here.
    pub async fn entity_names(
        &self,
        ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, String>, InventoryError> {
        if ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT entity_id, entity_name FROM eve_entity_names WHERE entity_id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(rows.into_iter().collect())
    }

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
