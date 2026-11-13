#!/bin/sh
# Package the multiplayer server for Linux, SDL2 included, so that nothing
# has to be installed beside it.
#
#   scripts/package-server.sh [toolchain]
#
# Result: dist/server/voxelrust-server-linux-x86_64.tar.gz, holding
#   voxelrust-server/voxelrust      the program (run with --server)
#   voxelrust-server/run.sh         starts it with the bundled SDL2
#   voxelrust-server/lib/           SDL2 (libSDL2-2.0.so.0)
#   voxelrust-server/mods/          the mods that come with the game
#   voxelrust-server/README.txt     what to do with it
#
# SDL2 is taken from $SDL2_LIB if that names the library file, else from the
# system's copy. The server never opens a window, so SDL2 only has to load.
set -eu
cd "$(dirname "$0")/.."
TOOLCHAIN="${1:-}"
CARGO="cargo ${TOOLCHAIN:++$TOOLCHAIN}"
COMMIT="${VOXELRUST_COMMIT:-$(git rev-parse --short=12 HEAD)}"
export VOXELRUST_COMMIT="$COMMIT"

sdl="${SDL2_LIB:-$(ldconfig -p | awk '/libSDL2-2\.0\.so\.0 / {print $NF; exit}')}"
if [ -z "$sdl" ] || [ ! -f "$sdl" ]; then
  echo "package-server: SDL2 not found; set SDL2_LIB to libSDL2-2.0.so.0" >&2
  exit 1
fi

$CARGO build --release
out="$PWD/dist/server"
stage="$out/voxelrust-server"
rm -rf "$stage"
mkdir -p "$stage/lib" "$stage/mods"
cp target/release/voxelrust "$stage/voxelrust"
cp -L "$sdl" "$stage/lib/libSDL2-2.0.so.0"
for m in $(grep -v '^#' mods/SHIPPED); do
  cp -R "mods/$m" "$stage/mods/"
done

cat > "$stage/run.sh" <<'RUN'
#!/bin/sh
# Starts the multiplayer server with the SDL2 that comes with it.
# Options are passed on; for example:
#   ./run.sh --world public --port 25590 --data ./data
here="$(cd "$(dirname "$0")" && pwd)"
LD_LIBRARY_PATH="$here/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export LD_LIBRARY_PATH
exec "$here/voxelrust" --server --no-update "$@"
RUN
chmod +x "$stage/run.sh" "$stage/voxelrust"

cat > "$stage/README.txt" <<TXT
VoxelRust multiplayer server (build $COMMIT)

  ./run.sh --world public --port 25590 --data ./data

Players join with the server's address and port. Open the port in the
firewall. Changes are saved in the data folder every 20 seconds. Stopping the
server (Ctrl-C, or systemctl stop) does not save first, so up to 20 seconds
of changes can be lost.

Options: run ./voxelrust --help. --players N sets the most players at once.
TXT

tar -C "$out" -czf "$out/voxelrust-server-linux-x86_64.tar.gz" voxelrust-server
echo "built $out/voxelrust-server-linux-x86_64.tar.gz (SDL2 from $sdl)"
