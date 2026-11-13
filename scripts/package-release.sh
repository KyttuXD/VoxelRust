#!/bin/sh
# Build every version of the game and lay out the files of a release.
#
#   scripts/package-release.sh [toolchain]
#
# Result, in dist/nightly/:
#   voxelrust-<build>.tar.gz / .zip   what people download: the program (an
#                                     app on the Mac), the mods that come with
#                                     the game, the docs
#   voxelrust-<build>.bin / .exe      the bare program, for the updater
#   voxelrust-web.zip                 the browser version
#   nightly.txt                       commit, and each bare program's
#                                     checksum and size (what the updater reads)
#   BUILDS.txt, SHA256SUMS.txt        for people
#
# <build> is what `voxelrust --version` prints: each kind of machine has its
# own, and the updater only ever takes its own.
set -eu
cd "$(dirname "$0")/.."
TOOLCHAIN="${1:-}"
CARGO="cargo ${TOOLCHAIN:++$TOOLCHAIN}"
COMMIT="${VOXELRUST_COMMIT:-$(git rev-parse --short=12 HEAD)}"
export VOXELRUST_COMMIT="$COMMIT"
out="$PWD/dist/nightly"
stage="$PWD/dist/stage"
rm -rf "$out" "$stage"
mkdir -p "$out" "$stage"

$CARGO build --release
sh scripts/build-windows.sh ${TOOLCHAIN:+"$TOOLCHAIN"}
rm -f dist/macos-legacy/voxelrust dist/macos/voxelrust
sh scripts/build-macos-legacy.sh ${TOOLCHAIN:+"$TOOLCHAIN"}
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
  case "$2" in
    *.app) ;; # the app carries its mods (and SDL2) inside
    *) mkdir -p "$dir/mods"
       for m in disasters earth landvehicles holidays; do cp -R "mods/$m" "$dir/mods/"; done ;;
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
package voxelrust-linux-x86_64         target/release/voxelrust     tar.gz
package voxelrust-windows-x86_64       dist/windows/voxelrust.exe   zip
package voxelrust-macos-x86_64         dist/macos/VoxelRust.app     zip
package voxelrust-macos-legacy-x86_64  dist/VoxelRust.app           zip
[ -d dist/web ] && (cd dist && zip -qry "$out/voxelrust-web.zip" web)

# The bare programs and the list the updater reads.
cp target/release/voxelrust       "$out/voxelrust-linux-x86_64.bin"
cp dist/windows/voxelrust.exe     "$out/voxelrust-windows-x86_64.exe"
cp dist/macos/voxelrust           "$out/voxelrust-macos-x86_64.bin"
cp dist/macos-legacy/voxelrust    "$out/voxelrust-macos-legacy-x86_64.bin"
{
  echo "commit $COMMIT"
  for pair in linux-x86_64:voxelrust-linux-x86_64.bin windows-x86_64:voxelrust-windows-x86_64.exe \
              macos-x86_64:voxelrust-macos-x86_64.bin macos-legacy-x86_64:voxelrust-macos-legacy-x86_64.bin; do
    build="${pair%%:*}"; file="${pair##*:}"
    echo "$build $file $(sha256sum "$out/$file" | cut -d' ' -f1) $(wc -c < "$out/$file" | tr -d ' ')"
  done
} > "$out/nightly.txt"
{
  echo "# build                 file                                   runs on"
  echo "linux-x86_64            voxelrust-linux-x86_64.tar.gz          Linux, 64-bit Intel/AMD"
  echo "windows-x86_64          voxelrust-windows-x86_64.zip           Windows 7 to 11, 64-bit"
  echo "macos-x86_64            voxelrust-macos-x86_64.zip             macOS 10.12 and later (Intel; Apple silicon through Rosetta)"
  echo "macos-legacy-x86_64     voxelrust-macos-legacy-x86_64.zip      Mac OS X 10.6 to 10.11 (64-bit Intel)"
  echo "web                     voxelrust-web.zip                      a modern browser (unzip onto any web server)"
  echo "# commit $COMMIT"
  echo "# SDL2 comes inside the Windows and Mac files; on Linux install your system's SDL2 package."
} > "$out/BUILDS.txt"
(cd "$out" && sha256sum voxelrust-* > SHA256SUMS.txt)
rm -rf "$stage"
ls -l "$out"
