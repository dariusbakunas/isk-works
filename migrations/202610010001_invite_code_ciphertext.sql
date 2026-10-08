-- Let admins re-reveal an invite code after creation. The code is stored
-- encrypted (AES-256-GCM envelope JSON via TOKEN_ENCRYPTION_KEY), never in
-- plaintext; `code_hash` remains the only thing redemption compares against.
-- NULL for invites minted before this column existed or via the CLI, which
-- therefore cannot be revealed.
ALTER TABLE invite_codes ADD COLUMN code_ciphertext text NULL;
