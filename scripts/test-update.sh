#!/bin/sh
# End-to-end test of the updater, with no GitHub: a local web server plays the
# release. Builds an older and a newer program (the update-test feature), then
#
#   1. the old program installs the new one and checks it with --stamp;
#   2. the new program is started by the update, notes it, and stops (it does
#      not restart again);
#   3. a release with a wrong checksum is refused, and the program is left as it was;
#   4. a release with the same number is left alone (no download);
#   5. a server updates itself;
#   6. a list that was changed after it was signed, or signed by another key,
#      is refused unread, and a missing signature too;
#   7. the plain-HTTP mirror: with the release unreachable, a program takes
#      the update from a mirror (python3 -m http.server), signature and all.
#
# Everything is signed with a throwaway key made here; the programs are built
# to trust its public key (VOXELRUST_UPDATE_KEY, read only by the update-test
# build), never the project's real one.
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
# The throwaway signing key, and its public key for the programs to trust.
openssl genpkey -algorithm ED25519 -out "$work/test-key.pem"
openssl genpkey -algorithm ED25519 -out "$work/other-key.pem"
export VOXELRUST_SIGN_KEY="$work/test-key.pem"
pub="$(openssl pkey -in "$work/test-key.pem" -pubout -outform DER | tail -c 32 | od -An -v -tx1 | tr -d ' \n')"

build() { # build COMMIT SEQ OUTPUT
  VOXELRUST_COMMIT="$1" VOXELRUST_SEQ="$2" VOXELRUST_RELEASE="$rel" VOXELRUST_UPDATE_KEY="$pub" \
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
[ ! -f "$work/data/tmp/update/update.txt" ] || fail "the restart marker was not consumed"
gets="$(grep -c 'GET /voxelrust-linux-x86_64.bin' "$work/http.log" || true)"
[ "$gets" = 1 ] || fail "the program was downloaded $gets times (expected once)"
echo "ok: the started copy ran once, and nothing restarted again"

# 3. A list changed after it was signed is refused (the signature no longer
#    fits), and the program is left byte-identical.
cp "$work/bin/voxelrust" "$work/before"
: > "$work/update.log"
sed -i 's/^seq 2$/seq 3/' "$work/serve/nightly.txt"
cp "$work/new-program" "$work/serve/voxelrust-linux-x86_64.bin"
run_updater
cmp -s "$work/before" "$work/bin/voxelrust" || fail "a list with a broken signature replaced the program"
grep -q "signature is not valid" "$work/update.log" || fail "the bad signature was not logged"
echo "ok: a changed list is refused"
# 3b. A properly signed list with a wrong checksum is refused at the download.
: > "$work/update.log"
sed -i 's/ [0-9a-f]\{64\} / 0000000000000000000000000000000000000000000000000000000000000000 /' "$work/serve/nightly.txt"
sh scripts/sign-file.sh "$work/serve/nightly.txt"
run_updater
cmp -s "$work/before" "$work/bin/voxelrust" || fail "a bad download replaced the program"
[ ! -f "$work/bin/voxelrust.new" ] || fail "a refused download was left behind"
grep -q "does not match the release list" "$work/update.log" || fail "the refusal was not logged"
echo "ok: a wrong checksum is refused and nothing is replaced"
# 3c. A list signed by another key, and a list with no signature at all.
: > "$work/update.log"
sh scripts/write-nightly-list.sh "$work/serve" bbbbbbbbbbbb 3
VOXELRUST_SIGN_KEY="$work/other-key.pem" sh scripts/sign-file.sh "$work/serve/nightly.txt"
run_updater
cmp -s "$work/before" "$work/bin/voxelrust" || fail "a list signed by another key replaced the program"
grep -q "signature is not valid" "$work/update.log" || fail "the foreign signature was not logged"
rm -f "$work/serve/nightly.txt.sig"
: > "$work/update.log"
run_updater
cmp -s "$work/before" "$work/bin/voxelrust" || fail "an unsigned list replaced the program"
grep -q "could not read the release list's signature" "$work/update.log" || fail "the missing signature was not logged"
echo "ok: a list signed by another key, or not signed, is refused"

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

# 6/7. The plain-HTTP mirror. The release address is made unreachable (its
# server stops), and a second server on another port, with the same signed
# files, is the mirror. The program takes the update from it and checks it all.
kill "$server" 2>/dev/null || true
mport=8766
mkdir -p "$work/mirror" "$work/data3" "$work/bin3"
cp "$work/new-program" "$work/mirror/voxelrust-linux-x86_64.bin"
sh scripts/write-nightly-list.sh "$work/mirror" bbbbbbbbbbbb 2
python3 -m http.server $mport --bind 127.0.0.1 -d "$work/mirror" > "$work/mirror.log" 2>&1 &
server=$!
sleep 1
cp "$work/target-1/release/voxelrust" "$work/bin3/voxelrust"
mirror="http://127.0.0.1:$mport/"
run_mirror() { "$work/bin3/voxelrust" --update-now --update-mirror "$mirror" --data "$work/data3" --log "$work/mirror-update.log" >/dev/null 2>&1; }
# A mirror whose list was changed is refused.
cp "$work/mirror/nightly.txt" "$work/mirror/nightly.txt.good"
sed -i 's/^seq 2$/seq 9/' "$work/mirror/nightly.txt"
cp "$work/bin3/voxelrust" "$work/before3"
run_mirror
cmp -s "$work/before3" "$work/bin3/voxelrust" || fail "a tampered mirror replaced the program"
grep -q "signature is not valid" "$work/mirror-update.log" || fail "the tampered mirror was not refused with a log line"
# The real one is used.
cp "$work/mirror/nightly.txt.good" "$work/mirror/nightly.txt"
run_mirror
"$work/bin3/voxelrust" --stamp "$work/stamp3.txt"
[ "$(cat "$work/stamp3.txt")" = "voxelrust-stamp linux-x86_64 bbbbbbbbbbbb 2" ] || fail "the mirror's update was not installed: $(cat "$work/stamp3.txt")"
grep -q 'GET /nightly.txt.sig' "$work/mirror.log" || fail "the mirror was not asked for the signature"
echo "ok: a program updated itself from a plain-HTTP mirror, and refused a tampered one"
echo "all update checks passed"
