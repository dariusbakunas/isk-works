# ESI Foundation and Accounting Compatibility

## Scope

The ESI foundation covers hosted EVE SSO, character connections, asset and wallet transaction synchronization, observation history, and explicit wallet-purchase recording. Synchronization can be started manually (`POST /api/eve/connections/:connection_id/sync/assets`, `.../sync/wallet-transactions`) and also runs in the background: `apps/iskworks-worker` refreshes due character sources, including assets and wallet transactions.

The boundary is:

```text
EVE ESI -> external observations -> review decisions -> Inventory Events -> projections
```

ESI never writes inventory balances directly. A wallet buy changes accounting only after a user previews and records it from Finance (`/api/finance/transactions/:observation_id/inventory-recording`).

## Official Contract

The SSO, asset, and wallet foundation uses:

- Authorization: `https://login.eveonline.com/v2/oauth/authorize`
- Token and refresh: `https://login.eveonline.com/v2/oauth/token`
- JWKS: `https://login.eveonline.com/oauth/jwks`
- Issuer: `https://login.eveonline.com`
- Assets: `GET /latest/characters/{character_id}/assets/`
- Wallet market transactions: `GET /latest/characters/{character_id}/wallet/transactions/`
- Accessible structure identity: `GET /latest/universe/structures/{structure_id}/`
- Asset scope: `esi-assets.read_assets.v1`
- Wallet scope: `esi-wallet.read_character_wallet.v1`
- Accessible structure identity scope: `esi-universe.read_structures.v1`

Authorization uses a 32-byte random, base64url PKCE verifier, S256 challenge, and a separate 32-byte random state. Pending authorization is hashed by state, expires after ten minutes, is single-use, and has a fixed return route.

JWT validation requires an RS256 signature from the current JWKS, the configured issuer, both the application client ID and `EVE Online` audiences, expiration validity, `CHARACTER:EVE:<id>` subject, character name, and the required granted scopes.

Assets use `page`, `X-Pages`, ETag/cache metadata, and a maximum of 1,000 records per page. Every reported page must succeed with a stable page count before a snapshot becomes active.

Wallet transactions use stable `transaction_id` values, up to 2,500 records per response, and `from_id` for older records. The endpoint is a limited history window and is not treated as all-time acquisition history. Unit prices are parsed from the lexical JSON number into `Decimal`; binary floating point is not used.

The transport recognizes authorization errors, missing scopes, 420/429 limiting, server failures, `Retry-After`, `X-ESI-Error-Limit-Remain`, `X-ESI-Error-Limit-Reset`, `ETag`, `Expires`, `Last-Modified`, and `X-Pages`. Manual requests use bounded endpoint loops and no unbounded retries.

Official references:

- [EVE SSO](https://developers.eveonline.com/docs/services/sso/)
- [ESI pagination](https://developers.eveonline.com/docs/services/esi/pagination/x-pages/)
- [ESI best practices](https://developers.eveonline.com/docs/services/esi/best-practices/)
- [ESI rate limiting](https://developers.eveonline.com/docs/services/esi/rate-limiting/)

## Storage and Invariants

Migration `202607250006_create_esi_observations.sql` adds:

- `eve_connections`
- `eve_connection_tokens`
- `eve_oauth_pending_authorizations`
- `esi_sync_runs`
- `esi_sync_checkpoints`
- `esi_asset_snapshots`
- `esi_asset_observations`
- `esi_wallet_transactions`
- `inventory_event_sources`

Refresh tokens, access tokens, and pending PKCE verifiers are encrypted before storage with AES-256-GCM, a random nonce, authenticated ciphertext, and a versioned JSON envelope. A stored access token is reused until shortly before it expires. Production encryption-key custody and rotation remain deployment concerns.

Token writes use an optimistic token revision. A rotated refresh token is written only when the caller owns the expected revision; a losing refresh reloads current state and retries. Disconnect deletes usable token material while retaining connections, observations, sync runs, recorded Inventory Events, and provenance.

Asset snapshots start as `collecting`. Only a completely fetched and persisted snapshot is marked `complete` and atomically replaces the prior active snapshot. Incomplete runs are never active and never advance the checkpoint.

Only the active snapshot is ever read, so completing a sync deletes the snapshot it replaces and the connection's earlier failed attempts (observations and hierarchy cascade). The worker's hourly `esi_gc` sweep (`ISKWORKS_WORKER_ESI_GC_POLL_SECONDS`) is the backstop: it deletes every snapshot that is neither its connection's active one nor its newest.

Wallet observation identity is `(connection_id, source_transaction_id)`. Each wallet-purchase recording is an `inventory_event_sources` row tied to the wallet observation. At most one active recording of an accounting effect may exist per observation (a partial unique index); a reverted recording keeps its `reverted_at` and `reversal_event_id` as history, and the transaction can be recorded again.

## Configuration

Real ESI is disabled when `EVE_SSO_CLIENT_ID` is absent. Required real-mode values are:

```text
EVE_SSO_CLIENT_ID
EVE_SSO_REDIRECT_URI
TOKEN_ENCRYPTION_KEY
WEB_APP_URL
```

`TOKEN_ENCRYPTION_KEY` is standard base64 for exactly 32 random bytes. Official protocol URLs have defaults and can be overridden with `EVE_SSO_AUTHORIZATION_URL`, `EVE_SSO_TOKEN_URL`, `EVE_SSO_JWKS_URL`, `EVE_SSO_ISSUER`, and `EVE_ESI_BASE_URL`.

`ISKWORKS_ESI_MOCK=1` explicitly enables deterministic local fixture mode. It is disabled by default and bypasses external authorization only to exercise the same encrypted-token, refresh, synchronization, storage, reconciliation, proposal, and accounting paths.

## Verification

Automated verification covers PKCE and state generation, URL construction, encryption round trips and tamper detection, token redaction, and lexical decimal parsing.
