# Session-replay privacy (LogRocket)

ISK Works can run LogRocket session replay to diagnose UI and planning bugs.
It is off unless the operator turns it on, and it is configured privacy-first: recording is
useful for debugging product flows, but credentials, tokens, free-form user
text, character identity, and account financial values are excluded
**before anything leaves the browser**.

All of this lives in `web/iskworks-web/src/observability/`:

| File | Role |
| --- | --- |
| `logrocket.ts` | The one `LogRocket.init` call, enablement gate, `identify` policy, backend-correlation bridge, `X-LogRocket-URL` request tagging. |
| `logrocket-sanitizers.ts` | Pure network + URL sanitizers (unit-tested). |
| `private.tsx` | `<Private>` / `<CharacterName>` / `PRIVATE_ATTR` DOM markers. |
| `session-replay-disclosure.tsx` | Plain-language disclosure copy + component. |

## Enablement

Session replay is configured **per deployment, at container start**, never
baked into the web image. One published image serves every deployment, so
each operator who wants replay supplies their own LogRocket app ID; a
build-time ID would send every deployment's sessions to one project.

The web container's entrypoint script
(`web/iskworks-web/docker/40-iskworks-runtime-config.sh`) writes
`/config.js` from two environment variables; `index.html` loads it before the
app bundle. Nothing records unless the container sets **both**:

- `LOGROCKET_APP_ID` — the operator's own LogRocket app id (letters, digits,
  `/`, `_`, `.`, `-` only; anything else stops the container)
- `LOGROCKET_ENABLED=true` (`1` and `yes` also work)

Unset means off, so a deployment records nothing unless the operator opts in.
`deploy/docker-compose.yml` passes both through from `.env`, defaulting to
off. If `LOGROCKET_ENABLED` is on but the app ID is empty, the container logs
a warning and replay stays off.

The API needs no replay configuration. It only reads the `X-LogRocket-URL`
header the browser sends (see "Backend correlation").

For local `vite dev`, where no `/config.js` is generated, the
`VITE_LOGROCKET_APP_ID` / `VITE_LOGROCKET_ENABLED` build variables are used as
a fallback. Local dev, CI, and unit tests are silent by default.
`VITE_APP_RELEASE` (version string / git SHA, passed as a build arg
when release images are built) tags recordings; it falls back to `"dev"`.

## What replay shows / hides / never sees

### Visible by design

- Product navigation and route changes
- Structured domain data: EVE item/type names, quantities, runs, ME/TE,
  facility selection labels, sourcing mode, graph topology, warnings, cost
  completeness state
- Build-planning values including required / making / in-inventory quantities
- Board interactions: ticket kind, workflow status, Epic relationships,
  assignee presence, blocker counts, drag/drop, filters
- API request method, route, status, and timing

### Masked (`data-private` in the DOM, or `redactSelectors` fallback)

- **All text inputs** — every `<input>` / `<select>` value is obfuscated by
  `dom.inputSanitizer: true`, and every `<textarea>` by
  `redactSelectors: ["textarea"]`. This is automatic; you do not need to
  remember to mark new forms.
- Free-form rendered text: generic Ticket titles, inventory event notes,
  blueprint assumption notes, price-source names & descriptions, saved
  finance-filter names, facility names, user-entered Build names.
- Character identity: EVE character names (`<CharacterName>`) and portraits
  (`EveCharacterPortrait` is `data-private`; alt/aria text omits the name).
- Finance: the transaction summary strip, the transaction table body, and
  per-character wallet balances.
- Assets inspector: the character field.

### Never captured

- Credentials, passwords, secrets
- Session identifiers / cookies (`Cookie`, `Set-Cookie`)
- `Authorization` / any token- or auth-bearing header
- OAuth `state` / `code` / PKCE values (query string **and** bodies)
- EVE access / refresh tokens, `TOKEN_ENCRYPTION_KEY` (never reaches the
  frontend anyway)
- Invite codes — `invite` / `inviteCode` / `code` query params and body keys
  are redacted, and `/api/auth/*` and `/api/admin/*` bodies are dropped

## Network capture rules (`logrocket-sanitizers.ts`)

Metadata is always kept (method, route, status, timing) so failed and slow
requests still show up. On top of that:

- **Headers** (all requests/responses): `authorization`,
  `proxy-authorization`, `cookie`, `set-cookie`, `x-csrf-token`, `x-xsrf-token`, `x-auth-token`, `x-api-key`,
  and anything matching `token|secret|password|api-key|auth` → `[redacted]`.
  `x-logrocket-url` (the app's own backend-correlation tag) is kept.
- **URLs** (network + page URL via `browser.urlSanitizer`): the query params
  `code, state, token, access_token, refresh_token, id_token, session,
  session_token, secret, sig, signature, code_verifier, code_challenge,
  invite, invite_code, invitecode`
  → `[redacted]`.
- **Auth / OAuth / admin routes** (`/api/auth/*`, `/api/admin/*`, `/api/eve/oauth/*`,
  `/api/eve/connections/authorize`, `/api/eve/connections/:id/refresh`):
  request **and** response body dropped entirely.
- **Financial routes** (`/api/finance/*`, `*/wallet-transactions`,
  `/api/eve/sync-runs*`, `/api/assets/summary`): response body dropped.
- **Bulk export routes** (any `.../export` — assets, inventory, facilities,
  transactions): response body dropped. These are CSV / full-snapshot dumps
  that would also bypass the JSON walker.
- **File-upload routes / non-JSON form bodies** (`/api/industry/market-imports`,
  or any request whose `content-type` is `multipart/form-data` /
  `application/x-www-form-urlencoded`): request body dropped. The JSON
  walker cannot reach into these.
- **Every other route**: JSON bodies are deep key-redacted.
  - *Always* (request + response, any depth): credential / token / OAuth /
    PKCE / invite keys, plus `notes`, `note`, `reason`, `sourceReference`,
    `description`.
  - *Request bodies only*: `name`, `title`, `capturedName`, `displayName` —
    a write body is by definition what the user just typed.
  - *Response bodies only*: `walletBalance`, `availableBalance`, `income`,
    `expenses`, `netIsk`, `averageDailyIsk`, `counterparty`,
    `counterpartyName`, **`characterName`**, `accessCharacterName`.
  - *Per-object (contextual), request + response*: `capturedName` when the
    object is a **generic** Ticket (a structured ticket's `capturedName`
    e.g. `"Tungsten Carbide"` is kept); `name` / `parentBuildName` on a
    **Build**; `name` on a **FacilityProfile** (identified by
    `materialReductionPercent` + `rigs`), a **PriceSource** (`itemCount` +
    `description`/`items`), a **SavedFinanceFilter** (`filter`), or a
    facility-import result row. Structured objects that merely have a
    `name` — EVE market-category nodes, rig target filters, SDE search
    hits — are matched by none of these shapes and stay visible.

Why contextual rather than a blanket `name` redaction on responses: a
canonical EVE name is frequently echoed back under a bare `name` key
(`/api/market/categories`, rig `industryTargetFilters`), and blanking those
would gut debugging of search / catalog / rig flows.

## identify()

`LogRocket.identify(workspaceId, { alphaUser: true })` — the workspace UUID
is opaque and not user-facing PII, and `alphaUser` is a fixed flag, the same
for everyone. No email, real name, or EVE character
identity is sent as an identity trait.

## Backend correlation

Two directions, no shared storage:

1. `X-LogRocket-URL` header on same-origin `/api/*` requests → the API lifts
   it into the request's tracing span (`apps/iskworks-api/src/observability.rs`),
   so backend log lines sit next to the replay.
2. When an API response carries a `correlationId` (redacted internal
   failure), `reportBackendError()` fires a `backend_error` LogRocket event
   with that id — so the replay timeline points back at the backend log.

## Console

Console capture stays on. There is currently no `console.*` usage in
application code; the rule is to fix an unsafe log, not disable capture.

## How to mark a new region private

- **A form field?** Nothing to do — inputs/selects/textareas are covered
  globally.
- **Rendered free-form user text, a character name, or a financial figure?**
  Wrap it: `<Private>{value}</Private>`, `<Private as="p" className="…">…</Private>`,
  or `<CharacterName name={x} />`. For a component that renders a bare
  string prop, add `data-private={cond ? "" : undefined}` on the element,
  or spread `{...PRIVATE_ATTR}`.
- **A whole structural region that's awkward to thread a prop into?** Add a
  stable selector to `dom.redactSelectors` in `logrocket.ts`.
- **The value also comes back in an API response?** DOM masking is not
  enough. If it lands under an *always*-redacted key (`notes`, `reason`,
  …) you're done. If it's a bare `name` / `title` / `capturedName` on a
  new user-authored entity, add a shape rule to `contextualUserLabelKeys`
  in `logrocket-sanitizers.ts` (key off a distinctive sibling key so
  structured domain objects are untouched) and a round-trip test.
- **A new route that returns a CSV / bulk dump or accepts a file/form
  body?** Confirm it's caught by `isBulkExportRoute` / `isFileUploadRoute` /
  the `content-type` check; add the route if not.
- Do **not** put a sensitive value in a `title` / `aria-label` / `alt`
  attribute on an otherwise-masked element — attributes are still captured.
  Use a generic label instead.

## Disclosure / opt-in status

`SessionReplayDisclosure` renders a plain-language notice on the sign-in
screen (only when replay is enabled). There is **no per-user opt-in toggle**:
when the operator enables replay, every session is recorded, including the sign-in screen. A real
opt-in would mean delaying LogRocket init and persisting a preference. This
doc is not a legal privacy policy and makes no consent claims.
