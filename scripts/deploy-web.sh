#!/bin/sh
# Build the browser version and put it on the website, with an atomic swap.
#
#   scripts/deploy-web.sh [--dry-run] [--skip-build] [--force] [TOOLCHAIN]
#
#   1. scripts/build-web.sh builds dist/web (--skip-build uses it as it is).
#   2. dist/web plus version.txt (commit and release number) and
#      version.txt.sig (signed with the update key) is made into a release
#      folder named <release number>-<commit>.
#   3. rsync uploads it to SITE_HOST:SITE_ROOT/releases/<name>/ (a new folder:
#      the site never serves a half-written one).
#   4. On the server, the symlink SITE_ROOT/current is flipped to it in one
#      step (ln + mv -T), SITE_ROOT/current-seq is written, and releases beyond
#      the newest SITE_KEEP (default 3) are removed. The web server's root is
#      SITE_ROOT/current. It never goes back to an older release number unless
#      --force.
#
# The website also follows releases by itself: scripts/site-updater.sh on the
# server (cron or a systemd timer) does steps 3 and 4 from the published
# release. This script is the push for when you do not want to wait for it.
#
# --dry-run prints every command and runs none. Settings: deploy.conf (see
# deploy.conf.example) or the environment. No secrets in either.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
. "$ROOT/scripts/lib-deploy.sh"

SKIP_BUILD=0; FORCE=0; TOOLCHAIN=""
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY=1 ;;
    --skip-build) SKIP_BUILD=1 ;;
    --force) FORCE=1 ;;
    -h|--help) sed -n '2,/^set -eu/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
    -*) die "unknown option $arg (try --help)" ;;
    *) TOOLCHAIN="$arg" ;;
  esac
done

load_conf
set_defaults
need_setting SITE_HOST
need_setting SITE_ROOT
check_plain SITE_HOST
check_plain SITE_ROOT
case "$SITE_KEEP" in ''|*[!0-9]*) die "SITE_KEEP must be a number" ;; esac
[ "$SITE_KEEP" -ge 2 ] || SITE_KEEP=2

commit="${VOXELRUST_COMMIT:-$(git rev-parse --short=7 HEAD)}"
seq="${VOXELRUST_SEQ:-$(git rev-list --count HEAD)}"
name="$seq-$commit"
stage="$ROOT/dist/web-site/$name"

if [ "$SKIP_BUILD" = 1 ]; then
  say "using dist/web as it is"
else
  run sh scripts/build-web.sh ${TOOLCHAIN:+"$TOOLCHAIN"}
fi

say "release $name -> $SITE_HOST:$SITE_ROOT"
if [ "$DRY" = 1 ]; then
  echo "  [dry-run] copy dist/web to $stage, write version.txt, sign it (scripts/sign-file.sh)"
else
  [ -f dist/web/index.html ] && [ -f dist/web/voxelrust.wasm ] || die "dist/web is not built (run without --skip-build)"
  rm -rf "$ROOT/dist/web-site"
  mkdir -p "$stage"
  cp -R dist/web/. "$stage/"
  printf 'commit %s\nseq %s\n' "$commit" "$seq" > "$stage/version.txt"
  sh scripts/sign-file.sh "$stage/version.txt"
fi

# Never move the site back to an older release by accident.
if [ "$DRY" != 1 ] && [ "$FORCE" != 1 ]; then
  # shellcheck disable=SC2086
  have="$(ssh ${SSH_OPTS:-} "$SITE_HOST" "cat $SITE_ROOT/current-seq 2>/dev/null || true")"
  case "$have" in ''|*[!0-9]*) have=0 ;; esac
  if [ "$have" -gt "$seq" ]; then
    die "the site is on release $have, newer than $seq: use --force to put an older one there"
  fi
fi

remote "$SITE_HOST" "mkdir -p $SITE_ROOT/releases"
# A new folder each time; --delete only touches that folder (a resumed upload).
run rsync -rlt --chmod=D755,F644 --delete "$stage/" "$SITE_HOST:$SITE_ROOT/releases/$name/"

# The flip, and the tidying, as one remote script (its text is fixed; the
# values are plain characters, checked above).
flip="cd $SITE_ROOT && ln -sfn releases/$name .current.new && mv -T .current.new current && echo $seq > current-seq.new && mv -f current-seq.new current-seq"
tidy="cd $SITE_ROOT/releases && ls -1dt * | tail -n +$((SITE_KEEP + 1)) | while read -r old; do [ \"\$old\" = \"$name\" ] || rm -rf \"\$old\"; done"
remote "$SITE_HOST" "$flip && $tidy"
say "the site now serves release $name"
if [ -n "${SITE_URL:-}" ] && [ "$DRY" != 1 ]; then
  say "check: curl -s $SITE_URL/version.txt"
fi
