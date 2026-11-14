#!/bin/sh
# Build VoxelRust for 64-bit Windows, from any host.
#
# Needs: a Rust toolchain (1.71+) with the x86_64-pc-windows-msvc target.
# Needs no Visual Studio, Windows SDK or MinGW: the game brings its own
# start-up code and the compiler writes the import tables itself.
#
#   scripts/build-windows.sh [toolchain]
#
# Result: dist/windows/voxelrust.exe  (put SDL2.dll, 64-bit, next to it)
set -eu
cd "$(dirname "$0")/.."
# A shared target folder (CARGO_TARGET_DIR) keeps compiled code between checkouts and scripts.
TARGET="${CARGO_TARGET_DIR:-target}"
TOOLCHAIN="${1:-}"
CARGO="cargo ${TOOLCHAIN:++$TOOLCHAIN}"
mkdir -p dist/windows
RUSTFLAGS="-C linker=rust-lld" \
  $CARGO build --release --target x86_64-pc-windows-msvc --target-dir "$TARGET"/windows
cp "$TARGET"/windows/x86_64-pc-windows-msvc/release/voxelrust.exe dist/windows/voxelrust.exe
echo "built dist/windows/voxelrust.exe"
