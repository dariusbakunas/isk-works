#!/usr/bin/env bash
# Checks the shipped monitoring files (deploy/prometheus, deploy/grafana):
# the Prometheus config and alert rules are valid, the alert unit tests
# pass, and the dashboard is valid JSON. Uses `promtool` from PATH, or the
# Prometheus image the compose stack pins.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
prometheus_dir="$repo_root/deploy/prometheus"
image="$(sed -n 's/^ *image: \(prom\/prometheus:.*\)$/\1/p' "$repo_root/deploy/docker-compose.yml")"

promtool() {
  if command -v promtool >/dev/null 2>&1; then
    (cd "$prometheus_dir" && command promtool "$@")
  else
    docker run --rm -v "$prometheus_dir:/p:ro" -w /p --entrypoint promtool "$image" "$@"
  fi
}

promtool check config prometheus.yml
promtool test rules esi-alerts.test.yml
python3 -m json.tool "$repo_root/deploy/grafana/esi-dashboard.json" >/dev/null
echo "monitoring config OK"
