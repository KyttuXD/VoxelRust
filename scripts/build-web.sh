#!/bin/sh
# Build VoxelRust for web browsers (WebAssembly + WebGL), from any host.
#
# Needs: a current Rust toolchain with the wasm32-unknown-unknown target
# (rustup target add wasm32-unknown-unknown). Needs nothing else: no npm, no
# bundler, no wasm-bindgen, no emscripten. The page and its JavaScript are
# written by hand and live in web/.
#
#   scripts/build-web.sh [toolchain]
#
# Result: dist/web/ (index.html, voxelrust.js, voxelrust.wasm and the icon).
# Serve that folder with any web server, for example
#   python3 -m http.server -d dist/web
# and open http://localhost:8000/. See docs/WEB.md.
set -eu
cd "$(dirname "$0")/.."
# A shared target folder (CARGO_TARGET_DIR) keeps compiled code between checkouts and scripts.
TARGET="${CARGO_TARGET_DIR:-target}"
TOOLCHAIN="${1:-}"
CARGO="cargo ${TOOLCHAIN:++$TOOLCHAIN}"
mkdir -p dist/web
# The stack is 8 MB, as the main thread's is on a desktop system (the
# linker's default for WebAssembly is 1 MB).
VOXELRUST_BUILD=web RUSTFLAGS="-C link-arg=-zstack-size=8388608" \
  $CARGO build --release --target wasm32-unknown-unknown --target-dir "$TARGET"/web
cp "$TARGET"/web/wasm32-unknown-unknown/release/voxelrust.wasm dist/web/voxelrust.wasm
cp web/index.html web/voxelrust.js dist/web/
cp assets/icon.png dist/web/icon.png
cp assets/icon.ico dist/web/favicon.ico
# The page must supply every function the game imports, or it will not start.
if command -v python3 >/dev/null 2>&1; then
  python3 web/check-imports.py dist/web/voxelrust.wasm web/voxelrust.js
fi
echo "built dist/web/ ($(wc -c < dist/web/voxelrust.wasm | tr -d ' ') bytes of WebAssembly)"
