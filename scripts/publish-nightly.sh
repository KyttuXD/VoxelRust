#!/bin/sh
# Publish the signed build in dist/nightly to the GitHub `nightly` release and
# the update mirror. scripts/package-release.sh runs this at its end, so every
# build ends up on the nightly tag (set VOXELRUST_PUBLISH=0 to build without
# publishing).
#
#   scripts/publish-nightly.sh [--dry-run]
#
# What it does: uploads every file of dist/nightly to the release (the programs
# first, then nightly.txt, then nightly.txt.sig, so a list never names a file
# that is not there), moves the `nightly` tag to the commit named in the list,
# pushes that tag, and copies the same files to the mirror (MIRROR_HOST) when
# one is set. It takes the commit from nightly.txt, not from HEAD, so it
# publishes exactly what was built. Settings: deploy.conf or the environment.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
. "$ROOT/scripts/lib-deploy.sh"
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY=1 ;;
    -h|--help) sed -n '2,/^set -eu/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option $arg" ;;
  esac
done
load_conf
set_defaults
out="$ROOT/dist/nightly"
[ -f "$out/nightly.txt" ] && [ -f "$out/nightly.txt.sig" ] || die "no signed list in dist/nightly (run scripts/package-release.sh first)"
command -v gh >/dev/null 2>&1 || die "the gh CLI is needed and must be logged in (gh auth login)"
commit="$(sed -n 's/^commit //p' "$out/nightly.txt" | head -1)"
seq="$(sed -n 's/^seq //p' "$out/nightly.txt" | head -1)"
full="$(git rev-parse --verify "$commit^{commit}" 2>/dev/null)" || die "the list's commit $commit is not in this repository"
say "release $seq, commit $commit -> GitHub $GITHUB_REPO (tag $RELEASE_TAG)"

files=""
for f in "$out"/*; do
  case "$f" in */nightly.txt|*/nightly.txt.sig) ;; *) files="$files $f" ;; esac
done
if [ "$DRY" = 1 ] || ! gh release view "$RELEASE_TAG" -R "$GITHUB_REPO" >/dev/null 2>&1; then
  run gh release create "$RELEASE_TAG" -R "$GITHUB_REPO" --prerelease --title "Nightly" --notes "Rolling build of the latest commit (the game's updater reads nightly.txt)." --target "$GIT_BRANCH" || true
fi
# shellcheck disable=SC2086
run gh release upload "$RELEASE_TAG" -R "$GITHUB_REPO" --clobber $files
run gh release upload "$RELEASE_TAG" -R "$GITHUB_REPO" --clobber "$out/nightly.txt"
run gh release upload "$RELEASE_TAG" -R "$GITHUB_REPO" --clobber "$out/nightly.txt.sig"
# The commit must be on GitHub for the tag to point at something.
run git push "$GIT_REMOTE" "$full:refs/heads/$GIT_BRANCH" 2>/dev/null || say "(the branch was not pushed from here; the tag needs the commit on $GIT_REMOTE)"
run git tag -f "$RELEASE_TAG" "$full"
run git push -f "$GIT_REMOTE" "refs/tags/$RELEASE_TAG"

if [ -n "${MIRROR_HOST:-}" ] && [ -n "${MIRROR_DIR:-}" ]; then
  check_plain MIRROR_HOST; check_plain MIRROR_DIR
  remote "$MIRROR_HOST" "mkdir -p $MIRROR_DIR"
  run sh scripts/publish-mirror.sh "$MIRROR_HOST" "$MIRROR_DIR"
else
  say "no mirror (MIRROR_HOST is not set)"
fi
say "published release $seq ($commit)"
