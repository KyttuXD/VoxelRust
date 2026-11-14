#!/bin/sh
# Runs the VoxelRust server and keeps it running: restarts it for an update,
# rolls an update back if the new program dies at once, and never crash-loops.
# package-server.sh installs this as run.sh next to the program, so this is
# what a systemd unit (or a terminal) starts:
#
#   ./run.sh --world public --port 25590 --data ./data
#
# What it does:
#   - Starts  voxelrust --server <your options>  with the bundled SDL2.
#   - The server downloads and checks a newer release by itself (see
#     docs/DEPLOY.md), warns its players (RESTARTING FOR AN UPDATE IN 30
#     SECONDS), saves, and exits with code 75. This script then starts the
#     new program.
#   - ROLLBACK: if a program started by an update stops with an error within
#     20 seconds, the previous program is put back and started again, and the
#     release is noted in <state>/rejected-seq so the server does not fetch
#     that release again (a newer one is tried). The bad program is kept as
#     <state>/rejected-program.
#   - A server that crashes (not after an update) is started again after 5
#     seconds, then 10, 20, 40, 60 (the wait resets after a run of a minute).
#   - Exit code 0 (Ctrl+C, SIGTERM, systemctl stop) ends the script too.
#
# Settings by environment (all optional): VOXELRUST_STATE_DIR (default
# ./supervisor next to this script), VOXELRUST_ROLLBACK_SECONDS (20),
# VOXELRUST_DELAY_UNIT (5: the seconds the waits are built from).
#
# The same rules are in src/client/server_update/schedule.rs (rollback_needed,
# crash_delay_seconds); scripts/test-supervisor.sh checks this script.
set -u
here="$(cd "$(dirname "$0")" && pwd)"
bin="${VOXELRUST_PROGRAM:-$here/voxelrust}"
state="${VOXELRUST_STATE_DIR:-$here/supervisor}"
window="${VOXELRUST_ROLLBACK_SECONDS:-20}"
unit="${VOXELRUST_DELAY_UNIT:-5}"
mkdir -p "$state" || exit 1
if [ -d "$here/lib" ]; then
  LD_LIBRARY_PATH="$here/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  export LD_LIBRARY_PATH
fi
# The server reads this: where to note the installed release, and which
# release was rejected.
VOXELRUST_SUPERVISED="$state"
export VOXELRUST_SUPERVISED

say() { echo "supervisor: $(date '+%Y-%m-%d %H:%M:%S') $*" >&2; }

stop=0
child=""
trap 'stop=1; [ -n "$child" ] && kill -TERM "$child" 2>/dev/null' TERM INT

trial=0       # 1 while running a program that an update has just installed
crashes=0     # crashes in a row
while :; do
  # The program as it is now: it becomes the known-good one once it has run a while.
  cp -f "$bin" "$state/candidate-program" 2>/dev/null
  [ "$trial" = 1 ] || rm -f "$state/installed-seq"
  started="$(date +%s)"
  "$bin" --server "$@" &
  child=$!
  wait "$child"
  rc=$?
  # A stop request arrived: wait for the server to finish saving, then end.
  if [ "$stop" = 1 ]; then
    while kill -0 "$child" 2>/dev/null; do wait "$child"; done
    say "stopped"
    exit 0
  fi
  child=""
  lived=$(( $(date +%s) - started ))

  if [ "$rc" = 0 ]; then
    say "the server stopped (exit 0)"
    exit 0
  fi

  # It ran long enough to count as good: it is what a rollback goes back to.
  if [ "$lived" -ge "$window" ] && [ -f "$state/candidate-program" ]; then
    mv -f "$state/candidate-program" "$state/good-program"
  fi

  if [ "$rc" = 75 ]; then
    say "an update was installed; starting the new program"
    trial=1
    crashes=0
    sleep "$unit"
    continue
  fi

  if [ "$trial" = 1 ] && [ "$lived" -lt "$window" ]; then
    # The updated program failed at once: put the previous one back.
    seq="$(cat "$state/installed-seq" 2>/dev/null || true)"
    say "the updated program stopped after ${lived}s (exit $rc): rolling back"
    if [ -f "$state/good-program" ]; then
      cp -f "$state/good-program" "$bin.rollback" && chmod 755 "$bin.rollback" \
        && mv -f "$bin" "$state/rejected-program" && mv -f "$bin.rollback" "$bin"
      [ -n "$seq" ] && echo "$seq" > "$state/rejected-seq"
      say "release ${seq:-?} is rejected; the previous program is back"
    else
      say "there is no earlier program to go back to"
    fi
    trial=0
    sleep "$unit"
    continue
  fi

  # An ordinary crash: wait, longer each time in a row.
  trial=0
  if [ "$lived" -ge 60 ]; then crashes=0; fi
  delay=$(( unit << (crashes < 4 ? crashes : 4) ))
  [ "$delay" -gt $(( unit * 12 )) ] && delay=$(( unit * 12 ))
  crashes=$(( crashes + 1 ))
  say "the server stopped (exit $rc) after ${lived}s; starting it again in ${delay}s"
  sleep "$delay"
done
