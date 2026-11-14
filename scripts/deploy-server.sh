#!/bin/sh
# Build, sign and publish a VoxelRust release: the programs the game and the
# public server update from. (The web version has its own script,
# scripts/deploy-web.sh; --web here runs it after this.)
#
#   scripts/deploy-server.sh [--dry-run] [--yes] [--skip-build] [--no-push]
#                            [--no-mirror] [--install] [--web] [TOOLCHAIN]
#
# Steps (see docs/DEPLOY.md):
#   1. scripts/package-release.sh builds every version and writes the signed
#      nightly.txt (it refuses without the signing key, and when the release
#      number would go backwards). --skip-build uses dist/nightly as it is.
#   2. scripts/package-server.sh makes the Linux server package.
#   3. git push of the branch to the (private) GitHub repository.
#   4. The files go up as assets of the `nightly` release with the gh CLI: the
#      programs and archives first, nightly.txt, and nightly.txt.sig last, so
#      that no list ever names a file that is not there. The `nightly` tag is
#      then moved to this commit.
#   5. The plain-HTTP mirror on your server (MIRROR_HOST, MIRROR_DIR) gets the
#      same signed files, list last. It is how a private repository still
#      serves updates (the game's update_mirror setting) and what
#      scripts/site-updater.sh reads. --no-mirror skips it.
#   6. --install also copies the server package to SERVER_HOST:SERVER_DIR
#      (only needed for the first install, or to change run.sh or SDL2: a
#      running server updates its own program from step 4 or 5).
#
# --dry-run prints every command and runs none (nothing is built, pushed,
# uploaded or copied). Nothing here asks for or stores a secret: the signing
# key is a file (VOXELRUST_SIGN_KEY), ssh uses your ssh keys and agent, and gh
# uses its own login.
#
# Settings: deploy.conf (see deploy.conf.example) or the environment.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
. "$ROOT/scripts/lib-deploy.sh"

SKIP_BUILD=0; PUSH=1; MIRROR=1; INSTALL=0; WEB=0; YES=0; TOOLCHAIN=""
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY=1 ;;
    --yes) YES=1 ;;
    --skip-build) SKIP_BUILD=1 ;;
    --no-push) PUSH=0 ;;
    --no-mirror) MIRROR=0 ;;
    --install) INSTALL=1 ;;
    --web) WEB=1 ;;
    -h|--help) sed -n '2,/^set -eu/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
    -*) die "unknown option $arg (try --help)" ;;
    *) TOOLCHAIN="$arg" ;;
  esac
done

load_conf
set_defaults
out="$ROOT/dist/nightly"
commit="$(git rev-parse --short=12 HEAD)"

command -v gh >/dev/null 2>&1 || [ "$DRY" = 1 ] || die "the gh CLI is needed (https://cli.github.com/) and must be logged in: gh auth login"
if [ "$MIRROR" = 1 ] && [ -n "${MIRROR_HOST:-}" ]; then need_setting MIRROR_DIR; check_plain MIRROR_HOST; check_plain MIRROR_DIR; fi
if [ "$INSTALL" = 1 ]; then need_setting SERVER_HOST; need_setting SERVER_DIR; check_plain SERVER_HOST; check_plain SERVER_DIR; fi
if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  say "warning: the working tree has uncommitted changes; the release is built from them but named $commit"
fi
branch="$(git rev-parse --abbrev-ref HEAD)"
if [ "$PUSH" = 1 ] && [ "$branch" != "$GIT_BRANCH" ]; then
  die "you are on '$branch', not '$GIT_BRANCH' (GIT_BRANCH): releases come from $GIT_BRANCH. Use --no-push to publish without pushing."
fi

say "release $commit -> GitHub $GITHUB_REPO (tag $RELEASE_TAG)${MIRROR_HOST:+, mirror $MIRROR_HOST:${MIRROR_DIR:-?}}"
if [ "$DRY" != 1 ] && [ "$YES" != 1 ]; then
  printf 'Publish now? This pushes and uploads. Type yes: '
  read -r answer
  [ "$answer" = yes ] || die "not confirmed; nothing was done"
fi

# 1, 2: build and sign.
if [ "$SKIP_BUILD" = 1 ]; then
  say "using the files in dist/nightly as they are"
else
  run sh scripts/package-release.sh ${TOOLCHAIN:+"$TOOLCHAIN"}
fi
if [ "$INSTALL" = 1 ]; then run sh scripts/package-server.sh ${TOOLCHAIN:+"$TOOLCHAIN"}; fi

if [ "$DRY" != 1 ]; then
  [ -f "$out/nightly.txt" ] && [ -f "$out/nightly.txt.sig" ] || die "no signed list in dist/nightly (run without --skip-build)"
  listed="$(sed -n 's/^commit //p' "$out/nightly.txt" | head -1)"
  [ "$listed" = "$commit" ] || die "dist/nightly is release $listed, but HEAD is $commit: rebuild (without --skip-build)"
  say "signed list: release $(sed -n 's/^seq //p' "$out/nightly.txt" | head -1), commit $listed"
fi

# 3: the source.
if [ "$PUSH" = 1 ]; then
  run git push "$GIT_REMOTE" "$GIT_BRANCH"
fi

# 4: the release assets, the list last.
if [ "$DRY" = 1 ]; then
  files="dist/nightly/<every file but nightly.txt and nightly.txt.sig>"
else
  files=""
  for f in "$out"/*; do
    case "$f" in */nightly.txt|*/nightly.txt.sig) ;; *) files="$files $f" ;; esac
  done
fi
if [ "$DRY" = 1 ] || ! gh release view "$RELEASE_TAG" -R "$GITHUB_REPO" >/dev/null 2>&1; then
  run gh release create "$RELEASE_TAG" -R "$GITHUB_REPO" --prerelease --title "Nightly" --notes "Rolling build of the latest commit (the game's updater reads nightly.txt)." --target "$GIT_BRANCH" || true
fi
# shellcheck disable=SC2086
run gh release upload "$RELEASE_TAG" -R "$GITHUB_REPO" --clobber $files
run gh release upload "$RELEASE_TAG" -R "$GITHUB_REPO" --clobber "$out/nightly.txt"
run gh release upload "$RELEASE_TAG" -R "$GITHUB_REPO" --clobber "$out/nightly.txt.sig"
if [ "$PUSH" = 1 ]; then
  run git tag -f "$RELEASE_TAG" HEAD
  run git push -f "$GIT_REMOTE" "refs/tags/$RELEASE_TAG"
fi

# 5: the mirror.
if [ "$MIRROR" = 1 ] && [ -n "${MIRROR_HOST:-}" ]; then
  remote "$MIRROR_HOST" "mkdir -p $MIRROR_DIR"
  run sh scripts/publish-mirror.sh "$MIRROR_HOST" "$MIRROR_DIR"
else
  say "no mirror (MIRROR_HOST is not set, or --no-mirror)"
fi

# 6: the server package.
if [ "$INSTALL" = 1 ]; then
  remote "$SERVER_HOST" "mkdir -p $SERVER_DIR"
  run rsync -v dist/server/voxelrust-server-linux-x86_64.tar.gz "$SERVER_HOST:$SERVER_DIR/"
  say "unpack it on the server: tar -xzf voxelrust-server-linux-x86_64.tar.gz (see docs/DEPLOY.md)"
fi

if [ "$WEB" = 1 ]; then
  if [ "$DRY" = 1 ]; then run sh scripts/deploy-web.sh --skip-build --dry-run; else run sh scripts/deploy-web.sh --skip-build; fi
fi
if [ "$DRY" = 1 ]; then say "done (dry run: nothing was changed)"; else say "done"; fi
