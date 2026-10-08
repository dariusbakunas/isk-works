#!/bin/sh
# Writes the SPA's runtime config (/config.js) from container environment
# variables. Run by the nginx image's entrypoint before nginx starts, so one
# published image serves every deployment: session replay stays off unless
# the operator sets both LOGROCKET_APP_ID and LOGROCKET_ENABLED=true, and the
# community link / donation note only appear when configured.
set -eu

target="${ISKWORKS_RUNTIME_CONFIG_PATH:-/usr/share/nginx/html/config.js}"
app_id="${LOGROCKET_APP_ID:-}"
support_url="${ISKWORKS_SUPPORT_URL:-}"
support_label="${ISKWORKS_SUPPORT_LABEL:-}"
donation_character="${ISKWORKS_DONATION_CHARACTER:-}"
source_url="${ISKWORKS_SOURCE_URL:-}"

case "${LOGROCKET_ENABLED:-}" in
  true | TRUE | True | 1 | yes) enabled=true ;;
  *) enabled=false ;;
esac

# Every value is written into a JavaScript string, so each is checked against
# a conservative character set instead of being escaped.

# LogRocket app IDs look like "org/app".
case "$app_id" in
  *[!A-Za-z0-9/_.-]*)
    echo "LOGROCKET_APP_ID may only contain letters, digits, '/', '_', '.', and '-'" >&2
    exit 1
    ;;
esac

require_url() {
  name="$1"
  value="$2"
  [ -z "$value" ] && return 0
  case "$value" in
    http://* | https://*) ;;
    *)
      echo "$name must start with http:// or https://" >&2
      exit 1
      ;;
  esac
  case "$value" in
    *[!A-Za-z0-9:/?#\&=._~%+@-]*)
      echo "$name contains characters outside A-Z a-z 0-9 : / ? # & = . _ ~ % + @ -" >&2
      exit 1
      ;;
  esac
}

require_text() {
  name="$1"
  value="$2"
  case "$value" in
    *[!A-Za-z0-9\ ._\'-]*)
      echo "$name may only contain letters, digits, spaces, '.', '_', ''', and '-'" >&2
      exit 1
      ;;
  esac
}

require_url ISKWORKS_SUPPORT_URL "$support_url"
require_url ISKWORKS_SOURCE_URL "$source_url"
require_text ISKWORKS_SUPPORT_LABEL "$support_label"
require_text ISKWORKS_DONATION_CHARACTER "$donation_character"

if [ "$enabled" = true ] && [ -z "$app_id" ]; then
  echo "LOGROCKET_ENABLED is true but LOGROCKET_APP_ID is empty; session replay stays off" >&2
  enabled=false
fi

printf 'window.__ISKWORKS_CONFIG__ = {"logRocketAppId":"%s","logRocketEnabled":%s,"supportUrl":"%s","supportLabel":"%s","donationCharacter":"%s","sourceUrl":"%s"};\n' \
  "$app_id" "$enabled" "$support_url" "$support_label" "$donation_character" "$source_url" >"$target"
echo "iskworks runtime config: session replay $([ "$enabled" = true ] && echo "enabled ($app_id)" || echo disabled)"
