#!/bin/sh
# Build every version of the game and lay out the files of a release.
#
#   scripts/package-release.sh [toolchain]
#
# Result, in dist/nightly/:
#   voxelrust-<build>.tar.gz / .zip   what people download: the program (an
#                                     app on the Mac) and the docs; no mods
#                                     come with the game
#   voxelrust-<build>.bin / .exe      the bare program, for the updater
#   voxelrust-web.zip                 the browser version
#   nightly.txt                       commit, and each bare program's
#                                     checksum and size (what the updater reads)
#   nightly.txt.sig                   its Ed25519 signature (key: VOXELRUST_SIGN_KEY,
#                                     default ~/.voxelrust/update-signing-key.pem);
#                                     no key, no release
#   BUILDS.txt, SHA256SUMS.txt        for people
#
# <build> is what `voxelrust --version` prints: each kind of machine has its
# own, and the updater only ever takes its own.
set -eu
cd "$(dirname "$0")/.."
# A shared target folder (CARGO_TARGET_DIR) keeps compiled code between checkouts and scripts.
TARGET="${CARGO_TARGET_DIR:-target}"
# The updater's list must be signed, so no key means no release: checked now,
# before the long builds, and the key must be the one the game trusts
# (src/client/update_key.rs), or no game would accept the list.
SIGN_KEY="${VOXELRUST_SIGN_KEY:-$HOME/.voxelrust/update-signing-key.pem}"
export VOXELRUST_SIGN_KEY="$SIGN_KEY"
if [ ! -f "$SIGN_KEY" ]; then
  echo "refusing: no signing key at $SIGN_KEY, so nightly.txt could not be signed." >&2
  echo "Make one with scripts/make-signing-key.sh (and put its public key in src/client/update_key.rs)," >&2
  echo "or set VOXELRUST_SIGN_KEY to the key's path." >&2
  exit 1
fi
have_pub="$(openssl pkey -in "$SIGN_KEY" -pubout -outform DER | tail -c 32 | od -An -v -tx1 | tr -d ' \n')"
game_pub="$(sed -n '/^const EMBEDDED/,/^];/p' src/client/update_key.rs | grep -o '0x[0-9a-fA-F][0-9a-fA-F]' | sed 's/0x//' | tr -d '\n' | tr 'A-F' 'a-f')"
if [ "$have_pub" != "$game_pub" ]; then
  echo "refusing: the signing key at $SIGN_KEY is not the one in src/client/update_key.rs" >&2
  echo "  key file:  $have_pub" >&2
  echo "  in game:   $game_pub" >&2
  exit 1
fi
TOOLCHAIN="${1:-}"
CARGO="cargo ${TOOLCHAIN:++$TOOLCHAIN}"
COMMIT="${VOXELRUST_COMMIT:-$(git rev-parse --short=12 HEAD)}"
export VOXELRUST_COMMIT="$COMMIT"
# The release number: how many commits the release branch has. It only ever
# goes up, so that programs can tell a newer release from an older one.
if [ -z "${VOXELRUST_SEQ:-}" ]; then
  if [ "$(git rev-parse --is-shallow-repository)" = true ]; then
    echo "refusing: this is a shallow clone, so the release number would be wrong." >&2
    echo "Fetch the full history, or set VOXELRUST_SEQ." >&2
    exit 1
  fi
  VOXELRUST_SEQ="$(git rev-list --count HEAD)"
fi
SEQ="$VOXELRUST_SEQ"
export VOXELRUST_SEQ
# Never publish a release older than the one already out (or a different
# build with the same number). VOXELRUST_ALLOW_SEQ_DECREASE=1 overrides this,
# for a deliberate rollback.
published="$(mktemp)"
code="$(curl -sSL -o "$published" -w '%{http_code}' "https://github.com/KyttuXD/VoxelRust/releases/download/nightly/nightly.txt" || echo 000)"
case "$code" in
  404) echo "no release is published yet; this will be the first" ;;
  200)
    pub_commit="$(sed -n 's/^commit //p' "$published" | head -1)"
    pub_seq="$(sed -n 's/^seq //p' "$published" | head -1)"
    case "$pub_seq" in
      '') echo "the published list is from before release numbers; this one will be the first with a number"; pub_seq=0 ;;
      *[!0-9]*) echo "refusing: the published release number is not a number." >&2; exit 1 ;;
    esac
    if [ "$SEQ" -lt "$pub_seq" ] || { [ "$SEQ" -eq "$pub_seq" ] && [ "$COMMIT" != "$pub_commit" ]; }; then
      if [ "${VOXELRUST_ALLOW_SEQ_DECREASE:-}" != 1 ]; then
        echo "refusing: release $SEQ ($COMMIT) is not newer than the published $pub_seq ($pub_commit)." >&2
        echo "Set VOXELRUST_ALLOW_SEQ_DECREASE=1 for a deliberate rollback." >&2
        rm -f "$published"; exit 1
      fi
    fi ;;
  *) echo "refusing: could not read the published release list (HTTP $code)." >&2; rm -f "$published"; exit 1 ;;
esac
rm -f "$published"
out="$PWD/dist/nightly"
stage="$PWD/dist/stage"
rm -rf "$out" "$stage"
mkdir -p "$out" "$stage"

$CARGO build --release
sh scripts/build-windows.sh ${TOOLCHAIN:+"$TOOLCHAIN"}
rm -f dist/macos-legacy/voxelrust dist/macos/voxelrust
sh scripts/build-macos-legacy.sh 1.77.0
sh scripts/package-macos-app.sh
VARIANT=modern sh scripts/package-macos-app.sh
sh scripts/build-web.sh ${TOOLCHAIN:+"$TOOLCHAIN"} || echo "web build skipped"

package() { # package NAME PROGRAM-OR-APP FORMAT
  dir="$stage/$1"
  mkdir -p "$dir"
  cp -R "$2" "$dir/"
  for extra in README.md LICENSE docs examples; do
    [ -e "$extra" ] && cp -R "$extra" "$dir/"
  done
  # SDL2 beside the Windows program, when vendor/sdl2 has it.
  case "$2" in
    *.exe) [ -f vendor/sdl2/SDL2.dll ] && cp vendor/sdl2/SDL2.dll "$dir/" ;;
  esac
  if [ "$3" = "tar.gz" ]; then
    mkdir -p "$dir/share"
    cp -R assets/hicolor "$dir/share/icons-hicolor"
    cp assets/voxelrust.desktop assets/icon.png "$dir/"
    tar -czf "$out/$1.tar.gz" -C "$stage" "$1"
  else
    (cd "$stage" && zip -qry "$out/$1.zip" "$1")
  fi
}
package voxelrust-linux-x86_64         "$TARGET"/release/voxelrust     tar.gz
package voxelrust-windows-x86_64       dist/windows/voxelrust.exe   zip
package voxelrust-macos-x86_64         dist/macos/VoxelRust.app     zip
package voxelrust-macos-legacy-x86_64  dist/VoxelRust.app           zip
[ -d dist/web ] && (cd dist && zip -qry "$out/voxelrust-web.zip" web)

# The bare programs and the list the updater reads.
cp "$TARGET"/release/voxelrust       "$out/voxelrust-linux-x86_64.bin"
cp dist/windows/voxelrust.exe     "$out/voxelrust-windows-x86_64.exe"
cp dist/macos/voxelrust           "$out/voxelrust-macos-x86_64.bin"
cp dist/macos-legacy/voxelrust    "$out/voxelrust-macos-legacy-x86_64.bin"
sh scripts/write-nightly-list.sh "$out" "$COMMIT" "$SEQ"
{
  echo "# build                 file                                   runs on"
  echo "linux-x86_64            voxelrust-linux-x86_64.tar.gz          Linux, 64-bit Intel/AMD"
  echo "windows-x86_64          voxelrust-windows-x86_64.zip           Windows 7 to 11, 64-bit"
  echo "macos-x86_64            voxelrust-macos-x86_64.zip             macOS 10.12 and later (Intel; Apple silicon through Rosetta)"
  echo "macos-legacy-x86_64     voxelrust-macos-legacy-x86_64.zip      Mac OS X 10.6 to 10.11 (64-bit Intel)"
  echo "web                     voxelrust-web.zip                      a modern browser (unzip onto any web server)"
  echo "# commit $COMMIT, release $SEQ"
  echo "# SDL2 comes inside the Windows and Mac files; on Linux install your system's SDL2 package."
} > "$out/BUILDS.txt"
(cd "$out" && sha256sum voxelrust-* > SHA256SUMS.txt)
rm -rf "$stage"
ls -l "$out"
