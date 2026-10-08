use super::*;

impl PgEsiRepository {
    pub async fn list_connections(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<ConnectedCharacter>, InventoryError> {
        sqlx::query_as::<_, ConnectionRow>(
            r#"
            SELECT id, workspace_id, owner_id, eve_character_id, character_name, status,
                   granted_scopes, access_token_expires_at, last_refreshed_at,
                   last_error_code, last_error_message, connected_at, updated_at,
                   disconnected_at, revision
            FROM eve_connections WHERE workspace_id = $1
            ORDER BY connected_at DESC
            "#,
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        .into_iter()
        .map(ConnectionRow::into_domain)
        .collect()
    }

    pub async fn get_connection(
        &self,
        id: ConnectedCharacterId,
    ) -> Result<ConnectedCharacter, InventoryError> {
        sqlx::query_as::<_, ConnectionRow>(
            r#"
            SELECT id, workspace_id, owner_id, eve_character_id, character_name, status,
                   granted_scopes, access_token_expires_at, last_refreshed_at,
                   last_error_code, last_error_message, connected_at, updated_at,
                   disconnected_at, revision
            FROM eve_connections WHERE id = $1
            "#,
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?
        .ok_or(InventoryError::ItemNotFound)?
        .into_domain()
    }

    pub async fn begin_authorization(
        &self,
        state_hash: String,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        verifier: EncryptedSecret,
        scopes: &[String],
    ) -> Result<(), InventoryError> {
        sqlx::query(
            r#"
            INSERT INTO eve_oauth_pending_authorizations (
              state_hash, workspace_id, owner_id, pkce_verifier_envelope,
              requested_scopes, return_path, created_at, expires_at
            ) VALUES ($1,$2,$3,$4,$5,'/settings/eve/callback',now(),now() + interval '10 minutes')
            "#,
        )
        .bind(state_hash)
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .bind(serde_json::to_value(verifier).map_err(map_json)?)
        .bind(scopes)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    pub async fn consume_authorization(
        &self,
        state_hash: &str,
    ) -> Result<PendingAuthorization, InventoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        let row = sqlx::query_as::<_, PendingRow>(
            r#"
            SELECT workspace_id, owner_id, pkce_verifier_envelope, requested_scopes, return_path
            FROM eve_oauth_pending_authorizations
            WHERE state_hash = $1 AND consumed_at IS NULL AND expires_at > now()
            FOR UPDATE
            "#,
        )
        .bind(state_hash)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx)?
        .ok_or_else(|| {
            InventoryError::Validation(
                "Authorization state is invalid, expired, or already used.".to_string(),
            )
        })?;
        sqlx::query(
            "UPDATE eve_oauth_pending_authorizations SET consumed_at = now() WHERE state_hash = $1",
        )
        .bind(state_hash)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        tx.commit().await.map_err(map_sqlx)?;
        Ok(PendingAuthorization {
            workspace_id: WorkspaceId(row.workspace_id),
            owner_id: OwnerId(row.owner_id),
            verifier: serde_json::from_value(row.pkce_verifier_envelope).map_err(map_json)?,
            requested_scopes: row.requested_scopes,
            return_path: row.return_path,
        })
    }

    pub async fn complete_connection(
        &self,
        pending: &PendingAuthorization,
        eve_character_id: i64,
        character_name: &str,
        scopes: &[String],
        expires_at: DateTime<Utc>,
        refresh_token: EncryptedSecret,
    ) -> Result<ConnectedCharacter, InventoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        let id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO eve_connections (
              id, workspace_id, owner_id, eve_character_id, character_name, status,
              granted_scopes, access_token_expires_at, last_refreshed_at,
              connected_at, updated_at, revision
            ) VALUES ($1,$2,$3,$4,$5,'connected',$6,$7,now(),now(),now(),1)
            ON CONFLICT (workspace_id, owner_id, eve_character_id) DO UPDATE SET
              character_name = EXCLUDED.character_name,
              status = 'connected',
              granted_scopes = EXCLUDED.granted_scopes,
              access_token_expires_at = EXCLUDED.access_token_expires_at,
              last_refreshed_at = now(),
              disconnected_at = NULL,
              last_error_code = NULL,
              last_error_message = NULL,
              updated_at = now(),
              revision = eve_connections.revision + 1
            RETURNING id
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(pending.workspace_id.0)
        .bind(pending.owner_id.0)
        .bind(eve_character_id)
        .bind(character_name)
        .bind(scopes)
        .bind(expires_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        sqlx::query(
            r#"
            INSERT INTO eve_connection_tokens (connection_id, refresh_token_envelope, token_revision, updated_at)
            VALUES ($1,$2,1,now())
            ON CONFLICT (connection_id) DO UPDATE SET
              refresh_token_envelope = EXCLUDED.refresh_token_envelope,
              access_token_envelope = NULL,
              token_revision = eve_connection_tokens.token_revision + 1,
              updated_at = now(),
              last_refresh_error = NULL
            "#,
        )
        .bind(id)
        .bind(serde_json::to_value(refresh_token).map_err(map_json)?)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        for kind in CharacterSourceKind::all() {
            sqlx::query(
                "INSERT INTO character_source_sync_state (connection_id, source_kind)
                 VALUES ($1, $2)
                 ON CONFLICT (connection_id, source_kind) DO NOTHING",
            )
            .bind(id)
            .bind(kind.as_db_str())
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;
        }
        tx.commit().await.map_err(map_sqlx)?;
        self.get_connection(ConnectedCharacterId(id)).await
    }

    /// Fixture-mode connect: grants exactly `scopes` (the service passes the
    /// full requested set, so fixture characters exercise every feature).
    pub async fn mock_connect(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        envelope: EncryptedSecret,
        scopes: &[String],
    ) -> Result<ConnectedCharacter, InventoryError> {
        let pending = PendingAuthorization {
            workspace_id,
            owner_id,
            verifier: envelope.clone(),
            requested_scopes: vec![],
            return_path: "/settings/eve".to_string(),
        };
        self.complete_connection(
            &pending,
            2_119_000_001,
            "Fixture Industrialist",
            scopes,
            crate::db_now() + chrono::Duration::minutes(20),
            envelope,
        )
        .await
    }

    pub async fn load_refresh_token(
        &self,
        id: ConnectedCharacterId,
    ) -> Result<StoredRefreshToken, InventoryError> {
        let row = sqlx::query_as::<_, TokenRow>(
            r#"
            SELECT t.refresh_token_envelope, t.access_token_envelope, t.token_revision,
                   c.id, c.workspace_id, c.owner_id, c.eve_character_id, c.character_name,
                   c.status, c.granted_scopes, c.access_token_expires_at, c.last_refreshed_at,
                   c.last_error_code, c.last_error_message, c.connected_at, c.updated_at,
                   c.disconnected_at, c.revision
            FROM eve_connection_tokens t
            JOIN eve_connections c ON c.id = t.connection_id
            WHERE c.id = $1 AND c.disconnected_at IS NULL
            "#,
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?
        .ok_or_else(|| {
            InventoryError::Validation(
                "This EVE connection is disconnected or unavailable.".to_string(),
            )
        })?;
        let connection = row.connection().into_domain()?;
        Ok(StoredRefreshToken {
            envelope: serde_json::from_value(row.refresh_token_envelope).map_err(map_json)?,
            access_token: row
                .access_token_envelope
                .map(|value| serde_json::from_value(value).map_err(map_json))
                .transpose()?,
            token_revision: to_u64(row.token_revision)?,
            connection,
        })
    }

    pub async fn save_refreshed_token(
        &self,
        id: ConnectedCharacterId,
        expected_token_revision: u64,
        envelope: Option<EncryptedSecret>,
        access_token: EncryptedSecret,
        scopes: &[String],
        expires_at: DateTime<Utc>,
    ) -> Result<bool, InventoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        // A NULL `$3` keeps the stored refresh token (EVE didn't rotate it).
        let result = sqlx::query(
            r#"
            UPDATE eve_connection_tokens
            SET refresh_token_envelope = COALESCE($3, refresh_token_envelope),
              access_token_envelope = $4,
              token_revision = token_revision + 1, updated_at = now(), last_refresh_error = NULL
            WHERE connection_id = $1 AND token_revision = $2
            "#,
        )
        .bind(id.0)
        .bind(
            i64::try_from(expected_token_revision)
                .map_err(|_| InventoryError::ArithmeticOverflow)?,
        )
        .bind(
            envelope
                .map(|envelope| serde_json::to_value(envelope).map_err(map_json))
                .transpose()?,
        )
        .bind(serde_json::to_value(access_token).map_err(map_json)?)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        if result.rows_affected() == 0 {
            tx.rollback().await.map_err(map_sqlx)?;
            return Ok(false);
        }
        sqlx::query(
            "UPDATE eve_connections SET status = 'connected', granted_scopes = $2, access_token_expires_at = $3, last_refreshed_at = now(), last_error_code = NULL, last_error_message = NULL, updated_at = now(), revision = revision + 1 WHERE id = $1",
        )
        .bind(id.0)
        .bind(scopes)
        .bind(expires_at)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        tx.commit().await.map_err(map_sqlx)?;
        Ok(true)
    }

    /// Records that a connection's token can no longer be refreshed (EVE
    /// rejected it, or it lacks a required scope), which takes it out of the
    /// worker's character-sync queue until the user reconnects -- a fresh
    /// authorization resets `status` to `connected`.
    pub async fn mark_connection_unrefreshable(
        &self,
        id: ConnectedCharacterId,
        status: ConnectionStatus,
        error_code: &str,
    ) -> Result<(), InventoryError> {
        let status = match status {
            ConnectionStatus::NeedsReconnection => "needs_reconnection",
            ConnectionStatus::MissingScope => "missing_scope",
            ConnectionStatus::TemporarilyUnavailable => "temporarily_unavailable",
            ConnectionStatus::Connected | ConnectionStatus::Disconnected => {
                return Err(InventoryError::Validation(
                    "Only a failure status can mark a connection unrefreshable.".to_string(),
                ));
            }
        };
        sqlx::query(
            "UPDATE eve_connections
             SET status = $2, last_error_code = $3, updated_at = now(), revision = revision + 1
             WHERE id = $1 AND disconnected_at IS NULL",
        )
        .bind(id.0)
        .bind(status)
        .bind(error_code)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    pub async fn disconnect(
        &self,
        id: ConnectedCharacterId,
    ) -> Result<ConnectedCharacter, InventoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        sqlx::query("DELETE FROM eve_connection_tokens WHERE connection_id = $1")
            .bind(id.0)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;
        let affected = sqlx::query("UPDATE eve_connections SET status = 'disconnected', disconnected_at = now(), updated_at = now(), revision = revision + 1 WHERE id = $1 AND disconnected_at IS NULL")
            .bind(id.0)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?
            .rows_affected();
        if affected == 0 {
            return Err(InventoryError::ItemNotFound);
        }
        tx.commit().await.map_err(map_sqlx)?;
        self.get_connection(id).await
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct ConnectionRow {
    id: Uuid,
    workspace_id: Uuid,
    owner_id: Uuid,
    eve_character_id: i64,
    character_name: String,
    status: String,
    granted_scopes: Vec<String>,
    access_token_expires_at: Option<DateTime<Utc>>,
    last_refreshed_at: Option<DateTime<Utc>>,
    last_error_code: Option<String>,
    last_error_message: Option<String>,
    connected_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    disconnected_at: Option<DateTime<Utc>>,
    revision: i64,
}

impl ConnectionRow {
    pub(super) fn into_domain(self) -> Result<ConnectedCharacter, InventoryError> {
        Ok(ConnectedCharacter {
            id: ConnectedCharacterId(self.id),
            workspace_id: WorkspaceId(self.workspace_id),
            owner_id: OwnerId(self.owner_id),
            eve_character_id: self.eve_character_id,
            character_name: self.character_name,
            status: connection_status_from_str(&self.status)?,
            granted_scopes: self.granted_scopes,
            access_token_expires_at: self.access_token_expires_at,
            last_refreshed_at: self.last_refreshed_at,
            last_error_code: self.last_error_code,
            last_error_message: self.last_error_message,
            connected_at: self.connected_at,
            updated_at: self.updated_at,
            disconnected_at: self.disconnected_at,
            revision: to_u64(self.revision)?,
        })
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct PendingRow {
    workspace_id: Uuid,
    owner_id: Uuid,
    pkce_verifier_envelope: Value,
    requested_scopes: Vec<String>,
    return_path: String,
}

#[derive(sqlx::FromRow)]
pub(super) struct TokenRow {
    refresh_token_envelope: Value,
    access_token_envelope: Option<Value>,
    token_revision: i64,
    id: Uuid,
    workspace_id: Uuid,
    owner_id: Uuid,
    eve_character_id: i64,
    character_name: String,
    status: String,
    granted_scopes: Vec<String>,
    access_token_expires_at: Option<DateTime<Utc>>,
    last_refreshed_at: Option<DateTime<Utc>>,
    last_error_code: Option<String>,
    last_error_message: Option<String>,
    connected_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    disconnected_at: Option<DateTime<Utc>>,
    revision: i64,
}

impl TokenRow {
    pub(super) fn connection(&self) -> ConnectionRow {
        ConnectionRow {
            id: self.id,
            workspace_id: self.workspace_id,
            owner_id: self.owner_id,
            eve_character_id: self.eve_character_id,
            character_name: self.character_name.clone(),
            status: self.status.clone(),
            granted_scopes: self.granted_scopes.clone(),
            access_token_expires_at: self.access_token_expires_at,
            last_refreshed_at: self.last_refreshed_at,
            last_error_code: self.last_error_code.clone(),
            last_error_message: self.last_error_message.clone(),
            connected_at: self.connected_at,
            updated_at: self.updated_at,
            disconnected_at: self.disconnected_at,
            revision: self.revision,
        }
    }
}

pub(super) fn parse_esi_expiry(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc2822(value)
        .or_else(|_| DateTime::parse_from_rfc3339(value))
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

pub(super) fn connection_status_from_str(value: &str) -> Result<ConnectionStatus, InventoryError> {
    match value {
        "connected" => Ok(ConnectionStatus::Connected),
        "needs_reconnection" => Ok(ConnectionStatus::NeedsReconnection),
        "missing_scope" => Ok(ConnectionStatus::MissingScope),
        "temporarily_unavailable" => Ok(ConnectionStatus::TemporarilyUnavailable),
        "disconnected" => Ok(ConnectionStatus::Disconnected),
        _ => Err(invalid_projection("connection status")),
    }
}
