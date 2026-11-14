#!/bin/sh
# Wrap a Mac build as a double-clickable VoxelRust.app.
#
#   scripts/package-macos-app.sh [path/to/SDL2.framework]
#   VARIANT=modern scripts/package-macos-app.sh [path/to/SDL2.framework]
#
# There are two Mac builds, and each app says which systems it is for, so
# macOS itself refuses to start the wrong one:
#   legacy (the default)  dist/VoxelRust.app         Mac OS X 10.6 and later
#   modern                dist/macos/VoxelRust.app   macOS 10.12 and later
# Builds the programs first if they are missing. If an SDL2.framework is
# given it is copied inside the app, which then needs nothing else installed
# (use the official SDL2 2.0.4 one for Mac OS X 10.6).
set -eu
cd "$(dirname "$0")/.."
[ -f dist/macos-legacy/voxelrust ] || scripts/build-macos-legacy.sh
if [ "${VARIANT:-legacy}" = "modern" ]; then
  BIN=dist/macos/voxelrust; APP=dist/macos/VoxelRust.app; MIN=10.12
else
  BIN=dist/macos-legacy/voxelrust; APP=dist/VoxelRust.app; MIN=10.6
fi
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Frameworks"
cp "$BIN" "$APP/Contents/MacOS/voxelrust"
chmod +x "$APP/Contents/MacOS/voxelrust"
[ -f assets/icon.icns ] && cp assets/icon.icns "$APP/Contents/Resources/icon.icns"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>VoxelRust</string>
  <key>CFBundleDisplayName</key><string>VoxelRust</string>
  <key>CFBundleExecutable</key><string>voxelrust</string>
  <key>CFBundleIdentifier</key><string>io.github.voxelrust</string>
  <key>CFBundleIconFile</key><string>icon</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleSignature</key><string>????</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>$MIN</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.games</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSPrincipalClass</key><string>NSApplication</string>
</dict>
</plist>
PLIST
printf 'APPL????' > "$APP/Contents/PkgInfo"
# (No mods ride inside the app: players install them in the data folder's
# mods folder, or a server sends them.)
# SDL2 rides inside the app, so nothing else need be installed: the
# framework named on the command line, or the one in vendor/sdl2.
if [ -n "${1:-}" ]; then
  cp -R "$1" "$APP/Contents/Frameworks/"
else
  # vendor/ is not in git: in a clean checkout or a worktree, use the main checkout's.
  VENDOR=vendor
  [ -d vendor/sdl2/SDL2.framework ] || VENDOR="$(cd "$(git rev-parse --git-common-dir)/.." && pwd)/vendor"
  [ -d "$VENDOR/sdl2/SDL2.framework" ] || { echo "refusing: no SDL2.framework in vendor/sdl2 (see vendor/README.md): the app would not open" >&2; exit 1; }
  cp -R "$VENDOR/sdl2/SDL2.framework" "$APP/Contents/Frameworks/"
fi
[ -f "$APP/Contents/Frameworks/SDL2.framework/Versions/A/SDL2" ] || { echo "refusing: the SDL2.framework inside the app is incomplete" >&2; exit 1; }
echo "built $APP (for macOS $MIN and later)"
