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

## Metrics

Every series is prefixed `iskworks_`. Labels never carry character,
workspace, structure or item IDs, or tokens. Per-character sync health stays
in the app's own UI.

| Metric | Type | Labels | Meaning |
|---|---|---|---|
| `iskworks_build_info` | gauge (always 1) | `service` (`api`/`worker`), `version` | Which build is running. |

Histograms named `*_duration_seconds` share these buckets: 10 ms, 25 ms,
50 ms, 100 ms, 250 ms, 500 ms, 1 s, 2.5 s, 5 s, 10 s, 30 s, 1 min, 2 min, 5 min.
