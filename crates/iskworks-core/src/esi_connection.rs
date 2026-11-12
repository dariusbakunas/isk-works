//! A workspace's EVE SSO connections and their tokens, as a port: the
//! login flow and token refresh write through it, and storage implements
//! it for `PgEsiRepository`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{
    CharacterConnectionReader, ConnectedCharacter, ConnectedCharacterId, ConnectionStatus,
    EncryptedSecret, OwnerId, StoreError, WorkspaceId,
};

/// An SSO login that was started and not yet completed, found by the hash
/// of its `state` parameter.
#[derive(Debug, Clone)]
pub struct PendingAuthorization {
    pub workspace_id: WorkspaceId,
    pub owner_id: OwnerId,
    pub verifier: EncryptedSecret,
    pub requested_scopes: Vec<String>,
    pub return_path: String,
}

/// A connection with its stored tokens.
#[derive(Debug, Clone)]
pub struct StoredRefreshToken {
    pub connection: ConnectedCharacter,
    pub envelope: EncryptedSecret,
    /// The current access token, valid until
    /// `connection.access_token_expires_at`. `None` until the first refresh.
    pub access_token: Option<EncryptedSecret>,
    pub token_revision: u64,
}

/// Connecting, refreshing and disconnecting a workspace's characters.
#[async_trait]
pub trait EsiConnectionRepository: CharacterConnectionReader {
    /// Stores a started login's PKCE verifier under its state hash.
    async fn begin_authorization(
        &self,
        state_hash: String,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        verifier: EncryptedSecret,
        scopes: &[String],
    ) -> Result<(), StoreError>;

    /// Consumes a started, unexpired login: each state works once.
    async fn consume_authorization(
        &self,
        state_hash: &str,
    ) -> Result<PendingAuthorization, StoreError>;

    /// Creates or reconnects the character a completed login names.
    async fn complete_connection(
        &self,
        pending: &PendingAuthorization,
        eve_character_id: i64,
        character_name: &str,
        scopes: &[String],
        expires_at: DateTime<Utc>,
        refresh_token: EncryptedSecret,
    ) -> Result<ConnectedCharacter, StoreError>;

    /// Fixture-mode connect: grants exactly `scopes` (the service passes the
    /// full requested set, so fixture characters exercise every feature).
    async fn mock_connect(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        envelope: EncryptedSecret,
        scopes: &[String],
    ) -> Result<ConnectedCharacter, StoreError>;

    async fn load_refresh_token(
        &self,
        id: ConnectedCharacterId,
    ) -> Result<StoredRefreshToken, StoreError>;

    /// Saves refreshed tokens if the stored ones are still at
    /// `expected_token_revision`. Returns `false` when another refresh won.
    async fn save_refreshed_token(
        &self,
        id: ConnectedCharacterId,
        expected_token_revision: u64,
        envelope: Option<EncryptedSecret>,
        access_token: EncryptedSecret,
        scopes: &[String],
        expires_at: DateTime<Utc>,
    ) -> Result<bool, StoreError>;

    /// Records that a connection's token can no longer be refreshed (EVE
    /// rejected it, or it lacks a required scope), which takes it out of the
    /// worker's character-sync queue until the user reconnects -- a fresh
    /// authorization resets `status` to `connected`.
    async fn mark_connection_unrefreshable(
        &self,
        id: ConnectedCharacterId,
        status: ConnectionStatus,
        error_code: &str,
    ) -> Result<(), StoreError>;

    /// Deletes the connection's tokens and marks it disconnected.
    async fn disconnect(&self, id: ConnectedCharacterId) -> Result<ConnectedCharacter, StoreError>;
}
