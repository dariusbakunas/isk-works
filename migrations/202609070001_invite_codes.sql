-- Invite-only onboarding for the public alpha (security hardening, slice 3).
--
-- Gates NEW identity / NEW workspace provisioning only. Existing `users`
-- rows keep logging in untouched. Deliberately small and easy to drop later:
-- one table plus a nullable server-side reference on the login pending-auth
-- row. See docs/security/invite-only-alpha.md.
--
-- The usable code is NEVER stored -- only `code_hash`, a SHA-256 of the
-- normalized code. Codes are high-entropy CSPRNG tokens (~80 bits), so a
-- fast cryptographic hash is sufficient (this is not a password).

CREATE TABLE invite_codes (
  id uuid PRIMARY KEY,
  code_hash text NOT NULL UNIQUE,
  created_at timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz NULL,
  disabled_at timestamptz NULL,
  max_uses integer NOT NULL,
  use_count integer NOT NULL DEFAULT 0,
  note text NULL,
  CONSTRAINT invite_codes_max_uses_positive CHECK (max_uses > 0),
  CONSTRAINT invite_codes_use_count_non_negative CHECK (use_count >= 0),
  CONSTRAINT invite_codes_use_count_within_max CHECK (use_count <= max_uses)
);

-- Server-side association only: the login pending-auth row remembers which
-- invite the user pre-validated, by id. The raw code never touches this
-- table, the OAuth `state`, or any URL. NULL for returning-user logins and
-- for every login when invite mode is off.
ALTER TABLE login_oauth_pending_authorizations
  ADD COLUMN invite_id uuid NULL REFERENCES invite_codes (id) ON DELETE SET NULL;
