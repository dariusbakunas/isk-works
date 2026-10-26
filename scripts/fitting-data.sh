#!/usr/bin/env bash
# Download the fitting engine's data file (sde.dat from the @eveshipfit/sde
# npm package) at the version pinned in fitting-data.lock, verify it against
# the pinned npm integrity, and write <out_dir>/sde.dat + <out_dir>/VERSION.
#
# Usage: scripts/fitting-data.sh [out_dir]   (default: data/fitting)
#
# FITTING_DATA_LOCK and FITTING_DATA_URL override the lock file and the
# tarball URL (the contract test uses both; nothing else should need to).
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
lock="${FITTING_DATA_LOCK:-$repo_root/fitting-data.lock}"
out_dir="${1:-$repo_root/data/fitting}"

lock_value() {
  sed -n "s/^$1=//p" "$lock" | head -n 1
}

version="$(lock_value version)"
integrity="$(lock_value integrity)"
if [[ -z "$version" || "$integrity" != sha512-* ]]; then
  printf '%s needs version= and a sha512- integrity= line\n' "$lock" >&2
  exit 2
fi

if [[ -f "$out_dir/sde.dat" && "$(cat "$out_dir/VERSION" 2>/dev/null)" == "$version" ]]; then
  printf 'fitting data %s is up to date in %s\n' "$version" "$out_dir"
  exit 0
fi

url="${FITTING_DATA_URL:-https://registry.npmjs.org/@eveshipfit/sde/-/sde-$version.tgz}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

curl -fsSL "$url" -o "$tmp_dir/sde.tgz"
actual="sha512-$(openssl dgst -sha512 -binary "$tmp_dir/sde.tgz" | openssl base64 -A)"
if [[ "$actual" != "$integrity" ]]; then
  printf 'fitting data %s failed its integrity check\n  expected: %s\n  actual:   %s\n' \
    "$version" "$integrity" "$actual" >&2
  exit 1
fi

tar -xzf "$tmp_dir/sde.tgz" -C "$tmp_dir" package/dist/sde.dat
mkdir -p "$out_dir"
mv "$tmp_dir/package/dist/sde.dat" "$out_dir/sde.dat"
printf '%s\n' "$version" >"$out_dir/VERSION"
printf 'fitting data %s written to %s\n' "$version" "$out_dir"
