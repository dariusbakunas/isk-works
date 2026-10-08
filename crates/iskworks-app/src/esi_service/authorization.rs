use super::*;

impl EsiApplicationService {
    pub async fn begin_authorization(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
    ) -> Result<AuthorizationStart, EsiApplicationError> {
        let scopes = authorization_scopes();
        if self.fixture_mode {
            let connection = self
                .repository
                .mock_connect(
                    workspace_id,
                    owner_id,
                    self.cipher.encrypt("fixture-refresh-token")?,
                    &scopes,
                )
                .await?;
            return Ok(AuthorizationStart {
                authorization_url: "/characters?connected=fixture".to_string(),
                fixture_mode: true,
                connection: Some(connection),
                requested_scopes: scopes,
                state: None,
            });
        }
        let state = OAuthState::generate();
        let (verifier, challenge) = new_pkce();
        self.repository
            .begin_authorization(
                hash_state(state.expose()),
                workspace_id,
                owner_id,
                self.cipher.encrypt(verifier.expose())?,
                &scopes,
            )
            .await?;
        Ok(AuthorizationStart {
            authorization_url: authorization_url(
                &self.authorization_url,
                &self.client_id,
                &self.redirect_uri,
                &scopes,
                &state,
                &challenge,
            )?,
            fixture_mode: false,
            connection: None,
            requested_scopes: scopes,
            state: Some(state),
        })
    }

    /// Character-link and login share one EVE app registration and one
    /// callback URL (EVE's developer portal only allows one per app) — the
    /// callback route tells the two apart by which pending-authorization
    /// table `state` is found in, trying this one and `AuthService`'s login
    /// equivalent in turn. Returns `None`, not an error, when `state`
    /// doesn't belong to this flow, so the caller can try the other one.
    pub async fn try_consume_authorization(
        &self,
        state: &str,
    ) -> Result<Option<PendingAuthorization>, EsiApplicationError> {
        match self
            .repository
            .consume_authorization(&hash_state(state))
            .await
        {
            Ok(pending) => Ok(Some(pending)),
            Err(InventoryError::Validation(_)) => Ok(None),
            Err(other) => Err(other.into()),
        }
    }

    pub async fn finish_authorization(
        &self,
        pending: PendingAuthorization,
        code: &str,
    ) -> Result<ConnectedCharacter, EsiApplicationError> {
        let verifier = PkceVerifier::from_secret(self.cipher.decrypt(&pending.verifier)?)?;
        let token = self.transport.exchange_code(code, &verifier).await?;
        require_requested_scopes(&token.identity)?;
        self.repository
            .complete_connection(
                &pending,
                token.identity.character_id,
                &token.identity.character_name,
                &token.identity.scopes.iter().cloned().collect::<Vec<_>>(),
                token.expires_at,
                self.cipher.encrypt(&token.refresh_token)?,
            )
            .await
            .map_err(Into::into)
    }

    /// Disconnects a character: deletes its token locally, then revokes the
    /// refresh token at EVE SSO so the grant stops working everywhere. The
    /// local disconnect is what matters and always stands; revocation is
    /// best effort and only logged if EVE can't be reached.
    pub async fn disconnect(
        &self,
        id: ConnectedCharacterId,
    ) -> Result<ConnectedCharacter, EsiApplicationError> {
        let refresh_token = match self.repository.load_refresh_token(id).await {
            Ok(stored) => self.cipher.decrypt(&stored.envelope).ok(),
            Err(_) => None,
        };
        let connection = self.repository.disconnect(id).await?;
        if let Some(refresh_token) = refresh_token {
            if let Err(error) = self.transport.revoke_refresh_token(&refresh_token).await {
                tracing::warn!(
                    %error,
                    connection_id = %id.0,
                    "could not revoke the disconnected character's refresh token at EVE"
                );
            }
        }
        Ok(connection)
    }

    /// Refreshes a connection's access token. When EVE rejects the refresh
    /// for good (see `reconnect_status`), the connection is flagged so the
    /// worker stops retrying it and the UI asks the user to reconnect.
    pub async fn refresh(
        &self,
        id: ConnectedCharacterId,
    ) -> Result<(ConnectedCharacter, String), EsiApplicationError> {
        let result = self.refresh_token(id).await;
        if let Err(error) = &result {
            if let Some(status) = reconnect_status(error) {
                let code = match error {
                    EsiApplicationError::Protocol(protocol) => error_code(protocol),
                    _ => "esi_failure",
                };
                if let Err(mark_error) = self
                    .repository
                    .mark_connection_unrefreshable(id, status, code)
                    .await
                {
                    tracing::warn!(
                        error = %mark_error,
                        connection_id = %id.0,
                        "could not flag connection for reconnection"
                    );
                }
            }
        }
        result
    }

    async fn refresh_token(
        &self,
        id: ConnectedCharacterId,
    ) -> Result<(ConnectedCharacter, String), EsiApplicationError> {
        for _ in 0..2 {
            let stored = self.repository.load_refresh_token(id).await?;
            // EVE already rejected this grant (or it lacks scopes); only a
            // reconnect fixes that, so don't ask SSO again on every click.
            match stored.connection.status {
                ConnectionStatus::NeedsReconnection => {
                    return Err(EsiError::AuthorizationRequired.into());
                }
                ConnectionStatus::MissingScope => return Err(EsiError::MissingScope.into()),
                _ => {}
            }
            if let Some(access_token) = reusable_access_token(&stored, Utc::now()) {
                let access_token = self.cipher.decrypt(access_token)?;
                return Ok((stored.connection, access_token));
            }
            let refresh_token = self.cipher.decrypt(&stored.envelope)?;
            let refreshed = self.transport.refresh(&refresh_token).await?;
            require_sync_scopes(&refreshed.identity)?;
            if refreshed.identity.character_id != stored.connection.eve_character_id {
                return Err(EsiError::InvalidIdentity.into());
            }
            let rotated = refreshed
                .rotated_refresh_token
                .as_deref()
                .map(|value| self.cipher.encrypt(value))
                .transpose()?;
            if self
                .repository
                .save_refreshed_token(
                    id,
                    stored.token_revision,
                    rotated,
                    self.cipher.encrypt(&refreshed.access_token)?,
                    &refreshed
                        .identity
                        .scopes
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>(),
                    refreshed.expires_at,
                )
                .await?
            {
                return Ok((
                    self.repository.get_connection(id).await?,
                    refreshed.access_token,
                ));
            }
        }
        Err(InventoryError::RevisionConflict.into())
    }
}

pub(super) fn require_sync_scopes(identity: &Identity) -> Result<(), EsiError> {
    if SYNC_SCOPES
        .iter()
        .all(|scope| identity.scopes.contains(*scope))
    {
        Ok(())
    } else {
        Err(EsiError::MissingScope)
    }
}

pub(super) fn authorization_scopes() -> Vec<String> {
    REQUESTED_SCOPES
        .iter()
        .chain(OPTIONAL_SCOPES.iter())
        .map(ToString::to_string)
        .collect()
}

pub(super) fn require_requested_scopes(identity: &Identity) -> Result<(), EsiError> {
    if REQUESTED_SCOPES
        .iter()
        .all(|scope| identity.scopes.contains(*scope))
    {
        Ok(())
    } else {
        Err(EsiError::MissingScope)
    }
}

/// How long before expiry a stored access token stops being handed out. A
/// character sync makes many sequential requests with one token, so it must
/// outlive the whole sync, not just the first call.
pub(super) const ACCESS_TOKEN_REUSE_MARGIN: Duration = Duration::minutes(5);

/// The stored access token, when it can still be used: the connection is
/// healthy and the token has more than `ACCESS_TOKEN_REUSE_MARGIN` left.
/// Reusing it instead of asking EVE SSO for a new one on every call is what
/// CCP expects of an SSO client.
pub(super) fn reusable_access_token(
    stored: &StoredRefreshToken,
    now: DateTime<Utc>,
) -> Option<&EncryptedSecret> {
    let expires_at = stored.connection.access_token_expires_at?;
    (stored.connection.status == ConnectionStatus::Connected
        && expires_at > now + ACCESS_TOKEN_REUSE_MARGIN)
        .then_some(stored.access_token.as_ref())
        .flatten()
}
