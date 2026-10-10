# Self-hosted deployment

Runs ISK Works on a single server behind its own Traefik reverse proxy, with
automatic Let's Encrypt certificates via Cloudflare DNS-01.

Services: `traefik` (edge/TLS), `postgres`, `iskworks-api`, `iskworks-worker`, `iskworks-web`.
Everything sits on one domain — Traefik routes `/api/*` to `iskworks-api`
and everything else to `iskworks-web` (an nginx container serving the built
SPA) — so there's no CORS to configure.

DNS-01 challenges prove domain ownership via a TXT record, not by Let's
Encrypt reaching your server over HTTP. That means `ISK_DOMAIN` does **not**
need to be publicly resolvable to issue certificates — it just needs to be a
real domain/subdomain whose DNS is on Cloudflare. If this server isn't
publicly reachable, point `ISK_DOMAIN` at its private/internal IP (or add a
`/etc/hosts` entry on machines that need to reach it) and issuance will
still work.

## Prerequisites

- Docker and the Docker Compose plugin on the target server.
- A domain (or subdomain) whose DNS is managed by Cloudflare.
- The published images: `dariusbakunas/iskworks-api`, `dariusbakunas/iskworks-worker`,
  `dariusbakunas/iskworks-web`, and `dariusbakunas/iskworks-sde-import` on Docker Hub. Each release
  is tagged `X.Y.Z`, `X.Y`, and `latest`.

## One-time setup

### 1. Cloudflare API token

Create a token at <https://dash.cloudflare.com/profile/api-tokens> →
**Create Token** → **Edit zone DNS** template, scoped to only the zone that
owns `ISK_DOMAIN`. Copy it for `CF_DNS_API_TOKEN` below — Traefik uses it to
create/remove the ACME TXT record automatically, nothing else.

### 2. DNS record

Add an A (or AAAA) record for `ISK_DOMAIN` pointing at this server, proxy
status doesn't matter for DNS-01. If the server is only reachable
internally, point it at the internal IP — see the DNS-01 note above.

### 3. EVE SSO application

Register an app at <https://developers.eveonline.com> with the callback URL:

```
https://<ISK_DOMAIN>/api/eve/oauth/callback
```

Login and character-linking are deliberately separate OAuth flows at the
domain level, but share this one callback URL and app registration — EVE's developer
portal only allows a single callback URL per app, so the callback route
tells the two flows apart itself (by which pending-authorization table the
OAuth `state` belongs to) rather than by URL.

Copy the generated **Client ID** for `EVE_SSO_CLIENT_ID` below. No client
secret is needed — the API uses EVE SSO's public-client flow.

### 4. Token encryption key

The API encrypts stored EVE SSO tokens at rest. Generate a key:

```bash
openssl rand -base64 32
```

### 5. Configure `.env`

```bash
cd deploy
cp .env.example .env
```

Fill in `.env` with: `ISK_DOMAIN`, `ACME_EMAIL`, `CF_DNS_API_TOKEN`,
`POSTGRES_PASSWORD` (pick anything strong — it's internal-only), `EVE_SSO_CLIENT_ID`,
and `TOKEN_ENCRYPTION_KEY` from the steps above, plus `ISKWORKS_ESI_CONTACT`.
`.env` is gitignored; never commit it.

`ISKWORKS_ESI_CONTACT` is how CCP can reach you about this instance's ESI
traffic: an email address (preferred), a Discord handle, or an EVE character
name. It is sent in the `User-Agent` of every ESI and SSO request, as
[CCP's ESI guidelines](https://developers.eveonline.com/docs/services/esi/best-practices/)
ask, so a problem gets you a message instead of a block. See
[`docs/esi-usage.md`](../docs/esi-usage.md) for how the app uses ESI.

## Stage / non-production marker

On a stage (or any non-production) instance, set
`ISKWORKS_ENVIRONMENT_LABEL=STAGE` in `.env`. The API reports it on the
public `/api/health` endpoint and the web app renders a hazard-striped
banner, frames the viewport, and prefixes the tab title with `[STAGE]` —
including on the sign-in screen. It's read at runtime, so no image rebuild
is needed; just `docker compose up -d` to recreate the API container. Leave
it blank on production.

## Session replay (optional)

The web image records nothing by default. To debug your own instance with
[LogRocket](https://logrocket.com) session replay, set your own app ID in
`.env` and recreate the web container (`docker compose up -d iskworks-web`):

```
LOGROCKET_APP_ID=your-org/your-app
LOGROCKET_ENABLED=true
```

It's read at container start, so no rebuild is needed. If you turn it on,
tell your users: the About & Legal page mentions it automatically, but
your own privacy notice is your responsibility. See
`docs/security/session-replay-privacy.md` for what is masked.

## Metrics (optional)

The API and worker can serve Prometheus metrics on a separate, internal
port. Set `ISKWORKS_METRICS_ADDR` (e.g. `0.0.0.0:9100`) for both. Never
publish that port. See `docs/monitoring.md` for what's exported.

## Community links

The footer, About & Legal page, and Help show your instance's community
channel and an ISK donation note only when you configure them. Set any of
these in `.env` and recreate the web container
(`docker compose up -d iskworks-web`):

```
ISKWORKS_SUPPORT_URL=https://discord.gg/your-invite
ISKWORKS_SUPPORT_LABEL=Discord
ISKWORKS_DONATION_CHARACTER=Your Character Name
ISKWORKS_SOURCE_URL=https://github.com/you/your-fork
```

Without `ISKWORKS_SUPPORT_URL`, users are told to contact the operator of
the instance. The "Source code" link defaults to the upstream repository.
ISK Works is licensed under the GNU AGPL v3, so if you run a **modified**
version you must point `ISKWORKS_SOURCE_URL` at its source.

## Admin section

Set `ISKWORKS_ADMIN_CHARACTER_IDS` in `.env` to a comma-separated list of
EVE **character ids** (not workspace ids). Those characters get an Admin
link in the web app, where invites can be created, listed and disabled.
Leave it empty for no admins; `iskworks-admin invite ...` (see
`docs/security/invite-only-alpha.md`) still works as the bootstrap path. A
malformed value stops the API from starting.

On the Users tab an admin can **disable** a user (reversible: ends their
sessions and refuses EVE sign-in until re-enabled) or **delete** them
(type-the-name confirmation). Delete **permanently erases the account and
everything in their workspace** (inventory and history, builds, orders,
market and finance data, characters and ESI tokens) in one all-or-nothing
transaction; the character can later register again as a brand-new user.
Workspaces left behind by accounts deleted before full erase existed show up
as a "leftover workspaces" banner on the Users tab with an **Erase leftover
data** action. You cannot act on your own account or on another configured admin —
remove the character from `ISKWORKS_ADMIN_CHARACTER_IDS` first.

## Authentication is mandatory

`docker-compose.yml` pins `ISKWORKS_AUTH_REQUIRED=true` on `iskworks-api`
(hard-coded, not read from `.env`). With that set, the API **refuses to
start** — non-zero exit, a clear config error in the logs — if
`EVE_SSO_CLIENT_ID` or `TOKEN_ENCRYPTION_KEY` is missing or blank, instead
of falling back to the legacy mode that serves one shared workspace with no
login. Every product and reference/search API endpoint requires a valid
session; the only anonymous routes are `/api/health`, `/api/ready`, login initiation, the
session probe, logout, and the EVE OAuth callback.

With auth configured, the API also refuses (403) any state-changing request a
browser sent from an origin other than `WEB_APP_URL`, so `WEB_APP_URL` must be
the exact origin users load the app from. That covers what `SameSite=Lax`
cookies don't: a page on a sibling subdomain (e.g. a stage instance) is
same-site with the app, so its requests would otherwise carry the app's
session cookie.

The unauthenticated legacy mode (`ISKWORKS_AUTH_REQUIRED` unset/false, no
`EVE_SSO_CLIENT_ID`) exists only for local development and the test suite —
never for a public instance.

## Deploy

```bash
cd deploy
docker compose up -d
docker compose logs -f traefik   # watch certificate issuance
docker compose logs -f iskworks-api
docker compose logs -f iskworks-worker
```

The API runs its own database migrations on startup — no manual migration
step. First boot can take up to ~30s for Traefik to obtain the certificate;
until then requests to `https://<ISK_DOMAIN>` may show a TLS warning.

Verify:

```bash
curl https://<ISK_DOMAIN>/api/health
```

should return `{"status":"healthy","version":"vX.Y.Z (sha)"}`.

`/api/health` is liveness only — it never touches the database.
`/api/ready` additionally runs `SELECT 1` against Postgres (2s timeout) and
returns `200 {"status":"ready"}` or `503 {"status":"unavailable"}`; use it
for a Kubernetes `readinessProbe` and keep `/api/health` for the
`livenessProbe`, so a DB outage pulls pods from the Service without
restarting them.

### First login after upgrading an existing instance

If this server already had ISK Works running before EVE SSO login shipped
(i.e. it has one existing workspace with your Builds/Finance/Plans history),
that workspace stays **unclaimed** — with no owner — until someone actually
logs in through EVE SSO. The first successful login claims it, keeping all
its existing data intact; anyone who logs in after that gets a brand new,
empty workspace of their own.

**Log in yourself immediately after this upgrade, before sharing the URL
with anyone else** — there's no way to choose who claims it after the fact
short of editing the database directly. A fresh install (no pre-existing
workspace) doesn't need this: every login just provisions its own new
workspace from the start, no claiming involved.

## SDE imports

`iskworks-sde-import` is a one-shot CLI, not a service — it's defined in
`docker-compose.yml` under the `tools` profile, so `docker compose up -d`
never starts it. Run it on demand whenever CCP publishes a new SDE:

1. Download the **JSONL** static data export (not the YAML one) from
   <https://developers.eveonline.com/static-data> and get it onto the
   server (e.g. `scp`), or download it directly there with `curl`/a browser.
2. From `deploy/`, with `.env` already configured:
   ```bash
   SDE_ZIP_PATH=/path/to/eve-online-static-data-<version>-jsonl.zip \
     docker compose --profile tools run --rm iskworks-sde-import --path /data/sde.zip
   ```
   This reuses the same `DATABASE_URL` and network as the running stack, so
   it imports straight into the deployed Postgres. It prints the activated
   SDE version and counts on success, and safely no-ops if that version is
   already active — safe to re-run.

   After applying `202608210001_add_sde_classification_metadata.sql`, run
   the importer once with a complete JSONL SDE. Existing active data remains
   usable for legacy reads, but classification-aware candidate discovery is
   unavailable until categories, groups, meta groups, and market groups have
   been imported and activated.

   If you're upgrading to a release that knows how to parse a new data
   category (e.g. reaction formulas) but have no newer SDE zip to download
   yet, re-running against the *same* zip will still no-op by default
   because the checksum matches. Add `--force` to make it reimport and
   activate a fresh dataset regardless:
   ```bash
   SDE_ZIP_PATH=/path/to/eve-online-static-data-<version>-jsonl.zip \
     docker compose --profile tools run --rm iskworks-sde-import --path /data/sde.zip --force
   ```

The future profitability scanner can use a bounded read pattern provided by
this foundation: three SDE statements for candidate headers/materials/products,
one facility lookup, batch market-order-book reads, and one adjusted-price
read. Candidate arithmetic then runs in memory through the same transient
calculator used by normal Build previews.

## Updating

Pin `ISKWORKS_API_TAG`/`ISKWORKS_WORKER_TAG`/`ISKWORKS_WEB_TAG`/`ISKWORKS_SDE_IMPORT_TAG` in `.env`
to a specific release (recommended) rather than tracking `latest`, so
upgrades are deliberate:

```bash
# after editing .env with new tags
docker compose pull
docker compose up -d
```

Deployment remains manual: publishing an image does not pull it or restart any
production container.

The evidence worker polls PostgreSQL every 5 seconds for newly due or manually
prioritized market work, independently of the 15-minute market freshness target.
An empty poll performs no ESI request. Adjusted-price and system-index loops have
their own configurable schedules; stale successful evidence remains usable.

## Backups

Postgres data lives in the `iskworks_pg_data` named volume. Example ad hoc
dump:

```bash
docker exec iskworks-postgres pg_dump -U iskworks iskworks > backup.sql
```

## Troubleshooting

- **Certificate not issued**: check `docker compose logs traefik` for ACME
  errors — almost always a `CF_DNS_API_TOKEN` scope/zone mismatch.
- **`/api/*` returns the web app's `index.html` instead of JSON**: the
  `iskworks-api` router's `priority=10` label should always win over
  `iskworks-web`'s `priority=1` for the same host — confirm both containers
  are labeled and on the `iskworks_edge` network (`docker compose config`
  to inspect resolved labels).
- **EVE SSO login fails**: the callback URL registered on the EVE developer
  portal must exactly match `https://<ISK_DOMAIN>/api/eve/oauth/callback` —
  both login and character-linking (Settings → EVE Characters) use this same
  URL. A mismatch fails at EVE's side, before it reaches this app at all.
- **Traefik logs `client version 1.24 is too old. Minimum supported API
  version is 1.4x`**: Docker Engine 29+ raised its minimum supported API
  version, which older Traefik releases (pre-v3.6) hardcode an ancient
  client version against — see
  [traefik/traefik#12253](https://github.com/traefik/traefik/issues/12253).
  Fixed by staying on `traefik:v3.6+` (this compose file already pins
  `v3.7.10`); there's no env var workaround needed. If you still see this,
  the `traefik` image tag was probably rolled back below v3.6.
- **Certificate issuance times out with `authoritative nameservers ... did
  not return the expected TXT record`, but querying the same record from
  elsewhere (e.g. `dig TXT _acme-challenge.<domain> @<ns from the error>`)
  returns it fine**: the record really was created — the server just
  can't reach Cloudflare's authoritative nameservers directly over raw DNS
  (port 53), which is how lego verifies propagation by default. Common
  behind a router/firewall with DNS filtering (Pi-hole, ISP DNS hijacking,
  etc.). Already worked around via `dnschallenge.resolvers`, which routes
  the check through `1.1.1.1`/`8.8.8.8` instead. If those are also blocked
  outbound, try your own upstream resolver's IP instead.
