#!/bin/sh
# End-to-end test of the updater, with no GitHub: a local web server plays the
# release. Builds an older and a newer program (the update-test feature), then
#
#   1. the old program installs the new one and checks it with --stamp;
#   2. the new program is started by the update, notes it, and stops (it does
#      not restart again);
#   3. a release with a wrong checksum is refused, and the program is left as it was;
#   4. a release with the same number is left alone (no download).
#
#   sh scripts/test-update.sh
set -eu
cd "$(dirname "$0")/.."
. scripts/mods-dir.sh
# The toolchain to build with (TOOLCHAIN=1.77.0 to check the older one).
work="$(mktemp -d)"
port=8765
server=0
spid=0
trap 'kill "$server" "$spid" 2>/dev/null || true; pkill -f "$work/bin2/voxelrust" 2>/dev/null || true; [ -n "${KEEP:-}" ] || rm -rf "$work"' EXIT
rel="http://127.0.0.1:$port/"

build() { # build COMMIT SEQ OUTPUT
  VOXELRUST_COMMIT="$1" VOXELRUST_SEQ="$2" VOXELRUST_RELEASE="$rel" \
    cargo +"${TOOLCHAIN:-1.99.1}" build --release --features update-test --target-dir "$work/target-$2" >/dev/null 2>&1
  cp "$work/target-$2/release/voxelrust" "$3"
}
mkdir -p "$work/serve" "$work/bin" "$work/data"
build aaaaaaaaaaaa 1 "$work/bin/voxelrust"
build bbbbbbbbbbbb 2 "$work/new-program"
cp "$work/new-program" "$work/serve/voxelrust-linux-x86_64.bin"
sh scripts/write-nightly-list.sh "$work/serve" bbbbbbbbbbbb 2
python3 -m http.server $port --bind 127.0.0.1 -d "$work/serve" > "$work/http.log" 2>&1 &
server=$!
sleep 1

fail() { echo "FAIL: $1" >&2; exit 1; }
run_updater() { "$work/bin/voxelrust" --update-now --data "$work/data" --log "$work/update.log" >/dev/null 2>&1; }

# 1. The old program installs the new one.
run_updater
"$work/bin/voxelrust" --stamp "$work/stamp.txt"
[ "$(cat "$work/stamp.txt")" = "voxelrust-stamp linux-x86_64 bbbbbbbbbbbb 2" ] || fail "the program was not replaced: $(cat "$work/stamp.txt")"
grep -q "starting the new version" "$work/update.log" || fail "no restart was started"
echo "ok: the old program installed release 2"

# 2. The started copy notes the update, and does not restart again.
for _ in 1 2 3 4 5 6 7 8 9 10; do [ -f "$work/data/update-child.txt" ] && break; sleep 1; done
[ -f "$work/data/update-child.txt" ] || fail "the new program was not started by the update"
[ ! -f "$work/data/update.txt" ] || fail "the restart marker was not consumed"
gets="$(grep -c 'GET /voxelrust-linux-x86_64.bin' "$work/http.log" || true)"
[ "$gets" = 1 ] || fail "the program was downloaded $gets times (expected once)"
echo "ok: the started copy ran once, and nothing restarted again"

# 3. A wrong checksum is refused, and the program is left byte-identical.
cp "$work/bin/voxelrust" "$work/before"
sed -i 's/^seq 2$/seq 3/' "$work/serve/nightly.txt"
cp "$work/new-program" "$work/serve/voxelrust-linux-x86_64.bin"
sed -i 's/ [0-9a-f]\{64\} / 0000000000000000000000000000000000000000000000000000000000000000 /' "$work/serve/nightly.txt"
run_updater
cmp -s "$work/before" "$work/bin/voxelrust" || fail "a bad download replaced the program"
[ ! -f "$work/bin/voxelrust.new" ] || fail "a refused download was left behind"
grep -q "does not match the release list" "$work/update.log" || fail "the refusal was not logged"
echo "ok: a wrong checksum is refused and nothing is replaced"

# 4. The same number is left alone: no download at all.
sh scripts/write-nightly-list.sh "$work/serve" bbbbbbbbbbbb 1
before_gets="$(grep -c 'GET /voxelrust' "$work/http.log" || true)"
run_updater
after_gets="$(grep -c 'GET /voxelrust' "$work/http.log" || true)"
[ "$before_gets" = "$after_gets" ] || fail "a release with an older or equal number was downloaded"
echo "ok: an older or equal release is not downloaded"
# 5. A server installs an update and restarts into it once nobody is playing.
mkdir -p "$work/bin2" "$work/data2"
cp "$work/target-1/release/voxelrust" "$work/bin2/voxelrust"
cp "$work/new-program" "$work/serve/voxelrust-linux-x86_64.bin"
sh scripts/write-nightly-list.sh "$work/serve" bbbbbbbbbbbb 2
"$work/bin2/voxelrust" --server --port 25599 --data "$work/data2" --log "$work/server.log" >/dev/null 2>&1 &
spid=$!
for _ in $(seq 1 40); do grep -q "restarting into the new version" "$work/server.log" 2>/dev/null && break; sleep 1; done
if ! grep -q "restarting into the new version" "$work/server.log"; then tail -20 "$work/server.log"; cat "$work/http.log" | tail -4; fail "the server did not restart into the update"; fi
# The new copy is a server too: it serves the same world on the same port.
for _ in $(seq 1 30); do [ "$(grep -c 'serving world' "$work/server.log")" -ge 2 ] && break; sleep 1; done
[ "$(grep -c 'serving world' "$work/server.log")" -ge 2 ] || fail "the restarted server did not start serving"
echo "ok: an empty server restarted into the update"
echo "all update checks passed"
