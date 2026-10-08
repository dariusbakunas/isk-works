-- Identity/session layer for EVE SSO login (multi-tenant access), separate
-- from `owners` (who data within a workspace is attributed to) and from
-- `eve_oauth_pending_authorizations` (character-link OAuth, which requires
-- an existing workspace_id/owner_id -- login is what creates the workspace,
-- so it needs its own pending-authorization record with no such precondition).
-- See docs/superpowers/specs/2026-08-14-eve-sso-login-and-tenancy-design.md.

ALTER TABLE workspaces
  ADD COLUMN claimed_at timestamptz NULL;

DROP INDEX workspaces_singleton_idx;

CREATE TABLE users (
  id uuid PRIMARY KEY,
  eve_character_id bigint NOT NULL UNIQUE,
  eve_character_name text NOT NULL,
  workspace_id uuid NOT NULL UNIQUE REFERENCES workspaces (id) ON DELETE RESTRICT,
  created_at timestamptz NOT NULL,
  last_login_at timestamptz NOT NULL
);

CREATE TABLE sessions (
  token_hash text PRIMARY KEY,
  user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  created_at timestamptz NOT NULL,
  expires_at timestamptz NOT NULL
);

CREATE INDEX sessions_user_id_idx ON sessions (user_id);

CREATE TABLE login_oauth_pending_authorizations (
  state_hash text PRIMARY KEY,
  pkce_verifier_encrypted jsonb NOT NULL,
  created_at timestamptz NOT NULL,
  consumed_at timestamptz NULL
);
