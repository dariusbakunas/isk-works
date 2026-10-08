#!/usr/bin/env bash
# Contract test for the web container's runtime config writer
# (web/iskworks-web/docker/40-iskworks-runtime-config.sh).
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$repo_root/web/iskworks-web/docker/40-iskworks-runtime-config.sh"
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT
export ISKWORKS_RUNTIME_CONFIG_PATH="$work_dir/config.js"
unset LOGROCKET_APP_ID LOGROCKET_ENABLED ISKWORKS_SUPPORT_URL ISKWORKS_SUPPORT_LABEL \
  ISKWORKS_DONATION_CHARACTER ISKWORKS_SOURCE_URL

expect_config() {
  local expected="$1"
  local actual
  actual="$(cat "$ISKWORKS_RUNTIME_CONFIG_PATH")"
  if [[ "$actual" != "$expected" ]]; then
    printf 'unexpected config.js\n  expected: %s\n  actual:   %s\n' "$expected" "$actual" >&2
    exit 1
  fi
}

# Unset: replay is off.
sh "$script" >/dev/null
expect_config 'window.__ISKWORKS_CONFIG__ = {"logRocketAppId":"","logRocketEnabled":false,"supportUrl":"","supportLabel":"","donationCharacter":"","sourceUrl":""};'

# Both set: replay is on.
LOGROCKET_APP_ID=org/app-1 LOGROCKET_ENABLED=true sh "$script" >/dev/null
expect_config 'window.__ISKWORKS_CONFIG__ = {"logRocketAppId":"org/app-1","logRocketEnabled":true,"supportUrl":"","supportLabel":"","donationCharacter":"","sourceUrl":""};'

# An app ID alone does not enable replay.
LOGROCKET_APP_ID=org/app-1 sh "$script" >/dev/null
expect_config 'window.__ISKWORKS_CONFIG__ = {"logRocketAppId":"org/app-1","logRocketEnabled":false,"supportUrl":"","supportLabel":"","donationCharacter":"","sourceUrl":""};'

# Enabled without an app ID stays off.
LOGROCKET_ENABLED=true sh "$script" >/dev/null 2>&1
expect_config 'window.__ISKWORKS_CONFIG__ = {"logRocketAppId":"","logRocketEnabled":false,"supportUrl":"","supportLabel":"","donationCharacter":"","sourceUrl":""};'

# Anything that could break out of the JS string is refused.
if LOGROCKET_APP_ID='x"};alert(1);//' LOGROCKET_ENABLED=true sh "$script" >/dev/null 2>&1; then
  printf 'unsafe LOGROCKET_APP_ID was accepted\n' >&2
  exit 1
fi

# Community links are written through when set.
ISKWORKS_SUPPORT_URL='https://discord.gg/Abc123' ISKWORKS_SUPPORT_LABEL=Discord \
  ISKWORKS_DONATION_CHARACTER="Some O'Pilot" ISKWORKS_SOURCE_URL='https://example.com/fork' \
  sh "$script" >/dev/null
expect_config 'window.__ISKWORKS_CONFIG__ = {"logRocketAppId":"","logRocketEnabled":false,"supportUrl":"https://discord.gg/Abc123","supportLabel":"Discord","donationCharacter":"Some O'"'"'Pilot","sourceUrl":"https://example.com/fork"};'

expect_refused() {
  local description="$1"
  shift
  if env "$@" sh "$script" >/dev/null 2>&1; then
    printf '%s was accepted\n' "$description" >&2
    exit 1
  fi
}
expect_refused 'non-http ISKWORKS_SUPPORT_URL' ISKWORKS_SUPPORT_URL='javascript:alert(1)'
expect_refused 'quote in ISKWORKS_SUPPORT_URL' ISKWORKS_SUPPORT_URL='https://x/"};alert(1);//'
expect_refused 'quote in ISKWORKS_SUPPORT_LABEL' ISKWORKS_SUPPORT_LABEL='x"};alert(1);//'
expect_refused 'backslash in ISKWORKS_DONATION_CHARACTER' ISKWORKS_DONATION_CHARACTER='x\'
expect_refused 'non-http ISKWORKS_SOURCE_URL' ISKWORKS_SOURCE_URL='data:text/html,x'

printf 'web runtime config contract: PASS\n'
