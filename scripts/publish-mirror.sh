#!/bin/sh
# Copy the updater's files to a plain-HTTP mirror, for programs that cannot do
# HTTPS (the build for Mac OS X 10.6).
#
#   sh scripts/publish-mirror.sh <ssh host> <remote dir>
#   sh scripts/publish-mirror.sh me@example.org /var/www/voxelrust
#
# Copies from dist/nightly/ (made by scripts/package-release.sh): nightly.txt,
# nightly.txt.sig and the bare programs (*.bin, *.exe), into <remote dir> on
# the host, which a web server serves as a static folder (see docs/BUILDING.md).
# Players then set  update_mirror=http://your.host/voxelrust/  in settings.txt.
#
# The mirror needs no trust: the game checks nightly.txt's signature and every
# program's checksum. The programs go first and the list last, so that a game
# never sees a list that names files not there yet.
# MIRROR_TOOL=scp uses scp instead of rsync.
set -eu
cd "$(dirname "$0")/.."
host="${1:?usage: publish-mirror.sh <ssh host> <remote dir>}"
dir="${2:?usage: publish-mirror.sh <ssh host> <remote dir>}"
src="dist/nightly"
[ -f "$src/nightly.txt" ] && [ -f "$src/nightly.txt.sig" ] || { echo "no signed list in $src (run scripts/package-release.sh first)" >&2; exit 1; }
programs=""
for f in "$src"/voxelrust-*.bin "$src"/voxelrust-*.exe; do
  [ -f "$f" ] && programs="$programs $f"
done
[ -n "$programs" ] || { echo "no programs in $src" >&2; exit 1; }
if command -v rsync >/dev/null 2>&1 && [ "${MIRROR_TOOL:-rsync}" = rsync ]; then
  # shellcheck disable=SC2086
  rsync -v --chmod=F644 $programs "$host:$dir/"
  rsync -v --chmod=F644 "$src/nightly.txt.sig" "$src/nightly.txt" "$host:$dir/"
else
  # shellcheck disable=SC2086
  scp $programs "$host:$dir/"
  scp "$src/nightly.txt.sig" "$src/nightly.txt" "$host:$dir/"
fi
echo "published to $host:$dir (the list last)"
