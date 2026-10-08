-- Admin-controlled sign-in block. A disabled user's sessions stop resolving
-- and EVE SSO sign-in is refused; nothing is deleted and it is reversible.
ALTER TABLE users ADD COLUMN disabled_at timestamptz NULL;
