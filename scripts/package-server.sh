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
mkdir -p "$stage/lib"
cp target/release/voxelrust "$stage/voxelrust"
cp -L "$sdl" "$stage/lib/libSDL2-2.0.so.0"

# run.sh is the supervisor: it starts the server, restarts it for an update,
# rolls an update back if the new program dies at once, and backs off after
# crashes (see scripts/server-supervisor.sh and docs/DEPLOY.md).
cp scripts/server-supervisor.sh "$stage/run.sh"
chmod +x "$stage/run.sh" "$stage/voxelrust"

cat > "$stage/README.txt" <<TXT
VoxelRust multiplayer server (build $COMMIT)

  ./run.sh --world public --port 25590 --data ./data

Players join with the server's address and port. Open the port in the
firewall. Changes are saved in the data folder every 20 seconds, and again
when the server is stopped (Ctrl-C, or systemctl stop).

The server updates itself from the project's release: it looks every 90
seconds (update_check_seconds in the data folder's settings.txt, 30 to 600),
and when a newer one is out it installs it, warns the players in chat,
saves, and restarts in 30 seconds. If the new program stops within 20
seconds, run.sh puts the previous one back. Only the program is replaced.
./run.sh --no-update turns this off. See docs/DEPLOY.md in the source.

No mods come with the server. To use some, put each one in a folder of its
own inside the data folder's mods folder (with the example above:
./data/mods/<name>/pack.txt; the server makes the folder, with a README.txt,
when it starts). Players who join are sent the mods that have only a
pack.txt, textures, models, sounds and scripts, and keep them for that visit
only. --serve-mods a,b,c chooses exactly which mods players download (by
default every mod whose pack.txt does not say "client no"). A mod that needs
compiled code cannot be sent: build the server with
  cargo build --release --features native-mods
and give players a client built the same way.

Options: run ./voxelrust --help. --players N sets the most players at once.
TXT

tar -C "$out" -czf "$out/voxelrust-server-linux-x86_64.tar.gz" voxelrust-server
echo "built $out/voxelrust-server-linux-x86_64.tar.gz (SDL2 from $sdl)"
