#!/usr/bin/env bash
# Sets Postgres up for monitoring (issue #14; see docs/monitoring.md):
#   - the pg_stat_statements extension, so per-query timings can be read;
#   - a read-only `iskworks_monitor` login with the built-in pg_monitor
#     role, for postgres_exporter. Never point the exporter at the app user.
#
# The compose stacks mount this into /docker-entrypoint-initdb.d/, so it
# runs once when the data volume is first created. For an existing
# database, run it by hand (it is safe to re-run; it also updates the
# monitor password):
#   docker compose exec -e POSTGRES_MONITOR_PASSWORD postgres \
#     /docker-entrypoint-initdb.d/10-monitoring.sh
#
# pg_stat_statements also needs `shared_preload_libraries=pg_stat_statements`
# in the server config, which the compose files set. Without it the
# extension installs but its view errors until Postgres restarts with it.
set -euo pipefail

psql=(psql -v ON_ERROR_STOP=1 --username "${POSTGRES_USER:-postgres}" --dbname "${POSTGRES_DB:-${POSTGRES_USER:-postgres}}")

"${psql[@]}" <<'SQL'
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;
SQL

if [[ -z "${POSTGRES_MONITOR_PASSWORD:-}" ]]; then
  echo "init-monitoring: POSTGRES_MONITOR_PASSWORD is not set; skipping the iskworks_monitor role" >&2
  exit 0
fi

# The password goes in as a psql variable, quoted by psql itself (:'...').
# pg_stat_statements stores utility statements verbatim, password included,
# and the exporter would publish that text. The compose files set
# track_utility=off server-wide; turning it off here as well keeps the
# password out even on a server configured without that.
"${psql[@]}" -v monitor_password="$POSTGRES_MONITOR_PASSWORD" <<'SQL'
SET pg_stat_statements.track_utility = off;
SELECT 'CREATE ROLE iskworks_monitor LOGIN'
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'iskworks_monitor')
\gexec
ALTER ROLE iskworks_monitor WITH LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD :'monitor_password';
GRANT pg_monitor TO iskworks_monitor;
SQL
echo "init-monitoring: pg_stat_statements and iskworks_monitor are ready"
