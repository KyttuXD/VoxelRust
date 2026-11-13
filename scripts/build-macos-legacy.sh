#!/bin/sh
# Build VoxelRust for Mac OS X 10.6 and later (64-bit Intel), from any host.
#
# Needs: a Rust toolchain with the x86_64-apple-darwin target, and python3.
# Needs no Apple SDK, Xcode or cross-compiler: the game imports about twenty
# functions from libSystem and nothing else, so a tiny text stub naming them
# is all the linker requires, and Rust's bundled linker (rust-lld) does the
# rest.
#
#   scripts/build-macos-legacy.sh [toolchain]     e.g.  ... 1.77.0
#
# Result: dist/macos-legacy/voxelrust
set -eu
cd "$(dirname "$0")/.."
TOOLCHAIN="${1:-}"
CARGO="cargo ${TOOLCHAIN:++$TOOLCHAIN}"
SDK="$PWD/target/macos-legacy-sdk"
OUT="$PWD/dist/macos-legacy"

mkdir -p "$SDK/usr/lib" "$OUT"
cat > "$SDK/usr/lib/libSystem.tbd" <<'TBD'
--- !tapi-tbd
tbd-version: 4
targets: [ x86_64-macos ]
install-name: '/usr/lib/libSystem.B.dylib'
current-version: 1.0
compatibility-version: 1.0
exports:
  - targets: [ x86_64-macos ]
    symbols: [ _malloc, _realloc, _calloc, _free, _abort, _exit, _getenv, _puts,
               _fflush, _dlopen, _dlsym, _memcpy, _memmove, _memset, _memcmp,
               _bcmp, _bzero, ___bzero, _memset_pattern16, _system,
               _socket, _connect, _bind, _listen, _accept, _send, _recv, _close,
               _getaddrinfo, _freeaddrinfo, _setsockopt, _getsockopt, _getsockname,
               _fcntl, _poll, ___error, _opendir, _readdir, _closedir, __NSGetExecutablePath, _rename, _unlink, _mkdir, _chmod,
               dyld_stub_binder ]
...
TBD

# target-cpu=core2 keeps the code runnable on every 64-bit Intel Mac.
SDKROOT="$SDK" MACOSX_DEPLOYMENT_TARGET=10.6 VOXELRUST_BUILD=macos-legacy-x86_64 \
RUSTFLAGS="-C linker=rust-lld -C linker-flavor=ld64.lld -C target-cpu=core2 \
  -C link-arg=-e -C link-arg=_voxelrust_start \
  -C link-arg=-headerpad -C link-arg=0x400 -C link-arg=-no_pie" \
  $CARGO build --release --target x86_64-apple-darwin --target-dir target/macos-legacy

python3 scripts/macho_legacy.py \
  target/macos-legacy/x86_64-apple-darwin/release/voxelrust "$OUT/voxelrust"
chmod +x "$OUT/voxelrust"
echo "built $OUT/voxelrust"

# The same source as an ordinary modern build (LC_MAIN entry, 10.12 and
# later), for comparison on newer Intel Macs. The legacy binary above is
# meant to run there too.
mkdir -p "$PWD/dist/macos"
SDKROOT="$SDK" MACOSX_DEPLOYMENT_TARGET=10.12 VOXELRUST_BUILD=macos-x86_64 \
RUSTFLAGS="-C linker=rust-lld -C linker-flavor=ld64.lld" \
  $CARGO build --release --target x86_64-apple-darwin --target-dir target/macos-modern
cp target/macos-modern/x86_64-apple-darwin/release/voxelrust "$PWD/dist/macos/voxelrust"
echo "built $PWD/dist/macos/voxelrust"
echo "Copy it to the Mac together with an SDL2 built for 10.6 (libSDL2-2.0.0.dylib"
echo "next to it, or SDL2.framework in /Library/Frameworks). See docs/PORTING.md."
