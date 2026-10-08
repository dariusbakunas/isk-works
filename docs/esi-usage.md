# How ISK Works uses ESI

ISK Works reads your EVE data through CCP's official API (ESI) and EVE SSO. When
you self-host, those requests come from **your** server, under **your** SSO
application (`EVE_SSO_CLIENT_ID`). If the app misbehaved, CCP would block your app
or your IP, not ours. So the app is built to stay well inside
[CCP's ESI guidelines](https://developers.eveonline.com/docs/services/esi/best-practices/).
This page explains how.

## What it reads, and how often

**Public data** is fetched once per server and shared by every workspace:

| Data | Endpoint | How often |
|---|---|---|
| Regional market orders | `/markets/{region}/orders/?type_id=…` | At most every 15 min per item. Only for items something in the app uses; an item goes dormant 7 days after its last use |
| Adjusted prices | `/markets/prices/` | Every 6 h |
| Industry cost indices | `/industry/systems/` | Every 1 h |
| Names | `/universe/names/` | Once per new ID; results are cached |

**Character data** is read only for characters you connect, and only with the scopes you grant:

| Data | How often |
|---|---|
| Location | 10 min |
| Wallet balance | 15 min |
| Wallet transactions / journal, industry jobs, planets | 30 min |
| Skills, assets, blueprints | 60 min |
| Public character info | 24 h |
| Citadel markets you've added | 15 min |

None of these intervals is shorter than ESI's own cache time for that data. A
manual "sync now" is limited to once a minute per character.

## How it stays within the rules

- **It identifies itself.** Every request carries a `User-Agent` with the app
  version and the contact address you configure. CCP can then reach you rather
  than just block you.
- **It respects ESI's cache.** Background refreshes never run more often than
  ESI's cache allows. Clicking "refresh" on market data, prices or cost indices
  moves the item to the front of the queue but doesn't fetch it before its
  `Expires` time. A manual "sync now" for a character's assets or wallet is
  limited to once a minute. Market books and asset and wallet syncs send
  `If-None-Match`, so data that hasn't changed comes back as a cheap `304`.
- **It guards the error limit.** ESI allows about 100 failed requests per minute
  per IP. Once fewer than 20 remain, or ESI answers `420`, the app stops all ESI
  traffic until the window resets.
- **It sits out daily downtime.** Tranquility goes down at 11:00 UTC every day,
  usually for under 5 minutes, and every ESI request fails while it's down. From
  10:58 UTC the app stops calling ESI. From 11:00 it checks ESI's `/status/`
  route every 30 s, and resumes on the first healthy answer. Without this, a
  minute of failed requests would use up the whole error budget. The pause ends
  at 11:30 UTC at the latest.
- **It respects rate limits.** ESI limits each group of routes per character
  (or per IP for public data). The app reads ESI's rate-limit headers on every
  response. When a group is nearly used up, it pauses that group briefly, and
  after a `429` it waits out `Retry-After` for every request in the group, not
  just the one that failed. Errors that keep coming back are retried with
  exponential backoff, up to 6 h.
- **It doesn't repeat known failures.** A citadel your character can't access is
  remembered for 12 hours instead of being re-requested on every sync. IDs ESI
  can't name are skipped for a week. A character whose SSO authorization was
  revoked is paused until you reconnect it.
- **Its parallelism is small and fixed.** About ten requests at most are in flight
  per process, and each has a 30 s timeout. Adding workspaces or characters makes
  the queue longer, not the request rate higher.
- **It uses tokens sparingly.** Access tokens (valid about 20 min) are reused
  until shortly before they expire, not refreshed on every call. SSO's signing
  keys are cached for an hour. Refresh tokens are stored
  encrypted (`TOKEN_ENCRYPTION_KEY`).

## Self-hosting checklist

1. **Register your own SSO application** at
   [developers.eveonline.com](https://developers.eveonline.com/applications). Don't
   reuse someone else's client ID.
2. **Set `ISKWORKS_ESI_CONTACT`** to an email address (preferred), a Discord
   handle or an EVE character name. It's sent in the `User-Agent`, and the
   bundled compose file won't start without it.
3. **Run exactly one worker.** The bundled compose file already does. Several
   workers won't double-fetch data, but they will refresh tokens more often.
4. **Don't lower the refresh intervals** (`ISKWORKS_WORKER_*_FRESHNESS_SECONDS`)
   below the defaults. Shorter intervals don't give you fresher data, because
   ESI serves cached responses anyway. They only spend your rate limit.
5. **Check the logs** for `ESI error limit is nearly exhausted`. If you see it
   often, something is wrong; please open an issue.
