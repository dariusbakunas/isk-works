# Monitoring

The API and the worker can each serve Prometheus metrics. Both call ESI, so
scrape both. Metrics are off unless you turn them on.

## Turning it on

Set `ISKWORKS_METRICS_ADDR` to the address the metrics listener should bind,
for example:

```
ISKWORKS_METRICS_ADDR=0.0.0.0:9100
```

Each process then serves `GET /metrics` on that port, separately from the
API's own listener (`ISKWORKS_API_ADDR`). Unset or blank, no listener starts
and recording costs nothing.

**Never expose this port publicly.** It has no authentication. The bundled
compose stack doesn't publish it, and Traefik only routes the API's own port
(8080). Scrape it from inside the Docker network or cluster.

## Dashboard and alerts

The repo ships a ready-made setup:

- `deploy/grafana/esi-dashboard.json` is a Grafana dashboard ("ISK Works / ESI") with these sections:
  - **Overview**
  - **Traffic**
  - **Error budget (420)**
  - **Rate limits (429)**
  - **Latency**
  - **Sync**
  - **Worker loops**

  It picks its Prometheus data source and service (`job`) from dashboard variables.
- `deploy/prometheus/esi-alerts.yml` holds Prometheus alert rules: any 420, a low error budget, a high share of 5xx, failing or stale syncs, and stalled or failing worker loops. Unit tests for the rules are in `esi-alerts.test.yml` (`promtool test rules`).
- `deploy/prometheus/prometheus.yml` is a scrape config. **The dashboard and alerts expect the scrape jobs to be named `iskworks-api` and `iskworks-worker`.** Keep those names if you write your own.

### With the bundled compose stack

1. In `.env`, set:

   ```
   ISKWORKS_METRICS_ADDR=0.0.0.0:9100
   GRAFANA_ADMIN_PASSWORD=<something strong>
   ```

   If you leave `GRAFANA_ADMIN_PASSWORD` blank, Grafana starts with admin/admin and asks you to change it on first login.
2. Start the `monitoring` profile:

   ```
   docker compose --profile monitoring up -d
   ```

   This recreates the API and worker with metrics on, and adds Prometheus (15 days of retention; change it with `PROMETHEUS_RETENTION`) and Grafana with the dashboard provisioned.
3. Grafana listens on `127.0.0.1:3000` on the host only (`GRAFANA_PORT` changes the port). From your machine, tunnel to it with `ssh -L 3000:127.0.0.1:3000 your-server` and open http://localhost:3000. Neither Prometheus nor Grafana is routed through Traefik.

Alerts show up in Prometheus and in Grafana's alert list. Sending them anywhere (email, Discord, …) needs an Alertmanager or Grafana contact point, which isn't bundled.

### Elsewhere (Kubernetes, existing Prometheus)

Scrape port 9100 of both services with the job names above. Load `esi-alerts.yml` as a rule file (or PrometheusRule). Import the dashboard JSON into Grafana and pick your Prometheus data source.

## Metrics

Every series is prefixed `iskworks_`. Labels never carry character,
workspace, structure or item IDs, or tokens. Per-character sync health stays
in the app's own UI.

| Metric | Type | Labels | Meaning |
|---|---|---|---|
| `iskworks_build_info` | gauge (always 1) | `service` (`api`/`worker`), `version` | Which build is running. |

### ESI requests

| Metric | Type | Labels | Meaning |
|---|---|---|---|
| `iskworks_esi_requests_total` | counter | `route`, `method`, `status_class`, `outcome` | Every ESI and EVE SSO request, including ones a local guard refused. |
| `iskworks_esi_request_duration_seconds` | histogram | `route` | Send until response headers, for requests that were actually sent. |
| `iskworks_esi_pages_fetched_total` | counter | `route` | Pages fetched (200 or 304) on paginated routes. Spikes mean pagination blowups. |
| `iskworks_esi_not_modified_total` | counter | `route` | 304 answers to `If-None-Match`. |

- `route` is ESI's path template (`/characters/{character_id}/assets/`), `/status/` for the downtime probe, or `sso:token` / `sso:revoke` / `sso:jwks` for EVE SSO.
- `status_class` is `2xx`, `3xx`, `4xx`, `5xx`, or `none` when no response came back.
- `outcome` is one of:
  - `ok`
  - `not_modified`
  - `client_error`
  - `server_error`
  - `rate_limited` (429)
  - `error_limited` (420)
  - `transport_error` (timeout, connection failure)
  - `blocked`: never sent, because the error budget was low, the route group was paused, or Tranquility was in downtime.

### ESI guards

ESI's error budget is per IP. If it runs dry, every request from the host is refused with 420. ISK Works stops sending once fewer than `iskworks_esi_error_limit_floor` errors remain.

| Metric | Type | Labels | Meaning |
|---|---|---|---|
| `iskworks_esi_error_limit_remaining` | gauge | – | Last `X-ESI-Error-Limit-Remain` ESI reported. ESI sends it only on routes not yet moved to per-group rate limits (market and status routes don't), so it can be missing until such a route is called. |
| `iskworks_esi_error_limit_reset_seconds` | gauge | – | Last `X-ESI-Error-Limit-Reset`. |
| `iskworks_esi_error_limit_floor` | gauge | – | The budget level at which ISK Works pauses itself. |
| `iskworks_esi_error_limited_total` | counter | – | 420 responses. Should be zero. |
| `iskworks_esi_error_limit_blocked_total` | counter | – | Requests refused locally to protect the budget. |
| `iskworks_esi_rate_limited_total` | counter | `group` | 429 responses per ESI rate-limit group. |
| `iskworks_esi_rate_limit_paused_total` | counter | `group`, `reason` (`retry_after`/`low_remaining`) | Times a group was paused. |
| `iskworks_esi_rate_limit_blocked_total` | counter | `group` | Requests refused locally while their group was paused. |
| `iskworks_esi_rate_limit_remaining_ratio` | gauge | `group` | Last remaining/limit ESI reported for the group (any caller). |
| `iskworks_esi_downtime_paused` | gauge (0/1) | – | 1 while requests wait out Tranquility's daily downtime. |
| `iskworks_esi_downtime_probes_total` | counter | `result` (`healthy`/`unhealthy`) | `/status/` checks made during downtime. |

`group` is ESI's `X-Ratelimit-Group` name. Before ESI has named a route's group, the label is the route's URL path with IDs replaced by `{}`.

### Sync runs and SSO

| Metric | Type | Labels | Meaning |
|---|---|---|---|
| `iskworks_esi_sync_runs_total` | counter | `kind`, `result` (`success`/`incomplete`/`failed`) | Finished sync runs. |
| `iskworks_esi_sync_duration_seconds` | histogram | `kind` | How long each run took. |
| `iskworks_esi_sync_last_success_timestamp_seconds` | gauge | `kind` | Unix time of this process's last successful run of `kind`. |
| `iskworks_esi_token_refresh_total` | counter | `result` (`success`/`reauth_required`/`failed`) | EVE SSO access-token refreshes. `reauth_required` means the character must be reconnected. |

- `kind` is a character data source: `character_info`, `location`, `skills`, `wallet`, `industry_jobs`, `assets`, `wallet_transactions` or `planets`.
- The worker records its scheduled refreshes. The API records the manual "Sync now" and asset/wallet import buttons.
- To tell the two apart, use the scrape job, e.g. `max by (kind) (...)` across both for "last success anywhere".
- A sync that can't start because its token refresh failed counts only in `token_refresh_total`.

### Worker loops

| Metric | Type | Labels | Meaning |
|---|---|---|---|
| `iskworks_worker_loop_runs_total` | counter | `loop`, `result` (`ok`/`failed`) | Finished passes. `failed` means the pass itself errored, e.g. it couldn't query the database. A pass where individual ESI fetches failed is still `ok`; those failures show in `iskworks_esi_requests_total`. |
| `iskworks_worker_loop_duration_seconds` | histogram | `loop` | Pass duration. |
| `iskworks_worker_loop_last_run_timestamp_seconds` | gauge | `loop` | Unix time the loop last finished a pass. |

`loop` is one of `market`, `adjusted_price`, `system_index`, `character_sync`, `market_gc`, `esi_gc` or `auth_gc`.

Histograms named `*_duration_seconds` share these buckets: 10 ms, 25 ms,
50 ms, 100 ms, 250 ms, 500 ms, 1 s, 2.5 s, 5 s, 10 s, 30 s, 1 min, 2 min, 5 min.
