#!/usr/bin/env bash
# Contract test for the fitting engine data download (scripts/fitting-data.sh):
# integrity is enforced, and a matching download is not repeated.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$repo_root/scripts/fitting-data.sh"
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

fail() {
  printf '%s\n' "$1" >&2
  exit 1
}

# A tarball shaped like the npm package: package/dist/sde.dat (+ a file we skip).
mkdir -p "$work_dir/src/package/dist"
printf 'fake sde data' >"$work_dir/src/package/dist/sde.dat"
printf 'names we do not want' >"$work_dir/src/package/dist/names.dat"
tar -czf "$work_dir/sde.tgz" -C "$work_dir/src" package
integrity="sha512-$(openssl dgst -sha512 -binary "$work_dir/sde.tgz" | openssl base64 -A)"

write_lock() {
  printf 'version=%s\nintegrity=%s\n' "$1" "$2" >"$work_dir/fitting-data.lock"
}

export FITTING_DATA_LOCK="$work_dir/fitting-data.lock"
export FITTING_DATA_URL="file://$work_dir/sde.tgz"

# A good download writes sde.dat and VERSION, and nothing else.
write_lock 13.1.0 "$integrity"
bash "$script" "$work_dir/out" >/dev/null
[[ "$(cat "$work_dir/out/sde.dat")" == 'fake sde data' ]] || fail 'sde.dat not extracted'
[[ "$(cat "$work_dir/out/VERSION")" == '13.1.0' ]] || fail 'VERSION not written'
[[ ! -e "$work_dir/out/names.dat" ]] || fail 'names.dat should not be extracted'

# A second run with the same version does not download again.
FITTING_DATA_URL="file://$work_dir/missing.tgz" bash "$script" "$work_dir/out" >/dev/null \
  || fail 'an up-to-date run tried to download'

# A new version downloads again.
write_lock 13.2.0 "$integrity"
bash "$script" "$work_dir/out" >/dev/null
[[ "$(cat "$work_dir/out/VERSION")" == '13.2.0' ]] || fail 'a version bump did not refresh'

# A tampered integrity fails and leaves no data behind.
write_lock 13.1.0 'sha512-AAAA'
if bash "$script" "$work_dir/bad" >/dev/null 2>&1; then
  fail 'a wrong integrity was accepted'
fi
[[ ! -e "$work_dir/bad/sde.dat" ]] || fail 'sde.dat written despite a wrong integrity'

# A lock without an integrity is refused.
printf 'version=13.1.0\n' >"$work_dir/fitting-data.lock"
if bash "$script" "$work_dir/nolock" >/dev/null 2>&1; then
  fail 'a lock without integrity was accepted'
fi

printf 'fitting data download: ok\n'
