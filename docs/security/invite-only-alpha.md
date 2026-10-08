# Invite-only sign-up

Optionally gates **new user / new workspace provisioning** behind an invite
code. Existing users keep signing in normally, with no invite.

## What it does and does not do

- **Does**: when enabled, a genuinely new EVE identity cannot create a
  user/workspace without redeeming a valid invite code.
- **Does not**: touch existing users, add referrals / waitlists / invite
  emails / approval queues, or change anything about how
  authentication itself works. Invite authorization *supplements*
  authentication (`ISKWORKS_AUTH_REQUIRED`); it does not replace it.

## Enabling it

Set on the API process (and only the API — the worker does not serve login):

```
ISKWORKS_INVITE_REQUIRED=true
```

Accepted truthy spellings: `true`, `1`, `yes` (case- and
whitespace-insensitive). Anything else, including unset, is **off**
(open registration — the local-dev / test default).

> Invite mode is driven **only** by this flag — it is never inferred from
> whether `invite_codes` rows happen to exist. With the flag off, anyone who
> can complete EVE SSO gets a new workspace.

`deploy/docker-compose.yml` sets it for the `iskworks-api` service
(`ISKWORKS_INVITE_REQUIRED: ${ISKWORKS_INVITE_REQUIRED:-true}` — defaults on,
overridable from `.env`).

## Turning it off

Set `ISKWORKS_INVITE_REQUIRED=false` and restart the API. New users
immediately onboard without an invite again. Existing invite rows are left
alone and simply stop mattering; turning the flag back on makes them
redeemable again.

## Managing invites

Invites can be managed two ways, and both write the same `invite_codes`
table.

**In the app.** Characters listed in `ISKWORKS_ADMIN_CHARACTER_IDS`
(comma-separated EVE character ids) see an **Admin** link that opens the
invite manager: create (max uses, optional expiry, note), copy or reveal the
code, list, **disable** (stops the code; the row and its history stay) or
**delete** (permanent, with confirmation). The server enforces admin access on
every `/api/admin/*` route (403 otherwise, including when auth is off).

**Operator CLI** (`apps/iskworks-admin`). This is the bootstrap path: in
invite mode even a character listed in `ISKWORKS_ADMIN_CHARACTER_IDS` is a
new identity on first sign-in and needs an invite like anyone else, so the
first code has to come from the CLI. It is also the break-glass path when no
admin can sign in. Run it wherever `DATABASE_URL` reaches the ISK Works
database:

```bash
# one-use code (the default)
cargo run -p iskworks-admin -- invite create --note "for @somebody"

# a 5-use code that stops working after a date
cargo run -p iskworks-admin -- invite create --max-uses 5 --expires-at 2026-10-01T00:00:00Z

# see what exists (never prints a code — only the hash is stored)
cargo run -p iskworks-admin -- invite list

# stop a code from being redeemed (row kept for audit; idempotent)
cargo run -p iskworks-admin -- invite disable <uuid>
```

`make invite ARGS="list"` / `make invite-create ARGS="--max-uses 5"` wrap the
same commands.

In the deployed stack, run it inside the API image, which already has the
binary and `DATABASE_URL`:

```bash
docker compose run --rm --entrypoint iskworks-admin iskworks-api invite create --note "first admin"
```

### Revealing a code later (admin UI)

Invites created in the admin UI also store an **AES-256-GCM encrypted copy**
of the code (`invite_codes.code_ciphertext`, same `TOKEN_ENCRYPTION_KEY` and
envelope as ESI tokens) so an admin can click **Reveal** while the invite is
active. Redemption still compares only `code_hash`. Trade-off: a database
leak *plus* the encryption key exposes unused invite codes, so this is not
hash-only any more for UI-created invites. CLI-created invites and invites
that predate this column are hash-only and cannot be revealed.
`GET /api/admin/invites/:id/code` is admin-gated and `Cache-Control: no-store`.

### CLI: the plaintext code is shown exactly once

`invite create` prints the code (`ISK-XXXX-XXXX-XXXX-XXXX`) once. Only its
SHA-256 hash is written to the database — the code cannot be recovered
afterwards, and `invite list` never shows it. If you lose it, disable that
row and mint a new one.

## Code format

`ISK-XXXX-XXXX-XXXX-XXXX` — a fixed `ISK` prefix plus 16 symbols from a
Crockford base32 alphabet (`0-9 A-Z` minus the ambiguous `I L O U`), drawn
from a CSPRNG. 80 bits of entropy. The code encodes nothing — no user id,
workspace id, or timestamp.

Input is normalized before hashing: trimmed, upper-cased, dashes/whitespace
and the `ISK` prefix removed, and Crockford input leniency applied
(`O`→`0`, `I`/`L`→`1`). So `isk-7k3m…`, `7K3M …`, and the exact code all
resolve to the same row.

## Sign-in UX

The sign-in screen always offers **Continue with EVE Online** (returning
users and, when invite mode is off, everyone). When invite mode is on it
also shows a **Have an invite?** link, which reveals a single invite field
and a join button. Both paths run the *same*
OAuth flow — `POST /api/auth/eve/login` just carries an optional
`inviteCode` in its JSON body.

- **Returning user**: clicks Continue, no invite needed. Even if an invite
  is somehow attached to the pending-auth row, it is **not** consumed —
  their identity already exists.
- **New user with an invite**: enters it first; it is validated before the
  EVE redirect, so a bad code fails fast without burning an OAuth round
  trip.
- **New user who clicks Continue without an invite**: completes EVE SSO,
  then the callback bounces back to `/login?status=invite_required` and the
  invite field opens with an explanation. No session, no workspace created.

## How consumption works (atomicity)

- Validation at **login start** is a UX check only — it does not reserve the
  invite.
- Consumption at **provisioning** is authoritative and atomic. Inside the
  same transaction that creates the workspace and user:

  ```sql
  UPDATE invite_codes
     SET use_count = use_count + 1
   WHERE id = $1
     AND disabled_at IS NULL
     AND (expires_at IS NULL OR expires_at > now())
     AND use_count < max_uses
  RETURNING id;
  ```

  Zero rows returned ⇒ the whole provisioning rolls back
  (`invite_invalid`), nothing is created. Two concurrent redemptions of a
  `max_uses = 1` code serialize on the row lock; exactly one wins.

- An invite that **expires or is disabled while the user is at EVE** is
  caught here: the callback fails cleanly, no workspace, no consumption.
- A **replayed callback** cannot double-consume: the pending-auth row is
  single-use (10-minute TTL, PKCE, one-time `state`), so there is no second
  provisioning.

## Multi-use codes

`max_uses` can be 1 to 1000 (both the CLI and the admin UI validate this).
The default is `1` (individual invites). A larger value gives a shared code,
e.g. one corp code with 5 uses. There is no per-redemption attribution
table; only the atomic counter is kept.

## What is exposed to the frontend

`GET /api/auth/session` returns `inviteRequired: bool` alongside its other
fields, including to signed-out callers. That is the only invite-related
public surface: a single boolean, no counts, ids, hashes, or config paths.

## Privacy

- Raw invite codes live only transiently: the sign-in input, the HTTPS
  `POST` body, and server memory while hashing. In the database only as a
  hash, plus an encrypted copy for admin-UI-created invites (see "Revealing
  a code later"). Never in plaintext in the database, a log line, a tracing field, the OAuth `state`, a redirect URL, the session
  cookie, browser storage, or LogRocket.
- The LogRocket sanitizer drops `/api/auth/*` request and response bodies
  wholesale (and `/api/admin/*`, which carries invite codes), and also redacts `invite` / `inviteCode` keys and query params
  anywhere as defence in depth. The sign-in invite input carries
  `data-private`.
- When an operator enables session replay, it is **disclosure-only**: there
  is no per-user opt-in (LogRocket initializes before app render, so a real
  opt-in would mean delaying init and persisting a preference). See
  [session-replay-privacy.md](session-replay-privacy.md).

## Legacy workspace claim

A brand-new production database has zero workspaces, so a first invited user
provisions a fresh one. On a database that predates multi-tenancy and still
has one unclaimed workspace, an invited new user claims it *after* the
invite is consumed — the invite is still required. Invite mode never says
"an unclaimed workspace exists, therefore no invite needed."
