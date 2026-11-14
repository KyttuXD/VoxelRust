#!/bin/sh
# Tests scripts/site-updater.sh with a local folder as the "release" and a
# throwaway key (no network, no game build):
#
#   1. a signed release is installed, and current points at it;
#   2. the same release again changes nothing;
#   3. a list changed after signing, a wrong checksum and an older release are
#      refused, and the site keeps serving what it had;
#   4. a newer release replaces it, and old releases are pruned to SITE_KEEP;
#   5. --dry-run changes nothing.
#
#   sh scripts/test-site-updater.sh
set -eu
cd "$(dirname "$0")/.."
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
fail() { echo "FAIL: $1" >&2; exit 1; }
openssl genpkey -algorithm ED25519 -out "$work/key.pem" 2>/dev/null
openssl genpkey -algorithm ED25519 -out "$work/other.pem" 2>/dev/null
openssl pkey -in "$work/key.pem" -pubout -out "$work/pub.pem"
src="$work/src"; site="$work/site"; mkdir -p "$src" "$site"
export SITE_ROOT="$site" SITE_SOURCE="$src" SITE_PUBKEY="$work/pub.pem" SITE_KEEP=2 SITE_UPDATER_CONF=/nonexistent
export VOXELRUST_SIGN_KEY="$work/key.pem"

release() { # release SEQ COMMIT
  rm -rf "$work/web" "$src/voxelrust-web.zip"
  mkdir -p "$work/web/web"
  echo "<html>release $1</html>" > "$work/web/web/index.html"
  echo "js $1" > "$work/web/web/voxelrust.js"
  echo "wasm $1" > "$work/web/web/voxelrust.wasm"
  (cd "$work/web" && zip -qr "$src/voxelrust-web.zip" web)
  size="$(wc -c < "$src/voxelrust-web.zip" | tr -d ' ')"
  printf 'commit %s\nseq %s\nweb voxelrust-web.zip %s %s\n' "$2" "$1" "$(sha256sum "$src/voxelrust-web.zip" | cut -d' ' -f1)" "$size" > "$src/nightly.txt"
  sh scripts/sign-file.sh "$src/nightly.txt"
}
serving() { cat "$site/current/index.html"; }

release 5 aaaaaaaaaaaa
sh scripts/site-updater.sh --dry-run >/dev/null
[ ! -e "$site/current" ] || fail "--dry-run changed the site"
sh scripts/site-updater.sh >/dev/null
[ "$(serving)" = "<html>release 5</html>" ] || fail "release 5 is not served"
[ "$(cat "$site/current-seq")" = 5 ] || fail "current-seq"
[ "$(sed -n 's/^seq //p' "$site/current/version.txt")" = 5 ] || fail "version.txt"
echo "ok: a signed release is installed"

sh scripts/site-updater.sh >/dev/null
[ "$(ls "$site/releases" | wc -l | tr -d ' ')" = 1 ] || fail "the same release made another folder"
echo "ok: the same release again does nothing"

# A changed list: the signature no longer fits.
release 6 bbbbbbbbbbbb
sed -i 's/^seq 6$/seq 7/' "$src/nightly.txt"
sh scripts/site-updater.sh >/dev/null 2>&1 && fail "a changed list was accepted"
[ "$(serving)" = "<html>release 5</html>" ] || fail "a changed list replaced the site"
rm -f "$site/.retry-after" "$site/.retry-count"
# Signed by another key.
release 6 bbbbbbbbbbbb
VOXELRUST_SIGN_KEY="$work/other.pem" sh scripts/sign-file.sh "$src/nightly.txt"
sh scripts/site-updater.sh >/dev/null 2>&1 && fail "a list signed by another key was accepted"
rm -f "$site/.retry-after" "$site/.retry-count"
# Right signature, wrong zip.
release 6 bbbbbbbbbbbb
echo tampered >> "$src/voxelrust-web.zip"
sh scripts/site-updater.sh >/dev/null 2>&1 && fail "a changed download was accepted"
[ "$(serving)" = "<html>release 5</html>" ] || fail "a changed download replaced the site"
[ -f "$site/.retry-after" ] || fail "no back-off after a failure"
rm -f "$site/.retry-after" "$site/.retry-count"
# An older release.
release 4 cccccccccccc
sh scripts/site-updater.sh >/dev/null
[ "$(serving)" = "<html>release 5</html>" ] || fail "an older release replaced the site"
echo "ok: tampered and older releases are refused"

release 6 bbbbbbbbbbbb
sh scripts/site-updater.sh >/dev/null
[ "$(serving)" = "<html>release 6</html>" ] || fail "release 6 is not served"
release 7 dddddddddddd
sh scripts/site-updater.sh >/dev/null
[ "$(serving)" = "<html>release 7</html>" ] || fail "release 7 is not served"
[ "$(ls "$site/releases" | wc -l | tr -d ' ')" = 2 ] || fail "old releases were not pruned to 2"
echo "ok: newer releases replace it, and old ones are pruned"
