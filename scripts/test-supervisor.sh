#!/bin/sh
# Tests scripts/server-supervisor.sh with stand-in "programs" (small shell
# scripts), so no game build is needed and it takes a few seconds:
#
#   1. a program that asks for a restart (exit 75) is followed by the new
#      program, which runs and stops cleanly: the supervisor ends with 0;
#   2. a program installed by an update that dies at once is rolled back: the
#      previous program is back, the release is written to rejected-seq, and
#      the previous program is started again;
#   3. an ordinary crash is started again after a wait, and then ends cleanly.
#
#   sh scripts/test-supervisor.sh
set -eu
cd "$(dirname "$0")/.."
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
fail() { echo "FAIL: $1" >&2; exit 1; }
export VOXELRUST_DELAY_UNIT=0 VOXELRUST_ROLLBACK_SECONDS=20

# --- 1 and 2: update, then a bad update --------------------------------
mkdir -p "$work/a"
cp scripts/server-supervisor.sh "$work/a/run.sh"
# Program A: first run, "installs" program B (which dies at once) and asks for a
# restart; later runs just record themselves and stop cleanly.
cat > "$work/a/voxelrust" <<'EOF'
#!/bin/sh
here="$(cd "$(dirname "$0")" && pwd)"
echo "A $*" >> "$here/runs.txt"
n="$(wc -l < "$here/runs.txt" | tr -d ' ')"
if [ "$n" = 1 ]; then
  printf '#!/bin/sh\necho "B" >> "$(dirname "$0")/runs.txt"\nexit 3\n' > "$here/voxelrust.new"
  chmod 755 "$here/voxelrust.new"; mv "$here/voxelrust.new" "$here/voxelrust"
  echo 42 > "$VOXELRUST_SUPERVISED/installed-seq"
  exit 75
fi
exit 0
EOF
chmod 755 "$work/a/voxelrust" "$work/a/run.sh"
# The first run lasts less than 20 s, so A is not "known good" yet: mark it so
# (a real server has run for hours before an update).
mkdir -p "$work/a/supervisor"
cp "$work/a/voxelrust" "$work/a/supervisor/good-program"
if ! "$work/a/run.sh" --world t --data "$work/a/d" 2> "$work/a/log.txt"; then cat "$work/a/log.txt"; fail "the supervisor did not end with 0"; fi
runs="$(tr '\n' ' ' < "$work/a/runs.txt")"
[ "$runs" = "A --server --world t --data $work/a/d B A --server --world t --data $work/a/d " ] || fail "runs were: $runs"
[ "$(cat "$work/a/supervisor/rejected-seq")" = 42 ] || fail "rejected-seq is not 42"
[ -f "$work/a/supervisor/rejected-program" ] || fail "the bad program was not kept"
grep -q "rolling back" "$work/a/log.txt" || fail "no rollback in the log"
echo "ok: an update is followed, a bad one is rolled back and its release rejected"

# --- 3: an ordinary crash is restarted, with the waits ------------------
mkdir -p "$work/b"
cp scripts/server-supervisor.sh "$work/b/run.sh"
cat > "$work/b/voxelrust" <<'EOF'
#!/bin/sh
here="$(cd "$(dirname "$0")" && pwd)"
echo run >> "$here/runs.txt"
n="$(wc -l < "$here/runs.txt" | tr -d ' ')"
[ "$n" -ge 3 ] && exit 0
exit 134
EOF
chmod 755 "$work/b/voxelrust" "$work/b/run.sh"
"$work/b/run.sh" 2> "$work/b/log.txt" || fail "the supervisor did not end with 0 after the crashes"
[ "$(wc -l < "$work/b/runs.txt" | tr -d ' ')" = 3 ] || fail "expected 3 runs"
grep -q "starting it again" "$work/b/log.txt" || fail "no restart after a crash in the log"
[ ! -f "$work/b/supervisor/rejected-seq" ] || fail "a plain crash must not reject a release"
echo "ok: a crash is restarted and ends cleanly"
