-- The connection's current ESI access token, encrypted like the refresh
-- token (docs/security/2026-10-06-esi-usage-audit.md, finding 3). It is
-- reused until shortly before `eve_connections.access_token_expires_at`
-- instead of asking EVE SSO for a new one on every ESI call. NULL until the
-- first refresh, and cleared whenever the character is reconnected.

ALTER TABLE eve_connection_tokens ADD COLUMN access_token_envelope jsonb;
