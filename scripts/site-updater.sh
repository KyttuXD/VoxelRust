#!/bin/sh
# Runs on the website's server (not on your machine): keeps the browser
# version on the site in step with the published release, with no deploy by
# hand. Run it from cron or a systemd timer every 1 to 3 minutes, or with
# --loop as a service.
#
#   scripts/site-updater.sh [--dry-run] [--loop] [--once]
#
# Each run:
#   1. fetches nightly.txt and nightly.txt.sig from SITE_SOURCE (a web folder,
#      such as the mirror that scripts/deploy-server.sh fills, or the GitHub
#      release folder if the repository is public; or a local folder);
#   2. checks the signature with the project's public key (SITE_PUBKEY) and
#      stops if it is wrong: whatever serves the files is not trusted;
#   3. stops if the release number is not larger than the one on the site
#      (SITE_ROOT/current-seq): so nearly every run is two tiny downloads;
#   4. otherwise downloads the web build named by the list's `web` line,
#      checks its size and SHA-256, unpacks it into SITE_ROOT/releases/<seq>-<commit>
#      and flips the symlink SITE_ROOT/current to it in one step (ln, then
#      mv -T), keeps the newest SITE_KEEP releases, and removes the rest.
#   After a failure it waits (1, 2, 4... minutes, at most 30) before trying
#   again, so a broken release is not fetched every minute.
#
# --dry-run: fetches and verifies the list, says what it would do, changes
# nothing under SITE_ROOT. --once is the default; --loop repeats every
# SITE_CHECK_SECONDS (default 90, 30 to 600, plus a little jitter).
#
# Settings (environment, or the file SITE_UPDATER_CONF, default
# /etc/voxelrust/site-updater.conf; no secrets in either):
#   SITE_ROOT           the folder the web server's root points into: it holds
#                       current -> releases/<name>   (required)
#   SITE_SOURCE         URL or folder holding nightly.txt  (required)
#   SITE_PUBKEY         the update public key as a PEM file  (required; make it
#                       once with scripts/export-public-key.sh)
#   SITE_KEEP           releases to keep (default 3, at least 2)
#   SITE_CHECK_SECONDS  for --loop
# Needs: sh, curl (only for a URL), openssl, sha256sum, unzip (or python3).
set -u

CONF="${SITE_UPDATER_CONF:-/etc/voxelrust/site-updater.conf}"
for n in SITE_ROOT SITE_SOURCE SITE_PUBKEY SITE_KEEP SITE_CHECK_SECONDS; do
  eval "if [ \"\${$n+x}\" = x ]; then keep_$n=\$$n; kept_$n=1; fi"
done
# shellcheck disable=SC1090
[ -f "$CONF" ] && . "$CONF"
for n in SITE_ROOT SITE_SOURCE SITE_PUBKEY SITE_KEEP SITE_CHECK_SECONDS; do
  eval "if [ \"\${kept_$n:-}\" = 1 ]; then $n=\$keep_$n; fi"
done

DRY=0; LOOP=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY=1 ;;
    --loop) LOOP=1 ;;
    --once) LOOP=0 ;;
    -h|--help) sed -n '2,/^set -u/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
    *) echo "site-updater: unknown option $arg" >&2; exit 2 ;;
  esac
done

say() { echo "site-updater: $(date '+%Y-%m-%d %H:%M:%S') $*"; }
fail() { say "$*" >&2; return 1; }

: "${SITE_ROOT:?SITE_ROOT is not set}"
: "${SITE_SOURCE:?SITE_SOURCE is not set}"
: "${SITE_PUBKEY:?SITE_PUBKEY is not set}"
SITE_KEEP="${SITE_KEEP:-3}"
case "$SITE_KEEP" in ''|*[!0-9]*) SITE_KEEP=3 ;; esac
[ "$SITE_KEEP" -ge 2 ] || SITE_KEEP=2
interval="${SITE_CHECK_SECONDS:-90}"
case "$interval" in ''|*[!0-9]*) interval=90 ;; esac
[ "$interval" -ge 30 ] || interval=30
[ "$interval" -le 600 ] || interval=600
[ -f "$SITE_PUBKEY" ] || { echo "site-updater: no public key at $SITE_PUBKEY" >&2; exit 2; }
SITE_SOURCE="${SITE_SOURCE%/}"

fetch() { # fetch NAME DEST
  case "$SITE_SOURCE" in
    http://*|https://*) curl -fsSL --connect-timeout 8 --max-time 300 -A "voxelrust-site-updater" -o "$2" "$SITE_SOURCE/$1" ;;
    *) cp "$SITE_SOURCE/$1" "$2" ;;
  esac
}

# verify FILE SIG: an Ed25519 signature over the exact bytes of FILE.
verify() {
  openssl pkeyutl -verify -pubin -inkey "$SITE_PUBKEY" -rawin -in "$1" -sigfile "$2" >/dev/null 2>&1 \
    || openssl pkeyutl -verify -pubin -inkey "$SITE_PUBKEY" -in "$1" -sigfile "$2" >/dev/null 2>&1
}

plain_name() { case "$1" in ''|*[!A-Za-z0-9._-]*) return 1 ;; *) return 0 ;; esac; }

# One look at the release, and the update if it is newer. Returns 1 on a failure.
check_once() {
  mkdir -p "$SITE_ROOT/releases" || { fail "cannot make $SITE_ROOT/releases"; return 1; }
  # After failures: wait.
  if [ -f "$SITE_ROOT/.retry-after" ]; then
    until_t="$(cat "$SITE_ROOT/.retry-after" 2>/dev/null || echo 0)"
    case "$until_t" in ''|*[!0-9]*) until_t=0 ;; esac
    if [ "$(date +%s)" -lt "$until_t" ]; then return 0; fi
  fi
  work="$SITE_ROOT/.work-$$"
  rm -rf "$work"; mkdir -p "$work" || { fail "cannot make $work"; return 1; }
  ok=1
  update_from_release "$work" || ok=0
  rm -rf "$work"
  if [ "$ok" = 1 ]; then
    rm -f "$SITE_ROOT/.retry-after" "$SITE_ROOT/.retry-count"
    return 0
  fi
  if [ "$DRY" != 1 ]; then
    n="$(cat "$SITE_ROOT/.retry-count" 2>/dev/null || echo 0)"
    case "$n" in ''|*[!0-9]*) n=0 ;; esac
    n=$((n + 1)); [ "$n" -le 5 ] || n=5
    wait=$((60 << (n - 1))); [ "$wait" -le 1800 ] || wait=1800
    echo "$n" > "$SITE_ROOT/.retry-count"
    echo $(($(date +%s) + wait)) > "$SITE_ROOT/.retry-after"
    say "will try again in ${wait}s"
  fi
  return 1
}

update_from_release() { # update_from_release WORKDIR
  work="$1"
  fetch nightly.txt "$work/nightly.txt" || { fail "could not read $SITE_SOURCE/nightly.txt"; return 1; }
  fetch nightly.txt.sig "$work/nightly.txt.sig" || { fail "could not read the list's signature"; return 1; }
  verify "$work/nightly.txt" "$work/nightly.txt.sig" || { fail "the signature of the release list is NOT valid: refusing it"; return 1; }
  [ "$(wc -c < "$work/nightly.txt")" -lt 65536 ] || { fail "the release list is too big"; return 1; }
  [ "$(grep -c '^seq ' "$work/nightly.txt")" = 1 ] || { fail "the release list has no single seq line"; return 1; }
  seq="$(sed -n 's/^seq //p' "$work/nightly.txt")"
  commit="$(sed -n 's/^commit //p' "$work/nightly.txt" | head -1)"
  case "$seq" in ''|*[!0-9]*) fail "bad release number"; return 1 ;; esac
  case "$commit" in ''|*[!0-9a-fA-F]*) fail "bad commit in the list"; return 1 ;; esac
  cur="$(cat "$SITE_ROOT/current-seq" 2>/dev/null || echo 0)"
  case "$cur" in ''|*[!0-9]*) cur=0 ;; esac
  if [ "$seq" -le "$cur" ]; then
    [ "${VERBOSE:-0}" = 1 ] && say "release $cur is current"
    return 0
  fi
  set -- $(awk '$1 == "web" { print $2, $3, $4; exit }' "$work/nightly.txt")
  file="${1:-}"; sum="${2:-}"; size="${3:-}"
  if [ -z "$file" ]; then fail "release $seq has no web build in its list"; return 1; fi
  plain_name "$file" || { fail "the web file name in the list is not a plain name"; return 1; }
  case "$sum" in *[!0-9a-fA-F]*|'') fail "bad checksum in the list"; return 1 ;; esac
  [ "${#sum}" = 64 ] || { fail "bad checksum in the list"; return 1; }
  case "$size" in ''|*[!0-9]*) fail "bad size in the list"; return 1 ;; esac
  name="$seq-$commit"
  if [ "$DRY" = 1 ]; then
    say "dry run: would install release $seq (commit $commit) from $file ($size bytes) as releases/$name and point current at it (now: $cur)"
    return 0
  fi
  say "release $seq is out (the site has $cur): fetching $file"
  fetch "$file" "$work/web.zip" || { fail "could not download $file"; return 1; }
  [ "$(wc -c < "$work/web.zip" | tr -d ' ')" = "$size" ] || { fail "the download is not $size bytes"; return 1; }
  got="$(sha256sum "$work/web.zip" | cut -d' ' -f1)"
  [ "$got" = "$(echo "$sum" | tr 'A-F' 'a-f')" ] || { fail "the download's SHA-256 does not match the signed list"; return 1; }
  mkdir -p "$work/x"
  if command -v unzip >/dev/null 2>&1; then
    unzip -q -o "$work/web.zip" -d "$work/x" || { fail "could not unpack the web build"; return 1; }
  else
    python3 -m zipfile -e "$work/web.zip" "$work/x" || { fail "could not unpack the web build"; return 1; }
  fi
  src="$work/x"; [ -d "$work/x/web" ] && src="$work/x/web"
  for need in index.html voxelrust.js voxelrust.wasm; do
    [ -f "$src/$need" ] || { fail "the web build has no $need"; return 1; }
  done
  # Zip entries must not have reached outside the folder (a symlink, say).
  if [ -n "$(find "$work/x" -type l 2>/dev/null)" ]; then fail "the web build holds a symlink: refusing it"; return 1; fi
  printf 'commit %s\nseq %s\n' "$commit" "$seq" > "$src/version.txt"
  cp "$work/nightly.txt" "$src/release-list.txt"; cp "$work/nightly.txt.sig" "$src/release-list.txt.sig"
  chmod -R u+rwX,go+rX "$src"
  rm -rf "$SITE_ROOT/releases/$name"
  mv "$src" "$SITE_ROOT/releases/$name" || { fail "could not put the release in place"; return 1; }
  ln -sfn "releases/$name" "$SITE_ROOT/.current.new" && mv -T "$SITE_ROOT/.current.new" "$SITE_ROOT/current" \
    || { fail "could not switch the site to the new release"; return 1; }
  echo "$seq" > "$SITE_ROOT/current-seq.new" && mv -f "$SITE_ROOT/current-seq.new" "$SITE_ROOT/current-seq"
  say "the site now serves release $seq (commit $commit)"
  # Keep the newest few; never the one just made.
  ( cd "$SITE_ROOT/releases" && ls -1dt ./* 2>/dev/null | sed 's|^\./||' | tail -n +$((SITE_KEEP + 1)) \
    | while read -r old; do [ "$old" = "$name" ] || rm -rf "$old"; done )
  return 0
}

# One run at a time (a slow download must not meet the next minute's run).
mkdir -p "$SITE_ROOT" || exit 2
lock="$SITE_ROOT/.lock"
if ! mkdir "$lock" 2>/dev/null; then
  old="$(cat "$lock/pid" 2>/dev/null || true)"
  if [ -n "$old" ] && kill -0 "$old" 2>/dev/null; then exit 0; fi
  rm -rf "$lock"; mkdir "$lock" 2>/dev/null || exit 0
fi
echo $$ > "$lock/pid"
trap 'rm -rf "$lock" "$SITE_ROOT/.work-$$"' EXIT
trap 'exit 1' INT TERM

if [ "$LOOP" = 1 ]; then
  while :; do
    check_once || true
    sleep $((interval + $(date +%s) % (interval / 6 + 1)))
  done
fi
check_once
