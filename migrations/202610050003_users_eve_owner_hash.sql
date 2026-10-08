-- The EVE SSO JWT `owner` claim for each login identity: a hash of the EVE
-- account that owns the character. A character transferred to another
-- account (Character Bazaar) keeps its character_id but gets a new owner
-- hash, and must not inherit the previous owner's workspace. NULL until the
-- user's next sign-in records it.
ALTER TABLE users ADD COLUMN eve_owner_hash text NULL;
